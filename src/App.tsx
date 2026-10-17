import { useMemo } from "react";
import { isTauri } from "./backend";
import { WindowChrome } from "./components/WindowChrome";
import { stepOf } from "./state/machine";
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

  return (
    <Ctx.Provider value={ctx}>
      <div className={native ? undefined : "stage-page"}>
        <WindowChrome
          dark={dark}
          native={native}
          step={stepOf(s.screen)}
          onLog={() => ctl.store.dispatch({ type: "stubOpened", stub: "log" })}
          onSettings={() => ctl.store.dispatch({ type: "stubOpened", stub: "settings" })}
        >
          <Screen />
          {s.stub && <ComingSoon which={s.stub} onClose={() => ctl.store.dispatch({ type: "stubClosed" })} />}
        </WindowChrome>
      </div>
    </Ctx.Provider>
  );
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
