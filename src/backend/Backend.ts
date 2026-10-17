import type {
  BinFile,
  ConnectOptions,
  DeviceSession,
  FlashPlan,
  ProbeId,
  ProbeInfo,
  Progress,
  QuickWriteSettings,
  ReadJob,
  ReadResult,
  SaveTarget,
  WriteJob,
  WriteResult,
} from "./types";

export type Unsubscribe = () => void;

export type DeviceEvent =
  | { type: "connected"; probe: ProbeInfo }
  | { type: "disconnected"; probeId: ProbeId };

export interface OpOptions {
  onProgress: (p: Progress) => void;
  /** Checked between blocks; an aborted operation rejects with `Cancelled`. */
  signal: AbortSignal;
}

/**
 * Everything the UI needs from the device side. Implemented by `MockBackend`
 * now and by a Tauri command bridge later. All failures reject with
 * `BackendFailure`.
 */
export interface Backend {
  readonly kind: "mock" | "tauri";

  /** Hot-plug; emits `connected` for already present probes right away. */
  watchDevices(cb: (e: DeviceEvent) => void): Unsubscribe;
  /** `StLink::open` → `connect` → `identify` (+ RDP, geometry). */
  identify(probeId: ProbeId, opts: ConnectOptions): Promise<DeviceSession>;
  /** `flash::plan_write`; rejects for invalid address/size. */
  planWrite(probeId: ProbeId, addr: number, len: number): Promise<FlashPlan>;
  /** `write_flash` (+ `reset_and_run` if asked). */
  write(probeId: ProbeId, job: WriteJob, o: OpOptions): Promise<WriteResult>;
  /** `dump` into `job.target`. */
  read(probeId: ProbeId, job: ReadJob, o: OpOptions): Promise<ReadResult>;
  resetAndRun(probeId: ProbeId): Promise<void>;

  /** Open dialog for a `.bin`; `null` if cancelled. */
  pickBinFile(): Promise<BinFile | null>;
  /** A `.bin` dropped onto the window (browser drag-and-drop). */
  acceptDroppedFile(file: File): Promise<BinFile>;
  /** Save dialog; `null` if cancelled. */
  pickSavePath(defaultName: string): Promise<SaveTarget | null>;
  /** Default location for dumps (before the user picks one). */
  defaultSaveTarget(defaultName: string): SaveTarget;
  revealInFinder(target: SaveTarget): Promise<void>;
  /** Called with the new metadata (size, mtime, CRC) after each change. */
  watchFile(fileId: string, cb: (f: BinFile) => void): Unsubscribe;

  loadQuickWrite(uid: string): QuickWriteSettings | null;
  saveQuickWrite(uid: string, s: QuickWriteSettings): void;
  /** All stored settings, most recently updated first. */
  listQuickWrite(): { uid: string; settings: QuickWriteSettings }[];
}
