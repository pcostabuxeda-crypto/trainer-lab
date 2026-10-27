type Props = { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean };

/** Interruptor accessible (role="switch"). */
export function Switch({ checked, onChange, label, disabled }: Props) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative h-6 w-11 shrink-0 rounded-full border transition disabled:opacity-50 ${
        checked ? "border-accent bg-accent" : "border-line bg-panel-2"
      }`}
    >
      <span
        className={`absolute top-0.5 size-4.5 rounded-full transition-all ${
          checked ? "left-[22px] bg-accent-ink" : "left-0.5 bg-muted"
        }`}
      />
    </button>
  );
}
