import type { Banner, BannerAction } from "../state/machine";
import { Button } from "./Button";
import { WarnIcon } from "./Icons";

const LABELS: Record<BannerAction, string> = {
  retryWrite: "Повторить запись",
  retryRead: "Повторить",
  retryIdentify: "Повторить",
  retryReset: "Повторить",
  connectUnderReset: "Подключение со сбросом",
  keepDebugLowPower: "Держать отладку в low-power",
};

interface Props {
  banner: Banner;
  onAction: (a: BannerAction) => void;
  onDismiss: () => void;
  /** Actions need a connected device. */
  actionsDisabled?: boolean;
}

// TODO(design): there are no error screens in the mockups; this inline banner
// follows the «Автозапись включена» card on the empty screen.
export function ErrorBanner({ banner, onAction, onDismiss, actionsDisabled }: Props) {
  const notice = banner.kind === "notice";
  return (
    <div
      className="card"
      role={notice ? "status" : "alert"}
      style={{
        padding: "10px 10px 10px 14px",
        display: "flex",
        alignItems: "center",
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
      <span style={{ flex: 1, lineHeight: 1.45, userSelect: "text" }}>
        <b>{banner.title}.</b> {banner.text}
      </span>
      {banner.actions.map((a) => (
        <Button key={a} style={{ height: 26 }} disabled={actionsDisabled} onClick={() => onAction(a)}>
          {LABELS[a]}
        </Button>
      ))}
      <button className="icon-btn" style={{ width: 26, height: 26 }} aria-label="Скрыть" onClick={onDismiss}>
        <svg width="10" height="10" viewBox="0 0 10 10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
          <path d="m1.5 1.5 7 7M8.5 1.5l-7 7" />
        </svg>
      </button>
    </div>
  );
}
