import { useSyncExternalStore } from "react";

const query = "(prefers-color-scheme: dark)";

function subscribe(cb: () => void) {
  const m = window.matchMedia(query);
  m.addEventListener("change", cb);
  return () => m.removeEventListener("change", cb);
}

/** Follows the system appearance. */
export function useDarkMode(): boolean {
  return useSyncExternalStore(subscribe, () => window.matchMedia(query).matches);
}
