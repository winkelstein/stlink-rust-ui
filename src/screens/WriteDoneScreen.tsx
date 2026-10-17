import { Button } from "../components/Button";
import { Footer, Spacer } from "../components/Footer";
import { CheckLargeIcon } from "../components/Icons";
import { KeyValue } from "../components/KeyValue";
import { fmtHex32, fmtRange, fmtSeconds, fmtSize, plural, SECTOR_FORMS } from "../domain/format";
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

export function WriteDoneScreen() {
  const { s, ctl, quickKey } = useApp();
  const w = s.writeResult;
  if (!w) return null;
  const { result, pending } = w;
  const n = result.plan.sectors.length;
  const sectorSize = s.session?.target.geometry?.sectorSize ?? 0;
  return (
    <div style={screenStyle}>
      <div
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          gap: 12,
          padding: 32,
        }}
      >
        <DoneMark />
        <h1 style={{ margin: 0, fontSize: 22, fontWeight: 600 }}>
          {result.verified ? "Прошивка записана и проверена" : "Прошивка записана"}
        </h1>
        <p style={{ margin: 0, color: "var(--text2)", fontSize: 13.5 }}>
          {pending.file.name} · {fmtSize(result.plan.len)} за {fmtSeconds(result.elapsedMs / 1000)} ·{" "}
          {result.resetDone ? "МК перезапущен" : "ядро остановлено"}
        </p>
        <section className="card" style={{ marginTop: 10, padding: "6px 18px", width: 380 }}>
          <KeyValue k="Область" mono>
            {fmtRange(result.plan.addr, result.plan.len)}
          </KeyValue>
          <KeyValue k="Стёрто">
            {n} {plural(n, SECTOR_FORMS)} · {fmtSize(n * sectorSize)}
          </KeyValue>
          <KeyValue k="CRC32" mono>
            {fmtHex32(result.crc32)}
          </KeyValue>
        </section>
      </div>
      <Footer>
        <DeviceChip />
        <Spacer />
        <Button onClick={() => ctl.store.dispatch({ type: "navigate", to: "device" })}>К устройству</Button>
        <Button primary kbd={quickKey} disabled={isBusy(s) || !s.quick?.file} onClick={() => void ctl.quickWrite("quick")}>
          Записать снова
        </Button>
      </Footer>
    </div>
  );
}
