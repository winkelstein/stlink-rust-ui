// Side effects of the state machine: backend calls, hot-plug, file watching,
// persistence of settings, cancellation.

import type { Backend, Unsubscribe } from "../backend/Backend";
import { toBackendError, type BinFile, type ConnectOptions, type ProbeId } from "../backend/types";
import { chipShortName } from "../domain/chipNames";
import { parseNumber, checkReadRange } from "../domain/validate";
import { readableFlash } from "../domain/geometry";
import { DEFAULT_QUICK, FILE_CHANGE_DEBOUNCE_MS, needsConfirm, onFileChanged } from "../domain/quickWrite";
import {
  connectOptions,
  isBusy,
  type AppState,
  type BannerAction,
  type PendingWrite,
  type ReadRange,
  type WriteOrigin,
} from "./machine";
import { defaultReadName, rangeFields } from "./readRange";
import type { Store } from "./store";

export class Controller {
  private opSeq = 0;
  private abort: AbortController | null = null;
  private unsubs: Unsubscribe[] = [];
  private fileWatch: { key: string; unsub: Unsubscribe } | null = null;
  private debounce: ReturnType<typeof setTimeout> | null = null;
  private latestChange: BinFile | null = null;
  private prev: AppState;

  constructor(
    readonly backend: Backend,
    readonly store: Store,
  ) {
    this.prev = store.getState();
  }

  private get s(): AppState {
    return this.store.getState();
  }

  private probeId(): ProbeId | null {
    return this.s.probe?.id ?? null;
  }

  start(): () => void {
    this.unsubs.push(
      // A found probe is only shown (`released`): the app never connects on its own.
      this.backend.watchDevices((e) => {
        if (e.type === "connected") {
          this.store.dispatch({ type: "probeConnected", probe: e.probe });
        } else {
          this.abort?.abort();
          this.store.dispatch({ type: "probeDisconnected", probeId: e.probeId });
        }
      }),
      this.store.subscribe(() => this.onState()),
    );
    return () => {
      this.unsubs.forEach((u) => u());
      this.unsubs = [];
      this.fileWatch?.unsub();
      this.fileWatch = null;
      if (this.debounce) clearTimeout(this.debounce);
    };
  }

  /** Persist settings and keep the file watcher in sync with the state. */
  private onState() {
    const s = this.s;
    const prev = this.prev;
    this.prev = s;
    const uid = s.session?.target.uid;
    if (uid && s.quick && s.quick !== prev.quick) this.backend.saveQuickWrite(uid, s.quick);
    if (s.settings !== prev.settings) this.backend.saveSettings(s.settings);

    const want = s.session && s.quick?.onFileChange && s.quick.file ? s.quick.file.id : null;
    if ((this.fileWatch?.key ?? null) !== want) {
      this.fileWatch?.unsub();
      this.fileWatch = null;
      if (this.debounce) clearTimeout(this.debounce);
      if (want) {
        this.fileWatch = { key: want, unsub: this.backend.watchFile(want, (f) => this.onFileChange(f)) };
      }
    }
  }

  private onFileChange(f: BinFile) {
    this.store.dispatch({ type: "fileUpdated", file: f });
    this.latestChange = f;
    if (this.debounce) clearTimeout(this.debounce);
    this.debounce = setTimeout(() => {
      this.debounce = null;
      const file = this.latestChange;
      if (!file) return;
      const s = this.s;
      const busy = isBusy(s) || !["device", "write", "writeDone"].includes(s.screen);
      if (onFileChanged(s.quick, file, busy) === "write") void this.quickWrite("onFileChange");
    }, FILE_CHANGE_DEBOUNCE_MS);
  }

  /** «Подключиться»: probe options from the settings, unless overridden (error banner). */
  async connect(override?: Partial<ConnectOptions>): Promise<void> {
    const id = this.probeId();
    if (!id || isBusy(this.s)) return;
    const opts = { ...connectOptions(this.s.settings), ...override };
    this.store.dispatch({ type: "connectStarted" });
    try {
      const session = await this.backend.connect(id, opts);
      if (this.probeId() !== id) return;
      const uid = session.target.uid;
      const quick = uid
        ? (this.backend.loadQuickWrite(uid) ?? {
            ...DEFAULT_QUICK,
            chipName: chipShortName(session.target),
            updatedAt: Date.now(),
          })
        : null;
      this.store.dispatch({ type: "connected", session, quick });
    } catch (e) {
      if (this.probeId() !== id) return;
      this.store.dispatch({ type: "connectFailed", error: toBackendError(e) });
    }
  }

