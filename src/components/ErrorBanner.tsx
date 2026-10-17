import { useState } from "react";
import type { Dict } from "../i18n";
import { useT } from "../i18n";
import type { Banner, BannerAction } from "../state/machine";
import { Button } from "./Button";
import { WarnIcon } from "./Icons";

function actionLabel(t: Dict, a: BannerAction): string {
  switch (a) {
    case "retryWrite":
      return t.actRetryWrite;
    case "connectUnderReset":
      return t.actUnderReset;
    case "keepDebugLowPower":
      return t.actKeepDebug;
    default:
      return t.actRetry;
  }
}

/** Title and text of a banner in the current language. */
export function bannerText(t: Dict, b: Banner): { title: string; text: string } {
  const partial = b.context === "write" ? t.partial : b.context === "erase" ? t.partialErase : "";
  if (b.kind === "notice") {
    const title = { write: t.cancelledWrite, read: t.cancelledRead, erase: t.cancelledErase }[b.context as "write"] ?? t.cancelled;
    return { title, text: b.context === "read" ? t.notSaved : partial };
  }
  switch (b.kind) {
    case "Disconnected":
      return b.context === "write"
        ? { title: t.errDisconnected, text: `${t.errDisconnectedWrite} ${t.partial} ${t.errDisconnectedWriteTail}` }
        : { title: t.errDisconnected, text: b.context === "erase" ? `${t.errDisconnectedText} ${t.partialErase}` : t.errDisconnectedText };
    case "CoreNotAccessible":
      return { title: t.errCore, text: t.errCoreText };
    case "FlashProtected":
      return { title: t.errProtected, text: t.errProtectedText };
    case "Busy":
      return { title: t.errBusy, text: t.errBusyText };
    case "Unsupported":
      return { title: t.errUnsupported, text: t.errUnsupportedText };
    default: {
      const title = { write: t.errWrite, read: t.errRead, erase: t.errErase }[b.context as "write"] ?? t.errOther;
      return { title, text: [t.errOtherText, partial].filter(Boolean).join(" ") };
    }
  }
}

interface Props {
  banner: Banner;
  onAction: (a: BannerAction) => void;
  onDismiss: () => void;
  /** Actions need a connected device. */
  actionsDisabled?: boolean;
}

// TODO(design): there are no error screens in the mockups; this inline banner
// follows the cards of the device screen. The backend message is under «Подробнее».
export function ErrorBanner({ banner, onAction, onDismiss, actionsDisabled }: Props) {
  const { t } = useT();
  const [open, setOpen] = useState(false);
  const notice = banner.kind === "notice";
  const { title, text: defaultText } = bannerText(t, banner);
  const text = banner.note ?? defaultText;
  const details = !notice && banner.message !== "" && banner.note === undefined;
  // Two hints do not fit next to the text: put them under it.
  const stacked = banner.actions.length > 1;
  const buttons = banner.actions.map((a) => (
    <Button key={a} style={{ height: 26 }} disabled={actionsDisabled} onClick={() => onAction(a)}>
      {actionLabel(t, a)}
    </Button>
  ));
  return (
    <div
      className="card"
      role={notice ? "status" : "alert"}
      style={{
        padding: "10px 10px 10px 14px",
        display: "flex",
        alignItems: stacked || open ? "flex-start" : "center",
        gap: 12,
        fontSize: 12.5,
        boxShadow: notice ? "var(--shadow)" : "var(--shadow), inset 0 0 0 1px var(--warnFill)",
      }}
    >
      <span
        style={{
          width: 28,
          height: 28,
          borderRadius: 7,
          flex: "none",
          display: "grid",
          placeItems: "center",
          background: notice ? "var(--fill)" : "var(--warnSoft)",
          color: notice ? "var(--text2)" : "var(--warn)",
        }}
      >
        <WarnIcon size={16} strokeWidth={1.8} />
      </span>
      <span style={{ flex: 1, lineHeight: 1.45, userSelect: "text", minWidth: 0 }}>
        <b>{title}.</b> {text}
        {details && (
          <>
            {" "}
            <button
              type="button"
              aria-expanded={open}
              onClick={() => setOpen((v) => !v)}
              style={{ border: 0, padding: 0, background: "none", color: "var(--accentText)", font: "inherit", cursor: "pointer" }}
            >
              {t.details} {open ? "▴" : "▾"}
            </button>
            {open && (
              <span className="mono" style={{ display: "block", marginTop: 6, fontSize: 11.5, color: "var(--text2)", wordBreak: "break-word" }}>
                {banner.message}
              </span>
            )}
          </>
        )}
        {stacked && <span style={{ display: "flex", gap: 8, marginTop: 8 }}>{buttons}</span>}
      </span>
      {!stacked && buttons}
      <button className="icon-btn" style={{ width: 26, height: 26 }} aria-label={t.hide} onClick={onDismiss}>
        <svg width="10" height="10" viewBox="0 0 10 10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
          <path d="m1.5 1.5 7 7M8.5 1.5l-7 7" />
        </svg>
      </button>
    </div>
  );
}
