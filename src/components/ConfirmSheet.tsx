import { useRef, type ReactNode } from "react";
import { useT } from "../i18n";
import { Button } from "./Button";
import { EraseIcon, WarnIcon } from "./Icons";
import { Sheet } from "./Sheet";

interface Props {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  /** `danger`: red icon and button (full erase). */
  danger?: boolean;
  width?: number;
  onConfirm: () => void;
  onCancel: () => void;
}

/** Confirmation sheet. Esc cancels, Enter confirms (the confirm button has focus). */
export function ConfirmSheet({ title, children, confirmLabel, danger, width = 440, onConfirm, onCancel }: Props) {
  const { t } = useT();
  const primary = useRef<HTMLButtonElement>(null);
  return (
    <Sheet labelledBy="cf-title" onClose={onCancel} initialFocus={primary} style={{ width, padding: "22px 24px 18px", gap: 12 }}>
      <div
        style={{
          width: 40,
          height: 40,
          borderRadius: 10,
          background: danger ? "var(--errSoft)" : "var(--warnSoft)",
          color: danger ? "var(--err)" : "var(--warn)",
          display: "grid",
          placeItems: "center",
        }}
      >
        {danger ? <EraseIcon size={22} /> : <WarnIcon />}
      </div>
      <h3 id="cf-title" style={{ margin: 0, fontSize: 15, fontWeight: 600 }}>
        {title}
      </h3>
      <p style={{ margin: 0, fontSize: 13, lineHeight: 1.5, color: "var(--text2)" }}>{children}</p>
      <div style={{ display: "flex", justifyContent: "flex-end", gap: 10, marginTop: 6 }}>
        <Button onClick={onCancel}>{t.cancel}</Button>
        <Button primary={!danger} className={danger ? "btn-danger" : undefined} ref={primary} onClick={onConfirm}>
          {confirmLabel}
        </Button>
      </div>
    </Sheet>
  );
}
