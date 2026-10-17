import { Footer } from "../components/Footer";
import { ProbeLargeIcon } from "../components/Icons";
import { useT } from "../i18n";
import { BannerSlot, screenStyle, useApp } from "./shared";

export function EmptyScreen() {
  const { s } = useApp();
  const { t } = useT();
  return (
    <div style={screenStyle}>
      <div
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          gap: 14,
          padding: 32,
        }}
      >
        <div style={{ position: "relative", width: 112, height: 112, display: "grid", placeItems: "center", marginBottom: 8 }}>
          <span className="pulse" aria-hidden="true" />
          <div
            style={{
              width: 112,
              height: 112,
              borderRadius: "50%",
              background: "var(--accentSoft)",
              color: "var(--accentText)",
              display: "grid",
              placeItems: "center",
            }}
          >
            <ProbeLargeIcon />
          </div>
        </div>
        <h1 style={{ margin: 0, fontSize: 22, fontWeight: 600, letterSpacing: "-0.01em" }}>{t.emptyTitle}</h1>
        <p style={{ margin: 0, maxWidth: 440, textAlign: "center", color: "var(--text2)", fontSize: 13.5, lineHeight: 1.5 }}>
          {t.emptyText}
        </p>
        <div style={{ display: "flex", gap: 8, marginTop: 6 }}>
          <span className="chip">ST-Link V2 · V2-1 · V3</span>
          <span className="chip">STM32 USB DFU</span>
        </div>
        {s.banner && (
          <div style={{ marginTop: 18, maxWidth: 560, width: "100%" }}>
            <BannerSlot />
          </div>
        )}
      </div>
      <Footer>
        <span className="spin sm" aria-hidden="true" />
        <span style={{ fontSize: 12, color: "var(--text2)" }}>{t.waiting}</span>
      </Footer>
    </div>
  );
}
