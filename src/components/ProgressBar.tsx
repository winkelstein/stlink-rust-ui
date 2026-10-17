import type { CSSProperties } from "react";

interface Props {
  /** 0..1 */
  value: number;
  large?: boolean;
  /** Red (full erase). */
  danger?: boolean;
  style?: CSSProperties;
  label?: string;
}

export function ProgressBar({ value, large, danger, style, label }: Props) {
  const pct = Math.round(Math.min(1, Math.max(0, value)) * 100);
  return (
    <div
      className={danger ? "bar danger" : "bar"}
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={pct}
      style={large ? { height: 8, borderRadius: 4, ...style } : style}
    >
      <div style={{ width: `${pct}%` }} />
    </div>
  );
}
