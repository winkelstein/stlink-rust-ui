// The one place that knows the OS: shortcut modifier and labels, Finder vs
// Explorer, the overlay titlebar. `navigator.userAgentData` where available
// (WebView2, Chromium), `navigator.platform` otherwise (WKWebView, WebKitGTK).

export type Platform = "macos" | "windows" | "linux";

interface UaData {
  platform?: string;
}

export function detectPlatform(nav: { userAgentData?: UaData; platform?: string; userAgent?: string } | undefined): Platform {
  const p = `${nav?.userAgentData?.platform ?? ""} ${nav?.platform ?? ""} ${nav?.userAgent ?? ""}`.toLowerCase();
  if (/mac|iphone|ipad/.test(p)) return "macos";
  if (/win/.test(p)) return "windows";
  return "linux";
}

export const PLATFORM: Platform = detectPlatform(
  typeof navigator === "undefined" ? undefined : (navigator as Navigator & { userAgentData?: UaData }),
);

export const IS_MAC = PLATFORM === "macos";

/** The shortcut modifier is pressed: ⌘ on macOS, Ctrl elsewhere. */
export function isMod(e: { metaKey: boolean; ctrlKey: boolean }): boolean {
  return IS_MAC ? e.metaKey : e.ctrlKey;
}

/** `⌘R` / `Ctrl+R`; `alt` adds ⌥ / Alt. */
export function shortcut(key: string, alt = false): string {
  return IS_MAC ? `${alt ? "⌥" : ""}⌘${key}` : `Ctrl+${alt ? "Alt+" : ""}${key}`;
}
