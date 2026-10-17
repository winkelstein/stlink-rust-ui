// App state machine: screens, forms, running operation, banners. Pure — all
// backend calls live in `controller.ts`, which dispatches the results here.

import type {
  BackendError,
  BinFile,
  ConnectOptions,
  DeviceSession,
  FlashPlan,
  ProbeInfo,
  Progress,
  QuickWriteSettings,
  ReadJob,
  ReadResult,
  SaveTarget,
  Stage,
  WriteJob,
  WriteResult,
} from "../backend/types";
import { fmtHex32 } from "../domain/format";
import { DEFAULT_QUICK, withChosenFile } from "../domain/quickWrite";

export type Screen =
  | "empty"
  | "device"
  | "write"
  | "confirm"
  | "writeProgress"
  | "writeDone"
  | "read"
  | "readProgress"
  | "readDone";

export type ReadRange = "all" | "b1" | "b2" | "custom";

/** What started a write. */
export type WriteOrigin = "wizard" | "quick" | "onConnect" | "onFileChange";

export const DEFAULT_SWD_KHZ = 1800;

export const DEFAULT_CONNECT: ConnectOptions = {
  swdKhz: DEFAULT_SWD_KHZ,
  underReset: false,
  keepDebugInLowPower: false,
};

export type BannerAction =
  | "retryWrite"
  | "retryRead"
  | "retryIdentify"
  | "retryReset"
  | "connectUnderReset"
  | "keepDebugLowPower";

// TODO(design): error banners are not in the mockups yet.
export interface Banner {
  kind: BackendError["kind"] | "notice";
  title: string;
  text: string;
  actions: BannerAction[];
}

