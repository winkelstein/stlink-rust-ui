import type { ReactNode } from "react";

/** 60 px action bar at the bottom of every screen. */
export function Footer({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        height: 60,
        flex: "none",
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: "0 16px",
        background: "var(--bar)",
        borderTop: "1px solid var(--sep)",
      }}
    >
      {children}
    </div>
  );
}

export function Spacer() {
  return <div style={{ flex: 1 }} />;
}
