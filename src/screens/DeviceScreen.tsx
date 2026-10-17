import type { ReactNode } from "react";
import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { Footer, Spacer } from "../components/Footer";
import { ChipIcon, ProbeIcon, ReadIcon, WriteIcon } from "../components/Icons";
import { KeyValue } from "../components/KeyValue";
import { StatusChip } from "../components/StatusChip";
import { chipFullName, coreName, probeModelName } from "../domain/chipNames";
import { fmtAgo, fmtFreq, fmtHex16, fmtHex32, fmtSize, fmtUid, fmtVoltage } from "../domain/format";
import { bankCount, sectorsPerBank } from "../domain/geometry";
import { isBusy } from "../state/machine";
import type { RdpLevel } from "../backend/types";
import { BannerSlot, contentStyle, screenStyle, useApp, useNow } from "./shared";

const RDP: Record<RdpLevel, { text: string; color: string }> = {
  Level0: { text: "RDP 0 · нет", color: "var(--ok)" },
  Level1: { text: "RDP 1 · чтение запрещено", color: "var(--warn)" },
  Level2: { text: "RDP 2 · отладка отключена", color: "var(--warn)" },
};

export function DeviceScreen() {
  const { s, ctl } = useApp();
  const busy = isBusy(s);
  const ready = !!s.session && !busy;
  return (
    <div style={screenStyle}>
      <div style={{ ...contentStyle, display: "flex", flexDirection: "column", gap: 16, padding: "20px 28px" }}>
        <BannerSlot />
        <QuickWriteCard />
        <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 16 }}>
          <ProbeCard />
          <TargetCard />
        </div>
        <div>
          <div className="eyebrow" style={{ marginBottom: 8 }}>
            Другое действие
          </div>
          <div style={{ display: "flex", gap: 16 }}>
            <Tile
              icon={<WriteIcon />}
              title="Записать прошивку"
              sub=".bin → flash · стирание и проверка"
              disabled={!ready || !s.session?.target.geometry}
              onClick={() => ctl.store.dispatch({ type: "navigate", to: "write" })}
            />
            <Tile
              icon={<ReadIcon />}
              title="Прочитать в файл"
              sub="flash → .bin · ядро не останавливается"
              disabled={!ready}
              onClick={() => ctl.store.dispatch({ type: "navigate", to: "read" })}
            />
          </div>
        </div>
      </div>
      <Footer>
        <StatusChip>1 устройство</StatusChip>
        <Spacer />
        <Button disabled={!ready} onClick={() => void ctl.resetMcu()}>
          {s.resetting && <span className="spin sm" aria-hidden="true" />}
          Сбросить МК
        </Button>
      </Footer>
    </div>
  );
}

function Tile(props: { icon: ReactNode; title: string; sub: string; disabled: boolean; onClick: () => void }) {
  return (
    <button className="tile" disabled={props.disabled} onClick={props.onClick}>
      <span className="ic">{props.icon}</span>
      <span style={{ flex: 1 }}>
        <span style={{ display: "block", fontSize: 14, fontWeight: 600 }}>{props.title}</span>
        <span style={{ display: "block", fontSize: 12, color: "var(--text2)", marginTop: 3 }}>{props.sub}</span>
      </span>
      <span aria-hidden="true" style={{ color: "var(--text3)", fontSize: 18 }}>
        ›
      </span>
    </button>
  );
}

function QuickWriteCard() {
  const { s, ctl, quickKey } = useApp();
  const now = useNow(30_000, true);
  const q = s.quick;
  const file = q?.file ?? null;
  const ready = !!s.session && !isBusy(s) && !!s.session.target.geometry;
  const chip = q?.onFileChange ? "слежу за файлом" : file ? `изменён ${fmtAgo(file.modifiedAt, now)}` : "нет файла";
  return (
    <section
      className="card"
      aria-label="Быстрая запись"
      style={{
        padding: "12px 14px 12px 14px",
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
          <span className="eyebrow">Быстрая запись</span>
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
            // TODO(design): quick write card without a remembered file.
            <span style={{ fontWeight: 400, color: "var(--text2)" }}>
              {s.session ? "Запишите прошивку один раз — файл и адрес запомнятся для этого МК" : "Подключение к МК…"}
            </span>
          )}
        </div>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 7, flex: "none" }}>
        <Checkbox
          style={{ fontSize: 12.5 }}
          checked={!!q?.onConnect}
          disabled={!file}
          onChange={() => ctl.store.dispatch({ type: "quickToggled", option: "onConnect" })}
        >
          При подключении платы
        </Checkbox>
        <Checkbox
          style={{ fontSize: 12.5 }}
          checked={!!q?.onFileChange}
          disabled={!file}
          onChange={() => ctl.store.dispatch({ type: "quickToggled", option: "onFileChange" })}
        >
          При изменении файла
        </Checkbox>
      </div>
      <Button primary style={{ flex: "none" }} kbd={quickKey} disabled={!ready || !file} onClick={() => void ctl.quickWrite("quick")}>
        Записать
      </Button>
    </section>
  );
}

