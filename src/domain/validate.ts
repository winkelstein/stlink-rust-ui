// Parsing and validation of user-entered addresses and sizes, with the hint
// texts shown under the fields.

import type { FlashGeometry } from "../backend/types";
import { bankBase, bankOf, flashEnd, planWrite, sectorOf, type PlanError } from "./geometry";
import { fmtHex32, fmtRange, fmtSize } from "./format";

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

export function planErrorText(g: FlashGeometry, error: PlanError, addr: number, len: number): string {
  switch (error) {
    case "empty":
      return "Файл пустой — записывать нечего";
    case "invalidGeometry":
      return "Неизвестная раскладка flash";
    case "outside":
      return `Адрес вне flash (${fmtRange(g.base, g.size)})`;
    case "unaligned":
      return `Адрес должен быть кратен ${g.writeGranularity} байтам`;
    case "doesNotFit":
      return `Файл не помещается: до конца flash ${fmtSize(Math.max(0, flashEnd(g) - addr))}, нужно ${fmtSize(len)}`;
  }
}

/** Address hint for the write form; `len` is `null` before a file is chosen. */
export function writeAddrHint(g: FlashGeometry, text: string, len: number | null): Hint {
  const addr = parseNumber(text);
  if (addr === null) return { tone: "error", text: "Введите адрес: 0x08000000 или десятичное число" };
  const res = planWrite(g, addr, len ?? g.writeGranularity);
  if (!res.ok) return { tone: "error", text: planErrorText(g, res.error, addr, len ?? 0) };
  const bank = bankOf(g, addr);
  const sector = sectorOf(g, addr);
  const where =
    addr === g.base
      ? `Начало flash, банк ${bank + 1}`
      : addr === bankBase(g, bank)
        ? `Начало банка ${bank + 1}`
        : `Сектор ${sector.index}, банк ${bank + 1}`;
  if (sector.addr === addr) return { tone: "ok", text: `${where} · выровнено по сектору` };
  return {
    tone: "warn",
    text: `${where} · не с начала сектора: сектор с ${fmtHex32(sector.addr)} будет стёрт целиком`,
  };
}

export type ReadCheck = { ok: true; addr: number; size: number } | { ok: false; text: string };

export function checkReadRange(startText: string, sizeText: string): ReadCheck {
  const addr = parseNumber(startText);
  if (addr === null) return { ok: false, text: "Введите начальный адрес: 0x08000000" };
  const size = parseNumber(sizeText);
  if (size === null || size === 0) return { ok: false, text: "Введите размер: 0x40000, 256K или 262144" };
  if (addr + size > 2 ** 32) return { ok: false, text: "Диапазон выходит за конец адресного пространства" };
  return { ok: true, addr, size };
}
