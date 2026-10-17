// `Backend` over the Tauri commands and events of `src-tauri` (stlink library).

import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { Backend, DeviceEvent, OpOptions, Unsubscribe } from "./Backend";
import {
  BackendFailure,
  normalizeSettings,
  type AppSettings,
  type BackendError,
  type BinFile,
  type ConnectOptions,
  type DeviceSession,
  type EraseResult,
  type FlashPlan,
  type LogEntry,
  type ProbeId,
  type ProbeSummary,
  type Progress,
  type QuickWriteSettings,
  type ReadJob,
  type ReadResult,
  type SaveTarget,
  type WriteJob,
  type WriteResult,
} from "./types";
import { migrateQuick } from "../domain/quickWrite";

/** What `init` returns (`src-tauri/src/lib.rs`). */
interface InitData {
  downloadsDir: string;
  downloadsDisplay: string;
  /** Native path separator (`\\` on Windows). */
  pathSep: string;
  quickWrite: { uid: string; settings: Partial<QuickWriteSettings> & Record<string, unknown> }[];
  settings: unknown;
  logPath: string | null;
}

function isBackendError(e: unknown): e is BackendError {
  return typeof e === "object" && e !== null && "kind" in e && "message" in e;
}

/** `invoke` that rejects with `BackendFailure` only. */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (isBackendError(e)) throw new BackendFailure(e);
    throw new BackendFailure({ kind: "Other", message: e instanceof Error ? e.message : String(e) });
  }
}

/** Run `subscribe` (async) now; the returned function unsubscribes even if it has not resolved yet. */
function lazyUnsub(subscribe: () => Promise<() => void>): Unsubscribe {
  let unsub: (() => void) | null = null;
  let closed = false;
  subscribe().then(
    (u) => {
      if (closed) u();
      else unsub = u;
    },
    (e: unknown) => console.error("subscription failed", e),
  );
  return () => {
    closed = true;
    unsub?.();
  };
}

function joinPath(dir: string, name: string, sep: string): string {
  return dir.endsWith(sep) ? dir + name : `${dir}${sep}${name}`;
}

export class TauriBackend implements Backend {
  readonly kind = "tauri" as const;

  private quick = new Map<string, QuickWriteSettings>();
  private settings: AppSettings;
  private opSeq = 0;

  private constructor(private readonly initData: InitData) {
    for (const { uid, settings } of initData.quickWrite) {
      this.quick.set(uid, migrateQuick(settings));
    }
    this.settings = normalizeSettings(initData.settings);
  }

  /** Loads what the synchronous parts of `Backend` need (settings, Downloads). */
  static async create(): Promise<TauriBackend> {
    const data = await call<InitData>("init");
    if (data.logPath) console.info(`log: ${data.logPath}`);
    return new TauriBackend(data);
  }

  watchDevices(cb: (e: DeviceEvent) => void): Unsubscribe {
    // `connected` once per probe: the event and the initial list may overlap.
    const known = new Set<ProbeId>();
    const deliver = (e: DeviceEvent) => {
      if (e.type === "connected") {
        if (known.has(e.probe.id)) return;
        known.add(e.probe.id);
      } else {
        if (!known.delete(e.probeId)) return;
      }
      cb(e);
    };
    return lazyUnsub(async () => {
      const unlisten = await listen<DeviceEvent>("device-event", (ev) => deliver(ev.payload));
      const present = await call<ProbeSummary[]>("list_devices");
      present.forEach((probe) => deliver({ type: "connected", probe }));
      return unlisten;
    });
  }

  connect(probeId: ProbeId, opts: ConnectOptions): Promise<DeviceSession> {
    return call("connect", { probeId, opts });
  }

  disconnect(probeId: ProbeId): Promise<void> {
    return call("disconnect", { probeId });
  }

  planWrite(probeId: ProbeId, addr: number, len: number): Promise<FlashPlan> {
    return call("plan_write", { probeId, addr, len });
  }

  write(probeId: ProbeId, job: WriteJob, o: OpOptions): Promise<WriteResult> {
    return this.op("write_flash", probeId, job, o);
  }

  read(probeId: ProbeId, job: ReadJob, o: OpOptions): Promise<ReadResult> {
    return this.op("read_flash", probeId, job, o);
  }

  eraseAll(probeId: ProbeId, o: OpOptions): Promise<EraseResult> {
    return this.op("erase_all", probeId, null, o);
  }

