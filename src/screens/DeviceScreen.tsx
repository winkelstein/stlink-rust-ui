import type { ReactNode } from "react";
import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { ConfirmSheet } from "../components/ConfirmSheet";
import { Footer, Spacer } from "../components/Footer";
import { ChipIcon, EraseIcon, ProbeIcon, ReadIcon, RebootIcon, ReleaseIcon, WriteIcon } from "../components/Icons";
import { KeyValue } from "../components/KeyValue";
import { chipFullName, coreName, probeModelName } from "../domain/chipNames";
import { fmtHex16, fmtHex32, fmtRange, fmtUid } from "../domain/format";
import { bankCount, sectorsPerBank } from "../domain/geometry";
import { useT } from "../i18n";
import { isBusy } from "../state/machine";
import { BannerSlot, contentStyle, screenStyle, useApp, useNow } from "./shared";

const RDP_COLOR = { Level0: "var(--ok)", Level1: "var(--warn)", Level2: "var(--warn)" } as const;

export function DeviceScreen() {
  const { s, ctl } = useApp();
  const { t, f } = useT();
  const busy = isBusy(s);
  const ready = !!s.session && !busy && s.screen === "device";
  const g = s.session?.target.geometry ?? null;
  const model = s.probe ? probeModelName(s.probe.descriptor.model) : "ST-Link";
  return (
    <div style={screenStyle}>
      <div style={{ ...contentStyle, display: "flex", flexDirection: "column", gap: 14, padding: "18px 28px" }}>
        <BannerSlot />
        <QuickWriteCard />
        <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 16 }}>
          <ProbeCard />
          <TargetCard />
        </div>
        <div>
          <div className="eyebrow" style={{ marginBottom: 8 }}>
            {t.other}
          </div>
          <div style={{ display: "flex", gap: 12 }}>
            <Tile
              icon={<WriteIcon />}
              title={t.tileWrite}
              sub={t.tileWriteSub}
              disabled={!ready || !g}
              hint={g ? undefined : t.noFlashDriver}
              onClick={() => ctl.store.dispatch({ type: "navigate", to: "write" })}
            />
            <Tile
              icon={<ReadIcon />}
              title={t.tileRead}
              sub={t.tileReadSub}
              disabled={!ready}
              onClick={() => ctl.store.dispatch({ type: "navigate", to: "read" })}
            />
            <Tile
              danger
              icon={<EraseIcon />}
              title={t.tileErase}
              sub={t.tileEraseSub(f.size(g?.size ?? s.session?.target.flashSize ?? 0))}
              disabled={!ready || !g}
              hint={g ? undefined : t.noFlashDriver}
              onClick={() => ctl.requestErase()}
            />
          </div>
        </div>
      </div>
      <Footer>
        <span className="chip">
          <span className="led" />
          {t.chipConnected(model)}
        </span>
        <Button title={t.releaseHint} disabled={busy} onClick={() => void ctl.disconnect()}>
          <ReleaseIcon />
          {t.release}
        </Button>
        <Spacer />
        <Button title={t.rebootHint} disabled={!ready} onClick={() => void ctl.resetMcu()}>
          {s.resetting ? <span className="spin sm" aria-hidden="true" /> : <RebootIcon />}
          {t.reboot}
        </Button>
      </Footer>
      {s.screen === "eraseConfirm" && <EraseConfirm />}
    </div>
  );
}

function EraseConfirm() {
  const { s, ctl } = useApp();
  const { t, f } = useT();
  const g = s.session?.target.geometry;
  if (!g) return null;
  return (
    <ConfirmSheet
      danger
      width={460}
      title={t.eraseCfTitle}
      confirmLabel={t.eraseAllBtn}
      onConfirm={() => ctl.confirmErase()}
      onCancel={() => ctl.cancelConfirm()}
    >
      {t.eraseCf1(bankCount(g))} (<span className="mono">{fmtRange(g.base, g.size)}</span>, {f.size(g.size)}) {t.eraseCf2}
    </ConfirmSheet>
  );
}

function Tile(props: {
  icon: ReactNode;
  title: string;
  sub: string;
  disabled: boolean;
  danger?: boolean;
  hint?: string;
  onClick: () => void;
}) {
  return (
    <button className={props.danger ? "tile danger" : "tile"} disabled={props.disabled} title={props.hint} onClick={props.onClick}>
      <span className="ic">{props.icon}</span>
      <span style={{ flex: 1, minWidth: 0 }}>
        <span style={{ display: "block", fontSize: 13.5, fontWeight: 600 }}>{props.title}</span>
        <span style={{ display: "block", fontSize: 11.5, color: "var(--text2)", marginTop: 3 }}>{props.sub}</span>
      </span>
    </button>
  );
}

