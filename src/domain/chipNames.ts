// Display names for chips. `dev_type` from the chip table names a whole line
// (`STM32H74x_H75x`); the UI shows a friendlier name where we know one.

import type { CoreInfo, ProbeModel, TargetInfo } from "../backend/types";
import type { Dict } from "../i18n/core";

const NAMES: Record<number, { full: string; short: string }> = {
  0x450: { full: "STM32H743 / H753", short: "STM32H743" },
};

export function chipFullName(t: TargetInfo, d: Dict): string {
  return NAMES[t.chipId]?.full ?? t.chip?.devType ?? d.unknownChip;
}

/** Also used in file names: no translation. */
export function chipShortName(t: TargetInfo): string {
  return NAMES[t.chipId]?.short ?? t.chip?.devType ?? "MCU";
}

const CORE_NAMES: Record<Exclude<CoreInfo["core"], "Unknown">, string> = {
  M0: "Cortex-M0",
  M0Plus: "Cortex-M0+",
  M3: "Cortex-M3",
  M4: "Cortex-M4",
  M7: "Cortex-M7",
  M23: "Cortex-M23",
  M33: "Cortex-M33",
};

/** `Cortex-M7 r1p1`. */
export function coreName(c: CoreInfo, d: Dict): string {
  const name = c.core === "Unknown" ? d.unknownCore : CORE_NAMES[c.core];
  return `${name} r${c.revision[0]}p${c.revision[1]}`;
}

const PROBE_NAMES: Record<ProbeModel, string> = {
  V2: "ST-Link V2",
  V2_1: "ST-Link V2-1",
  V3: "ST-Link V3",
};

/** `usb::ProbeModel` `Display`. */
export function probeModelName(m: ProbeModel): string {
  return PROBE_NAMES[m];
}