  /** Long operation: progress over a channel, abort → `cancel_op`. */
  private async op<T>(cmd: string, probeId: ProbeId, job: WriteJob | ReadJob | null, o: OpOptions): Promise<T> {
    if (o.signal.aborted) throw new BackendFailure({ kind: "Cancelled", message: "cancelled" });
    const opId = `${Date.now()}-${++this.opSeq}`;
    const onProgress = new Channel<Progress>((p) => o.onProgress(p));
    const onAbort = () => void call("cancel_op", { opId }).catch(() => undefined);
    o.signal.addEventListener("abort", onAbort, { once: true });
    try {
      const args = job ? { probeId, job, opId, onProgress } : { probeId, opId, onProgress };
      return await call<T>(cmd, args);
    } finally {
      o.signal.removeEventListener("abort", onAbort);
    }
  }

  resetAndRun(probeId: ProbeId): Promise<void> {
    return call("reset_and_run", { probeId });
  }

  // ---- files ----

  pickBinFile(): Promise<BinFile | null> {
    return call("pick_bin_file");
  }

  acceptDroppedFile(file: File): Promise<BinFile> {
    // Tauri delivers file drops as paths (`watchFileDrops`); a browser `File` has none.
    return Promise.reject(new BackendFailure({ kind: "Unsupported", message: `${file.name}: no path for a browser File` }));
  }

  watchFileDrops(cb: (paths: string[]) => void): Unsubscribe {
    return lazyUnsub(() =>
      getCurrentWebview().onDragDropEvent((ev) => {
        if (ev.payload.type === "drop" && ev.payload.paths.length > 0) cb(ev.payload.paths);
      }),
    );
  }

  openFile(path: string): Promise<BinFile> {
    return call("open_bin_file", { path });
  }

  pickSavePath(defaultName: string): Promise<SaveTarget | null> {
    return call("pick_save_path", { defaultName });
  }

  defaultSaveTarget(defaultName: string): SaveTarget {
    return {
      name: defaultName,
      dir: this.initData.downloadsDisplay,
      path: joinPath(this.initData.downloadsDir, defaultName, this.initData.pathSep),
    };
  }

  openDriverPage(url: string): Promise<void> {
    return call("open_driver_page", { url });
  }

  revealInFinder(target: SaveTarget): Promise<void> {
    if (!target.path) return Promise.resolve();
    return call("reveal_in_finder", { path: target.path });
  }

  watchFile(fileId: string, cb: (f: BinFile) => void): Unsubscribe {
    // The id of a file is its absolute path.
    return lazyUnsub(async () => {
      const onChange = new Channel<BinFile>(cb);
      const id = await call<number>("watch_file", { path: fileId, onChange });
      return () => void call("unwatch_file", { id }).catch(() => undefined);
    });
  }

  // ---- quick write settings (cached; persisted in the app data dir) ----

  loadQuickWrite(uid: string): QuickWriteSettings | null {
    return this.quick.get(uid) ?? null;
  }

  saveQuickWrite(uid: string, s: QuickWriteSettings): void {
    this.quick.set(uid, s);
    call("save_quick_write", { uid, settings: s }).catch((e: unknown) =>
      console.error("cannot save quick write settings", e),
    );
  }

  listQuickWrite(): { uid: string; settings: QuickWriteSettings }[] {
    return [...this.quick.entries()]
      .map(([uid, settings]) => ({ uid, settings }))
      .sort((a, b) => b.settings.updatedAt - a.settings.updatedAt);
  }

  // ---- app settings (settings.json in the app config dir) ----

  loadSettings(): AppSettings {
    return this.settings;
  }

  saveSettings(s: AppSettings): void {
    this.settings = s;
    call("save_settings", { settings: s }).catch((e: unknown) => console.error("cannot save settings", e));
  }

  // ---- journal (ring buffer in src-tauri) ----

  logEntries(): Promise<LogEntry[]> {
    return call("log_entries");
  }

  watchLog(cb: (entries: LogEntry[]) => void): Unsubscribe {
    return lazyUnsub(() => listen<LogEntry[]>("log://entry", (ev) => cb(ev.payload)));
  }

  clearLog(): Promise<void> {
    return call("log_clear");
  }

  saveLog(text: string, defaultName: string): Promise<string | null> {
    return call("log_save", { text, defaultName });
  }
}
