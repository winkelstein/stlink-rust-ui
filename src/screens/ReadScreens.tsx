import { Button } from "../components/Button";
import { Footer, Spacer } from "../components/Footer";
import { FolderIcon } from "../components/Icons";
import { KeyValue } from "../components/KeyValue";
import { ProgressBar } from "../components/ProgressBar";
import { Segmented } from "../components/Segmented";
import { estimateRead, remainingSeconds, SPEED } from "../domain/estimate";
import { fmtHex32, fmtRange } from "../domain/format";
import { checkReadRange, parseNumber, readCheckText } from "../domain/validate";
import { useT, type Dict } from "../i18n";
import { isBusy, type ReadRange } from "../state/machine";
import { availableRanges } from "../state/readRange";
import { DoneMark } from "./WriteDoneScreen";
import { BannerSlot, contentStyle, DeviceChip, screenStyle, useApp, useNow } from "./shared";

function rangeLabel(t: Dict, r: ReadRange): string {
  return { all: t.rangeAll, b1: t.bank1, b2: t.bank2, custom: t.rangeCustom }[r];
}

export function ReadScreen() {
  const { s, ctl } = useApp();
  const tr = useT();
  const { t, f: fm } = tr;
  const g = s.session?.target.geometry;
  const f = s.readForm;
  const locked = f.range !== "custom";
  const check = checkReadRange(f.startText, f.sizeText);
  const name = f.target?.name ?? ctl.readName();
  const dir = f.target?.dir ?? ctl.backend.defaultSaveTarget(name).dir;
  const ranges = g ? availableRanges(g) : (["custom"] as ReadRange[]);
  const dispatch = ctl.store.dispatch;
  // Locked size fields read `0x200000 · 2 МБ`.
  const shown = (key: "startText" | "sizeText") => {
    const v = parseNumber(f[key]);
    return locked && key === "sizeText" && v !== null ? `${f[key]} · ${fm.size(v)}` : f[key];
  };
  const field = (id: string, label: string, key: "startText" | "sizeText") => (
    <div>
      <label className="label" htmlFor={id}>
        {label}
      </label>
      <input
        id={id}
        className={`field mono${!locked && !check.ok ? " invalid" : ""}`}
        value={shown(key)}
        disabled={locked}
        spellCheck={false}
        autoComplete="off"
        onChange={(e) => dispatch({ type: "readFieldChanged", field: key, text: e.currentTarget.value })}
        onKeyDown={(e) => e.key === "Enter" && check.ok && ctl.startRead()}
      />
    </div>
  );
  return (
    <div style={screenStyle}>
      <div style={{ ...contentStyle, display: "flex", flexDirection: "column", gap: 18, padding: "22px 28px" }}>
        <h2 style={{ margin: 0, fontSize: 18, fontWeight: 600 }}>{t.readTitle}</h2>
        <BannerSlot />
        <section className="card" style={{ padding: 18, display: "flex", flexDirection: "column", gap: 16 }}>
          <div>
            <span className="label">{t.what}</span>
            <Segmented
              label={t.what}
              value={f.range}
              options={ranges.map((r) => ({ value: r, label: rangeLabel(t, r) }))}
              onChange={(r) => ctl.setReadRange(r)}
            />
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 16 }}>
            {field("r-start", t.start, "startText")}
            {field("r-size", t.size, "sizeText")}
          </div>
          {!locked && !check.ok && (
            <div style={{ fontSize: 11.5, color: "var(--warn)", marginTop: -6 }}>{readCheckText(tr, check.error)}</div>
          )}
        </section>
        <section className="card" style={{ padding: "14px 16px", display: "flex", alignItems: "center", gap: 12 }}>
          <div
            style={{
              width: 36,
              height: 36,
              borderRadius: 8,
              background: "var(--fill)",
              color: "var(--text2)",
              display: "grid",
              placeItems: "center",
              flex: "none",
            }}
          >
            <FolderIcon />
          </div>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="mono" style={{ fontSize: 13, fontWeight: 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
              {name}
            </div>
            <div style={{ fontSize: 12, color: "var(--text2)", marginTop: 3 }}>{dir}</div>
          </div>
          <Button onClick={() => void ctl.pickSaveTarget()}>{t.change}</Button>
        </section>
        <div style={{ fontSize: 12, color: "var(--text2)" }}>
          {check.ok
            ? t.readEstimate(Math.max(1, Math.round(estimateRead(check.size))), fm.size(check.size), fm.rate(SPEED.read))
            : t.enterRange}
        </div>
      </div>
      <Footer>
        <DeviceChip />
        <Spacer />
        <Button onClick={() => dispatch({ type: "navigate", to: "device" })}>{t.back}</Button>
        <Button primary disabled={!check.ok || isBusy(s)} onClick={() => ctl.startRead()}>
          {t.readBtn}
        </Button>
      </Footer>
    </div>
  );
}

