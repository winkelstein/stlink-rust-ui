import type { CSSProperties, ReactNode } from "react";

interface Props {
  checked: boolean;
  onChange: () => void;
  disabled?: boolean;
  style?: CSSProperties;
  children: ReactNode;
}

export function Checkbox({ checked, onChange, disabled, style, children }: Props) {
  return (
    <label className="check" style={style}>
      <input type="checkbox" checked={checked} onChange={onChange} disabled={disabled} />
      {children}
    </label>
  );
}
