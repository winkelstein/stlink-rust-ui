import { useEffect, useMemo, useState, type DragEvent } from "react";
import { isTauri } from "./backend";
import { isMod, shortcut } from "./platform";
import { MockBackend } from "./backend/mock/MockBackend";
import { DevPanel } from "./dev/DevPanel";
import { WindowChrome } from "./components/WindowChrome";
import { dict, I18nProvider, resolveLang } from "./i18n";
import { isBusy, stepOf, type AppState } from "./state/machine";
import type { Controller } from "./state/controller";
import { useAppState } from "./state/store";
import { useDarkMode } from "./useDarkMode";
import { Ctx, useApp, type AppCtx } from "./screens/shared";
import { EmptyScreen } from "./screens/EmptyScreen";
import { ReleasedScreen } from "./screens/ReleasedScreen";
import { DeviceScreen } from "./screens/DeviceScreen";
import { WriteScreen } from "./screens/WriteScreen";
import { EraseProgressScreen, WriteProgressScreen } from "./screens/WriteProgressScreen";
import { EraseDoneScreen, WriteDoneScreen } from "./screens/WriteDoneScreen";
import { ReadDoneScreen, ReadProgressScreen, ReadScreen } from "./screens/ReadScreens";
import { SettingsSheet } from "./screens/SettingsSheet";
import { LogSheet } from "./screens/LogSheet";

export default function App({ ctl }: { ctl: Controller }) {
  const s = useAppState(ctl.store);
  const systemDark = useDarkMode();
  const dark = s.settings.theme === "system" ? systemDark : s.settings.theme === "dark";
  const lang = resolveLang(s.settings.lang);
  const native = useMemo(isTauri, []);
  const ctx: AppCtx = { s, ctl, quickKey: shortcut("R") };
  const mock = ctl.backend instanceof MockBackend ? ctl.backend : null;
  const [devOpen, setDevOpen] = useState(false);

  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = isMod(e);
      // ⌥⌘D / Ctrl+Alt+D: dev panel (mock only). `code`, since ⌥ changes `key`.
      if (mod && e.altKey && e.code === "KeyD") {
        if (mock) {
          e.preventDefault();
          setDevOpen((v) => !v);
        }
        return;
      }
      // ⌘, / Ctrl+,: settings.
      if (mod && !e.altKey && e.key === ",") {
        e.preventDefault();
        ctl.store.dispatch({ type: "overlayOpened", overlay: "settings" });
        return;
      }
      // ⌘R / Ctrl+R: quick write. Always swallowed so the webview never reloads.
      if (mod && !e.altKey && !e.shiftKey && e.code === "KeyR") {
        e.preventDefault();
        if (!e.repeat && !ctl.store.getState().overlay) void ctl.quickWrite("quick");
        return;
      }
      if (e.key === "Escape") {
        const st = ctl.store.getState();
        if (st.overlay) ctl.store.dispatch({ type: "overlayClosed" });
        else if (st.screen === "confirm" || st.screen === "eraseConfirm") ctl.cancelConfirm();
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
        acceptDrop(ctl, path.split(/[\\/]/).pop() ?? path, path);
      }),
    [ctl],
  );

  const toggleOverlay = (o: "log" | "settings") =>
    ctl.store.dispatch(s.overlay === o ? { type: "overlayClosed" } : { type: "overlayOpened", overlay: o });

  return (
    <I18nProvider lang={lang}>
      <Ctx.Provider value={ctx}>
        <div className={native ? undefined : "stage-page"} onDragOver={onDragOver} onDrop={onDrop}>
          <WindowChrome
            dark={dark}
            native={native}
            step={stepOf(s.screen)}
            overlay={s.overlay}
            onLog={() => toggleOverlay("log")}
            onSettings={() => toggleOverlay("settings")}
          >
            <Screen />
            {s.overlay === "settings" && <SettingsSheet />}
            {s.overlay === "log" && <LogSheet />}
            {mock && devOpen && <DevPanel mock={mock} ctl={ctl} onClose={() => setDevOpen(false)} />}
          </WindowChrome>
        </div>
      </Ctx.Provider>
    </I18nProvider>
  );
}

function canDropIn(s: AppState): boolean {
  return !!s.session?.target.geometry && !isBusy(s) && !s.overlay && ["device", "write", "writeDone", "eraseDone"].includes(s.screen);
}

/** Only `.bin`; then open the write form with the file. */
function acceptDrop(ctl: Controller, name: string, source: File | string) {
  if (!name.toLowerCase().endsWith(".bin")) {
    const t = dict(resolveLang(ctl.store.getState().settings.lang));
    ctl.store.dispatch({
      type: "operationRejected",
      context: "write",
      error: { kind: "Unsupported", message: name },
      note: t.onlyBin(name),
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
    case "released":
      return <ReleasedScreen />;
    case "device":
    case "eraseConfirm":
      return <DeviceScreen />;
    case "eraseProgress":
      return <EraseProgressScreen />;
    case "eraseDone":
      return <EraseDoneScreen />;
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
