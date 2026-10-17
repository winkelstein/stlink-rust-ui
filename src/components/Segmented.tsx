interface Props<T extends string> {
  label: string;
  value: T;
  options: readonly { value: T; label: string }[];
  onChange: (v: T) => void;
  disabled?: boolean;
}

export function Segmented<T extends string>({ label, value, options, onChange, disabled }: Props<T>) {
  return (
    <div className="seg" role="group" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          className={o.value === value ? "on" : ""}
          aria-pressed={o.value === value}
          disabled={disabled}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
