import { Button } from "../components/Button";
import { Footer } from "../components/Footer";
import { ProbeLargeIcon } from "../components/Icons";
import { BannerSlot, screenStyle, useApp } from "./shared";

export function EmptyScreen() {
  const { s, ctl } = useApp();
  const armed = s.armed?.settings.onConnect && s.armed.settings.file ? s.armed.settings : null;
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
        <h1 style={{ margin: 0, fontSize: 22, fontWeight: 600, letterSpacing: "-0.01em" }}>Подключите программатор</h1>
        <p style={{ margin: 0, maxWidth: 420, textAlign: "center", color: "var(--text2)", fontSize: 13.5, lineHeight: 1.5 }}>
          ST-Link по USB или плату в режиме DFU (BOOT0 = 1). Устройство появится здесь автоматически.
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
        {armed?.file && (
          <div
            className="card"
            style={{
              marginTop: s.banner ? 0 : 18,
              padding: "10px 10px 10px 14px",
              display: "flex",
              alignItems: "center",
              gap: 12,
              fontSize: 12.5,
              maxWidth: 560,
            }}
          >
            <span className="led" style={{ background: "var(--accent)", boxShadow: "0 0 0 3px var(--accentSoft)" }} />
            <span style={{ flex: 1, lineHeight: 1.45 }}>
              Автозапись включена: <b>{armed.file.name}</b> запишется сразу после подключения этого же{" "}
              {armed.chipName ?? "МК"}
            </span>
            <Button style={{ height: 26 }} onClick={() => ctl.store.dispatch({ type: "disarm" })}>
              Выключить
            </Button>
          </div>
        )}
      </div>
      <Footer>
        <span className="spin sm" aria-hidden="true" />
        <span style={{ fontSize: 12, color: "var(--text2)" }}>Ожидание устройства…</span>
      </Footer>
    </div>
  );
}
