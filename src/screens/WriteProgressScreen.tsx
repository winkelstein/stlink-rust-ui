import { Button } from "../components/Button";
import { Footer, Spacer } from "../components/Footer";
import { CheckSmallIcon, WarnIcon } from "../components/Icons";
import { ProgressBar } from "../components/ProgressBar";
import { chipShortName } from "../domain/chipNames";
import { estimateWrite, remainingSeconds, SPEED, WRITE_STAGES, writeFraction } from "../domain/estimate";
import { fmtHex32, fmtSeconds, fmtSize, plural, SECTOR_FORMS } from "../domain/format";
import type { WriteStage } from "../backend/types";
import { screenStyle, useApp, useNow } from "./shared";

const LABELS: Record<WriteStage, string> = {
  erasing: "Стирание",
  programming: "Программирование",
  verifying: "Проверка",
};

export function WriteProgressScreen() {
  const { s, ctl } = useApp();
  const now = useNow(100);
  const op = s.op?.kind === "write" ? s.op : null;
  if (!op || !s.session) return null;
  const { plan, job, file } = op.pending;
  const p = op.progress;
  const est = estimateWrite(plan, job.verify);
  const idx = WRITE_STAGES.indexOf(p.stage as WriteStage);
  const stageStart = (st: WriteStage) => op.stageStart[st] ?? op.startedAt;

  // Erase reports once per sector (~1 s): interpolate inside the current one.
  let frac = p.total > 0 ? p.done / p.total : 0;
  if (p.stage === "erasing" && p.total > 0 && p.done < p.total) {
    const perSector =
      p.done > 0 ? (op.lastProgressAt - stageStart("erasing")) / p.done : SPEED.eraseSector * 1000;
    frac = (p.done + Math.min(0.95, (now - op.lastProgressAt) / Math.max(1, perSector))) / p.total;
  }
  const total = writeFraction(est, p, frac);
  const left = remainingSeconds(est.total, total, (now - op.startedAt) / 1000);
  const pct = Math.round(total * 100);

  const stageSeconds = (st: WriteStage, i: number) => {
    const next = WRITE_STAGES[i + 1];
    const end = next && op.stageStart[next] !== undefined ? stageStart(next) : now;
    return (end - stageStart(st)) / 1000;
  };
  const speedKBs = () => {
    const t = (now - stageStart(p.stage as WriteStage)) / 1000;
    return t > 0.3 ? Math.round(p.done / 1024 / t) : Math.round(SPEED.program / 1024);
  };
  const n = plan.sectors.length;

  const stages = WRITE_STAGES.map((st, i) => {
    const skipped = st === "verifying" && !job.verify;
    const isDone = i < idx;
    const isActive = i === idx;
    let detail = "ожидание";
    if (skipped) detail = "выключено";
    else if (isDone) {
      detail = [
        `${n} ${plural(n, SECTOR_FORMS)} · ${fmtSeconds(stageSeconds(st, i))}`,
        `${fmtSize(plan.len)} · ${fmtSeconds(stageSeconds(st, i))}`,
        `совпадает · ${fmtSeconds(stageSeconds(st, i))}`,
      ][i];
    } else if (isActive) {
      const stPct = Math.round(frac * 100);
      detail = [
        `сектор ${Math.min(n, p.done + 1)} из ${n}`,
        `${stPct} % · ${Math.round(p.done / 1024)} из ${Math.round(p.total / 1024)} КБ · ${speedKBs()} КБ/с`,
        `${stPct} %`,
      ][i];
    }
    return { st, isDone, isActive: isActive && !skipped, isPending: !isDone && (!isActive || skipped), detail };
  });

  return (
    <div style={screenStyle}>
      <div style={{ flex: 1, display: "flex", flexDirection: "column", justifyContent: "center", gap: 22, padding: "24px 120px" }}>
        <div>
          <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between", gap: 16 }}>
            <h2 style={{ margin: 0, fontSize: 18, fontWeight: 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
              Запись {file.name}
            </h2>
            <span className="mono" style={{ fontSize: 22, fontWeight: 600 }}>
              {pct}%
            </span>
          </div>
          <div style={{ fontSize: 12.5, color: "var(--text2)", margin: "4px 0 12px" }}>
            {chipShortName(s.session.target)} · <span className="mono">{fmtHex32(plan.addr)}</span> · осталось {fmtSeconds(left)}
          </div>
          <ProgressBar value={total} large label="Общий прогресс" />
        </div>
        <section className="card" style={{ padding: "4px 18px" }}>
          {stages.map((x) => (
            <div className="stage" key={x.st}>
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
                  <span style={{ fontSize: 13.5, fontWeight: 600, color: x.isPending ? "var(--text2)" : "var(--text)" }}>
                    {LABELS[x.st]}
                  </span>
                  <span className="mono" style={{ fontSize: 12, color: "var(--text2)" }}>
                    {x.detail}
                  </span>
                </div>
                {x.isActive && <ProgressBar value={frac} style={{ marginTop: 8 }} label={LABELS[x.st]} />}
              </div>
            </div>
          ))}
        </section>
      </div>
      <Footer>
        <span style={{ fontSize: 12, color: "var(--warn)", display: "flex", alignItems: "center", gap: 6 }}>
          <WarnIcon size={14} strokeWidth={2} />
          Не отключайте программатор
        </span>
        <Spacer />
        <Button onClick={() => ctl.cancelOp()}>Отменить</Button>
      </Footer>
    </div>
  );
}
