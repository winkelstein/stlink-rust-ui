// Speeds measured on the bench (H743 @ 4 MHz SWD, CLAUDE.md) and the
// time estimates / overall progress derived from them.

import type { FlashPlan, Progress, WriteStage } from "../backend/types";

export const SPEED = {
  /** Seconds per 128 KB sector. */
  eraseSector: 0.95,
  /** Bytes per second. */
  program: 137 * 1024,
  verify: 148 * 1024,
  read: 148 * 1024,
} as const;

export const WRITE_STAGES: readonly WriteStage[] = ["erasing", "programming", "verifying"];

export type WriteEstimate = Record<WriteStage, number> & { total: number };

export function estimateWrite(plan: FlashPlan, verify: boolean): WriteEstimate {
  const erasing = plan.sectors.length * SPEED.eraseSector;
  const programming = plan.paddedLen / SPEED.program;
  const verifying = verify ? plan.paddedLen / SPEED.verify : 0;
  return { erasing, programming, verifying, total: erasing + programming + verifying };
}

export function estimateRead(size: number): number {
  return size / SPEED.read;
}

/**
 * Overall write progress 0..1, weighting each stage by its estimated time.
 * `stageFrac` overrides the fraction of the active stage (for interpolation).
 */
export function writeFraction(est: WriteEstimate, p: Progress, stageFrac?: number): number {
  if (est.total <= 0) return 0;
  const idx = WRITE_STAGES.indexOf(p.stage as WriteStage);
  if (idx < 0) return 0;
  const frac = stageFrac ?? (p.total > 0 ? p.done / p.total : 0);
  let t = 0;
  WRITE_STAGES.forEach((s, i) => {
    if (i < idx) t += est[s];
    else if (i === idx) t += est[s] * Math.min(1, Math.max(0, frac));
  });
  return t / est.total;
}

/**
 * Remaining seconds: the estimate for the rest, corrected by how fast the
 * operation actually went so far.
 */
export function remainingSeconds(estTotal: number, fraction: number, elapsedS: number): number {
  const rest = estTotal * (1 - fraction);
  const expectedSoFar = estTotal * fraction;
  if (elapsedS < 0.5 || expectedSoFar < 0.2) return rest;
  return rest * (elapsedS / expectedSoFar);
}
