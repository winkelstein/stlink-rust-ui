import type { CSSProperties, ReactNode } from "react";

interface Props {
  k: string;
  children: ReactNode;
  mono?: boolean;
  valueStyle?: CSSProperties;
}

export function KeyValue({ k, children, mono, valueStyle }: Props) {
  return (
    <div className="kv">
      <span className="k">{k}</span>
      <span className={mono ? "v mono" : "v"} style={valueStyle}>
        {children}
      </span>
    </div>
  );
}
