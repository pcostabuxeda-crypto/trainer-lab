import { useState, type KeyboardEvent } from "react";

const MODIFIERS = new Set(["Control", "Alt", "Shift", "Meta", "OS"]);

/** Converteix una tecla premuda al format de drecera de Tauri ("Ctrl+Shift+F1"). */
function toAccelerator(e: KeyboardEvent): string | null {
  if (MODIFIERS.has(e.key)) return null;
  const key = e.code.startsWith("Key") ? e.code.slice(3) : e.code.startsWith("Digit") ? e.code.slice(5) : e.code;
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean);
  return [...mods, key].join("+");
}

type Props = {
  value: string | null;
  label: string;
  error?: string | null;
  /** "" = desactivar, null = tornar a la de la definició */
  onChange: (accelerator: string | null) => void;
};

/** Mostra una drecera i, en clicar-hi, captura la següent combinació de tecles. */
export function HotkeyInput({ value, label, error, onChange }: Props) {
  const [capturing, setCapturing] = useState(false);

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (!capturing) return;
    e.preventDefault();
    if (e.key === "Escape") return setCapturing(false);
    if (e.key === "Backspace" || e.key === "Delete") {
      setCapturing(false);
      return onChange("");
    }
    const accel = toAccelerator(e);
    if (accel) {
      setCapturing(false);
      onChange(accel);
    }
  };

  return (
    <span className="inline-flex items-center gap-1">
      <button
        onClick={() => setCapturing(true)}
        onBlur={() => setCapturing(false)}
        onKeyDown={onKeyDown}
        aria-label={`Drecera de ${label}: ${value || "cap"}. Clica per canviar-la`}
        title={error ?? "Clica i prem una combinació. Supr esborra, Esc cancel·la."}
        className={`min-w-12 rounded-md border px-2 py-0.5 font-mono text-xs transition ${
          capturing
            ? "border-accent text-accent"
            : error
              ? "border-bad/60 text-bad"
              : "border-line text-muted hover:border-muted hover:text-ink"
        }`}
      >
        {capturing ? "Prem…" : value || "—"}
      </button>
      <button
        onClick={() => onChange(null)}
        aria-label={`Restaura la drecera per defecte de ${label}`}
        title="Restaura la drecera de la definició"
        className="rounded px-1 text-xs text-muted hover:text-ink"
      >
        ↺
      </button>
    </span>
  );
}
