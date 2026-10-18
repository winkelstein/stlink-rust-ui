// Time estimates per flash family, and the overall progress derived from them.
// H7 values are measured on the bench (H743 @ 4 MHz SWD, CLAUDE.md); the other
// families use conservative guesses until measured (docs/hardware-checklist.md).

import type { FlashPlan, Progress, WriteStage } from "../backend/types";

const KIB = 1024;

export interface FamilySpeed {
  /** Seconds to erase one page/sector of `size` bytes. */
  eraseUnit: (size: number) => number;
  /** Seconds per MB of bank erase (mass erase). */
  eraseBankPerMb: number;
  /** Bytes per second. */
  program: number;
  verify: number;
  read: number;
}

/** H74x/H75x: measured (0.95 s per 128 KB sector, 5.5 s per 1 MB bank). */
const H7: FamilySpeed = {
  eraseUnit: (size) => 0.95 * (size / (128 * KIB)),
  eraseBankPerMb: 5.5,
  program: 137 * KIB,
  verify: 148 * KIB,
  read: 148 * KIB,
};

const F0F1: FamilySpeed = {
  eraseUnit: () => 0.03,
  eraseBankPerMb: 0.5,
  program: 21 * KIB,
  verify: 100 * KIB,
  read: 100 * KIB,
};

const BY_FLASH_TYPE: Record<string, FamilySpeed> = {
  H7,
  // F103C8, measured at 1.8 MHz: 29 ms per 1 KiB page, 0.03 s mass erase, 21.5 KiB/s programming (SRAM loader).
  F0_F1_F3: F0F1,
  F1_XL: F0F1,
  // TODO(measure): F2/F4/F7 — sector erase ≈ 0.3 s + 13 ms/KB (x32), bank erase up to 16 s/MB.
  F2_F4: { eraseUnit: (size) => 0.3 + (size / KIB) * 0.013, eraseBankPerMb: 12, program: 60 * KIB, verify: 120 * KIB, read: 120 * KIB },
  F7: { eraseUnit: (size) => 0.3 + (size / KIB) * 0.013, eraseBankPerMb: 12, program: 60 * KIB, verify: 120 * KIB, read: 120 * KIB },
  // TODO(measure): G4 — page erase ≤ 40 ms, double-word programming.
  G4: { eraseUnit: () => 0.06, eraseBankPerMb: 1, program: 50 * KIB, verify: 120 * KIB, read: 120 * KIB },
};

/** Speeds of a flash family (`ChipInfo.flashType`); H7 when unknown. */
export function speedFor(flashType: string | null | undefined): FamilySpeed {
  return (flashType && BY_FLASH_TYPE[flashType]) || H7;
}

export const WRITE_STAGES: readonly WriteStage[] = ["erasing", "programming", "verifying"];

export type WriteEstimate = Record<WriteStage, number> & { total: number };

export function estimateWrite(plan: FlashPlan, verify: boolean, speed: FamilySpeed = H7): WriteEstimate {
  const erasing = plan.sectors.reduce((t, s) => t + speed.eraseUnit(s.size), 0);
  const programming = plan.paddedLen / speed.program;
  const verifying = verify ? plan.paddedLen / speed.verify : 0;
  return { erasing, programming, verifying, total: erasing + programming + verifying };
}

export function estimateRead(size: number, speed: FamilySpeed = H7): number {
  return size / speed.read;
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

/** Expected seconds to erase one bank of `bankSize` bytes. */
export function estimateBankErase(bankSize: number, speed: FamilySpeed = H7): number {
  return (bankSize / (1024 * 1024)) * speed.eraseBankPerMb;
}
