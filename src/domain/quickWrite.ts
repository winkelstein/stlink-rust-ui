// Quick write rules (docs/design/README.md, «Быстрая запись — правила»).

import type { BinFile, FlashPlan, QuickWriteSettings } from "../backend/types";
import { sameSectors } from "./geometry";

/** Debounce after the last file change before rewriting. */
export const FILE_CHANGE_DEBOUNCE_MS = 1000;

export const DEFAULT_QUICK: QuickWriteSettings = {
  file: null,
  addr: 0x0800_0000,
  onFileChange: false,
  lastConfirmedSectors: null,
  lastWrittenCrc: null,
  chipName: null,
  updatedAt: 0,
};

/**
 * Skip the confirmation only if exactly the last confirmed sectors are erased
 * and «Всегда спрашивать перед стиранием» is off.
 */
export function needsConfirm(q: QuickWriteSettings, plan: FlashPlan, alwaysConfirm: boolean): boolean {
  return alwaysConfirm || !sameSectors(q.lastConfirmedSectors, plan.sectors);
}

/** Stored settings of any version: defaults for missing keys, unknown keys
 * (the removed `onConnect`) dropped. */
export function migrateQuick(raw: Partial<QuickWriteSettings> & Record<string, unknown>): QuickWriteSettings {
  const s: QuickWriteSettings = { ...DEFAULT_QUICK };
  for (const k of Object.keys(DEFAULT_QUICK) as (keyof QuickWriteSettings)[]) {
    if (raw[k] !== undefined) (s as unknown as Record<string, unknown>)[k] = raw[k];
  }
  return s;
}

export function sameFile(a: BinFile | null, b: BinFile | null): boolean {
  if (!a || !b) return false;
  // In the browser there are no paths and every pick gets a new id: compare names.
  return a.path !== null && b.path !== null ? a.path === b.path : a.name === b.name;
}

export type FileChangeDecision = "write" | "sameCrc" | "busy" | "off";

/** Decision when the debounce timer after a file change fires. */
export function onFileChanged(q: QuickWriteSettings | null, file: BinFile, busy: boolean): FileChangeDecision {
  if (!q || !q.onFileChange || !sameFile(q.file, file)) return "off";
  if (q.lastWrittenCrc !== null && q.lastWrittenCrc === file.crc32) return "sameCrc";
  if (busy) return "busy";
  return "write";
}
