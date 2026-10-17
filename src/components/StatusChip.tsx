import type { CSSProperties, ReactNode } from "react";

interface Props {
  children: ReactNode;
  led?: boolean;
  style?: CSSProperties;
}

export function StatusChip({ children, led = true, style }: Props) {
  return (
    <span className="chip" style={style}>
      {led && <span className="led" />}
      {children}
    </span>
  );
}
