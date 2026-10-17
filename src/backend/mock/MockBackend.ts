// Backend simulation on timers: hot-plug, connect/disconnect, erase/program/
// verify, mass erase and read with bench speeds, cancellation, injected errors
// (dev panel), a journal like the real one.

import type { Backend, DeviceEvent, OpOptions, Unsubscribe } from "../Backend";
import {
  BackendFailure,
  normalizeSettings,
  type AppSettings,
  type BackendErrorKind,
  type BinFile,
  type ConnectOptions,
  type DeviceSession,
  type EraseResult,
  type FlashPlan,
  type LogEntry,
  type LogLevel,
  type ProbeId,
  type QuickWriteSettings,
  type ReadJob,
  type ReadResult,
  type SaveTarget,
  type WriteJob,
  type WriteResult,
  type WriteStage,
} from "../types";
import { H743_GEOMETRY, planWrite } from "../../domain/geometry";
import { crc32 } from "../../domain/crc32";
import { SPEED, estimateBankErase } from "../../domain/estimate";
import { DEFAULT_QUICK, migrateQuick } from "../../domain/quickWrite";
import {
  DEMO_FILE,
  MOCK_LOG,
  MOCK_MESSAGES,
  MOCK_PROBE,
  MOCK_SUMMARY,
  MOCK_TARGET,
  MOCK_UID,
  mockConnection,
  synthFirmware,
} from "./mockData";

/** Faults the dev panel can inject into the next operation. */
export type MockFault = "Disconnected" | "CoreNotAccessible" | "FlashProtected" | "Busy" | "EraseFault";

export interface MockControlsState {
  /** On USB. */
  connected: boolean;
  /** Opened by the app («Подключиться»). */
  opened: boolean;
  nextFault: MockFault | null;
  speed: number;
  watchedFiles: number;
}

const STORAGE_PREFIX = "stlink-ui.quick.";
const SETTINGS_KEY = "stlink-ui.settings";
/** Delay before the probe "appears" after start. */
const STARTUP_CONNECT_MS = 2000;
const CONNECT_MS = 450;
/** Block size between progress reports / cancellation checks. */
const BLOCK = 16 * 1024;
const LOG_CAPACITY = 5000;

interface StoredFile {
  meta: BinFile;
  data: Uint8Array;
  /** Number of simulated rebuilds (changes the synthesized content). */
  generation: number;
}

function fail(kind: BackendErrorKind, message: string): BackendFailure {
  return new BackendFailure({ kind, message });
}