  /** «Отключить»: close the probe so other programs can use it. */
  async disconnect(): Promise<void> {
    const id = this.probeId();
    if (!id || isBusy(this.s) || !this.s.session) return;
    try {
      await this.backend.disconnect(id);
    } catch (e) {
      // The worker is gone (unplugged): hot-plug shows `empty` anyway.
      console.warn("disconnect failed", toBackendError(e));
    }
    this.store.dispatch({ type: "released" });
  }

  // ---- write ----

  /** «Стереть и записать» on the write form: plan → confirmation sheet. */
  async requestWrite(): Promise<void> {
    const s = this.s;
    const id = this.probeId();
    const file = s.writeForm.file;
    const addr = parseNumber(s.writeForm.addrText);
    if (!id || !file || addr === null || isBusy(s) || s.screen !== "write") return;
    try {
      const plan = await this.backend.planWrite(id, addr, file.size);
      const { verify, resetAfter } = this.s.settings;
      this.store.dispatch({
        type: "confirmRequested",
        returnTo: "write",
        pending: { file, plan, origin: "wizard", job: { fileId: file.id, addr, verify, resetAfter } },
      });
    } catch (e) {
      this.store.dispatch({ type: "operationRejected", error: toBackendError(e), context: "write" });
    }
  }

  /** ⌘R / «Записать» in the quick write card / auto triggers. */
  async quickWrite(origin: WriteOrigin): Promise<void> {
    const s = this.s;
    const id = this.probeId();
    const q = s.quick;
    if (!id || !q?.file || isBusy(s) || !s.session) return;
    if (!["device", "writeDone", "write"].includes(s.screen)) return;
    const file = q.file;
    try {
      const plan = await this.backend.planWrite(id, q.addr, file.size);
      const pending: PendingWrite = {
        file,
        plan,
        origin,
        job: { fileId: file.id, addr: q.addr, verify: s.settings.verify, resetAfter: s.settings.resetAfter },
      };
      if (needsConfirm(q, plan, s.settings.alwaysConfirm)) {
        this.store.dispatch({ type: "confirmRequested", pending, returnTo: "device" });
      } else {
        this.runWrite(pending);
      }
    } catch (e) {
      this.store.dispatch({ type: "operationRejected", error: toBackendError(e), context: "write" });
    }
  }

  confirmWrite() {
    const c = this.s.confirm;
    if (!c || this.s.screen !== "confirm") return;
    this.store.dispatch({ type: "confirmAccepted" });
    this.runWrite(c.pending);
  }

  cancelConfirm() {
    this.store.dispatch({ type: "confirmCancelled" });
  }

  // ---- erase all ----

  requestErase() {
    this.store.dispatch({ type: "navigate", to: "eraseConfirm" });
  }

  confirmErase() {
    const probe = this.probeId();
    if (!probe || this.s.screen !== "eraseConfirm" || isBusy(this.s)) return;
    const id = ++this.opSeq;
    const abort = new AbortController();
    this.abort = abort;
    this.store.dispatch({ type: "eraseStarted", id, at: performance.now() });
    this.backend
      .eraseAll(probe, {
        signal: abort.signal,
        onProgress: (progress) => this.store.dispatch({ type: "opProgress", id, progress, at: performance.now() }),
      })
      .then(
        (result) => this.store.dispatch({ type: "eraseFinished", id, result }),
        (e: unknown) => this.store.dispatch({ type: "opFailed", id, error: toBackendError(e) }),
      )
      .finally(() => {
        if (this.abort === abort) this.abort = null;
      });
  }

  private runWrite(pending: PendingWrite) {
    const probe = this.probeId();
    if (!probe) return;
    const id = ++this.opSeq;
    const abort = new AbortController();
    this.abort = abort;
    this.store.dispatch({ type: "writeStarted", id, pending, at: performance.now() });
    this.backend
      .write(probe, pending.job, {
        signal: abort.signal,
        onProgress: (progress) => this.store.dispatch({ type: "opProgress", id, progress, at: performance.now() }),
      })
      .then(
        (result) => this.store.dispatch({ type: "writeFinished", id, result }),
        (e: unknown) => this.store.dispatch({ type: "opFailed", id, error: toBackendError(e) }),
      )
      .finally(() => {
        if (this.abort === abort) this.abort = null;
      });
  }

