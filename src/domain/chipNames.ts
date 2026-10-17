// Display names for chips. `dev_type` from the chip table names a whole line
// (`STM32H74x_H75x`); the UI shows a friendlier name where we know one.

import type { CoreInfo, TargetInfo } from "../backend/types";

const NAMES: Record<number, { full: string; short: string }> = {
  0x450: { full: "STM32H743 / H753", short: "STM32H743" },
};

export function chipFullName(t: TargetInfo): string {
  return NAMES[t.chipId]?.full ?? t.chip?.devType ?? "Неизвестный МК";
}

export function chipShortName(t: TargetInfo): string {
  return NAMES[t.chipId]?.short ?? t.chip?.devType ?? "МК";
}

const CORE_NAMES: Record<CoreInfo["core"], string> = {
  M0: "Cortex-M0",
  M0Plus: "Cortex-M0+",
  M3: "Cortex-M3",
  M4: "Cortex-M4",
  M7: "Cortex-M7",
  M33: "Cortex-M33",
  Unknown: "Неизвестное ядро",
};

/** `Cortex-M7 r1p1`. */
export function coreName(c: CoreInfo): string {
  return `${CORE_NAMES[c.core]} r${c.revision[0]}p${c.revision[1]}`;
}
