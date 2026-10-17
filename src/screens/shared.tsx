import { createContext, useContext, useEffect, useState } from "react";
import type { Controller } from "../state/controller";
import type { AppState } from "../state/machine";
import { chipShortName, probeModelName } from "../domain/chipNames";
import { useT } from "../i18n";
import { StatusChip } from "../components/StatusChip";
import { ErrorBanner } from "../components/ErrorBanner";

export interface AppCtx {
  s: AppState;
  ctl: Controller;
  /** `⌘R` on macOS, `Ctrl+R` elsewhere. */
  quickKey: string;
}

export const Ctx = createContext<AppCtx | null>(null);

export function useApp(): AppCtx {
  const c = useContext(Ctx);
  if (!c) throw new Error("useApp outside of the app context");
  return c;
}

/** Re-render every `ms` while mounted; `performance.now()`, or `Date.now()` if `wall`. */
export function useNow(ms: number, wall = false): number {
  const clock = wall ? Date.now : () => performance.now();
  const [now, setNow] = useState(clock);
  useEffect(() => {
    const read = wall ? Date.now : () => performance.now();
    const t = setInterval(() => setNow(read()), ms);
    return () => clearInterval(t);
  }, [ms, wall]);
  return now;
}

/** `ST-Link V2 · STM32H743 · 3,28 В` */
export function DeviceChip() {
  const { s } = useApp();
  const { f } = useT();
  if (!s.probe) return null;
  const parts = [probeModelName(s.probe.descriptor.model)];
  if (s.session) parts.push(chipShortName(s.session.target));
  const v = s.session?.probe.targetVoltage ?? null;
  if (v !== null) parts.push(f.voltage(v));
  return <StatusChip>{parts.join(" · ")}</StatusChip>;
}

/** The current banner, wired to the controller. */
export function BannerSlot() {
  const { s, ctl } = useApp();
  if (!s.banner) return null;
  return (
    <ErrorBanner
      banner={s.banner}
      actionsDisabled={!s.probe}
      onAction={(a) => ctl.bannerAction(a)}
      onDismiss={() => ctl.store.dispatch({ type: "bannerDismissed" })}
    />
  );
}

export const contentStyle = { flex: 1, minHeight: 0, overflow: "auto", overscrollBehavior: "contain" } as const;
export const screenStyle = { flex: 1, display: "flex", flexDirection: "column", minHeight: 0 } as const;