function QuickWriteCard() {
  const { s, ctl, quickKey } = useApp();
  const { t, f } = useT();
  const now = useNow(30_000, true);
  const q = s.quick;
  const file = q?.file ?? null;
  const ready = !!s.session && !isBusy(s) && !!s.session.target.geometry;
  const chip = q?.onFileChange ? t.watching : file ? t.changedAgo(f.ago(file.modifiedAt, now)) : t.noFile;
  return (
    <section
      className="card"
      aria-label={t.quick}
      style={{
        padding: "12px 14px",
        display: "flex",
        alignItems: "center",
        gap: 14,
        boxShadow: "var(--shadow), inset 0 0 0 1px var(--accentSoft)",
      }}
    >
      <div
        style={{
          width: 34,
          height: 40,
          borderRadius: 6,
          background: "var(--accentSoft)",
          color: "var(--accentText)",
          display: "grid",
          placeItems: "center",
          flex: "none",
        }}
      >
        <span className="mono" style={{ fontSize: 9, fontWeight: 700 }}>
          .BIN
        </span>
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span className="eyebrow">{t.quick}</span>
          <span
            className="chip"
            style={{ height: 18, padding: "0 7px", fontSize: 10.5, background: "var(--accentSoft)", color: "var(--accentText)" }}
          >
            {chip}
          </span>
        </div>
        <div style={{ fontSize: 13.5, fontWeight: 600, marginTop: 3, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
          {file ? (
            <>
              {file.name}{" "}
              <span className="mono" style={{ fontWeight: 400, fontSize: 12, color: "var(--text2)" }}>
                → {fmtHex32(q?.addr ?? 0)}
              </span>
            </>
          ) : (
            <span style={{ fontWeight: 400, color: "var(--text2)" }}>{t.quickEmpty}</span>
          )}
        </div>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 7, flex: "none" }}>
        <Checkbox
          style={{ fontSize: 12.5 }}
          checked={!!q?.onFileChange}
          disabled={!file}
          onChange={() => ctl.store.dispatch({ type: "quickToggled", option: "onFileChange" })}
        >
          {t.onChange}
        </Checkbox>
      </div>
      <Button primary style={{ flex: "none" }} kbd={quickKey} disabled={!ready || !file} onClick={() => void ctl.quickWrite("quick")}>
        {t.flashBtn}
      </Button>
    </section>
  );
}

function CardHead(props: { icon: ReactNode; accent?: boolean; eyebrow: string; title: string; right?: ReactNode }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 8 }}>
      <div
        style={{
          width: 36,
          height: 36,
          borderRadius: 8,
          background: props.accent ? "var(--accentSoft)" : "var(--fill)",
          color: props.accent ? "var(--accentText)" : "var(--text2)",
          display: "grid",
          placeItems: "center",
        }}
      >
        {props.icon}
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div className="eyebrow">{props.eyebrow}</div>
        <div style={{ fontSize: 15, fontWeight: 600, marginTop: 2 }}>{props.title}</div>
      </div>
      {props.right}
    </div>
  );
}

function ProbeCard() {
  const { s } = useApp();
  const { t, f } = useT();
  const p = s.session?.probe;
  if (!p) return null;
  const khz = s.session?.connection.swdKhz ?? s.settings.swdKhz;
  return (
    <section className="card" style={{ padding: "12px 18px" }}>
      <CardHead
        icon={<ProbeIcon />}
        eyebrow={t.programmer}
        title={probeModelName(p.descriptor.model)}
        right={
          <span className="chip" style={{ height: 22, fontSize: 11 }}>
            <span className="led" />
            {t.connected}
          </span>
        }
      />
      <KeyValue k="USB" mono>
        {fmtHex16(p.descriptor.vid).toLowerCase()}:{fmtHex16(p.descriptor.pid).toLowerCase()}
      </KeyValue>
      <KeyValue k={t.firmware} mono>
        {p.version?.display ?? "—"}
      </KeyValue>
      <KeyValue k={t.serial} mono>
        {p.descriptor.serial}
      </KeyValue>
      <KeyValue k={t.voltage}>{p.targetVoltage === null ? "—" : f.voltage(p.targetVoltage)}</KeyValue>
      <KeyValue k={t.swd}>{f.freq(khz)}</KeyValue>
    </section>
  );
}

function TargetCard() {
  const { s } = useApp();
  const { t, f } = useT();
  const tg = s.session?.target;
  if (!tg) {
    // TODO(design): MCU card while connecting.
    return (
      <section className="card" style={{ padding: "12px 18px" }}>
        <CardHead icon={<ChipIcon />} accent eyebrow={t.mcu} title={s.connecting ? t.connecting : t.mcuNoLink} />
        <div style={{ display: "flex", alignItems: "center", gap: 10, fontSize: 12.5, color: "var(--text2)", padding: "5px 0" }}>
          {s.connecting ? (
            <>
              <span className="spin sm" aria-hidden="true" /> {t.mcuReading}
            </>
          ) : (
            t.mcuCheckWiring
          )}
        </div>
      </section>
    );
  }
  const g = tg.geometry;
  const flash =
    tg.flashSize === null ? "—" : g ? t.flashVal(f.size(tg.flashSize), bankCount(g), sectorsPerBank(g)) : f.size(tg.flashSize);
  return (
    <section className="card" style={{ padding: "12px 18px" }}>
      <CardHead icon={<ChipIcon />} accent eyebrow={t.mcu} title={chipFullName(tg, t)} />
      <KeyValue k={t.core}>
        {coreName(tg.core, t)} · <span className="mono">0x{tg.chipId.toString(16).toUpperCase().padStart(3, "0")}</span>
      </KeyValue>
      <KeyValue k="Flash">{flash}</KeyValue>
      <KeyValue k="SRAM">{tg.chip ? f.size(tg.chip.sramSize) : "—"}</KeyValue>
      <KeyValue k="UID" mono>
        {tg.uid ? fmtUid(tg.uid) : "—"}
      </KeyValue>
      <KeyValue k={t.rdp} valueStyle={tg.rdp ? { color: RDP_COLOR[tg.rdp], fontWeight: 600 } : undefined}>
        {tg.rdp ? t.rdpVal[tg.rdp] : "—"}
      </KeyValue>
    </section>
  );
}