export function ReadProgressScreen() {
  const { s, ctl } = useApp();
  const { t, f } = useT();
  const now = useNow(100);
  const op = s.op?.kind === "read" ? s.op : null;
  if (!op) return null;
  const p = op.progress;
  const frac = p.total > 0 ? p.done / p.total : 0;
  const elapsed = (now - op.startedAt) / 1000;
  const left = remainingSeconds(estimateRead(p.total), frac, elapsed);
  const speed = elapsed > 0.3 && p.done > 0 ? p.done / elapsed : SPEED.read;
  return (
    <div style={screenStyle}>
      <div style={{ flex: 1, display: "flex", flexDirection: "column", justifyContent: "center", gap: 12, padding: "24px 120px" }}>
        <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between" }}>
          <h2 style={{ margin: 0, fontSize: 18, fontWeight: 600 }}>{t.readingTitle}</h2>
          <span className="mono" style={{ fontSize: 22, fontWeight: 600 }}>
            {Math.round(frac * 100)}%
          </span>
        </div>
        <ProgressBar value={frac} large label={t.readingTitle} />
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12.5, color: "var(--text2)" }}>
          <span className="mono">{t.ofMb(f.mb(p.done), f.mb(p.total))}</span>
          <span>{t.rateLeft(f.rate(speed), Math.max(0, Math.round(left)))}</span>
        </div>
        <div style={{ marginTop: 14, fontSize: 12, color: "var(--text2)" }}>
          {t.toFile} <span className="mono">{op.job.target.name}</span> · {t.coreRuns}
        </div>
      </div>
      <Footer>
        <DeviceChip />
        <Spacer />
        <Button onClick={() => ctl.cancelOp()}>{t.abort}</Button>
      </Footer>
    </div>
  );
}

export function ReadDoneScreen() {
  const { s, ctl } = useApp();
  const { t, f } = useT();
  const r = s.readResult;
  if (!r) return null;
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
        <h1 style={{ margin: 0, fontSize: 22, fontWeight: 600 }}>{t.saved}</h1>
        <p className="mono" style={{ margin: 0, color: "var(--text2)", fontSize: 13 }}>
          {r.savedTo.dir}/{r.savedTo.name}
        </p>
        <section className="card" style={{ marginTop: 10, padding: "6px 18px", width: 380 }}>
          <KeyValue k={t.region} mono>
            {fmtRange(r.addr, r.size)}
          </KeyValue>
          <KeyValue k={t.size}>{t.sizeVal(f.size(r.size), f.int(r.size))}</KeyValue>
          <KeyValue k={t.time}>{f.seconds(r.elapsedMs / 1000)}</KeyValue>
          <KeyValue k="CRC32" mono>
            {fmtHex32(r.crc32)}
          </KeyValue>
        </section>
      </div>
      <Footer>
        <DeviceChip />
        <Spacer />
        <Button onClick={() => void ctl.reveal()}>{t.finder}</Button>
        <Button primary onClick={() => ctl.store.dispatch({ type: "navigate", to: "device" })}>
          {t.done}
        </Button>
      </Footer>
    </div>
  );
}