export interface WriteForm {
  file: BinFile | null;
  addrText: string;
  verify: boolean;
  resetAfter: boolean;
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

export type Operation =
  | {
      kind: "write";
      id: number;
      pending: PendingWrite;
      progress: Progress;
      startedAt: number;
      /** When each stage got its first progress report. */
      stageStart: Partial<Record<Stage, number>>;
      lastProgressAt: number;
    }
  | {
      kind: "read";
      id: number;
      job: ReadJob;
      progress: Progress;
      startedAt: number;
      lastProgressAt: number;
    };

export interface AppState {
  screen: Screen;
  probe: ProbeInfo | null;
  session: DeviceSession | null;
  identifying: boolean;
  connectOptions: ConnectOptions;
  resetting: boolean;
  writeForm: WriteForm;
  readForm: ReadForm;
  /** Write waiting for confirmation, and where «Отмена» returns. */
  confirm: { pending: PendingWrite; returnTo: "write" | "device" } | null;
  op: Operation | null;
  /** Last started write — for «Повторить» / «Записать снова». */
  lastWrite: PendingWrite | null;
  writeResult: { result: WriteResult; pending: PendingWrite } | null;
  readResult: ReadResult | null;
  /** Quick write settings of the connected chip (by UID). */
  quick: QuickWriteSettings | null;
  /** Write-on-connect armed for a chip that is not connected (empty screen). */
  armed: { uid: string; settings: QuickWriteSettings } | null;
  banner: Banner | null;
  stub: "log" | "settings" | null;
}

export const initialState: AppState = {
  screen: "empty",
  probe: null,
  session: null,
  identifying: false,
  connectOptions: DEFAULT_CONNECT,
  resetting: false,
  writeForm: { file: null, addrText: fmtHex32(0x0800_0000), verify: true, resetAfter: true },
  readForm: { range: "all", startText: fmtHex32(0x0800_0000), sizeText: "0x40000", target: null },
  confirm: null,
  op: null,
  lastWrite: null,
  writeResult: null,
  readResult: null,
  quick: null,
  armed: null,
  banner: null,
  stub: null,
};

export type Action =
  | { type: "probeConnected"; probe: ProbeInfo }
  | { type: "probeDisconnected"; probeId: string }
  | { type: "identifyStarted"; opts: ConnectOptions }
  | { type: "identified"; session: DeviceSession; quick: QuickWriteSettings | null }
  | { type: "identifyFailed"; error: BackendError }
  | { type: "armedLoaded"; armed: AppState["armed"] }
  | { type: "disarm" }
  | { type: "navigate"; to: "device" | "write" | "read" }
  | { type: "fileChosen"; file: BinFile }
  | { type: "fileUpdated"; file: BinFile }
  | { type: "addrChanged"; text: string }
  | { type: "optionToggled"; option: "verify" | "resetAfter" }
  | { type: "quickToggled"; option: "onConnect" | "onFileChange" }
  | { type: "confirmRequested"; pending: PendingWrite; returnTo: "write" | "device" }
  | { type: "confirmCancelled" }
  | { type: "confirmAccepted" }
  | { type: "writeStarted"; id: number; pending: PendingWrite; at: number }
  | { type: "readStarted"; id: number; job: ReadJob; at: number }
  | { type: "opProgress"; id: number; progress: Progress; at: number }
  | { type: "writeFinished"; id: number; result: WriteResult }
  | { type: "readFinished"; id: number; result: ReadResult }
  | { type: "opFailed"; id: number; error: BackendError }
  | { type: "readRangeChanged"; range: ReadRange; startText: string; sizeText: string }
  | { type: "readFieldChanged"; field: "startText" | "sizeText"; text: string }
  | { type: "saveTargetChosen"; target: SaveTarget }
  | { type: "resetStarted" }
  | { type: "resetFinished"; error: BackendError | null }
  | { type: "operationRejected"; error: BackendError; context: "write" | "read" }
  | { type: "bannerDismissed" }
  | { type: "stubOpened"; stub: "log" | "settings" }
  | { type: "stubClosed" };

/** Step of the wizard shown in the titlebar. */
export function stepOf(screen: Screen): 1 | 2 | 3 {
  switch (screen) {
    case "empty":
    case "device":
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
  return s.op !== null || s.identifying || s.resetting;
}

const PARTIAL = "Flash мог остаться частично записан, ядро остановлено.";

function errorBanner(e: BackendError, context: "identify" | "write" | "read" | "reset"): Banner {
  const retry: Record<typeof context, BannerAction> = {
    identify: "retryIdentify",
    write: "retryWrite",
    read: "retryRead",
    reset: "retryReset",
  };
  const partial = context === "write" ? ` ${PARTIAL}` : "";
  switch (e.kind) {
    case "Disconnected":
      return {
        kind: e.kind,
        title: "Программатор отключён",
        text:
          context === "write"
            ? `Связь потеряна во время записи. ${PARTIAL} Подключите программатор и повторите запись.`
            : "Связь с программатором потеряна. Подключите его снова.",
        actions: context === "write" ? ["retryWrite"] : context === "read" ? ["retryRead"] : [],
      };
    case "CoreNotAccessible":
      return {
        kind: e.kind,
        title: "Ядро МК не отвечает",
        text: "Прошивка, вероятно, уводит МК в low-power с выключенной отладкой. Подключитесь со сбросом или включите отладку в low-power (DBGMCU_CR сохранится до выключения питания).",
        actions: ["connectUnderReset", "keepDebugLowPower"],
      };
    case "FlashProtected":
      return {
        kind: e.kind,
        title: "Flash защищена",
        text: "Включена защита чтения (RDP). Снять её из приложения нельзя — используйте STM32CubeProgrammer.",
        actions: [],
      };
    case "Busy":
      return {
        kind: e.kind,
        title: "Программатор занят",
        text: "ST-Link используется другой программой (openocd, st-util, STM32CubeProgrammer). Закройте её и повторите.",
        actions: [retry[context]],
      };
    case "Unsupported":
      return {
        kind: e.kind,
        title: "Не поддерживается",
        text: `${e.message}.`,
        actions: [],
      };
    case "Cancelled":
      return { kind: "notice", title: "Отменено", text: PARTIAL, actions: [] };
    case "Other":
      return {
        kind: e.kind,
        title: context === "write" ? "Ошибка записи" : context === "read" ? "Ошибка чтения" : "Ошибка",
        text: `${e.message}.${partial}`,
        actions: [retry[context]],
      };
  }
}

function formFromPending(form: WriteForm, p: PendingWrite): WriteForm {
  return { ...form, file: p.file, addrText: fmtHex32(p.job.addr), verify: p.job.verify, resetAfter: p.job.resetAfter };
}

function updateQuick(s: AppState, f: (q: QuickWriteSettings) => QuickWriteSettings): QuickWriteSettings | null {
  if (!s.session?.target.uid) return s.quick;
  return { ...f(s.quick ?? DEFAULT_QUICK), updatedAt: Date.now() };
}

/** Screens where the device may be used (no op, no sheet). */
function idle(s: AppState): boolean {
  return !isBusy(s) && s.session !== null && s.screen !== "confirm";
}

export function reducer(s: AppState, a: Action): AppState {
  switch (a.type) {
    case "probeConnected":
      if (s.probe) return s;
      return { ...s, probe: a.probe, session: null, screen: "device", stub: null };

    case "probeDisconnected": {
      if (!s.probe || s.probe.id !== a.probeId) return s;
      let banner = s.banner;
      if (s.op?.kind === "write") banner = errorBanner({ kind: "Disconnected", message: "" }, "write");
      else if (s.op?.kind === "read") banner = errorBanner({ kind: "Disconnected", message: "" }, "read");
      else if (banner && banner.kind !== "Disconnected") banner = null;
      const armed =
        s.quick?.onConnect && s.session?.target.uid ? { uid: s.session.target.uid, settings: s.quick } : s.armed;
      return {
        ...s,
        screen: "empty",
        probe: null,
        session: null,
        identifying: false,
        resetting: false,
        confirm: null,
        op: null,
        quick: null,
        armed,
        banner,
      };
    }

    case "identifyStarted":
      return { ...s, identifying: true, connectOptions: a.opts, banner: keepOpBanner(s.banner) };

    case "identified": {
      if (!s.probe) return s;
      const quick = a.quick;
      const writeForm =
        quick?.file && !s.writeForm.file
          ? { ...s.writeForm, file: quick.file, addrText: fmtHex32(quick.addr) }
          : s.writeForm;
      return { ...s, identifying: false, session: a.session, quick, armed: null, writeForm };
    }

    case "identifyFailed":
      if (!s.probe) return s;
      return { ...s, identifying: false, banner: errorBanner(a.error, "identify") };

    case "armedLoaded":
      return { ...s, armed: a.armed };

    case "disarm":
      if (s.armed) return { ...s, armed: { ...s.armed, settings: { ...s.armed.settings, onConnect: false } } };
      return { ...s, quick: updateQuick(s, (q) => ({ ...q, onConnect: false })) };

    case "navigate": {
      if (isBusy(s) || !s.session || s.screen === "confirm") return s;
      // A finished/cancelled operation's notice belongs to the screen it is shown on.
      const banner = a.to === "device" ? null : s.banner;
      return { ...s, screen: a.to, banner };
    }

    case "fileChosen":
      return {
        ...s,
        writeForm: { ...s.writeForm, file: a.file },
        quick: s.quick ? updateQuick(s, (q) => withChosenFile(q, a.file)) : s.quick,
        screen: idle(s) && (s.screen === "device" || s.screen === "writeDone") ? "write" : s.screen,
      };

    case "fileUpdated": {
      const writeForm = s.writeForm.file?.id === a.file.id ? { ...s.writeForm, file: a.file } : s.writeForm;
      const quick = s.quick?.file?.id === a.file.id ? { ...s.quick, file: a.file } : s.quick;
      return { ...s, writeForm, quick };
    }

    case "addrChanged":
      return { ...s, writeForm: { ...s.writeForm, addrText: a.text } };

    case "optionToggled":
      return { ...s, writeForm: { ...s.writeForm, [a.option]: !s.writeForm[a.option] } };

    case "quickToggled":
      if (!s.session?.target.uid) return s;
      return { ...s, quick: updateQuick(s, (q) => ({ ...q, [a.option]: !q[a.option] })) };

    case "confirmRequested":
      if (isBusy(s) || !s.session) return s;
      return {
        ...s,
        screen: "confirm",
        confirm: { pending: a.pending, returnTo: a.returnTo },
        writeForm: formFromPending(s.writeForm, a.pending),
        banner: null,
      };

    case "confirmCancelled":
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
        writeForm: formFromPending(s.writeForm, a.pending),
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

    case "opProgress": {
      if (!s.op || s.op.id !== a.id) return s;
      if (s.op.kind === "write") {
        const stageStart = s.op.stageStart[a.progress.stage]
          ? s.op.stageStart
          : { ...s.op.stageStart, [a.progress.stage]: a.at };
        return { ...s, op: { ...s.op, progress: a.progress, stageStart, lastProgressAt: a.at } };
      }
      return { ...s, op: { ...s.op, progress: a.progress, lastProgressAt: a.at } };
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

    case "opFailed": {
      if (!s.op || s.op.id !== a.id) return s;
      const kind = s.op.kind;
      const banner =
        a.error.kind === "Cancelled"
          ? kind === "write"
            ? { kind: "notice" as const, title: "Запись отменена", text: PARTIAL, actions: [] }
            : { kind: "notice" as const, title: "Чтение отменено", text: "Файл не сохранён.", actions: [] }
          : errorBanner(a.error, kind);
      return { ...s, screen: kind, op: null, banner };
    }

    case "operationRejected":
      // Failed before anything started (plan rejected, no file): stay on the form.
      return { ...s, banner: errorBanner(a.error, a.context) };

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

    case "stubOpened":
      return { ...s, stub: a.stub };

    case "stubClosed":
      return { ...s, stub: null };
  }
}

/** Keep the «disconnected during write» banner across reconnects. */
function keepOpBanner(b: Banner | null): Banner | null {
  return b && b.kind === "Disconnected" && b.actions.length > 0 ? b : null;
}
