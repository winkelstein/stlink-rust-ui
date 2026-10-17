import type {
  AppSettings,
  BinFile,
  ConnectOptions,
  DeviceSession,
  EraseResult,
  FlashPlan,
  LogEntry,
  ProbeId,
  ProbeSummary,
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
  | { type: "connected"; probe: ProbeSummary }
  | { type: "disconnected"; probeId: ProbeId };

export interface OpOptions {
  onProgress: (p: Progress) => void;
  /** Checked between blocks; an aborted operation rejects with `Cancelled`. */
  signal: AbortSignal;
}

/**
 * Everything the UI needs from the device side. Implemented by `MockBackend`
 * (browser, `VITE_BACKEND=mock`) and `TauriBackend` (src-tauri commands).
 * All failures reject with `BackendFailure`.
 */
export interface Backend {
  readonly kind: "mock" | "tauri";

  /**
   * Hot-plug; emits `connected` for already present probes right away. A
   * discovered probe is not opened: the app never connects on its own.
   */
  watchDevices(cb: (e: DeviceEvent) => void): Unsubscribe;
  /** «Подключиться»: `StLink::open` → `connect` → `identify` (+ RDP, geometry). */
  connect(probeId: ProbeId, opts: ConnectOptions): Promise<DeviceSession>;
  /** «Отключить»: close the device so other programs can use the probe. */
  disconnect(probeId: ProbeId): Promise<void>;
  /** `flash::plan_write`; rejects for invalid address/size. */
  planWrite(probeId: ProbeId, addr: number, len: number): Promise<FlashPlan>;
  /** `write_flash` (+ `reset_and_run` if asked). */
  write(probeId: ProbeId, job: WriteJob, o: OpOptions): Promise<WriteResult>;
  /** `dump` into `job.target`. */
  read(probeId: ProbeId, job: ReadJob, o: OpOptions): Promise<ReadResult>;
  resetAndRun(probeId: ProbeId): Promise<void>;
  /** `flash::mass_erase`: every bank; progress counts banks. */
  eraseAll(probeId: ProbeId, o: OpOptions): Promise<EraseResult>;

  /** Open dialog for a `.bin`; `null` if cancelled. */
  pickBinFile(): Promise<BinFile | null>;
  /** A `.bin` dropped onto the window (browser drag-and-drop). */
  acceptDroppedFile(file: File): Promise<BinFile>;
  /** Files dropped onto the native window (Tauri delivers paths, not `File`s). */
  watchFileDrops(cb: (paths: string[]) => void): Unsubscribe;
  /** A file by absolute path (native drop). */
  openFile(path: string): Promise<BinFile>;
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

  /** App settings (loaded before the UI starts). */
  loadSettings(): AppSettings;
  /** Persist; «Подробный журнал USB» takes effect right away. */
  saveSettings(s: AppSettings): void;

  /** The journal so far (ring buffer, oldest first). */
  logEntries(): Promise<LogEntry[]>;
  /** New journal lines as they come, in batches. */
  watchLog(cb: (entries: LogEntry[]) => void): Unsubscribe;
  clearLog(): Promise<void>;
  /** Save dialog for the journal text; the path, or `null` if cancelled. */
  saveLog(text: string, defaultName: string): Promise<string | null>;
}
