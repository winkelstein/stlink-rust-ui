// Parsing and validation of user-entered addresses and sizes, with the hint
// texts shown under the fields.

import type { FlashGeometry } from "../backend/types";
import type { I18n } from "../i18n/core";
import { bankBase, bankOf, flashEnd, planWrite, sectorOf, type FlashRange, type PlanError } from "./geometry";
import { fmtHex, fmtHex32, fmtRange } from "./format";

/**
 * `0x08000000`, `134217728`, `256K`, `1M`, `0x4_0000`. Separators (`_`, spaces)
 * are ignored; text after `·` is ignored (locked fields show `0x200000 · 2 МБ`).
 */
export function parseNumber(text: string): number | null {
  const t = text.split("·")[0].replace(/[\s_]/g, "").toLowerCase();
  if (t === "") return null;
  let m = /^0x([0-9a-f]+)$/.exec(t);
  if (m) return checkU32(parseInt(m[1], 16));
  m = /^(\d+)(k|kb|кб|m|mb|мб)?$/.exec(t);
  if (!m) return null;
  const mult = !m[2] ? 1 : m[2].startsWith("k") || m[2] === "кб" ? 1024 : 1024 * 1024;
  return checkU32(parseInt(m[1], 10) * mult);
}

function checkU32(n: number): number | null {
  return Number.isSafeInteger(n) && n >= 0 && n <= 0xffff_ffff ? n : null;
}

export type Tone = "ok" | "warn" | "error";

export interface Hint {
  tone: Tone;
  text: string;
}

export function planErrorText({ t, f }: I18n, g: FlashGeometry, error: PlanError, addr: number, len: number): string {
  switch (error) {
    case "empty":
      return t.planEmpty;
    case "invalidGeometry":
      return t.planGeometry;
    case "outside":
      return t.planOutside(fmtRange(g.base, g.size));
    case "unaligned":
      return t.planUnaligned(g.writeGranularity);
    case "doesNotFit":
      return t.planDoesNotFit(f.size(Math.max(0, flashEnd(g) - addr)), f.size(len));
  }
}

/** Address hint for the write form; `len` is `null` before a file is chosen. */
export function writeAddrHint(tr: I18n, g: FlashGeometry, text: string, len: number | null): Hint {
  const { t } = tr;
  const addr = parseNumber(text);
  if (addr === null) return { tone: "error", text: t.addrEnter };
  const res = planWrite(g, addr, len ?? g.writeGranularity);
  if (!res.ok) return { tone: "error", text: planErrorText(tr, g, res.error, addr, len ?? 0) };
  const bank = bankOf(g, addr);
  const sector = sectorOf(g, addr);
  const where =
    addr === g.base
      ? t.addrFlashStart(bank + 1)
      : addr === bankBase(g, bank)
        ? t.addrBankStart(bank + 1)
        : t.addrSector(sector.index, bank + 1);
  if (sector.addr === addr) return { tone: "ok", text: t.addrAligned(where) };
  return { tone: "warn", text: t.addrUnaligned(where, fmtHex32(sector.addr)) };
}

export type ReadCheckError = "start" | "size" | "outside" | "tooBig" | "unknown";
export type ReadCheck =
  | { ok: true; addr: number; size: number }
  | { ok: false; error: ReadCheckError; flash: FlashRange | null; max: number | null };

/**
 * A dump range must lie in flash (as `flash::read_len` in the library). An
 * empty size means «up to the end of flash». `max` — bytes from a valid start
 * address to the end of flash.
 */
export function checkReadRange(flash: FlashRange | null, startText: string, sizeText: string): ReadCheck {
  const fail = (error: ReadCheckError, max: number | null = null): ReadCheck => ({ ok: false, error, flash, max });
  if (!flash) return fail("unknown");
  const end = flash.base + flash.size;
  const addr = parseNumber(startText);
  if (addr === null) return fail("start");
  if (addr < flash.base || addr >= end) return fail("outside");
  const max = end - addr;
  if (sizeText.split("·")[0].trim() === "") return { ok: true, addr, size: max };
  const size = parseNumber(sizeText);
  if (size === null || size === 0) return fail("size", max);
  if (size > max) return fail("tooBig", max);
  return { ok: true, addr, size };
}

/** `0x40000 (256 КБ)`. */
export function fmtMax({ f }: I18n, bytes: number): string {
  return `${fmtHex(bytes)} (${f.size(bytes)})`;
}

export function readCheckText(tr: I18n, c: Extract<ReadCheck, { ok: false }>): string {
  const { t } = tr;
  switch (c.error) {
    case "start":
      return t.readEnterStart;
    case "size":
      return t.readEnterSize;
    case "outside":
      return c.flash ? t.planOutside(fmtRange(c.flash.base, c.flash.size)) : t.readUnknown;
    case "tooBig":
      return t.readTooBig(fmtMax(tr, c.max ?? 0));
    case "unknown":
      return t.readUnknown;
  }
}
