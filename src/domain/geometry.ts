// Flash layout and write planning — a port of `stlink/src/flash/geometry.rs`.
// Keep the checks in the same order as `FlashGeometry::plan_write`.

import type { FlashGeometry, FlashPlan, Sector } from "../backend/types";

/** STM32H74x/H75x: 2 banks × 8 sectors × 128 KB, 256-bit program word. */
export const H743_GEOMETRY: FlashGeometry = {
  base: 0x0800_0000,
  size: 2 * 1024 * 1024,
  bankSize: 1024 * 1024,
  sectorSize: 128 * 1024,
  writeGranularity: 32,
  erasedValue: 0xff,
};

export type PlanError = "empty" | "invalidGeometry" | "outside" | "unaligned" | "doesNotFit";

export type PlanResult = { ok: true; plan: FlashPlan } | { ok: false; error: PlanError };

export function flashEnd(g: FlashGeometry): number {
  return g.base + g.size;
}

export function bankCount(g: FlashGeometry): number {
  return Math.max(1, Math.round(g.size / Math.max(1, g.bankSize)));
}

export function sectorsPerBank(g: FlashGeometry): number {
  return Math.round(g.bankSize / Math.max(1, g.sectorSize));
}

export function inFlash(g: FlashGeometry, addr: number): boolean {
  return addr >= g.base && addr < flashEnd(g);
}

/** Bank number (0-based) of an address inside flash. */
export function bankOf(g: FlashGeometry, addr: number): number {
  return Math.floor((addr - g.base) / Math.max(1, g.bankSize));
}

export function bankBase(g: FlashGeometry, bank: number): number {
  return g.base + bank * g.bankSize;
}

export function sectorOf(g: FlashGeometry, addr: number): Sector {
  const bank = bankOf(g, addr);
  const base = bankBase(g, bank);
  const index = Math.floor((addr - base) / g.sectorSize);
  return { bank, index, addr: base + index * g.sectorSize };
}

/** All sectors in address order. */
export function allSectors(g: FlashGeometry): Sector[] {
  const out: Sector[] = [];
  for (let a = g.base; a < flashEnd(g); a += g.sectorSize) out.push(sectorOf(g, a));
  return out;
}

/** Validate a write of `len` bytes at `addr` and list the sectors to erase. */
export function planWrite(g: FlashGeometry, addr: number, len: number): PlanResult {
  if (len === 0) return { ok: false, error: "empty" };
  if (g.sectorSize === 0 || g.writeGranularity === 0 || g.bankSize === 0) {
    return { ok: false, error: "invalidGeometry" };
  }
  if (addr < g.base || addr >= flashEnd(g)) return { ok: false, error: "outside" };
  if (addr % g.writeGranularity !== 0) return { ok: false, error: "unaligned" };
  const paddedLen = Math.ceil(len / g.writeGranularity) * g.writeGranularity;
  if (addr + paddedLen > flashEnd(g)) return { ok: false, error: "doesNotFit" };
  const last = addr + paddedLen - 1;
  const sectors: Sector[] = [];
  for (let a = sectorOf(g, addr).addr; a <= last; a += g.sectorSize) {
    sectors.push(sectorOf(g, a));
  }
  return { ok: true, plan: { addr, len, paddedLen, sectors } };
}

export function sameSectors(a: readonly Sector[] | null, b: readonly Sector[]): boolean {
  return (
    a !== null &&
    a.length === b.length &&
    a.every((s, i) => s.bank === b[i].bank && s.index === b[i].index && s.addr === b[i].addr)
  );
}

/** Contiguous runs of sectors within one bank: `[{bank, first, last}]`. */
export interface SectorRun {
  bank: number;
  first: number;
  last: number;
  addr: number;
  /** One past the last byte. */
  end: number;
}

export function sectorRuns(g: FlashGeometry, sectors: readonly Sector[]): SectorRun[] {
  const runs: SectorRun[] = [];
  for (const s of sectors) {
    const prev = runs[runs.length - 1];
    if (prev && prev.bank === s.bank && prev.last + 1 === s.index) {
      prev.last = s.index;
      prev.end = s.addr + g.sectorSize;
    } else {
      runs.push({ bank: s.bank, first: s.index, last: s.index, addr: s.addr, end: s.addr + g.sectorSize });
    }
  }
  return runs;
}

/** Main flash as an address range (what `flash::read_len` accepts). */
export interface FlashRange {
  base: number;
  size: number;
}

/**
 * Readable flash of a target: from the geometry when there is a flash driver,
 * else from the flash size register (reading needs no driver). `null` when the
 * size is unknown — the library refuses to dump then.
 */
export function readableFlash(t: { geometry: FlashGeometry | null; flashSize: number | null }): FlashRange | null {
  if (t.geometry) return { base: t.geometry.base, size: t.geometry.size };
  return t.flashSize ? { base: 0x0800_0000, size: t.flashSize } : null;
}
