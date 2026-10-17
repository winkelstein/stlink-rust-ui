// The window is a native app, not a page: no pinch zoom, no Ctrl/⌘+wheel
// zoom, no Ctrl/⌘ +/−/0. (Tauri's `zoomHotkeysEnabled` is off as well.)

export function disableZoom(): () => void {
  const onWheel = (e: WheelEvent) => {
    // A trackpad pinch arrives as a wheel event with `ctrlKey` (Chromium, WebView2).
    if (e.ctrlKey) e.preventDefault();
  };
  const onKey = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && ["+", "=", "-", "_", "0"].includes(e.key)) e.preventDefault();
  };
  // WebKit (WKWebView) pinch gestures.
  const onGesture = (e: Event) => e.preventDefault();
  window.addEventListener("wheel", onWheel, { passive: false });
  window.addEventListener("keydown", onKey);
  for (const t of ["gesturestart", "gesturechange", "gestureend"]) document.addEventListener(t, onGesture, { passive: false });
  return () => {
    window.removeEventListener("wheel", onWheel);
    window.removeEventListener("keydown", onKey);
    for (const t of ["gesturestart", "gesturechange", "gestureend"]) document.removeEventListener(t, onGesture);
  };
}
