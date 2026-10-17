// The `useT()` hook (no third-party i18n library).

import { createContext, useContext, useMemo, type ReactNode } from "react";
import { i18n, type I18n, type Lang } from "./core";

export * from "./core";

const Ctx = createContext<I18n>(i18n("ru"));

export function I18nProvider({ lang, children }: { lang: Lang; children: ReactNode }) {
  const value = useMemo(() => i18n(lang), [lang]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

/** `{ t, f, lang }`: strings, formatting, current language. */
export function useT(): I18n {
  return useContext(Ctx);
}
