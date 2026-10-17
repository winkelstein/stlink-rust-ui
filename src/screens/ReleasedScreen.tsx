import { Button } from "../components/Button";
import { Footer } from "../components/Footer";
import { ProbeOffIcon } from "../components/Icons";
import { KeyValue } from "../components/KeyValue";
import { probeModelName } from "../domain/chipNames";
import { fmtHex16 } from "../domain/format";
import { useT } from "../i18n";
import { BannerSlot, screenStyle, useApp } from "./shared";

/**
 * The probe is on USB but not opened by the app (after start, «Отключить», or
 * a failed connect). Data from the USB descriptor only. `busy`: the last
 * connect found it claimed by another program.
 */
export function ReleasedScreen() {
  const { s, ctl } = useApp();
  const { t } = useT();
  const p = s.probe;
  if (!p) return null;
  const model = probeModelName(p.descriptor.model);
  const usb = `${fmtHex16(p.descriptor.vid)}:${fmtHex16(p.descriptor.pid)}`.toLowerCase();
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
            background: s.busy ? "var(--warnSoft)" : "var(--fill)",
            color: s.busy ? "var(--warn)" : "var(--text2)",
            display: "grid",
            placeItems: "center",
            marginBottom: 6,
            flex: "none",
          }}
        >
          <ProbeOffIcon />
        </div>
        {/* TODO(design): the busy variant is not drawn; same layout, other text. */}
        <h1 style={{ margin: 0, fontSize: 22, fontWeight: 600, letterSpacing: "-0.01em" }}>{s.busy ? t.busyTitle : t.relTitle}</h1>
        <p style={{ margin: 0, maxWidth: 460, textAlign: "center", color: "var(--text2)", fontSize: 13.5, lineHeight: 1.5 }}>
          {s.busy ? t.busyText(model) : t.relText(model)}
        </p>
        <Button
          primary
          style={{ height: 34, padding: "0 20px", fontSize: 13.5, marginTop: 6 }}
          disabled={s.connecting}
          onClick={() => void ctl.connect()}
        >
          {s.connecting && <span className="spin sm" aria-hidden="true" style={{ borderColor: "rgba(255,255,255,.35)", borderTopColor: "#fff" }} />}
          {s.connecting ? t.connecting : s.busy ? t.retry : t.connect}
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
