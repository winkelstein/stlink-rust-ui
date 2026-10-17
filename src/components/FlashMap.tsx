import type { ReactNode } from "react";
import type { FlashGeometry, FlashPlan } from "../backend/types";
import { allSectors, bankBase, bankCount, sectorRuns, type SectorRun } from "../domain/geometry";
import { fmtHex32, fmtRange } from "../domain/format";
import { useT, type Dict } from "../i18n";

/** `сектора 0–1, банк 1`, `сектор 7 банка 1 и 0 банка 2`. */
export function describeRuns(t: Dict, runs: readonly SectorRun[], longForm = false): string {
  const many = runs.some((r) => r.first !== r.last) || runs.length > 1;
  const items = runs.map((r) => ({
    span: r.first === r.last ? `${r.first}` : `${r.first}–${r.last}`,
    bank: r.bank + 1,
    many: r.first !== r.last,
  }));
  return t.runs(items, many, longForm);
}

interface Props {
  geometry: FlashGeometry;
  plan: FlashPlan | null;
  children?: ReactNode;
}

export function FlashMap({ geometry: g, plan, children }: Props) {
  const { t, f } = useT();
  const sectors = allSectors(g);
  const erase = new Set(plan?.sectors.map((s) => s.addr) ?? []);
  const erasedBytes = (plan?.sectors.length ?? 0) * g.sectorSize;
  const runs = plan ? sectorRuns(g, plan.sectors) : [];
  const banks = Array.from({ length: bankCount(g) }, (_, i) => i);
  return (
    <section className="card" style={{ padding: "14px 16px", display: "flex", flexDirection: "column", gap: 12 }}>
      <div className="eyebrow">{t.map(f.size(g.size))}</div>
      <div style={{ position: "relative" }}>
        <div style={{ display: "flex", gap: 3 }}>
          {sectors.map((s) => (
            <div
              key={s.addr}
              className={erase.has(s.addr) ? "sec erase" : "sec"}
              title={t.sectorTitle(s.bank + 1, s.index, fmtHex32(s.addr))}
            />
          ))}
        </div>
        {plan && (
          <div
            aria-hidden="true"
            style={{
              position: "absolute",
              left: `${((plan.addr - g.base) / g.size) * 100}%`,
              bottom: 0,
              width: `${Math.max(0.6, (plan.paddedLen / g.size) * 100)}%`,
              height: 18,
              borderRadius: "0 0 4px 4px",
              background: "var(--accent)",
            }}
          />
        )}
      </div>
      <div style={{ display: "flex", justifyContent: "space-between", fontSize: 11, color: "var(--text2)" }}>
        {banks.map((b) => (
          <span key={b} className="mono">
            {t.bankN(b + 1)} · {fmtHex32(bankBase(g, b))}
          </span>
        ))}
      </div>
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 8,
          paddingTop: 10,
          borderTop: "1px solid var(--sep)",
          fontSize: 12,
        }}
      >
        {plan ? (
          <>
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <span className="sw" style={{ background: "var(--warnSoft)", boxShadow: "inset 0 0 0 1px var(--warnFill)" }} />
              <span style={{ flex: 1 }}>{t.willErase(describeRuns(t, runs))}</span>
              <span style={{ color: "var(--text2)" }}>{f.size(erasedBytes)}</span>
            </div>
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <span className="sw" style={{ background: "var(--accent)" }} />
              <span style={{ flex: 1 }}>
                {t.fw}{" "}
                <span className="mono" style={{ color: "var(--text2)" }}>
                  {fmtRange(plan.addr, plan.len)}
                </span>
              </span>
              <span style={{ color: "var(--text2)" }}>{f.size(plan.len)}</span>
            </div>
          </>
        ) : null}
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <span className="sw" style={{ background: "var(--fill)" }} />
          <span style={{ flex: 1 }}>{t.unchanged}</span>
          <span style={{ color: "var(--text2)" }}>{f.size(g.size - erasedBytes)}</span>
        </div>
      </div>
      {children}
    </section>
  );
}