function tauriWebview(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function storage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

/** `14:31:58.402` today → ms since epoch. */
function todayAt(clock: string): number {
  const [h, m, rest] = clock.split(":");
  const [sec, ms] = rest.split(".");
  const d = new Date();
  d.setHours(Number(h), Number(m), Number(sec), Number(ms));
  return d.getTime();
}

const hex = (bytes: number[]) => bytes.map((b) => b.toString(16).toUpperCase().padStart(2, "0")).join(" ");

export class MockBackend implements Backend {
  readonly kind = "mock" as const;

  private connected = false;
  private opened = false;
  private startupTimer: ReturnType<typeof setTimeout> | null = null;
  private deviceListeners = new Set<(e: DeviceEvent) => void>();
  private fileWatchers = new Map<string, Set<(f: BinFile) => void>>();
  private files = new Map<string, StoredFile>();
  private flash = new Uint8Array(H743_GEOMETRY.size).fill(H743_GEOMETRY.erasedValue);
  private nextFault: MockFault | null = null;
  private speed = 1;
  private controlListeners = new Set<() => void>();
  private controlsSnapshot: MockControlsState;
  private fileSeq = 0;
  private startedOnce = false;
  private settings: AppSettings;
  private journal: LogEntry[] = [];
  private logSeq = 0;
  private logListeners = new Set<(e: LogEntry[]) => void>();

  constructor() {
    const demo = synthFirmware(DEMO_FILE.name, DEMO_FILE.size);
    this.flash.set(demo, 0);
    this.seedQuickWrite(demo);
    this.settings = this.readSettings();
    for (const [clock, level, message] of MOCK_LOG) this.push(level, message, todayAt(clock));
    this.controlsSnapshot = this.makeSnapshot();
  }

  // ---- dev panel controls ----

  readonly controls = {
    subscribe: (cb: () => void): Unsubscribe => {
      this.controlListeners.add(cb);
      return () => this.controlListeners.delete(cb);
    },
    getState: (): MockControlsState => this.controlsSnapshot,
    connect: () => this.setConnected(true),
    disconnect: () => this.setConnected(false),
    setNextFault: (f: MockFault | null) => {
      this.nextFault = f;
      this.emitControls();
    },
    setSpeed: (s: number) => {
      this.speed = s;
      this.emitControls();
    },
    /** Rebuild of every watched file: a few writes in a row, like a linker. */
    touchWatchedFiles: (changeContent: boolean) => {
      for (const id of this.fileWatchers.keys()) {
        const f = this.files.get(id);
        if (!f) continue;
        if (changeContent) {
          f.generation += 1;
          f.data = synthFirmware(f.meta.name, f.meta.size, f.generation);
        }
        [0, 150, 320].forEach((delay, i, all) =>
          setTimeout(() => {
            const last = i === all.length - 1;
            f.meta = {
              ...f.meta,
              modifiedAt: Date.now(),
              // A half-written file has a different CRC until the last write.
              crc32: last ? crc32(f.data) : (f.meta.crc32 ^ (i + 1)) >>> 0,
            };
            this.fileWatchers.get(id)?.forEach((cb) => cb(f.meta));
          }, delay),
        );
      }
    },
  };

  private makeSnapshot(): MockControlsState {
    return {
      connected: this.connected,
      opened: this.opened,
      nextFault: this.nextFault,
      speed: this.speed,
      watchedFiles: this.fileWatchers.size,
    };
  }

  private emitControls() {
    this.controlsSnapshot = this.makeSnapshot();
    this.controlListeners.forEach((cb) => cb());
  }

  private setConnected(on: boolean) {
    if (this.startupTimer) {
      clearTimeout(this.startupTimer);
      this.startupTimer = null;
    }
    if (this.connected === on) return;
    this.connected = on;
    if (!on) this.opened = false;
    const d = MOCK_PROBE.descriptor;
    if (on) this.log("INFO", `probe found: ST-Link V2 0483:${d.pid.toString(16)}, serial ${d.serial}`, "stlink_rust_ui_lib::hotplug");
    else this.log("INFO", `probe ${MOCK_PROBE.id} disconnected`, "stlink_rust_ui_lib::hotplug");
    const ev: DeviceEvent = on
      ? { type: "connected", probe: MOCK_SUMMARY }
      : { type: "disconnected", probeId: MOCK_PROBE.id };
    this.deviceListeners.forEach((cb) => cb(ev));
    this.emitControls();
  }

  /** Take the injected fault if it applies to this operation. */
  private takeFault(applies: readonly MockFault[]): MockFault | null {
    const f = this.nextFault;
    if (f === null || !applies.includes(f)) return null;
    this.nextFault = null;
    this.emitControls();
    return f;
  }

  private faultError(f: MockFault): BackendFailure {
    const kind: BackendErrorKind = f === "EraseFault" ? "Other" : f;
    this.log(f === "Disconnected" ? "ERROR" : "WARN", MOCK_MESSAGES[f]);
    return fail(kind, MOCK_MESSAGES[f]);
  }

  private ensurePresent(probeId: ProbeId) {
    if (!this.connected || probeId !== MOCK_PROBE.id) throw fail("Disconnected", MOCK_MESSAGES.Disconnected);
  }

  private ensureOpen(probeId: ProbeId) {
    this.ensurePresent(probeId);
    if (!this.opened) throw fail("Other", "target not connected");
  }

  /** Sleep that honours the speed multiplier, cancellation and unplugging. */
  private async sleep(ms: number, signal?: AbortSignal): Promise<void> {
    await new Promise<void>((resolve, reject) => {
      if (signal?.aborted) return reject(fail("Cancelled", "cancelled"));
      const t = setTimeout(() => {
        signal?.removeEventListener("abort", onAbort);
        resolve();
      }, ms / this.speed);
      const onAbort = () => {
        clearTimeout(t);
        reject(fail("Cancelled", "cancelled"));
      };
      signal?.addEventListener("abort", onAbort, { once: true });
    });
    if (!this.connected) throw fail("Disconnected", MOCK_MESSAGES.Disconnected);
  }

  /** USB packets in the journal, like `stlink=trace`. */
  private trace(out: number[], name: string, reply: number[], what = "OK") {
    if (!this.settings.verboseUsb) return;
    this.log("TRACE", `→ ${hex(out)} (${name})`, "stlink::usb::transport");
    this.log("TRACE", `← ${hex(reply)} (${name}, ${what})`, "stlink::usb::transport");
  }

  // ---- Backend: device ----

  watchDevices(cb: (e: DeviceEvent) => void): Unsubscribe {
    this.deviceListeners.add(cb);
    if (this.connected) cb({ type: "connected", probe: MOCK_SUMMARY });
    else if (!this.startedOnce) {
      // The probe "appears" a moment after the first subscriber.
      this.startedOnce = true;
      this.startupTimer = setTimeout(() => this.setConnected(true), STARTUP_CONNECT_MS);
    }
    return () => this.deviceListeners.delete(cb);
  }

  async connect(probeId: ProbeId, opts: ConnectOptions): Promise<DeviceSession> {
    this.ensurePresent(probeId);
    this.opened = false;
    await this.sleep(CONNECT_MS);
    const f = this.takeFault(["Busy", "CoreNotAccessible"]);
    if (f) throw this.faultError(f);
    this.opened = true;
    this.emitControls();
    this.trace([0xf1, 0x80], "GET_VERSION", [0x2e, 0x87, 0x83, 0x04, 0x48, 0x37], "ST-Link v2, JTAG v46, SWIM v7");
    this.log("INFO", `opened ST-Link V2 ${MOCK_PROBE.descriptor.serial} version=V2J46S7`, "stlink::probe");
    this.log("INFO", `SWD clock ${opts.swdKhz} kHz`, "stlink::probe");
    this.log("INFO", `chip chip_id=0x450 dev_type="STM32H74x_H75x"`, "stlink::target");
    return { probe: MOCK_PROBE, connection: mockConnection(opts), target: MOCK_TARGET };
  }

  async disconnect(probeId: ProbeId): Promise<void> {
    this.ensurePresent(probeId);
    this.opened = false;
    this.log("INFO", `probe ${probeId} released`, "stlink_rust_ui_lib::device");
    this.emitControls();
  }

  async planWrite(probeId: ProbeId, addr: number, len: number): Promise<FlashPlan> {
    this.ensureOpen(probeId);
    const r = planWrite(H743_GEOMETRY, addr, len);
    if (!r.ok) {
      const msg: Record<typeof r.error, string> = {
        empty: "invalid argument: nothing to write",
        invalidGeometry: "invalid argument: invalid flash geometry",
        outside: "invalid argument: address outside flash",
        unaligned: "invalid argument: address not aligned to the flash write granularity",
        doesNotFit: "invalid argument: data does not fit into flash",
      };
      throw fail("Other", msg[r.error]);
    }
    return r.plan;
  }

  async write(probeId: ProbeId, job: WriteJob, o: OpOptions): Promise<WriteResult> {
    this.ensureOpen(probeId);
    const file = this.files.get(job.fileId);
    if (!file) throw fail("Other", "file not found");
    const data = file.data;
    const plan = await this.planWrite(probeId, job.addr, data.length);
    const fault = this.takeFault(["Disconnected", "CoreNotAccessible", "FlashProtected", "Busy"]);
    await this.sleep(120, o.signal);
    if (fault && fault !== "Disconnected") throw this.faultError(fault);

    const g = H743_GEOMETRY;
    const image = new Uint8Array(plan.paddedLen).fill(g.erasedValue);
    image.set(data);
    const off = plan.addr - g.base;
    const started = performance.now();
    const stageMs: Record<WriteStage, number> = { erasing: 0, programming: 0, verifying: 0 };
    this.log("INFO", `write 0x${plan.addr.toString(16).padStart(8, "0").toUpperCase()}, ${plan.sectors.length} sector(s)`, "stlink::flash");

    // Erase
    let t0 = performance.now();
    const n = plan.sectors.length;
    o.onProgress({ stage: "erasing", done: 0, total: n });
    for (let i = 0; i < n; i++) {
      await this.sleep(SPEED.eraseSector * 1000, o.signal);
      const s = plan.sectors[i];
      this.flash.fill(g.erasedValue, s.addr - g.base, s.addr - g.base + g.sectorSize);
      o.onProgress({ stage: "erasing", done: i + 1, total: n });
    }
    stageMs.erasing = performance.now() - t0;

    // Program
    t0 = performance.now();
    const total = image.length;
    o.onProgress({ stage: "programming", done: 0, total });
    for (let done = 0; done < total; ) {
      const k = Math.min(BLOCK, total - done);
      await this.sleep((k / SPEED.program) * 1000, o.signal);
      this.flash.set(image.subarray(done, done + k), off + done);
      this.trace([0xf2, 0x08], "WRITEMEM_32BIT", [0x80], "OK");
      done += k;
      o.onProgress({ stage: "programming", done, total });
      if (fault === "Disconnected" && done >= total / 2) {
        this.setConnected(false);
        throw this.faultError("Disconnected");
      }
    }
    stageMs.programming = performance.now() - t0;

    // Verify
    if (job.verify) {
      t0 = performance.now();
      o.onProgress({ stage: "verifying", done: 0, total });
      for (let done = 0; done < total; ) {
        const k = Math.min(BLOCK, total - done);
        await this.sleep((k / SPEED.verify) * 1000, o.signal);
        done += k;
        o.onProgress({ stage: "verifying", done, total });
      }
      stageMs.verifying = performance.now() - t0;
    }

    if (job.resetAfter) await this.sleep(80);
    const crc = crc32(data);
    this.log("INFO", `programmed ${data.length} bytes, crc32 0x${crc.toString(16).toUpperCase().padStart(8, "0")}`, "stlink::flash");
    return {
      plan,
      crc32: crc,
      elapsedMs: performance.now() - started,
      stageMs,
      verified: job.verify,
      resetDone: job.resetAfter,
    };
  }

  async eraseAll(probeId: ProbeId, o: OpOptions): Promise<EraseResult> {
    this.ensureOpen(probeId);
    const fault = this.takeFault(["Disconnected", "FlashProtected", "Busy", "EraseFault"]);
    await this.sleep(80, o.signal);
    if (fault === "FlashProtected" || fault === "Busy") throw this.faultError(fault);

    const g = H743_GEOMETRY;
    const banks = Math.round(g.size / g.bankSize);
    const started = performance.now();
    const bankMs: number[] = [];
    this.log("INFO", `mass erase: ${banks} bank(s)`, "stlink::flash");
    o.onProgress({ stage: "erasing", done: 0, total: banks });
    for (let b = 0; b < banks; b++) {
      const t0 = performance.now();
      // A bank erase reports nothing until it ends; cancel only between banks.
      await this.sleep(estimateBankErase(g.bankSize) * 1000);
      if (b === 1 && fault === "Disconnected") {
        this.setConnected(false);
        throw this.faultError("Disconnected");
      }
      if (b === 1 && fault === "EraseFault") throw this.faultError("EraseFault");
      this.flash.fill(g.erasedValue, b * g.bankSize, (b + 1) * g.bankSize);
      bankMs.push(performance.now() - t0);
      this.log("DEBUG", `bank ${b + 1} erased`, "stlink::flash");
      o.onProgress({ stage: "erasing", done: b + 1, total: banks });
      if (o.signal.aborted && b + 1 < banks) throw fail("Cancelled", "cancelled");
    }
    const elapsedMs = performance.now() - started;
    this.log("INFO", `flash erased: ${banks} bank(s), ${g.size} bytes in ${Math.round(elapsedMs)} ms`, "stlink_rust_ui_lib::device");
    return { banks, size: g.size, elapsedMs, bankMs };
  }

  async read(probeId: ProbeId, job: ReadJob, o: OpOptions): Promise<ReadResult> {
    this.ensureOpen(probeId);
    const fault = this.takeFault(["Disconnected", "CoreNotAccessible", "FlashProtected", "Busy"]);
    await this.sleep(80, o.signal);
    if (fault && fault !== "Disconnected") throw this.faultError(fault);

    const g = H743_GEOMETRY;
    const out = new Uint8Array(job.size);
    const started = performance.now();
    o.onProgress({ stage: "reading", done: 0, total: job.size });
    for (let done = 0; done < job.size; ) {
      const k = Math.min(BLOCK, job.size - done);
      await this.sleep((k / SPEED.read) * 1000, o.signal);
      for (let i = 0; i < k; i++) {
        const a = job.addr + done + i - g.base;
        out[done + i] = a >= 0 && a < g.size ? this.flash[a] : 0;
      }
      this.trace([0xf2, 0x07], "READMEM_32BIT", [0x80], "OK");
      done += k;
      o.onProgress({ stage: "reading", done, total: job.size });
      if (fault === "Disconnected" && done >= job.size / 2) {
        this.setConnected(false);
        throw this.faultError("Disconnected");
      }
    }
    this.download(job.target.name, out);
    this.log("INFO", `read ${job.size} bytes → ${job.target.name}`, "stlink_rust_ui_lib::device");
    return {
      addr: job.addr,
      size: job.size,
      crc32: crc32(out),
      elapsedMs: performance.now() - started,
      savedTo: job.target,
    };
  }

  async resetAndRun(probeId: ProbeId): Promise<void> {
    this.ensureOpen(probeId);
    await this.sleep(250);
    const f = this.takeFault(["Busy", "CoreNotAccessible"]);
    if (f) throw this.faultError(f);
    this.log("INFO", "reset and run", "stlink::arm::reset");
  }

  private download(name: string, data: Uint8Array | string) {
    if (tauriWebview()) {
      // WKWebView has no download handling; the real backend writes the file.
      console.info(`[mock] ${name} not saved inside the Tauri webview`);
      return;
    }
    const blob = new Blob([data as BlobPart], { type: typeof data === "string" ? "text/plain" : "application/octet-stream" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  // ---- files ----

  private register(name: string, data: Uint8Array, modifiedAt: number, id?: string): BinFile {
    const meta: BinFile = {
      id: id ?? `mem:${++this.fileSeq}:${name}`,
      name,
      path: null,
      size: data.length,
      modifiedAt,
      crc32: crc32(data),
    };
    this.files.set(meta.id, { meta, data, generation: 0 });
    return meta;
  }

  private async fromFile(f: File): Promise<BinFile> {
    const data = new Uint8Array(await f.arrayBuffer());
    return this.register(f.name, data, f.lastModified || Date.now());
  }

  pickBinFile(): Promise<BinFile | null> {
    return new Promise((resolve, reject) => {
      const input = document.createElement("input");
      input.type = "file";
      input.accept = ".bin,application/octet-stream";
      input.addEventListener("change", () => {
        const f = input.files?.[0];
        if (!f) return resolve(null);
        this.fromFile(f).then(resolve, (e: unknown) =>
          reject(fail("Other", e instanceof Error ? e.message : "cannot read file")),
        );
      });
      input.addEventListener("cancel", () => resolve(null));
      input.click();
    });
  }

  acceptDroppedFile(file: File): Promise<BinFile> {
    return this.fromFile(file);
  }

  watchFileDrops(): Unsubscribe {
    // The browser delivers drops as DOM events (`acceptDroppedFile`).
    return () => undefined;
  }

  async openFile(path: string): Promise<BinFile> {
    throw fail("Unsupported", `${path}: no file system access in the browser`);
  }

  defaultSaveTarget(defaultName: string): SaveTarget {
    return { name: defaultName, dir: "~/Downloads", path: null };
  }

  async pickSavePath(defaultName: string): Promise<SaveTarget | null> {
    // No save dialog in the browser; the download goes to the browser's folder.
    const name = window.prompt("File name", defaultName)?.trim();
    if (!name) return null;
    return this.defaultSaveTarget(name.endsWith(".bin") ? name : `${name}.bin`);
  }

  async revealInFinder(target: SaveTarget): Promise<void> {
    console.info(`[mock] reveal ${target.dir}/${target.name}`);
  }

  watchFile(fileId: string, cb: (f: BinFile) => void): Unsubscribe {
    let set = this.fileWatchers.get(fileId);
    if (!set) {
      set = new Set();
      this.fileWatchers.set(fileId, set);
    }
    set.add(cb);
    this.emitControls();
    return () => {
      const s = this.fileWatchers.get(fileId);
      s?.delete(cb);
      if (s && s.size === 0) this.fileWatchers.delete(fileId);
      this.emitControls();
    };
  }

  // ---- quick write settings ----

  private seedQuickWrite(demo: Uint8Array) {
    const stored = this.loadQuickWrite(MOCK_UID);
    if (stored) return;
    const file = this.register(DEMO_FILE.name, demo, Date.now() - 2 * 60_000, "demo:blinky-h743.bin");
    this.saveQuickWrite(MOCK_UID, {
      ...DEFAULT_QUICK,
      file,
      chipName: "STM32H743",
      updatedAt: Date.now(),
    });
  }

  loadQuickWrite(uid: string): QuickWriteSettings | null {
    const raw = storage()?.getItem(STORAGE_PREFIX + uid);
    if (!raw) return null;
    try {
      const s = migrateQuick(JSON.parse(raw) as Partial<QuickWriteSettings> & Record<string, unknown>);
      if (s.file && !this.files.has(s.file.id)) {
        // After a reload the browser File is gone: keep a synthesized stand-in
        // with the same name and size.
        s.file = this.register(s.file.name, synthFirmware(s.file.name, s.file.size), s.file.modifiedAt, s.file.id);
      } else if (s.file) {
        s.file = this.files.get(s.file.id)?.meta ?? s.file;
      }
      return s;
    } catch {
      return null;
    }
  }

  saveQuickWrite(uid: string, s: QuickWriteSettings): void {
    try {
      storage()?.setItem(STORAGE_PREFIX + uid, JSON.stringify(s));
    } catch {
      // Storage unavailable (private mode): settings live for this session only.
    }
  }

  listQuickWrite(): { uid: string; settings: QuickWriteSettings }[] {
    const out: { uid: string; settings: QuickWriteSettings }[] = [];
    const st = storage();
    if (!st) return out;
    for (let i = 0; i < st.length; i++) {
      const key = st.key(i);
      if (!key?.startsWith(STORAGE_PREFIX)) continue;
      const uid = key.slice(STORAGE_PREFIX.length);
      const settings = this.loadQuickWrite(uid);
      if (settings) out.push({ uid, settings });
    }
    return out.sort((a, b) => b.settings.updatedAt - a.settings.updatedAt);
  }

  // ---- app settings (localStorage) ----

  private readSettings(): AppSettings {
    try {
      const raw = storage()?.getItem(SETTINGS_KEY);
      return normalizeSettings(raw ? JSON.parse(raw) : null);
    } catch {
      return normalizeSettings(null);
    }
  }

  loadSettings(): AppSettings {
    return this.settings;
  }

  saveSettings(s: AppSettings): void {
    if (s.verboseUsb !== this.settings.verboseUsb) {
      this.log("INFO", `verbose USB log ${s.verboseUsb ? "on" : "off"}`, "stlink_rust_ui_lib::logging");
    }
    this.settings = s;
    try {
      storage()?.setItem(SETTINGS_KEY, JSON.stringify(s));
    } catch {
      // Session only.
    }
  }

  // ---- journal ----

  private push(level: LogLevel, message: string, ts = Date.now(), target = "stlink"): LogEntry {
    const e: LogEntry = { seq: ++this.logSeq, ts, level, target, message };
    this.journal.push(e);
    if (this.journal.length > LOG_CAPACITY) this.journal.shift();
    return e;
  }

  private log(level: LogLevel, message: string, target = "stlink") {
    const e = this.push(level, message, Date.now(), target);
    this.logListeners.forEach((cb) => cb([e]));
  }

  async logEntries(): Promise<LogEntry[]> {
    return [...this.journal];
  }

  watchLog(cb: (entries: LogEntry[]) => void): Unsubscribe {
    this.logListeners.add(cb);
    return () => this.logListeners.delete(cb);
  }

  async clearLog(): Promise<void> {
    this.journal = [];
  }

  async saveLog(text: string, defaultName: string): Promise<string | null> {
    this.download(defaultName, text);
    return defaultName;
  }
}
