import type { CSSProperties, ReactNode } from "react";

interface Props {
  style?: CSSProperties;
  label?: string;
  children: ReactNode;
}

export function Card({ style, label, children }: Props) {
  return (
    <section className="card" aria-label={label} style={style}>
      {children}
    </section>
  );
}
