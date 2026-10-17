import type { Backend } from "./Backend";
import { MockBackend } from "./mock/MockBackend";
import { TauriBackend } from "./TauriBackend";

export type { Backend } from "./Backend";

/** Running inside the Tauri webview (native titlebar, real file paths). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** `TauriBackend` inside Tauri; the mock in the browser or with `VITE_BACKEND=mock`. */
export async function createBackend(): Promise<Backend> {
  if (import.meta.env.VITE_BACKEND === "mock" || !isTauri()) return new MockBackend();
  return TauriBackend.create();
}
