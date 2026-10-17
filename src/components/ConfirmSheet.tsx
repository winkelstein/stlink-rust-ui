import { useEffect, useRef, type KeyboardEvent, type ReactNode } from "react";
import { Button } from "./Button";
import { WarnIcon } from "./Icons";

interface Props {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}

/** macOS-style sheet under the titlebar. Esc cancels, Enter confirms, Tab stays inside. */
export function ConfirmSheet({ title, children, confirmLabel, onConfirm, onCancel }: Props) {
  const dialog = useRef<HTMLDivElement>(null);
  const primary = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    primary.current?.focus();
    return () => prev?.focus?.();
  }, []);

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onCancel();
    } else if (e.key === "Tab") {
      const items = dialog.current?.querySelectorAll<HTMLElement>("button:not(:disabled)");
      if (!items || items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  };

  return (
    <div
      onKeyDown={onKeyDown}
      onMouseDown={(e) => e.target === e.currentTarget && primary.current?.focus()}
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
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby="cf-title"
        style={{
          width: 440,
          marginTop: 0,
          padding: "22px 24px 18px",
          background: "var(--card)",
          border: "1px solid var(--sep)",
          borderTop: 0,
          borderRadius: "0 0 12px 12px",
          boxShadow: "0 18px 40px rgba(0,0,0,.25)",
          display: "flex",
          flexDirection: "column",
          gap: 12,
        }}
      >
        <div
          style={{
            width: 40,
            height: 40,
            borderRadius: 10,
            background: "var(--warnSoft)",
            color: "var(--warn)",
            display: "grid",
            placeItems: "center",
          }}
        >
          <WarnIcon />
        </div>
        <h3 id="cf-title" style={{ margin: 0, fontSize: 15, fontWeight: 600 }}>
          {title}
        </h3>
        <p style={{ margin: 0, fontSize: 13, lineHeight: 1.5, color: "var(--text2)" }}>{children}</p>
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 10, marginTop: 6 }}>
          <Button onClick={onCancel}>Отмена</Button>
          <Button primary ref={primary} onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
