// UI formatting by language: ru — decimal comma, space-grouped thousands,
// Russian units; en — decimal point, comma-grouped thousands. The locale data
// lives with the dictionaries (`NUMBERS` in src/i18n/{ru,en}.ts).

import { NUMBERS as EN } from "../i18n/en";
import { NUMBERS as RU } from "../i18n/ru";

export { plural } from "./plural";

export type Lang = "ru" | "en";

const KB = 1024;
const MB = 1024 * 1024;

/** Units, separators and "… ago" texts of a language (read lazily: the
 * dictionaries import this module). */
function numbers(lang: Lang) {
  return lang === "en" ? EN : RU;
}

const UNITS = {
  get ru() {
    return numbers("ru");
  },
  get en() {
    return numbers("en");
  },
};

/** Fixed decimals, trailing zeros trimmed if `trim`. */
export function fmtNum(n: number, digits: number, trim = false, lang: Lang = "ru"): string {
  let s = n.toFixed(digits);
  if (trim && s.includes(".")) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s.replace(".", UNITS[lang].dec);
}

/** `187 392` (non-breaking space) / `187,392`. */
export function fmtInt(n: number, lang: Lang = "ru"): string {
  return Math.round(n)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, UNITS[lang].group);
}

/** `183 KB`, `1.75 MB`, `2 MB`, `512 bytes` (units by language). */
export function fmtSize(bytes: number, lang: Lang = "ru"): string {
  const u = UNITS[lang];
  if (bytes >= MB) return `${fmtNum(bytes / MB, 2, true, lang)} ${u.mb}`;
  if (bytes >= KB) return `${fmtInt(Math.round(bytes / KB), lang)} ${u.kb}`;
  return `${fmtInt(bytes, lang)} ${u.b}`;
}

/** `2.00 MB` — fixed two decimals, for running counters. */
export function fmtMb(bytes: number, lang: Lang = "ru"): string {
  return `${fmtNum(bytes / MB, 2, false, lang)} ${UNITS[lang].mb}`;
}

/** `148 KB/s`. */
export function fmtRate(bytesPerS: number, lang: Lang = "ru"): string {
  const u = UNITS[lang];
  return `${Math.round(bytesPerS / KB)} ${u.kb}/${u.s}`;
}

/** `0x08000000`. */
export function fmtHex32(n: number): string {
  return "0x" + (n >>> 0).toString(16).toUpperCase().padStart(8, "0");
}

/** `0x200000` — shortest hex. */
export function fmtHex(n: number): string {
  return "0x" + (n >>> 0).toString(16).toUpperCase();
}

/** `0x08000000–0x0802DBFF` for `[addr, addr + len)`. */
export function fmtRange(addr: number, len: number): string {
  return `${fmtHex32(addr)}–${fmtHex32(addr + Math.max(1, len) - 1)}`;
}

export function fmtHex16(n: number): string {
  return (n & 0xffff).toString(16).toUpperCase().padStart(4, "0");
}

/** `1.9 s` (ru: decimal comma). */
export function fmtSeconds(s: number, digits = 1, lang: Lang = "ru"): string {
  return `${fmtNum(Math.max(0, s), digits, false, lang)} ${UNITS[lang].s}`;
}

/** `3.28 V` (ru: decimal comma). */
export function fmtVoltage(v: number, lang: Lang = "ru"): string {
  return `${fmtNum(v, 2, false, lang)} ${UNITS[lang].v}`;
}

/** `1.8 MHz`, `950 kHz`. */
export function fmtFreq(khz: number, lang: Lang = "ru"): string {
  const u = UNITS[lang];
  return khz >= 1000 ? `${fmtNum(khz / 1000, 1, true, lang)} ${u.mhz}` : `${khz} ${u.khz}`;
}

/** `003A0029 3133510D 36383837`. */
export function fmtUid(uid: string): string {
  return uid.toUpperCase().match(/.{1,8}/g)?.join(" ") ?? uid;
}

const p2 = (n: number) => String(n).padStart(2, "0");

/** `14:02`. */
export function fmtClock(ms: number): string {
  const d = new Date(ms);
  return `${p2(d.getHours())}:${p2(d.getMinutes())}`;
}

/** `14:31:58.402` (local time, for the log). */
export function fmtLogTime(ms: number): string {
  const d = new Date(ms);
  return `${p2(d.getHours())}:${p2(d.getMinutes())}:${p2(d.getSeconds())}.${String(d.getMilliseconds()).padStart(3, "0")}`;
}

/** `2026-10-17` (local date). */
export function fmtDate(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${p2(d.getMonth() + 1)}-${p2(d.getDate())}`;
}

/** `just now`, `2 min ago`, `3 h ago`, `2026-10-15`. */
export function fmtAgo(ms: number, now: number, lang: Lang = "ru"): string {
  const u = numbers(lang);
  const min = Math.floor((now - ms) / 60000);
  if (min < 1) return u.justNow;
  if (min < 60) return u.minAgo(min);
  const h = Math.floor(min / 60);
  if (h < 24) return u.hAgo(h);
  return fmtDate(ms);
}
