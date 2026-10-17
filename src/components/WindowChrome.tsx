import type { ReactNode } from "react";
import { useT } from "../i18n";
import { LogIcon, SettingsIcon } from "./Icons";

interface Props {
  dark: boolean;
  /** Tauri: native traffic lights over an overlay titlebar, window fills the viewport. */
  native: boolean;
  step: 1 | 2 | 3;
  overlay: "log" | "settings" | null;
  onLog: () => void;
  onSettings: () => void;
  children: ReactNode;
}

/** `data-tauri-drag-region` does not apply to children, so every passive element gets it. */
const drag = { "data-tauri-drag-region": "" } as const;

export function WindowChrome({ dark, native, step, overlay, onLog, onSettings, children }: Props) {
  const { t } = useT();
  const steps = [t.stepDevice, t.stepParams, t.stepRun];
  const cls = (n: number) => (n < step ? "step done" : n === step ? "step active" : "step");
  return (
    <div className={`win ${native ? "native" : "framed"}${dark ? " dark" : ""}`}>
      <div
        {...drag}
        style={{
          height: 52,
          flex: "none",
          display: "flex",
          alignItems: "center",
          gap: 16,
          padding: "0 14px 0 18px",
          background: "var(--bar)",
          borderBottom: "1px solid var(--sep)",
        }}
      >
        <div {...drag} aria-hidden="true" style={{ display: "flex", gap: 8, width: 120 }}>
          {!native && (
            <>
              <span style={{ width: 12, height: 12, borderRadius: "50%", background: "#FF5F57" }} />
              <span style={{ width: 12, height: 12, borderRadius: "50%", background: "#FEBC2E" }} />
              <span style={{ width: 12, height: 12, borderRadius: "50%", background: "#28C840" }} />
            </>
          )}
        </div>
        <ol
          {...drag}
          aria-label={t.steps}
          style={{
            flex: 1,
            display: "flex",
            justifyContent: "center",
            alignItems: "center",
            gap: 10,
            listStyle: "none",
            margin: 0,
            padding: 0,
          }}
        >
          {steps.map((label, i) => (
            <StepItem key={i} n={i + 1} label={label} cls={cls(i + 1)} last={i === steps.length - 1} current={i + 1 === step} />
          ))}
        </ol>
        <div style={{ display: "flex", gap: 2, width: 120, justifyContent: "flex-end" }}>
          <button className={overlay === "log" ? "icon-btn on" : "icon-btn"} aria-label={t.log} title={t.log} onClick={onLog}>
            <LogIcon />
          </button>
          <button
            className={overlay === "settings" ? "icon-btn on" : "icon-btn"}
            aria-label={t.settings}
            title={`${t.settings} (⌘,)`}
            onClick={onSettings}
          >
            <SettingsIcon />
          </button>
        </div>
      </div>
      {children}
    </div>
  );
}

function StepItem({ n, label, cls, last, current }: { n: number; label: string; cls: string; last: boolean; current: boolean }) {
  return (
    <>
      <li {...drag} className={cls} aria-current={current ? "step" : undefined}>
        <span {...drag} className="n">
          {n}
        </span>
        {label}
      </li>
      {!last && <li {...drag} aria-hidden="true" style={{ width: 28, height: 1, background: "var(--sep2)" }} />}
    </>
  );
}