function CardHead(props: { icon: ReactNode; accent?: boolean; eyebrow: string; title: string; right?: ReactNode }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 10 }}>
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
  const p = s.probe;
  if (!p) return null;
  const khz = s.session?.connection.swdKhz ?? s.connectOptions.swdKhz;
  return (
    <section className="card" style={{ padding: "14px 18px" }}>
      <CardHead
        icon={<ProbeIcon />}
        eyebrow="Программатор"
        title={probeModelName(p.descriptor.model)}
        right={
          <StatusChip style={{ height: 22, fontSize: 11 }}>Подключён</StatusChip>
        }
      />
      <KeyValue k="USB" mono>
        {fmtHex16(p.descriptor.vid).toLowerCase()}:{fmtHex16(p.descriptor.pid).toLowerCase()}
      </KeyValue>
      <KeyValue k="Прошивка" mono>
        {p.version.display}
      </KeyValue>
      <KeyValue k="Серийный номер" mono>
        {p.descriptor.serial}
      </KeyValue>
      <KeyValue k="Напряжение">{p.targetVoltage === null ? "—" : fmtVoltage(p.targetVoltage)}</KeyValue>
      <KeyValue k="Частота SWD">{fmtFreq(khz)}</KeyValue>
    </section>
  );
}

function TargetCard() {
  const { s } = useApp();
  const t = s.session?.target;
  if (!t) {
    // TODO(design): MCU card while connecting / after a failed connect.
    return (
      <section className="card" style={{ padding: "14px 18px" }}>
        <CardHead icon={<ChipIcon />} accent eyebrow="Микроконтроллер" title={s.identifying ? "Подключение…" : "Нет связи с МК"} />
        <div style={{ display: "flex", alignItems: "center", gap: 10, fontSize: 12.5, color: "var(--text2)", padding: "5px 0" }}>
          {s.identifying ? (
            <>
              <span className="spin sm" aria-hidden="true" /> Чтение IDCODE, CPUID и раскладки flash
            </>
          ) : (
            "Проверьте подключение SWDIO/SWCLK/GND и питание платы"
          )}
        </div>
      </section>
    );
  }
  const g = t.geometry;
  const flash =
    t.flashSize === null
      ? "—"
      : g
        ? `${fmtSize(t.flashSize)} · ${bankCount(g) > 1 ? `${bankCount(g)} банка по ${sectorsPerBank(g)} секторов` : `${sectorsPerBank(g)} секторов`}`
        : fmtSize(t.flashSize);
  const rdp = t.rdp ? RDP[t.rdp] : null;
  return (
    <section className="card" style={{ padding: "14px 18px" }}>
      <CardHead icon={<ChipIcon />} accent eyebrow="Микроконтроллер" title={chipFullName(t)} />
      <KeyValue k="Ядро">
        {coreName(t.core)} · <span className="mono">0x{t.chipId.toString(16).toUpperCase().padStart(3, "0")}</span>
      </KeyValue>
      <KeyValue k="Flash">{flash}</KeyValue>
      <KeyValue k="SRAM">{t.chip ? fmtSize(t.chip.sramSize) : "—"}</KeyValue>
      <KeyValue k="UID" mono>
        {t.uid ? fmtUid(t.uid) : "—"}
      </KeyValue>
      <KeyValue k="Защита чтения" valueStyle={rdp ? { color: rdp.color, fontWeight: 600 } : undefined}>
        {rdp?.text ?? "—"}
      </KeyValue>
    </section>
  );
}
