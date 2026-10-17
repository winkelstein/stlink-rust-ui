import { useEffect, useRef } from "react";
import { Button } from "../components/Button";

const TITLES = { log: "Журнал", settings: "Настройки" } as const;

/** Placeholder for screens that are not designed yet. */
export function ComingSoon({ which, onClose }: { which: keyof typeof TITLES; onClose: () => void }) {
  const close = useRef<HTMLButtonElement>(null);
  useEffect(() => close.current?.focus(), []);
  return (
    <div
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          onClose();
        }
      }}
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
      style={{
        position: "absolute",
        left: 0,
        right: 0,
        top: 52,
        bottom: 0,
        background: "var(--dim)",
        display: "flex",
        justifyContent: "center",
        alignItems: "flex-start",
        zIndex: 10,
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="soon-title"
        style={{
          width: 360,
          padding: "20px 24px 16px",
          background: "var(--card)",
          border: "1px solid var(--sep)",
          borderTop: 0,
          borderRadius: "0 0 12px 12px",
          boxShadow: "0 18px 40px rgba(0,0,0,.25)",
          display: "flex",
          flexDirection: "column",
          gap: 10,
        }}
      >
        <h3 id="soon-title" style={{ margin: 0, fontSize: 15, fontWeight: 600 }}>
          {TITLES[which]} — скоро
        </h3>
        <p style={{ margin: 0, fontSize: 13, lineHeight: 1.5, color: "var(--text2)" }}>Этот экран ещё не нарисован.</p>
        <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 4 }}>
          <Button ref={close} primary onClick={onClose}>
            Закрыть
          </Button>
        </div>
      </div>
    </div>
  );
}
