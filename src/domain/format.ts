// UI formatting by language: ru — decimal comma, space-grouped thousands,
// Russian units; en — decimal point, comma-grouped thousands.

export type Lang = "ru" | "en";

const KB = 1024;
const MB = 1024 * 1024;

const UNITS = {
  ru: { b: "байт", kb: "КБ", mb: "МБ", s: "с", v: "В", mhz: "МГц", khz: "кГц", group: "\u00a0", dec: "," },
  en: { b: "bytes", kb: "KB", mb: "MB", s: "s", v: "V", mhz: "MHz", khz: "kHz", group: ",", dec: "." },
} as const;

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

/** `183 КБ`, `1,75 МБ`, `2 МБ`, `512 байт` / `183 KB`, `1.75 MB`. */
export function fmtSize(bytes: number, lang: Lang = "ru"): string {
  const u = UNITS[lang];
  if (bytes >= MB) return `${fmtNum(bytes / MB, 2, true, lang)} ${u.mb}`;
  if (bytes >= KB) return `${fmtInt(Math.round(bytes / KB), lang)} ${u.kb}`;
  return `${fmtInt(bytes, lang)} ${u.b}`;
}

/** `2,00 МБ` — fixed two decimals, for running counters. */
export function fmtMb(bytes: number, lang: Lang = "ru"): string {
  return `${fmtNum(bytes / MB, 2, false, lang)} ${UNITS[lang].mb}`;
}

/** `148 КБ/с`. */
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

/** `1,9 с` / `1.9 s`. */
export function fmtSeconds(s: number, digits = 1, lang: Lang = "ru"): string {
  return `${fmtNum(Math.max(0, s), digits, false, lang)} ${UNITS[lang].s}`;
}

/** `3,28 В` / `3.28 V`. */
export function fmtVoltage(v: number, lang: Lang = "ru"): string {
  return `${fmtNum(v, 2, false, lang)} ${UNITS[lang].v}`;
}

/** `1,8 МГц`, `950 кГц`. */
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

/** Russian plural: `plural(2, ['сектор', 'сектора', 'секторов'])`. */
export function plural(n: number, forms: readonly [string, string, string]): string {
  const m10 = n % 10;
  const m100 = n % 100;
  if (m10 === 1 && m100 !== 11) return forms[0];
  if (m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14)) return forms[1];
  return forms[2];
}

/** `только что`, `2 мин назад`, `3 ч назад`, `2026-10-15`. */
export function fmtAgo(ms: number, now: number, lang: Lang = "ru"): string {
  const min = Math.floor((now - ms) / 60000);
  const en = lang === "en";
  if (min < 1) return en ? "just now" : "только что";
  if (min < 60) return en ? `${min} min ago` : `${min} мин назад`;
  const h = Math.floor(min / 60);
  if (h < 24) return en ? `${h} h ago` : `${h} ч назад`;
  return fmtDate(ms);
}
