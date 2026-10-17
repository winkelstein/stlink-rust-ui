import type { ButtonHTMLAttributes, ReactNode, Ref } from "react";

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  primary?: boolean;
  /** Shortcut hint, e.g. `⌘R`. */
  kbd?: string;
  children: ReactNode;
  ref?: Ref<HTMLButtonElement>;
}

export function Button({ primary, kbd, children, className, type = "button", ...rest }: Props) {
  return (
    <button type={type} className={`btn${primary ? " btn-primary" : ""}${className ? ` ${className}` : ""}`} {...rest}>
      {children}
      {kbd && <kbd>{kbd}</kbd>}
    </button>
  );
}
