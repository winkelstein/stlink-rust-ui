// DTOs of the backend contract. They mirror the `stlink` library types and
// are the shape the future Tauri commands will serialize (camelCase).

/** Stable id of a connected probe (`bus_id:device_address`). */
export type ProbeId = string;

/** `usb::ProbeModel`. */
export type ProbeModel = "V2" | "V2_1" | "V3";

/** `usb::ProbeDescriptor`. */
export interface ProbeDescriptor {
  vid: number;
  pid: number;
  model: ProbeModel;
  serial: string;
  busId: string;
  deviceAddress: number;
}

/** `probe::version::ApiVersion`. */
export type ApiVersion = "V1" | "V2" | "V3";

/** `probe::version::ProbeVersion`; `display` is its `Display` (`V2J46S7`). */
export interface ProbeVersion {
  stlink: number;
  jtag: number;
  swim: number;
  msd: number;
  bridge: number;
  api: ApiVersion;
  display: string;
}

/** Opened probe: descriptor, firmware version and target voltage. */
export interface ProbeInfo {
  id: ProbeId;
  descriptor: ProbeDescriptor;
  version: ProbeVersion;
  /** Volts; `null` if the probe cannot measure it. */
  targetVoltage: number | null;
}

/** `session::ConnectOptions`. */
export interface ConnectOptions {
  swdKhz: number;
  underReset: boolean;
  keepDebugInLowPower: boolean;
}

/** `session::Connection`. */
export interface Connection {
  /** Actual SWD frequency set on the probe. */
  swdKhz: number | null;
  haltedByConnect: boolean;
  lowPowerDebug: boolean | null;
}

/** `arm::CoreType`. */
export type CoreType = "M0" | "M0Plus" | "M3" | "M4" | "M7" | "M33" | "Unknown";

/** `arm::CoreInfo`. */
export interface CoreInfo {
  cpuid: number;
  implementer: number;
  core: CoreType;
  /** (variant, revision) → `r1p1`. */
  revision: [number, number];
}

/** The part of `chips::ChipDescriptor` the GUI needs. */
export interface ChipInfo {
  chipId: number;
  devType: string;
  flashType: string;
  flashPagesize: number;
  sramSize: number;
  dualBank: boolean;
}

/** `flash::geometry::FlashGeometry`. */
export interface FlashGeometry {
  base: number;
  size: number;
  bankSize: number;
  sectorSize: number;
  writeGranularity: number;
  erasedValue: number;
}

/** `flash::RdpLevel`. */
export type RdpLevel = "Level0" | "Level1" | "Level2";

/** `target::TargetInfo` plus the flash geometry and RDP level. */
export interface TargetInfo {
  core: CoreInfo;
  chip: ChipInfo | null;
  chipId: number;
  idcodeRaw: number;
  revId: number;
  flashSize: number | null;
  /** 96-bit UID as 24 hex digits (three big-endian words). */
  uid: string | null;
  /** `null` when the library has no flash driver for the chip (write → Unsupported). */
  geometry: FlashGeometry | null;
  rdp: RdpLevel | null;
}

/** Result of `identify`: everything shown on the device screen. */
export interface DeviceSession {
  probe: ProbeInfo;
  connection: Connection;
  target: TargetInfo;
}

/** `flash::geometry::Sector`. */
export interface Sector {
  bank: number;
  /** Index within the bank. */
  index: number;
  addr: number;
}

/** `flash::geometry::WritePlan` plus the unpadded length. */
export interface FlashPlan {
  addr: number;
  len: number;
  paddedLen: number;
  sectors: Sector[];
}

/** `flash::Stage`. */
export type Stage = "erasing" | "programming" | "verifying" | "reading";

/** `flash::Progress`. */
export interface Progress {
  stage: Stage;
  done: number;
  total: number;
}

export type WriteStage = Exclude<Stage, "reading">;

/** A firmware file known to the backend (contents stay in the backend). */
export interface BinFile {
  id: string;
  name: string;
  /** Absolute path in Tauri; `null` in the browser. */
  path: string | null;
  size: number;
  /** ms since epoch. */
  modifiedAt: number;
  crc32: number;
}

export interface WriteJob {
  fileId: string;
  addr: number;
  verify: boolean;
  resetAfter: boolean;
}

export interface WriteResult {
  plan: FlashPlan;
  crc32: number;
  elapsedMs: number;
  stageMs: Record<WriteStage, number>;
  verified: boolean;
  resetDone: boolean;
}

/** Where a dump goes. */
export interface SaveTarget {
  name: string;
  /** Directory as shown to the user (`~/Downloads`). */
  dir: string;
  /** Absolute path in Tauri; `null` in the browser (download). */
  path: string | null;
}

export interface ReadJob {
  addr: number;
  size: number;
  target: SaveTarget;
}

export interface ReadResult {
  addr: number;
  size: number;
  crc32: number;
  elapsedMs: number;
  savedTo: SaveTarget;
}

/**
 * `stlink::Error` mapped to `{ kind, message }`. Variants without a special
 * UI treatment (Timeout, SwdFault, Verify, ...) are `Other`.
 */
export type BackendErrorKind =
  | "Disconnected"
  | "CoreNotAccessible"
  | "FlashProtected"
  | "Busy"
  | "Unsupported"
  | "Cancelled"
  | "Other";

export interface BackendError {
  kind: BackendErrorKind;
  message: string;
}

/** The only error type backend promises reject with. */
export class BackendFailure extends Error {
  readonly error: BackendError;

  constructor(error: BackendError) {
    super(error.message);
    this.name = "BackendFailure";
    this.error = error;
  }
}

/** Normalize anything thrown by a backend call. */
export function toBackendError(e: unknown): BackendError {
  if (e instanceof BackendFailure) return e.error;
  if (e instanceof DOMException && e.name === "AbortError") {
    return { kind: "Cancelled", message: "cancelled" };
  }
  return { kind: "Other", message: e instanceof Error ? e.message : String(e) };
}

/** Quick write settings, stored per chip UID. */
export interface QuickWriteSettings {
  file: BinFile | null;
  addr: number;
  /** Write as soon as the same chip (UID) is connected. */
  onConnect: boolean;
  /** Rewrite when the file changes (debounced, CRC checked). */
  onFileChange: boolean;
  /** Sectors of the last write the user confirmed. */
  lastConfirmedSectors: Sector[] | null;
  /** CRC of the last successfully written image (for the file watcher). */
  lastWrittenCrc: number | null;
  /** For the «waiting for the board» banner while no device is connected. */
  chipName: string | null;
  updatedAt: number;
}
