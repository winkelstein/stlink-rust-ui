import { Button } from "../components/Button";
import { Footer, Spacer } from "../components/Footer";
import { CheckSmallIcon, WarnIcon } from "../components/Icons";
import { ProgressBar } from "../components/ProgressBar";
import { chipShortName } from "../domain/chipNames";
import { estimateBankErase, estimateWrite, remainingSeconds, WRITE_STAGES, writeFraction } from "../domain/estimate";
import { fmtHex32, fmtRange } from "../domain/format";
import { bankCount, bankSize, unitsInBank } from "../domain/geometry";
import { useT } from "../i18n";
import type { WriteStage } from "../backend/types";
import { screenStyle, useApp, useNow, useSpeed } from "./shared";

interface StageRow {
  key: string;
  label: string;
  detail: string;
  isDone: boolean;
  isActive: boolean;
  isPending: boolean;
  /** Fraction of the active stage. */
  frac: number;
}

/** Shared layout of the write and full-erase progress (Main.dc.html «Progress»). */
function ProgressLayout(props: {
  title: string;
  sub: string;
  total: number;
  stages: StageRow[];
  danger?: boolean;
  onCancel?: () => void;
}) {
  const { t } = useT();
  const pct = Math.round(props.total * 100);
  return (
    <div style={screenStyle}>
      <div style={{ flex: 1, display: "flex", flexDirection: "column", justifyContent: "center", gap: 22, padding: "24px 120px" }}>
        <div>
          <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between", gap: 16 }}>
            <h2 style={{ margin: 0, fontSize: 18, fontWeight: 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
              {props.title}
            </h2>
            <span className="mono" style={{ fontSize: 22, fontWeight: 600 }}>
              {pct}%
            </span>
          </div>
          <div style={{ fontSize: 12.5, color: "var(--text2)", margin: "4px 0 12px" }}>{props.sub}</div>
          <ProgressBar value={props.total} large danger={props.danger} label={t.progressLabel} />
        </div>
        <section className="card" style={{ padding: "4px 18px" }}>
          {props.stages.map((x) => (
            <div className="stage" key={x.key}>
              {x.isDone && (
                <span className="st-ic done">
                  <CheckSmallIcon />
                </span>
              )}
              {x.isActive && (
                <span className="st-ic">
                  <span className="spin" />
                </span>
              )}
              {x.isPending && <span className="st-ic pending" />}
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ display: "flex", justifyContent: "space-between", gap: 12 }}>
                  <span style={{ fontSize: 13.5, fontWeight: 600, color: x.isPending ? "var(--text2)" : "var(--text)" }}>{x.label}</span>
                  <span className="mono" style={{ fontSize: 12, color: "var(--text2)" }}>
                    {x.detail}
                  </span>
                </div>
                {x.isActive && <ProgressBar value={x.frac} danger={props.danger} style={{ marginTop: 8 }} label={x.label} />}
              </div>
            </div>
          ))}
        </section>
      </div>
      <Footer>
        <span style={{ fontSize: 12, color: "var(--warn)", display: "flex", alignItems: "center", gap: 6 }}>
          <WarnIcon size={14} strokeWidth={2} />
          {t.dontUnplug}
        </span>
        <Spacer />
        <Button disabled={!props.onCancel} onClick={props.onCancel}>
          {t.abort}
        </Button>
      </Footer>
    </div>
  );
}

