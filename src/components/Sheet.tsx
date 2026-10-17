import { useEffect, useRef, type CSSProperties, type KeyboardEvent, type ReactNode, type RefObject } from "react";

interface Props {
  labelledBy: string;
  /** Esc / click on the scrim. */
  onClose: () => void;
  /** Focused on open (default: the sheet itself). */
  initialFocus?: RefObject<HTMLElement | null>;
  style?: CSSProperties;
  children: ReactNode;
}

const FOCUSABLE = "button:not(:disabled), input:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex='-1'])";

/** macOS-style sheet under the titlebar (`.scrim` + `.sheet`). Esc closes, Tab stays inside. */
export function Sheet({ labelledBy, onClose, initialFocus, style, children }: Props) {
  const dialog = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    (initialFocus?.current ?? dialog.current)?.focus();
    return () => prev?.focus?.();
  }, [initialFocus]);

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    } else if (e.key === "Tab") {
      const items = dialog.current?.querySelectorAll<HTMLElement>(FOCUSABLE);
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
    <div className="scrim" style={{ zIndex: 10 }} onKeyDown={onKeyDown} onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={dialog} className="sheet" role="dialog" aria-modal="true" aria-labelledby={labelledBy} tabIndex={-1} style={{ outline: "none", ...style }}>
        {children}
      </div>
    </div>
  );
}
