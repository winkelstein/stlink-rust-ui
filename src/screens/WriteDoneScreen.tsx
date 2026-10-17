import { Button } from "../components/Button";
import { Footer, Spacer } from "../components/Footer";
import { CheckLargeIcon } from "../components/Icons";
import { KeyValue } from "../components/KeyValue";
import { fmtHex32, fmtRange } from "../domain/format";
import { bytesOf } from "../domain/geometry";
import { useT } from "../i18n";
import { isBusy } from "../state/machine";
import { DeviceChip, screenStyle, useApp } from "./shared";

export function DoneMark() {
  return (
    <div
      style={{
        width: 72,
        height: 72,
        borderRadius: "50%",
        background: "var(--okSoft)",
        color: "var(--ok)",
        display: "grid",
        placeItems: "center",
        marginBottom: 6,
      }}
    >
      <CheckLargeIcon />
    </div>
  );
}

const centered = {
  flex: 1,
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  justifyContent: "center",
  gap: 12,
  padding: 32,
} as const;

export function WriteDoneScreen() {
  const { s, ctl, quickKey } = useApp();
  const { t, f } = useT();
  const w = s.writeResult;
  if (!w) return null;
  const { result, pending } = w;
  const n = result.plan.sectors.length;
  const unit = s.session?.target.geometry?.unit ?? "sector";
  return (
    <div style={screenStyle}>
      <div style={centered}>
        <DoneMark />
        <h1 style={{ margin: 0, fontSize: 22, fontWeight: 600 }}>{result.verified ? t.doneTitle : t.doneTitleNoVerify}</h1>
        <p style={{ margin: 0, color: "var(--text2)", fontSize: 13.5 }}>
          {t.doneSub(pending.file.name, f.size(result.plan.len), f.seconds(result.elapsedMs / 1000), result.resetDone)}
        </p>
        <section className="card" style={{ marginTop: 10, padding: "6px 18px", width: 380 }}>
          <KeyValue k={t.region} mono>
            {fmtRange(result.plan.addr, result.plan.len)}
          </KeyValue>
          <KeyValue k={t.erased}>{t.erasedVal(n, f.size(bytesOf(result.plan.sectors)), unit)}</KeyValue>
          <KeyValue k="CRC32" mono>
            {fmtHex32(result.crc32)}
          </KeyValue>
        </section>
      </div>
      <Footer>
        <DeviceChip />
        <Spacer />
        <Button onClick={() => ctl.store.dispatch({ type: "navigate", to: "device" })}>{t.toDevice}</Button>
        <Button primary kbd={quickKey} disabled={isBusy(s) || !s.quick?.file} onClick={() => void ctl.quickWrite("quick")}>
          {t.again}
        </Button>
      </Footer>
    </div>
  );
}

export function EraseDoneScreen() {
  const { s, ctl } = useApp();
  const { t, f } = useT();
  const r = s.eraseResult;
  const g = s.session?.target.geometry;
  if (!r || !g) return null;
  return (
    <div style={screenStyle}>
      <div style={centered}>
        <DoneMark />
        <h1 style={{ margin: 0, fontSize: 22, fontWeight: 600 }}>{t.eraseDoneTitle}</h1>
        <p style={{ margin: 0, color: "var(--text2)", fontSize: 13.5 }}>{t.eraseDoneSub(f.size(r.size), f.seconds(r.elapsedMs / 1000))}</p>
        <section className="card" style={{ marginTop: 10, padding: "6px 18px", width: 380 }}>
          <KeyValue k={t.region} mono>
            {fmtRange(g.base, r.size || g.size)}
          </KeyValue>
          <KeyValue k={t.banks}>{t.banksVal(r.banks)}</KeyValue>
          <KeyValue k={t.mcuState}>{t.mcuHalted}</KeyValue>
        </section>
      </div>
      <Footer>
        <DeviceChip />
        <Spacer />
        <Button onClick={() => ctl.store.dispatch({ type: "navigate", to: "device" })}>{t.toDevice}</Button>
        <Button primary disabled={isBusy(s)} onClick={() => ctl.store.dispatch({ type: "navigate", to: "write" })}>
          {t.tileWrite}
        </Button>
      </Footer>
    </div>
  );
}
