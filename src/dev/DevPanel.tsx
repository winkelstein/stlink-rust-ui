import { useSyncExternalStore } from "react";
import type { MockBackend, MockFault } from "../backend/mock/MockBackend";
import type { LangSetting } from "../backend/types";
import { Button } from "../components/Button";
import { Segmented } from "../components/Segmented";
import type { Controller } from "../state/controller";
import { useAppState } from "../state/store";
import { shortcut } from "../platform";

// Dev-only (mock) tooling: not translated.

const FAULTS: { value: MockFault | "none"; label: string }[] = [
  { value: "none", label: "Нет" },
  { value: "Disconnected", label: "Disconnected" },
  { value: "CoreNotAccessible", label: "CoreNotAcc." },
  { value: "FlashProtected", label: "Protected" },
  { value: "Busy", label: "Busy" },
  { value: "EraseFault", label: "Erase err" },
];

const SPEEDS = [
  { value: "1", label: "×1" },
  { value: "5", label: "×5" },
  { value: "20", label: "×20" },
] as const;

const LANGS: { value: LangSetting; label: string }[] = [
  { value: "system", label: "Sys" },
  { value: "ru", label: "RU" },
  { value: "en", label: "EN" },
];

/** Mock-only controls (⌥⌘D / Ctrl+Alt+D): hot-plug, file changes, injected errors, speed, language. */
export function DevPanel({ mock, ctl, onClose }: { mock: MockBackend; ctl: Controller; onClose: () => void }) {
  const c = mock.controls;
  const st = useSyncExternalStore(c.subscribe, c.getState);
  const app = useAppState(ctl.store);
  const row = { display: "flex", alignItems: "center", gap: 8 } as const;
  const label = { fontSize: 11, color: "var(--text2)", width: 72, flex: "none" } as const;
  return (
    <div
      role="region"
      aria-label="Dev-панель"
      className="card"
      style={{
        position: "absolute",
        right: 12,
        bottom: 72,
        width: 540,
        padding: "10px 12px",
        zIndex: 20,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        fontSize: 12,
        boxShadow: "0 12px 32px rgba(0,0,0,.22)",
      }}
    >
      <div style={{ ...row, justifyContent: "space-between" }}>
        <span className="eyebrow">Mock · dev</span>
        <span style={{ color: "var(--text3)", fontSize: 11 }}>{shortcut("D", true)} — скрыть</span>
        <button className="icon-btn" style={{ width: 22, height: 22 }} aria-label="Закрыть dev-панель" onClick={onClose}>
          ×
        </button>
      </div>
      <div style={row}>
        <span style={label}>USB</span>
        <Button style={{ height: 26 }} onClick={() => (st.connected ? c.disconnect() : c.connect())}>
          {st.connected ? "Выдернуть" : "Вставить"}
        </Button>
        <span style={{ color: "var(--text2)" }}>
          {st.connected ? (st.opened ? "подключён к приложению" : "найден, не открыт") : "нет"}
        </span>
      </div>
      <div style={row}>
        <span style={label}>Занят</span>
        <Button
          style={{ height: 26 }}
          disabled={!st.connected || st.opened}
          onClick={() => {
            c.setNextFault("Busy");
            void ctl.connect();
          }}
        >
          Программатор занят другой программой
        </Button>
      </div>
      <div style={row}>
        <span style={label}>Файл</span>
        <Button style={{ height: 26 }} disabled={st.watchedFiles === 0} onClick={() => c.touchWatchedFiles(true)}>
          Изменить
        </Button>
        <Button style={{ height: 26 }} disabled={st.watchedFiles === 0} onClick={() => c.touchWatchedFiles(false)}>
          Тот же CRC
        </Button>
        <span style={{ color: "var(--text2)" }}>{st.watchedFiles === 0 ? "слежения нет" : "слежу"}</span>
      </div>
      <div style={row}>
        <span style={label}>Ошибка</span>
        <div style={{ flex: 1 }}>
          <Segmented
            label="Ошибка для следующей операции"
            value={st.nextFault ?? "none"}
            options={FAULTS}
            onChange={(v) => c.setNextFault(v === "none" ? null : v)}
          />
        </div>
      </div>
      <div style={row}>
        <span style={label}>Скорость</span>
        <div style={{ width: 160 }}>
          <Segmented
            label="Множитель скорости"
            value={String(st.speed) as (typeof SPEEDS)[number]["value"]}
            options={SPEEDS}
            onChange={(v) => c.setSpeed(Number(v))}
          />
        </div>
        <span style={label}>Язык</span>
        <div style={{ width: 150 }}>
          <Segmented
            label="Язык"
            value={app.settings.lang}
            options={LANGS}
            onChange={(lang) => ctl.store.dispatch({ type: "settingsChanged", patch: { lang } })}
          />
        </div>
      </div>
    </div>
  );
}
