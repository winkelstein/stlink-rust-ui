// Russian UI formatting (decimal comma, grouped thousands).

const KB = 1024;
const MB = 1024 * 1024;

/** Fixed decimals with a comma, trailing zeros trimmed if `trim`. */
export function fmtNum(n: number, digits: number, trim = false): string {
  let s = n.toFixed(digits);
  if (trim && s.includes(".")) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s.replace(".", ",");
}

/** `187 392` (non-breaking space). */
export function fmtInt(n: number): string {
  return Math.round(n)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, " ");
}

/** `183 КБ`, `1,75 МБ`, `2 МБ`, `512 байт`. */
export function fmtSize(bytes: number): string {
  if (bytes >= MB) return `${fmtNum(bytes / MB, 2, true)} МБ`;
  if (bytes >= KB) return `${fmtInt(Math.round(bytes / KB))} КБ`;
  return `${fmtInt(bytes)} байт`;
}

/** `2,00 МБ` — fixed two decimals, for running counters. */
export function fmtMb(bytes: number): string {
  return `${fmtNum(bytes / MB, 2)} МБ`;
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

/** `1,9 с`. */
export function fmtSeconds(s: number, digits = 1): string {
  return `${fmtNum(Math.max(0, s), digits)} с`;
}

/** `3,28 В`. */
export function fmtVoltage(v: number): string {
  return `${fmtNum(v, 2)} В`;
}

/** `1,8 МГц`, `950 кГц`. */
export function fmtFreq(khz: number): string {
  return khz >= 1000 ? `${fmtNum(khz / 1000, 1, true)} МГц` : `${khz} кГц`;
}

/** `003A0029 3133510D 36383837`. */
export function fmtUid(uid: string): string {
  return uid.toUpperCase().match(/.{1,8}/g)?.join(" ") ?? uid;
}

/** `14:02`. */
export function fmtClock(ms: number): string {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** `2026-10-17` (local date). */
export function fmtDate(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** Russian plural: `plural(2, ['сектор', 'сектора', 'секторов'])`. */
export function plural(n: number, forms: readonly [string, string, string]): string {
  const m10 = n % 10;
  const m100 = n % 100;
  if (m10 === 1 && m100 !== 11) return forms[0];
  if (m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14)) return forms[1];
  return forms[2];
}

export const SECTOR_FORMS = ["сектор", "сектора", "секторов"] as const;

/** `только что`, `2 мин назад`, `3 ч назад`, `в 14:02`. */
export function fmtAgo(ms: number, now: number): string {
  const min = Math.floor((now - ms) / 60000);
  if (min < 1) return "только что";
  if (min < 60) return `${min} мин назад`;
  const h = Math.floor(min / 60);
  if (h < 24) return `${h} ч назад`;
  return `${fmtDate(ms)}`;
}
