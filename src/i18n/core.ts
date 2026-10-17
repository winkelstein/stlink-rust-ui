// Language resolution, dictionaries and formatting bound to a language.
// No React here: usable from `domain/` and tests.

import {
  fmtAgo,
  fmtFreq,
  fmtInt,
  fmtMb,
  fmtNum,
  fmtRate,
  fmtSeconds,
  fmtSize,
  fmtVoltage,
  type Lang,
} from "../domain/format";
import { en } from "./en";
import { ru, type Dict } from "./ru";

export type { Dict } from "./ru";
export type { Lang } from "../domain/format";

/** The «Language» setting. */
export type LangSetting = "system" | Lang;

/** `ru*` → ru, anything else → en. */
export function systemLang(languages: readonly string[] = typeof navigator === "undefined" ? [] : navigator.languages ?? [navigator.language]): Lang {
  const first = languages.find(Boolean) ?? "";
  return first.toLowerCase().startsWith("ru") ? "ru" : "en";
}

export function resolveLang(setting: LangSetting): Lang {
  return setting === "system" ? systemLang() : setting;
}

export function dict(lang: Lang): Dict {
  return lang === "en" ? en : ru;
}

/** Number/unit formatting bound to a language. */
export function formatter(lang: Lang) {
  return {
    num: (n: number, digits: number, trim = false) => fmtNum(n, digits, trim, lang),
    int: (n: number) => fmtInt(n, lang),
    size: (bytes: number) => fmtSize(bytes, lang),
    mb: (bytes: number) => fmtMb(bytes, lang),
    rate: (bytesPerS: number) => fmtRate(bytesPerS, lang),
    seconds: (s: number, digits = 1) => fmtSeconds(s, digits, lang),
    voltage: (v: number) => fmtVoltage(v, lang),
    freq: (khz: number) => fmtFreq(khz, lang),
    ago: (ms: number, now: number) => fmtAgo(ms, now, lang),
  };
}

export type Fmt = ReturnType<typeof formatter>;

export interface I18n {
  lang: Lang;
  t: Dict;
  f: Fmt;
}

export function i18n(lang: Lang): I18n {
  return { lang, t: dict(lang), f: formatter(lang) };
}

