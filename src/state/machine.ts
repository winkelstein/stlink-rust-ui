// App state machine: screens, forms, running operation, banners. Pure — all
// backend calls live in `controller.ts`, which dispatches the results here.
// No UI text: banners carry the error kind and context, screens translate.

import {
  DEFAULT_SETTINGS,
  type AppSettings,
  type BackendError,
  type BinFile,
  type ConnectOptions,
  type DeviceSession,
  type EraseResult,
  type FlashPlan,
  type ProbeSummary,
  type Progress,
  type QuickWriteSettings,
  type ReadJob,
  type ReadResult,
  type SaveTarget,
  type Stage,
  type WriteJob,
  type WriteResult,
} from "../backend/types";
import { fmtHex32 } from "../domain/format";
import { DEFAULT_QUICK } from "../domain/quickWrite";
import { rangeFields } from "./readRange";

export type Screen =
  | "empty"
  | "released"
  | "device"
  | "eraseConfirm"
  | "eraseProgress"
  | "eraseDone"
  | "write"
  | "confirm"
  | "writeProgress"
  | "writeDone"
  | "read"
  | "readProgress"
  | "readDone";

export type Overlay = "settings" | "log";

export type ReadRange = "all" | "b1" | "b2" | "custom";

/** What started a write. */
export type WriteOrigin = "wizard" | "quick" | "onFileChange";

export function connectOptions(s: AppSettings): ConnectOptions {
  return { swdKhz: s.swdKhz, underReset: s.underReset, keepDebugInLowPower: s.keepDebugInLowPower };
}

export type BannerAction =
  | "retryWrite"
  | "retryRead"
  | "retryConnect"
  | "retryReset"
  | "retryErase"
  | "connectUnderReset"
  | "keepDebugLowPower";

export type BannerContext = "connect" | "write" | "read" | "reset" | "erase";

// TODO(design): error banners are not in the mockups yet.
export interface Banner {
  /** `notice` — a cancelled operation. */
  kind: BackendError["kind"] | "notice";
  context: BannerContext;
  /** Backend `message` (shown under «Details»). */
  message: string;
  /** Already translated text instead of the default one for the kind. */
  note?: string;
  actions: BannerAction[];
}

export interface WriteForm {
  file: BinFile | null;
  addrText: string;
}

export interface ReadForm {
  range: ReadRange;
  startText: string;
  sizeText: string;
  /** User-picked target; `null` → default name for the range. */
  target: SaveTarget | null;
}

export interface PendingWrite {
  job: WriteJob;
  plan: FlashPlan;
  file: BinFile;
  origin: WriteOrigin;
}

interface OpBase {
  id: number;
  progress: Progress;
  startedAt: number;
  lastProgressAt: number;
}

export type Operation =
  | (OpBase & {
      kind: "write";
      pending: PendingWrite;
      /** When each stage got its first progress report. */
      stageStart: Partial<Record<Stage, number>>;
    })
  | (OpBase & { kind: "read"; job: ReadJob })
  | (OpBase & {
      kind: "erase";
      /** When each bank started (index = bank). */
      bankStart: number[];
    });

export interface AppState {
  screen: Screen;
  overlay: Overlay | null;
  /** The discovered probe (from USB, not necessarily opened). */
  probe: ProbeSummary | null;
  /** Set after «Connect»; `null` while disconnected. */
  session: DeviceSession | null;
  connecting: boolean;
  /**
   * Why the last connect could not open the probe, shown as a variant of
   * `released`: `busy` — another program has it; `driver` — Windows, not on
   * WinUSB (`message` names the driver).
   */
  blocked: { reason: "busy" | "driver"; message: string } | null;
  settings: AppSettings;
  resetting: boolean;
  writeForm: WriteForm;
  readForm: ReadForm;
  /** Write waiting for confirmation, and where «Cancel» returns. */
  confirm: { pending: PendingWrite; returnTo: "write" | "device" } | null;
  op: Operation | null;
  /** Last started write — for «Retry» / «Flash again». */
  lastWrite: PendingWrite | null;
  writeResult: { result: WriteResult; pending: PendingWrite } | null;
  readResult: ReadResult | null;
  eraseResult: EraseResult | null;
  /** Quick write settings of the connected chip (by UID). */
  quick: QuickWriteSettings | null;
  banner: Banner | null;
}

