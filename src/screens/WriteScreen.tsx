import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { ConfirmSheet } from "../components/ConfirmSheet";
import { describeRuns, FlashMap } from "../components/FlashMap";
import { Footer, Spacer } from "../components/Footer";
import { estimateWrite } from "../domain/estimate";
import { fmtClock, fmtRange } from "../domain/format";
import { planWrite, sectorRuns } from "../domain/geometry";
import { parseNumber, writeAddrHint } from "../domain/validate";
import { useT } from "../i18n";
import { isBusy } from "../state/machine";
import { BannerSlot, contentStyle, DeviceChip, screenStyle, useApp } from "./shared";

const HINT_COLOR = { ok: "var(--ok)", warn: "var(--warn)", error: "var(--warn)" } as const;

export function WriteScreen() {
  const { s, ctl } = useApp();
  const tr = useT();
  const { t, f: fm } = tr;
  const g = s.session?.target.geometry ?? null;
  const f = s.writeForm;
  const file = f.file;
  const addr = parseNumber(f.addrText);
  const res = g && file && addr !== null ? planWrite(g, addr, file.size) : null;
  const plan = res?.ok ? res.plan : null;
  const hint = g ? writeAddrHint(tr, g, f.addrText, file?.size ?? null) : null;
  const { verify, resetAfter } = s.settings;
  const est = plan ? estimateWrite(plan, verify) : null;
  const canWrite = !!plan && !isBusy(s) && s.screen === "write";
  const dispatch = ctl.store.dispatch;
  const fileInfo = file ? t.fileMeta(fm.size(file.size), fm.int(file.size), fmtClock(file.modifiedAt)) : t.chooseBin;

  return (
    <div style={screenStyle}>
      <div style={{ ...contentStyle, display: "flex", flexDirection: "column", gap: 16, padding: "22px 28px" }}>
        <h2 style={{ margin: 0, fontSize: 18, fontWeight: 600 }}>{t.writeTitle}</h2>
        <BannerSlot />
        <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 16 }}>
          <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
            <section className="card" style={{ padding: "14px 16px", display: "flex", alignItems: "center", gap: 12 }}>
              <div
                style={{
                  width: 40,
                  height: 48,
                  borderRadius: 6,
                  background: "var(--accentSoft)",
                  color: "var(--accentText)",
                  display: "grid",
                  placeItems: "center",
                  flex: "none",
                }}
              >
                <span className="mono" style={{ fontSize: 10, fontWeight: 700 }}>
                  .BIN
                </span>
              </div>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 13.5, fontWeight: 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                  {file ? file.name : t.noFileChosen}
                </div>
                <div
                  title={fileInfo}
                  style={{ fontSize: 12, color: "var(--text2)", marginTop: 3, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}
                >
                  {fileInfo}
                </div>
              </div>
              <Button onClick={() => void ctl.pickFile()} disabled={isBusy(s)}>
                {t.choose}
              </Button>
            </section>
            <p style={{ margin: "-6px 0 0", fontSize: 11.5, color: "var(--text2)" }}>{t.dropHint}</p>
            <div>
              <label className="label" htmlFor="addr">
                {t.addr}
              </label>
              <input
                id="addr"
                className={`field mono${hint?.tone === "error" ? " invalid" : ""}`}
                value={f.addrText}
                spellCheck={false}
                autoComplete="off"
                aria-describedby="addr-hint"
                aria-invalid={hint?.tone === "error"}
                onChange={(e) => dispatch({ type: "addrChanged", text: e.currentTarget.value })}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && canWrite) void ctl.requestWrite();
                }}
              />
              {hint && (
                <div id="addr-hint" style={{ fontSize: 11.5, color: HINT_COLOR[hint.tone], marginTop: 6 }}>
                  {hint.text}
                </div>
              )}
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 10, marginTop: 2 }}>
              <Checkbox checked={verify} onChange={() => dispatch({ type: "settingsChanged", patch: { verify: !verify } })}>
                {t.verify}
              </Checkbox>
              <Checkbox checked={resetAfter} onChange={() => dispatch({ type: "settingsChanged", patch: { resetAfter: !resetAfter } })}>
                {t.resetAfter}
              </Checkbox>
              <Checkbox
                checked={!!s.quick?.onFileChange}
                disabled={!s.session?.target.uid}
                onChange={() => dispatch({ type: "quickToggled", option: "onFileChange" })}
              >
                {t.watch}
              </Checkbox>
            </div>
          </div>
          {g ? (
            <FlashMap geometry={g} plan={plan}>
              <div style={{ fontSize: 12, color: "var(--text2)" }}>
                {est
                  ? t.estimate(
                      Math.max(1, Math.round(est.total)),
                      fm.seconds(est.erasing),
                      fm.seconds(est.programming),
                      verify ? fm.seconds(est.verifying) : null,
                    )
                  : t.estimateNoFile}
              </div>
            </FlashMap>
          ) : (
            // TODO(design): chip without a flash driver (Error::Unsupported).
            <section className="card" style={{ padding: "14px 16px", fontSize: 12.5, color: "var(--text2)" }}>
              {t.writeUnsupported}
            </section>
          )}
        </div>
      </div>
      <Footer>
        <DeviceChip />
        <Spacer />
        <Button onClick={() => dispatch({ type: "navigate", to: "device" })}>{t.back}</Button>
        <Button primary disabled={!canWrite} onClick={() => void ctl.requestWrite()}>
          {t.eraseWrite}
        </Button>
      </Footer>
      {s.screen === "confirm" && <Confirm />}
    </div>
  );
}

function Confirm() {
  const { s, ctl } = useApp();
  const { t, f } = useT();
  const c = s.confirm;
  const g = s.session?.target.geometry;
  if (!c || !g) return null;
  const sectors = c.pending.plan.sectors;
  const runs = sectorRuns(g, sectors);
  const n = sectors.length;
  const what = describeRuns(t, runs, true);
  const first = runs[0];
  const last = runs[runs.length - 1];
  return (
    <ConfirmSheet
      title={t.cfTitle(n)}
      confirmLabel={t.eraseWrite}
      onConfirm={() => ctl.confirmWrite()}
      onCancel={() => ctl.cancelConfirm()}
    >
      {what.charAt(0).toUpperCase() + what.slice(1)} (
      <span className="mono">{fmtRange(first.addr, last.end - first.addr)}</span>, {f.size(n * g.sectorSize)}) {t.cf2(n)}
    </ConfirmSheet>
  );
}
