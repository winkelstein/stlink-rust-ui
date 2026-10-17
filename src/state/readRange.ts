import type { FlashGeometry } from "../backend/types";
import { fmtDate, fmtHex, fmtHex32 } from "../domain/format";
import { bankBase, bankCount } from "../domain/geometry";
import type { ReadRange } from "./machine";

/**
 * Field texts for a preset range; `custom` keeps the defaults from the mockup.
 * Locked fields show `0x200000 · 2 MB`: the size suffix is added when
 * rendering (it depends on the language).
 */
export function rangeFields(range: ReadRange, g: FlashGeometry): { startText: string; sizeText: string } {
  const locked = (addr: number, size: number) => ({
    startText: fmtHex32(addr),
    sizeText: fmtHex(size),
  });
  switch (range) {
    case "all":
      return locked(g.base, g.size);
    case "b1":
      return locked(g.base, g.bankSize);
    case "b2":
      return locked(bankBase(g, 1), g.bankSize);
    case "custom":
      return { startText: fmtHex32(g.base), sizeText: "0x40000" };
  }
}

export function availableRanges(g: FlashGeometry): ReadRange[] {
  return bankCount(g) > 1 ? ["all", "b1", "b2", "custom"] : ["all", "custom"];
}

/** `STM32H743_flash_2026-10-17.bin`, `STM32H743_08000000_2026-10-17.bin`. */
export function defaultReadName(range: ReadRange, chip: string, addr: number, now: number): string {
  const what = { all: "flash", b1: "bank1", b2: "bank2", custom: fmtHex32(addr).slice(2) }[range];
  return `${chip}_${what}_${fmtDate(now)}.bin`;
}