export function WriteProgressScreen() {
  const { s, ctl } = useApp();
  const { t, f } = useT();
  const now = useNow(100);
  const op = s.op?.kind === "write" ? s.op : null;
  if (!op || !s.session) return null;
  const { plan, job, file } = op.pending;
  const p = op.progress;
  const speed = useSpeed();
  const unit = s.session.target.geometry?.unit ?? "sector";
  const est = estimateWrite(plan, job.verify, speed);
  const idx = WRITE_STAGES.indexOf(p.stage as WriteStage);
  const stageStart = (st: WriteStage) => op.stageStart[st] ?? op.startedAt;

  // Erase reports once per sector (~1 s): interpolate inside the current one.
  let frac = p.total > 0 ? p.done / p.total : 0;
  if (p.stage === "erasing" && p.total > 0 && p.done < p.total) {
    const next = plan.sectors[p.done];
    const perSector =
      p.done > 0 ? (op.lastProgressAt - stageStart("erasing")) / p.done : speed.eraseUnit(next?.size ?? 0) * 1000;
    frac = (p.done + Math.min(0.95, (now - op.lastProgressAt) / Math.max(1, perSector))) / p.total;
  }
  const total = writeFraction(est, p, frac);
  const left = remainingSeconds(est.total, total, (now - op.startedAt) / 1000);

  const stageSeconds = (st: WriteStage, i: number) => {
    const next = WRITE_STAGES[i + 1];
    const end = next && op.stageStart[next] !== undefined ? stageStart(next) : now;
    return (end - stageStart(st)) / 1000;
  };
  const rate = () => {
    const sec = (now - stageStart(p.stage as WriteStage)) / 1000;
    return f.rate(sec > 0.3 ? p.done / sec : speed.program);
  };
  const n = plan.sectors.length;
  const labels = [t.stErase, t.stProgram, t.stVerify];

  const stages: StageRow[] = WRITE_STAGES.map((st, i) => {
    const skipped = st === "verifying" && !job.verify;
    const isDone = i < idx;
    const isActive = i === idx;
    let detail = t.stWaiting;
    if (skipped) detail = t.stOff;
    else if (isDone) {
      const sec = f.seconds(stageSeconds(st, i));
      detail = [`${t.unitsN(n, unit)} · ${sec}`, `${f.size(plan.len)} · ${sec}`, `${t.stMatch} · ${sec}`][i];
    } else if (isActive) {
      const stPct = Math.round(frac * 100);
      detail = [
        t.unitOf(Math.min(n, p.done + 1), n, unit),
        t.kbOf(stPct, Math.round(p.done / 1024), Math.round(p.total / 1024), rate()),
        `${stPct} %`,
      ][i];
    }
    return {
      key: st,
      label: labels[i],
      detail,
      isDone,
      isActive: isActive && !skipped,
      isPending: !isDone && (!isActive || skipped),
      frac,
    };
  });

  return (
    <ProgressLayout
      title={t.writing(file.name)}
      sub={`${chipShortName(s.session.target)} · ${fmtHex32(plan.addr)} · ${t.left(f.seconds(left))}`}
      total={total}
      stages={stages}
      onCancel={() => ctl.cancelOp()}
    />
  );
}

/**
 * Full erase: one stage per bank. A bank erase reports nothing until it ends
 * (~5.5 s per MB), so the bar inside a bank fills on the time estimate (or the
 * measured time of the previous bank) and stops short of the end.
 */
export function EraseProgressScreen() {
  const { s, ctl } = useApp();
  const { t, f } = useT();
  const speed = useSpeed();
  const now = useNow(100);
  const op = s.op?.kind === "erase" ? s.op : null;
  const g = s.session?.target.geometry;
  if (!op || !s.session || !g) return null;
  const banks = Math.max(1, op.progress.total || bankCount(g));
  const done = op.progress.done;
  const measured = done > 0 ? (op.bankStart[done] - op.bankStart[0]) / done / 1000 : 0;
  const bank = Math.min(done, g.banks.length - 1);
  const perBank = measured > 0 ? measured : estimateBankErase(bankSize(g, bank), speed);
  const current = op.bankStart[done] ?? op.startedAt;
  const inBank = done < banks ? Math.min(0.97, (now - current) / 1000 / perBank) : 0;
  const total = Math.min(1, (done + inBank) / banks);
  const left = remainingSeconds(perBank * banks, total, (now - op.startedAt) / 1000);
  const perBankUnits = unitsInBank(g, bank);

  const stages: StageRow[] = Array.from({ length: banks }, (_, i) => {
    const isDone = i < done;
    const isActive = i === done;
    const sec = isDone ? (op.bankStart[i + 1] - op.bankStart[i]) / 1000 : 0;
    const detail = isDone
      ? `${f.size(bankSize(g, i))} · ${f.seconds(sec)}`
      : isActive
        ? t.unitOf(Math.min(perBankUnits, Math.floor(inBank * perBankUnits) + 1), perBankUnits, g.unit)
        : t.stWaiting;
    return { key: `b${i}`, label: t.bankN(i + 1), detail, isDone, isActive, isPending: i > done, frac: inBank };
  });

  return (
    <ProgressLayout
      danger
      title={t.erasing}
      sub={`${chipShortName(s.session.target)} · ${fmtRange(g.base, g.size)} · ${t.left(f.seconds(left))}`}
      total={total}
      stages={stages}
      // Cancelling takes effect between banks.
      onCancel={done < banks - 1 ? () => ctl.cancelOp() : undefined}
    />
  );
}
