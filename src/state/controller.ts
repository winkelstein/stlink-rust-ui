// Side effects of the state machine: backend calls, hot-plug, file watching,
// persistence of quick write settings, cancellation.

import type { Backend, Unsubscribe } from "../backend/Backend";
import { toBackendError, type BinFile, type ConnectOptions, type ProbeId } from "../backend/types";
import { chipShortName } from "../domain/chipNames";
import { parseNumber, checkReadRange } from "../domain/validate";
import {
  DEFAULT_QUICK,
  FILE_CHANGE_DEBOUNCE_MS,
  needsConfirm,
  onFileChanged,
  shouldWriteOnConnect,
} from "../domain/quickWrite";
import { isBusy, type AppState, type BannerAction, type PendingWrite, type ReadRange, type WriteOrigin } from "./machine";
import { defaultReadName, rangeFields } from "./readRange";
import type { Store } from "./store";

export class Controller {
  private opSeq = 0;
  private abort: AbortController | null = null;
  private unsubs: Unsubscribe[] = [];
  private fileWatch: { key: string; unsub: Unsubscribe } | null = null;
  private debounce: ReturnType<typeof setTimeout> | null = null;
  private latestChange: BinFile | null = null;
  /** Write-on-connect is checked once per plug-in, not on re-identify. */
  private justConnected = false;
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
    this.loadArmed();
    this.unsubs.push(
      this.backend.watchDevices((e) => {
        if (e.type === "connected") {
          this.store.dispatch({ type: "probeConnected", probe: e.probe });
          this.justConnected = true;
          void this.identify(this.s.connectOptions);
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

  private loadArmed() {
    const armed = this.backend.listQuickWrite().find((x) => x.settings.onConnect && x.settings.file);
    this.store.dispatch({ type: "armedLoaded", armed: armed ?? null });
  }

  /** Persist settings and keep the file watcher in sync with the state. */
  private onState() {
    const s = this.s;
    const prev = this.prev;
    this.prev = s;
    const uid = s.session?.target.uid;
    if (uid && s.quick && s.quick !== prev.quick) this.backend.saveQuickWrite(uid, s.quick);
    if (s.armed && prev.armed && s.armed !== prev.armed) this.backend.saveQuickWrite(s.armed.uid, s.armed.settings);

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

  async identify(opts: ConnectOptions): Promise<void> {
    const id = this.probeId();
    if (!id || this.s.identifying) return;
    this.store.dispatch({ type: "identifyStarted", opts });
    try {
      const session = await this.backend.identify(id, opts);
      if (this.probeId() !== id) return;
      const uid = session.target.uid;
      const quick = uid
        ? (this.backend.loadQuickWrite(uid) ?? {
            ...DEFAULT_QUICK,
            chipName: chipShortName(session.target),
            updatedAt: Date.now(),
          })
        : null;
      this.store.dispatch({ type: "identified", session, quick });
      const first = this.justConnected;
      this.justConnected = false;
      if (first && shouldWriteOnConnect(quick)) void this.quickWrite("onConnect");
    } catch (e) {
      this.store.dispatch({ type: "identifyFailed", error: toBackendError(e) });
    }
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
      const f = this.s.writeForm;
      this.store.dispatch({
        type: "confirmRequested",
        returnTo: "write",
        pending: {
          file,
          plan,
          origin: "wizard",
          job: { fileId: file.id, addr, verify: f.verify, resetAfter: f.resetAfter },
        },
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
        job: { fileId: file.id, addr: q.addr, verify: s.writeForm.verify, resetAfter: s.writeForm.resetAfter },
      };
      if (needsConfirm(q, plan)) {
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
    const r = checkReadRange(s.readForm.startText, s.readForm.sizeText);
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

  async dropFile(file: File) {
    try {
      const f = await this.backend.acceptDroppedFile(file);
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
    const opts = this.s.connectOptions;
    switch (a) {
      case "retryWrite":
        return this.retryWrite();
      case "retryRead":
        this.store.dispatch({ type: "bannerDismissed" });
        this.store.dispatch({ type: "navigate", to: "read" });
        return this.startRead();
      case "retryIdentify":
        return void this.identify(opts);
      case "retryReset":
        return void this.resetMcu();
      case "connectUnderReset":
        return void this.identify({ ...opts, underReset: true });
      case "keepDebugLowPower":
        return void this.identify({ ...opts, keepDebugInLowPower: true });
    }
  }
}
