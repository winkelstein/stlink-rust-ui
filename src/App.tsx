import { useEffect, useMemo, useState, type DragEvent } from "react";
import { isTauri } from "./backend";
import { MockBackend } from "./backend/mock/MockBackend";
import { DevPanel } from "./dev/DevPanel";
import { WindowChrome } from "./components/WindowChrome";
import { isBusy, stepOf, type AppState } from "./state/machine";
import type { Controller } from "./state/controller";
import { useAppState } from "./state/store";
import { useDarkMode } from "./useDarkMode";
import { Ctx, useApp, type AppCtx } from "./screens/shared";
import { EmptyScreen } from "./screens/EmptyScreen";
import { DeviceScreen } from "./screens/DeviceScreen";
import { WriteScreen } from "./screens/WriteScreen";
import { WriteProgressScreen } from "./screens/WriteProgressScreen";
import { WriteDoneScreen } from "./screens/WriteDoneScreen";
import { ReadDoneScreen, ReadProgressScreen, ReadScreen } from "./screens/ReadScreens";
import { ComingSoon } from "./screens/ComingSoon";

export const IS_MAC = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

export default function App({ ctl }: { ctl: Controller }) {
  const s = useAppState(ctl.store);
  const dark = useDarkMode();
  const native = useMemo(isTauri, []);
  const ctx: AppCtx = { s, ctl, quickKey: IS_MAC ? "⌘R" : "Ctrl+R" };
  const mock = ctl.backend instanceof MockBackend ? ctl.backend : null;
  const [devOpen, setDevOpen] = useState(false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = IS_MAC ? e.metaKey : e.ctrlKey;
      // ⌥⌘D: dev panel (mock only). `code`, since ⌥ changes `key`.
      if (mod && e.altKey && e.code === "KeyD") {
        if (mock) {
          e.preventDefault();
          setDevOpen((v) => !v);
        }
        return;
      }
      // ⌘R: quick write. Always swallowed so the webview never reloads.
      if (mod && !e.altKey && !e.shiftKey && e.code === "KeyR") {
        e.preventDefault();
        if (!e.repeat) void ctl.quickWrite("quick");
        return;
      }
      if (e.key === "Escape") {
        const st = ctl.store.getState();
        if (st.stub) ctl.store.dispatch({ type: "stubClosed" });
        else if (st.screen === "confirm") ctl.cancelConfirm();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [ctl, mock]);

  // Drag-and-drop of a .bin onto the window: DOM events in the browser,
  // paths from the native window in Tauri.
  const canDrop = canDropIn(s);
  const onDragOver = (e: DragEvent) => {
    if (!canDrop || !e.dataTransfer.types.includes("Files")) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  };
  const onDrop = (e: DragEvent) => {
    if (!canDrop) return;
    e.preventDefault();
    const f = e.dataTransfer.files[0];
    if (f) acceptDrop(ctl, f.name, f);
  };
  useEffect(
    () =>
      ctl.backend.watchFileDrops((paths) => {
        if (!canDropIn(ctl.store.getState())) return;
        const path = paths[0];
        acceptDrop(ctl, path.split("/").pop() ?? path, path);
      }),
    [ctl],
  );

  return (
    <Ctx.Provider value={ctx}>
      <div className={native ? undefined : "stage-page"} onDragOver={onDragOver} onDrop={onDrop}>
        <WindowChrome
          dark={dark}
          native={native}
          step={stepOf(s.screen)}
          onLog={() => ctl.store.dispatch({ type: "stubOpened", stub: "log" })}
          onSettings={() => ctl.store.dispatch({ type: "stubOpened", stub: "settings" })}
        >
          <Screen />
          {s.stub && <ComingSoon which={s.stub} onClose={() => ctl.store.dispatch({ type: "stubClosed" })} />}
          {mock && devOpen && <DevPanel mock={mock} onClose={() => setDevOpen(false)} />}
        </WindowChrome>
      </div>
    </Ctx.Provider>
  );
}

function canDropIn(s: AppState): boolean {
  return !!s.session?.target.geometry && !isBusy(s) && ["device", "write", "writeDone"].includes(s.screen);
}

/** Only `.bin`; then open the write form with the file. */
function acceptDrop(ctl: Controller, name: string, source: File | string) {
  if (!name.toLowerCase().endsWith(".bin")) {
    ctl.store.dispatch({
      type: "operationRejected",
      context: "write",
      error: { kind: "Unsupported", message: `«${name}»: поддерживаются только файлы .bin` },
    });
    return;
  }
  void ctl.dropFile(source).then(() => ctl.store.dispatch({ type: "navigate", to: "write" }));
}

function Screen() {
  const { s } = useApp();
  switch (s.screen) {
    case "empty":
      return <EmptyScreen />;
    case "device":
      return <DeviceScreen />;
    case "write":
    case "confirm":
      return <WriteScreen />;
    case "writeProgress":
      return <WriteProgressScreen />;
    case "writeDone":
      return <WriteDoneScreen />;
    case "read":
      return <ReadScreen />;
    case "readProgress":
      return <ReadProgressScreen />;
    case "readDone":
      return <ReadDoneScreen />;
  }
}