export const initialState: AppState = {
  screen: "empty",
  overlay: null,
  probe: null,
  session: null,
  connecting: false,
  blocked: null,
  settings: DEFAULT_SETTINGS,
  resetting: false,
  writeForm: { file: null, addrText: fmtHex32(0x0800_0000) },
  readForm: { range: "all", startText: fmtHex32(0x0800_0000), sizeText: "0x40000", target: null },
  confirm: null,
  op: null,
  lastWrite: null,
  writeResult: null,
  readResult: null,
  eraseResult: null,
  quick: null,
  banner: null,
};

export type Action =
  | { type: "probeConnected"; probe: ProbeSummary }
  | { type: "probeDisconnected"; probeId: string }
  | { type: "connectStarted" }
  | { type: "connected"; session: DeviceSession; quick: QuickWriteSettings | null }
  | { type: "connectFailed"; error: BackendError }
  | { type: "released" }
  | { type: "navigate"; to: "device" | "write" | "read" | "eraseConfirm" }
  | { type: "fileChosen"; file: BinFile }
  | { type: "fileUpdated"; file: BinFile }
  | { type: "addrChanged"; text: string }
  | { type: "settingsChanged"; patch: Partial<AppSettings> }
  | { type: "quickToggled"; option: "onFileChange" }
  | { type: "confirmRequested"; pending: PendingWrite; returnTo: "write" | "device" }
  | { type: "confirmCancelled" }
  | { type: "confirmAccepted" }
  | { type: "writeStarted"; id: number; pending: PendingWrite; at: number }
  | { type: "readStarted"; id: number; job: ReadJob; at: number }
  | { type: "eraseStarted"; id: number; at: number }
  | { type: "opProgress"; id: number; progress: Progress; at: number }
  | { type: "writeFinished"; id: number; result: WriteResult }
  | { type: "readFinished"; id: number; result: ReadResult }
  | { type: "eraseFinished"; id: number; result: EraseResult }
  | { type: "opFailed"; id: number; error: BackendError }
  | { type: "readRangeChanged"; range: ReadRange; startText: string; sizeText: string }
  | { type: "readFieldChanged"; field: "startText" | "sizeText"; text: string }
  | { type: "saveTargetChosen"; target: SaveTarget }
  | { type: "resetStarted" }
  | { type: "resetFinished"; error: BackendError | null }
  | { type: "operationRejected"; error: BackendError; context: "write" | "read"; note?: string }
  | { type: "bannerDismissed" }
  | { type: "overlayOpened"; overlay: Overlay }
  | { type: "overlayClosed" };

/** Step of the wizard shown in the titlebar. */
export function stepOf(screen: Screen): 1 | 2 | 3 {
  switch (screen) {
    case "empty":
    case "released":
    case "device":
    case "eraseConfirm":
      return 1;
    case "write":
    case "confirm":
    case "read":
      return 2;
    default:
      return 3;
  }
}

export function isBusy(s: AppState): boolean {
  return s.op !== null || s.connecting || s.resetting;
}

function errorBanner(e: BackendError, context: BannerContext): Banner {
  const retry: Record<BannerContext, BannerAction> = {
    connect: "retryConnect",
    write: "retryWrite",
    read: "retryRead",
    reset: "retryReset",
    erase: "retryErase",
  };
  const base = { kind: e.kind, context, message: e.message };
  switch (e.kind) {
    case "Disconnected":
      return { ...base, actions: context === "write" ? ["retryWrite"] : context === "read" ? ["retryRead"] : [] };
    case "CoreNotAccessible":
      return { ...base, actions: ["connectUnderReset", "keepDebugLowPower"] };
    case "FlashProtected":
    case "Unsupported":
    case "DriverMissing":
      return { ...base, actions: [] };
    case "Cancelled":
      return { ...base, kind: "notice", actions: [] };
    case "Busy":
    case "Other":
      return { ...base, actions: [retry[context]] };
  }
}

function updateQuick(s: AppState, f: (q: QuickWriteSettings) => QuickWriteSettings): QuickWriteSettings | null {
  if (!s.session?.target.uid) return s.quick;
  return { ...f(s.quick ?? DEFAULT_QUICK), updatedAt: Date.now() };
}

/** Screens where the device may be used (no op, no sheet). */
function idle(s: AppState): boolean {
  return !isBusy(s) && s.session !== null && s.screen !== "confirm" && s.screen !== "eraseConfirm";
}

/** State after the probe is closed (unplugged or «Disconnect»). */
function closed(s: AppState): AppState {
  return {
    ...s,
    session: null,
    connecting: false,
    resetting: false,
    confirm: null,
    op: null,
    quick: null,
  };
}