  retryWrite() {
    const last = this.s.lastWrite;
    if (!last || isBusy(this.s) || !this.s.session) return;
    // The plan may differ after reconnecting: always re-plan and confirm.
    this.store.dispatch({ type: "navigate", to: "write" });
    void this.requestWrite();
  }

  cancelOp() {
    this.abort?.abort();
  }

  // ---- read ----

  setReadRange(range: ReadRange) {
    const g = this.s.session?.target.geometry;
    if (!g) return;
    this.store.dispatch({ type: "readRangeChanged", range, ...rangeFields(range, g) });
  }

  readName(): string {
    const s = this.s;
    const t = s.session?.target;
    const addr = parseNumber(s.readForm.startText) ?? 0;
    return defaultReadName(s.readForm.range, t ? chipShortName(t) : "STM32", addr, Date.now());
  }

  async pickSaveTarget() {
    const t = await this.backend.pickSavePath(this.s.readForm.target?.name ?? this.readName());
    if (t) this.store.dispatch({ type: "saveTargetChosen", target: t });
  }

  startRead() {
    const s = this.s;
    const probe = this.probeId();
    if (!probe || isBusy(s) || s.screen !== "read") return;
    const r = checkReadRange(s.session ? readableFlash(s.session.target) : null, s.readForm.startText, s.readForm.sizeText);
    if (!r.ok) return;
    const target = s.readForm.target ?? this.backend.defaultSaveTarget(this.readName());
    const job = { addr: r.addr, size: r.size, target };
    const id = ++this.opSeq;
    const abort = new AbortController();
    this.abort = abort;
    this.store.dispatch({ type: "readStarted", id, job, at: performance.now() });
    this.backend
      .read(probe, job, {
        signal: abort.signal,
        onProgress: (progress) => this.store.dispatch({ type: "opProgress", id, progress, at: performance.now() }),
      })
      .then(
        (result) => this.store.dispatch({ type: "readFinished", id, result }),
        (e: unknown) => this.store.dispatch({ type: "opFailed", id, error: toBackendError(e) }),
      )
      .finally(() => {
        if (this.abort === abort) this.abort = null;
      });
  }

  // ---- misc ----

  async pickFile() {
    try {
      const f = await this.backend.pickBinFile();
      if (f) this.store.dispatch({ type: "fileChosen", file: f });
    } catch (e) {
      this.store.dispatch({ type: "operationRejected", error: toBackendError(e), context: "write" });
    }
  }

  /** A dropped `.bin`: a browser `File` or a path (native window). */
  async dropFile(source: File | string) {
    try {
      const f = typeof source === "string" ? await this.backend.openFile(source) : await this.backend.acceptDroppedFile(source);
      this.store.dispatch({ type: "fileChosen", file: f });
    } catch (e) {
      this.store.dispatch({ type: "operationRejected", error: toBackendError(e), context: "write" });
    }
  }

  async resetMcu() {
    const id = this.probeId();
    if (!id || isBusy(this.s)) return;
    this.store.dispatch({ type: "resetStarted" });
    try {
      await this.backend.resetAndRun(id);
      this.store.dispatch({ type: "resetFinished", error: null });
    } catch (e) {
      this.store.dispatch({ type: "resetFinished", error: toBackendError(e) });
    }
  }

  async reveal() {
    const r = this.s.readResult;
    if (r) await this.backend.revealInFinder(r.savedTo);
  }

  bannerAction(a: BannerAction) {
    switch (a) {
      case "retryWrite":
        return this.retryWrite();
      case "retryRead":
        this.store.dispatch({ type: "bannerDismissed" });
        this.store.dispatch({ type: "navigate", to: "read" });
        return this.startRead();
      case "retryConnect":
        return void this.connect();
      case "retryReset":
        return void this.resetMcu();
      case "retryErase":
        this.store.dispatch({ type: "bannerDismissed" });
        return this.requestErase();
      case "connectUnderReset":
        return void this.connect({ underReset: true });
      case "keepDebugLowPower":
        return void this.connect({ keepDebugInLowPower: true });
    }
  }
}
