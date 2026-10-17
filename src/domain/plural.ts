// Plural form selection for Russian (three forms).

/** Russian plural: `forms` = [one, few, many] (1, 2–4, 5–20 …). */
export function plural(n: number, forms: readonly [string, string, string]): string {
  const m10 = n % 10;
  const m100 = n % 100;
  if (m10 === 1 && m100 !== 11) return forms[0];
  if (m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14)) return forms[1];
  return forms[2];
}
