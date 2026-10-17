import { Button } from "../components/Button";
import { Footer } from "../components/Footer";
import { ProbeOffIcon } from "../components/Icons";
import { KeyValue } from "../components/KeyValue";
import { probeModelName } from "../domain/chipNames";
import { fmtHex16 } from "../domain/format";
import { DRIVER_PAGES } from "../backend/types";
import { useT } from "../i18n";
import { BannerSlot, screenStyle, useApp } from "./shared";

/**
 * The probe is on USB but not opened by the app (after start, «Отключить», or
 * a failed connect). Data from the USB descriptor only. `blocked`: the last
 * connect found it claimed by another program, or (Windows) not on WinUSB.
 */
export function ReleasedScreen() {
  const { s, ctl } = useApp();
  const { t } = useT();
  const p = s.probe;
  if (!p) return null;
  const model = probeModelName(p.descriptor.model);
  const usb = `${fmtHex16(p.descriptor.vid)}:${fmtHex16(p.descriptor.pid)}`.toLowerCase();
  const blocked = s.blocked?.reason ?? null;
  const title = blocked === "busy" ? t.busyTitle : blocked === "driver" ? t.driverTitle : t.relTitle;
  // The plain state has no explanation (Main.dc.html); the variants explain what to do.
  const text = blocked === "busy" ? t.busyText(model) : blocked === "driver" ? t.driverText(model) : null;
  const link = (label: string, url: string) => (
    <button
      type="button"
      onClick={() => void ctl.backend.openDriverPage(url)}
      style={{ border: 0, padding: 0, background: "none", color: "var(--accentText)", font: "inherit", fontSize: 12.5, cursor: "pointer" }}
    >
      {label} ↗
    </button>
  );
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
          minHeight: 0,
          overflow: "auto",
        }}
      >
        <div
          style={{
            width: 96,
            height: 96,
            borderRadius: "50%",
            background: blocked ? "var(--warnSoft)" : "var(--fill)",
            color: blocked ? "var(--warn)" : "var(--text2)",
            display: "grid",
            placeItems: "center",
            marginBottom: 6,
            flex: "none",
          }}
        >
          <ProbeOffIcon />
        </div>
        {/* TODO(design): the busy and driver variants are not drawn; same layout, other text. */}
        <h1 style={{ margin: 0, fontSize: 22, fontWeight: 600, letterSpacing: "-0.01em" }}>{title}</h1>
        {text && (
          <p style={{ margin: 0, maxWidth: 460, textAlign: "center", color: "var(--text2)", fontSize: 13.5, lineHeight: 1.5 }}>{text}</p>
        )}
        {blocked === "driver" && (
          <div style={{ display: "flex", gap: 16 }}>
            {link(t.driverStLink, DRIVER_PAGES.stlink)}
            {link(t.driverZadig, DRIVER_PAGES.zadig)}
          </div>
        )}
        <Button
          primary
          style={{ height: 34, padding: "0 20px", fontSize: 13.5, marginTop: 6 }}
          disabled={s.connecting}
          onClick={() => void ctl.connect()}
        >
          {s.connecting && <span className="spin sm" aria-hidden="true" style={{ borderColor: "rgba(255,255,255,.35)", borderTopColor: "#fff" }} />}
          {s.connecting ? t.connecting : blocked ? t.retry : t.connect}
        </Button>
        {s.banner && (
          <div style={{ maxWidth: 560, width: "100%" }}>
            <BannerSlot />
          </div>
        )}
        <section className="card" style={{ marginTop: 12, padding: "6px 16px", width: 380 }}>
          <KeyValue k={t.programmer}>
            {model} · <span className="mono">{usb}</span>
          </KeyValue>
          <KeyValue k={t.serial} mono>
            {p.descriptor.serial || "—"}
          </KeyValue>
        </section>
      </div>
      <Footer>
        <span className="chip">
          <span className="led off" />
          {t.chipReleased(model)}
        </span>
      </Footer>
    </div>
  );
}
