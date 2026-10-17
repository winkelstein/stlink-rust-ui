// Parsing and validation of user-entered addresses and sizes, with the hint
// texts shown under the fields.

import type { FlashGeometry } from "../backend/types";
import type { I18n } from "../i18n/core";
import { bankBase, bankOf, flashEnd, planWrite, sectorOf, type PlanError } from "./geometry";
import { fmtHex32, fmtRange } from "./format";

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

export type ReadCheckError = "start" | "size" | "overflow";
export type ReadCheck = { ok: true; addr: number; size: number } | { ok: false; error: ReadCheckError };

export function checkReadRange(startText: string, sizeText: string): ReadCheck {
  const addr = parseNumber(startText);
  if (addr === null) return { ok: false, error: "start" };
  const size = parseNumber(sizeText);
  if (size === null || size === 0) return { ok: false, error: "size" };
  if (addr + size > 2 ** 32) return { ok: false, error: "overflow" };
  return { ok: true, addr, size };
}

export function readCheckText({ t }: I18n, e: ReadCheckError): string {
  return { start: t.readEnterStart, size: t.readEnterSize, overflow: t.readOverflow }[e];
}
