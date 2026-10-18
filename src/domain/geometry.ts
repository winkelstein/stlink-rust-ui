// Flash layout and write planning — a port of `stlink/src/flash/geometry.rs`.
// Keep the checks in the same order as `FlashGeometry::plan_write`.

import type { EraseUnit, FlashGeometry, FlashPlan, Sector, SectorRun as UnitRun } from "../backend/types";

const KIB = 1024;

/** Banks laid out back to back from `base` (as `FlashGeometry::new`). */
export function makeGeometry(base: number, banks: UnitRun[][], unit: EraseUnit, writeGranularity: number): FlashGeometry {
  let addr = base;
  const out = banks.map((runs) => {
    const size = runs.reduce((n, r) => n + r.count * r.size, 0);
    const bank = { base: addr, size, runs };
    addr += size;
    return bank;
  });
  return { base, size: addr - base, banks: out, unit, writeGranularity, erasedValue: 0xff };
}

/** `4x16 1x64 7x128` (KiB) → runs, as in `chips/layouts.txt`. */
export function runs(spec: string): UnitRun[] {
  return spec
    .trim()
    .split(/\s+/)
    .map((r) => {
      const [count, kib] = r.split("x").map(Number);
      return { count, size: kib * KIB };
    });
}

/** STM32H74x/H75x: 2 banks × 8 sectors × 128 KB, 256-bit program word. */
export const H743_GEOMETRY: FlashGeometry = makeGeometry(0x0800_0000, [runs("8x128"), runs("8x128")], "sector", 32);

export type PlanError = "empty" | "invalidGeometry" | "outside" | "unaligned" | "doesNotFit";

export type PlanResult = { ok: true; plan: FlashPlan } | { ok: false; error: PlanError };

export function flashEnd(g: FlashGeometry): number {
  return g.base + g.size;
}

export function bankCount(g: FlashGeometry): number {
  return Math.max(1, g.banks.length);
}

/** Bank number (0-based) of an address inside flash; the last bank otherwise. */
export function bankOf(g: FlashGeometry, addr: number): number {
  const i = g.banks.findIndex((b) => addr >= b.base && addr < b.base + b.size);
  return i < 0 ? g.banks.length - 1 : i;
}

export function bankBase(g: FlashGeometry, bank: number): number {
  return g.banks[bank]?.base ?? g.base;
}

export function bankSize(g: FlashGeometry, bank: number): number {
  return g.banks[bank]?.size ?? 0;
}

/** Pages/sectors of one bank. */
export function unitsInBank(g: FlashGeometry, bank: number): number {
  return (g.banks[bank]?.runs ?? []).reduce((n, r) => n + r.count, 0);
}

/** All pages/sectors in address order. */
export function allSectors(g: FlashGeometry): Sector[] {
  const out: Sector[] = [];
  g.banks.forEach((bank, b) => {
    let addr = bank.base;
    let index = 0;
    for (const run of bank.runs) {
      for (let i = 0; i < run.count; i++) {
        out.push({ bank: b, index: index++, addr, size: run.size });
        addr += run.size;
      }
    }
  });
  return out;
}

/** The page/sector containing `addr`, if inside flash. */
export function sectorOf(g: FlashGeometry, addr: number): Sector | null {
  return allSectors(g).find((s) => addr >= s.addr && addr < s.addr + s.size) ?? null;
}

/** Validate a write of `len` bytes at `addr` and list the units to erase. */
export function planWrite(g: FlashGeometry, addr: number, len: number): PlanResult {
  if (len === 0) return { ok: false, error: "empty" };
  if (g.banks.length === 0 || g.size === 0 || g.writeGranularity === 0) {
    return { ok: false, error: "invalidGeometry" };
  }
  if (addr < g.base || addr >= flashEnd(g)) return { ok: false, error: "outside" };
  if (addr % g.writeGranularity !== 0) return { ok: false, error: "unaligned" };
  const paddedLen = Math.ceil(len / g.writeGranularity) * g.writeGranularity;
  if (addr + paddedLen > flashEnd(g)) return { ok: false, error: "doesNotFit" };
  const end = addr + paddedLen;
  const sectors = allSectors(g).filter((s) => s.addr < end && s.addr + s.size > addr);
  return { ok: true, plan: { addr, len, paddedLen, sectors } };
}

/** Same units (by address and size) — the quick write confirmation rule. */
export function sameSectors(a: readonly Sector[] | null, b: readonly Sector[]): boolean {
  return a !== null && a.length === b.length && a.every((s, i) => s.addr === b[i].addr && s.size === b[i].size);
}

export function bytesOf(sectors: readonly Sector[]): number {
  return sectors.reduce((n, s) => n + s.size, 0);
}

/** Contiguous runs of units within one bank: `[{bank, first, last}]`. */
export interface SectorRun {
  bank: number;
  first: number;
  last: number;
  addr: number;
  /** One past the last byte. */
  end: number;
}

export function sectorRuns(sectors: readonly Sector[]): SectorRun[] {
  const out: SectorRun[] = [];
  for (const s of sectors) {
    const prev = out[out.length - 1];
    if (prev && prev.bank === s.bank && prev.last + 1 === s.index) {
      prev.last = s.index;
      prev.end = s.addr + s.size;
    } else {
      out.push({ bank: s.bank, first: s.index, last: s.index, addr: s.addr, end: s.addr + s.size });
    }
  }
  return out;
}

/** A block of the flash map: consecutive units of one bank drawn as one cell. */
export interface MapBlock {
  bank: number;
  addr: number;
  size: number;
  first: number;
  last: number;
}

/**
 * Units grouped into at most `maxPerBank` blocks per bank, so hundreds of
 * pages (F1, G4) still draw as a readable map; a few sectors stay one each.
 */
export function mapBlocks(g: FlashGeometry, maxPerBank = 32): MapBlock[] {
  const blocks: MapBlock[] = [];
  g.banks.forEach((_, b) => {
    const units = allSectors(g).filter((s) => s.bank === b);
    const per = Math.max(1, Math.ceil(units.length / maxPerBank));
    for (let i = 0; i < units.length; i += per) {
      const group = units.slice(i, i + per);
      const last = group[group.length - 1];
      blocks.push({ bank: b, addr: group[0].addr, size: last.addr + last.size - group[0].addr, first: group[0].index, last: last.index });
    }
  });
  return blocks;
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
