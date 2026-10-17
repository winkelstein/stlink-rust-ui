import type { Backend } from "./Backend";
import { MockBackend } from "./mock/MockBackend";

export type { Backend } from "./Backend";

/** Running inside the Tauri webview (native titlebar, real file paths). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Mock without Tauri or with `VITE_BACKEND=mock`. Inside Tauri the mock is
 * used too until the command bridge exists.
 */
export function createBackend(): Backend {
  if (import.meta.env.VITE_BACKEND === "mock" || !isTauri()) return new MockBackend();
  // TODO(stage2): `TauriBackend` over `invoke` + events (src-tauri commands).
  console.warn("TauriBackend is not implemented yet, using MockBackend");
  return new MockBackend();
}