export function reducer(s: AppState, a: Action): AppState {
  switch (a.type) {
    case "probeConnected":
      if (s.probe) return s;
      return { ...closed(s), probe: a.probe, screen: "released", blocked: null, banner: keepOpBanner(s.banner) };

    case "probeDisconnected": {
      if (!s.probe || s.probe.id !== a.probeId) return s;
      let banner = s.banner;
      const lost: BackendError = { kind: "Disconnected", message: "device disconnected" };
      if (s.op) banner = errorBanner(lost, s.op.kind);
      else if (banner && banner.kind !== "Disconnected") banner = null;
      return { ...closed(s), screen: "empty", probe: null, blocked: null, banner };
    }

    case "connectStarted":
      if (!s.probe || isBusy(s)) return s;
      return { ...s, connecting: true, blocked: null, banner: keepOpBanner(s.banner) };

    case "connected": {
      if (!s.probe) return s;
      const quick = a.quick;
      const writeForm =
        quick?.file && !s.writeForm.file ? { ...s.writeForm, file: quick.file, addrText: fmtHex32(quick.addr) } : s.writeForm;
      const g = a.session.target.geometry;
      const readForm = g ? { ...s.readForm, ...rangeFields(s.readForm.range, g) } : s.readForm;
      return { ...s, screen: "device", connecting: false, session: a.session, quick, writeForm, readForm };
    }

    case "connectFailed":
      if (!s.probe) return s;
      if (a.error.kind === "Busy" || a.error.kind === "DriverMissing") {
        const reason = a.error.kind === "Busy" ? "busy" : "driver";
        return { ...closed(s), screen: "released", blocked: { reason, message: a.error.message }, banner: null };
      }
      return { ...closed(s), screen: "released", blocked: null, banner: errorBanner(a.error, "connect") };

    case "released":
      if (!s.probe) return s;
      return { ...closed(s), screen: "released", blocked: null, banner: null };

    case "navigate": {
      if (isBusy(s) || !s.session || s.screen === "confirm") return s;
      if (a.to === "eraseConfirm" && (s.screen !== "device" || !s.session.target.geometry)) return s;
      // A finished/cancelled operation's notice belongs to the screen it is shown on.
      const banner = a.to === "device" ? null : s.banner;
      return { ...s, screen: a.to, banner };
    }

    case "fileChosen":
      return {
        ...s,
        writeForm: { ...s.writeForm, file: a.file },
        screen: idle(s) && (s.screen === "device" || s.screen === "writeDone") ? "write" : s.screen,
      };

    case "fileUpdated": {
      const writeForm = s.writeForm.file?.id === a.file.id ? { ...s.writeForm, file: a.file } : s.writeForm;
      const quick = s.quick?.file?.id === a.file.id ? { ...s.quick, file: a.file } : s.quick;
      return { ...s, writeForm, quick };
    }

    case "addrChanged":
      return { ...s, writeForm: { ...s.writeForm, addrText: a.text } };

    case "settingsChanged":
      return { ...s, settings: { ...s.settings, ...a.patch } };

    case "quickToggled":
      if (!s.session?.target.uid) return s;
      return { ...s, quick: updateQuick(s, (q) => ({ ...q, [a.option]: !q[a.option] })) };

    case "confirmRequested":
      if (isBusy(s) || !s.session) return s;
      return {
        ...s,
        screen: "confirm",
        confirm: { pending: a.pending, returnTo: a.returnTo },
        writeForm: { ...s.writeForm, file: a.pending.file, addrText: fmtHex32(a.pending.job.addr) },
        banner: null,
      };

    case "confirmCancelled":
      if (s.screen === "eraseConfirm") return { ...s, screen: "device" };
      if (s.screen !== "confirm" || !s.confirm) return s;
      return { ...s, screen: s.confirm.returnTo, confirm: null };

    case "confirmAccepted":
      // The write itself starts with `writeStarted`; remember what was confirmed.
      if (s.screen !== "confirm" || !s.confirm) return s;
      return {
        ...s,
        quick: updateQuick(s, (q) => ({ ...q, lastConfirmedSectors: s.confirm?.pending.plan.sectors ?? null })),
      };

    case "writeStarted":
      if (s.op || !s.session) return s;
      return {
        ...s,
        screen: "writeProgress",
        confirm: null,
        banner: null,
        writeForm: { ...s.writeForm, file: a.pending.file, addrText: fmtHex32(a.pending.job.addr) },
        lastWrite: a.pending,
        op: {
          kind: "write",
          id: a.id,
          pending: a.pending,
          progress: { stage: "erasing", done: 0, total: a.pending.plan.sectors.length },
          startedAt: a.at,
          stageStart: {},
          lastProgressAt: a.at,
        },
      };

    case "readStarted":
      if (s.op || !s.session) return s;
      return {
        ...s,
        screen: "readProgress",
        banner: null,
        op: {
          kind: "read",
          id: a.id,
          job: a.job,
          progress: { stage: "reading", done: 0, total: a.job.size },
          startedAt: a.at,
          lastProgressAt: a.at,
        },
      };

    case "eraseStarted": {
      if (s.op || !s.session || s.screen !== "eraseConfirm") return s;
      const g = s.session.target.geometry;
      const banks = g ? Math.max(1, Math.round(g.size / Math.max(1, g.bankSize))) : 1;
      return {
        ...s,
        screen: "eraseProgress",
        banner: null,
        op: {
          kind: "erase",
          id: a.id,
          progress: { stage: "erasing", done: 0, total: banks },
          startedAt: a.at,
          bankStart: [a.at],
          lastProgressAt: a.at,
        },
      };
    }

    case "opProgress": {
      if (!s.op || s.op.id !== a.id) return s;
      switch (s.op.kind) {
        case "write": {
          const stageStart = s.op.stageStart[a.progress.stage]
            ? s.op.stageStart
            : { ...s.op.stageStart, [a.progress.stage]: a.at };
          return { ...s, op: { ...s.op, progress: a.progress, stageStart, lastProgressAt: a.at } };
        }
        case "erase": {
          const bankStart = [...s.op.bankStart];
          if (bankStart[a.progress.done] === undefined) bankStart[a.progress.done] = a.at;
          return { ...s, op: { ...s.op, progress: a.progress, bankStart, lastProgressAt: a.at } };
        }
        case "read":
          return { ...s, op: { ...s.op, progress: a.progress, lastProgressAt: a.at } };
      }
      return s;
    }

    case "writeFinished": {
      if (s.op?.kind !== "write" || s.op.id !== a.id) return s;
      const pending = s.op.pending;
      return {
        ...s,
        screen: "writeDone",
        op: null,
        writeResult: { result: a.result, pending },
        quick: updateQuick(s, (q) => ({
          ...q,
          file: pending.file,
          addr: pending.job.addr,
          lastWrittenCrc: a.result.crc32,
        })),
      };
    }

    case "readFinished":
      if (s.op?.kind !== "read" || s.op.id !== a.id) return s;
      return { ...s, screen: "readDone", op: null, readResult: a.result };

    case "eraseFinished":
      if (s.op?.kind !== "erase" || s.op.id !== a.id) return s;
      // The flash no longer holds what was written last.
      return {
        ...s,
        screen: "eraseDone",
        op: null,
        eraseResult: a.result,
        quick: updateQuick(s, (q) => ({ ...q, lastWrittenCrc: null })),
      };

    case "opFailed": {
      if (!s.op || s.op.id !== a.id) return s;
      const kind = s.op.kind;
      const screen = kind === "erase" ? "device" : kind;
      return { ...s, screen, op: null, banner: errorBanner(a.error, kind) };
    }

    case "operationRejected":
      // Failed before anything started (plan rejected, no file): stay on the form.
      return { ...s, banner: { ...errorBanner(a.error, a.context), note: a.note } };

    case "readRangeChanged":
      return {
        ...s,
        readForm: { ...s.readForm, range: a.range, startText: a.startText, sizeText: a.sizeText, target: null },
      };

    case "readFieldChanged":
      if (s.readForm.range !== "custom") return s;
      return { ...s, readForm: { ...s.readForm, [a.field]: a.text, target: null } };

    case "saveTargetChosen":
      return { ...s, readForm: { ...s.readForm, target: a.target } };

    case "resetStarted":
      if (isBusy(s) || !s.session) return s;
      return { ...s, resetting: true, banner: null };

    case "resetFinished":
      return { ...s, resetting: false, banner: a.error ? errorBanner(a.error, "reset") : s.banner };

    case "bannerDismissed":
      return { ...s, banner: null };

    case "overlayOpened":
      return { ...s, overlay: a.overlay };

    case "overlayClosed":
      return { ...s, overlay: null };
  }
}

/** Keep the «disconnected during write» banner across reconnects. */
function keepOpBanner(b: Banner | null): Banner | null {
  return b && b.kind === "Disconnected" && b.actions.length > 0 ? b : null;
}
