import { useEffect, useState } from "react";
import { Button } from "../../components/ui/Button";
import { HotkeyInput } from "../../components/ui/HotkeyInput";
import { Switch } from "../../components/ui/Switch";
import type { CheatInput, CheatView } from "../../lib/ipc";

type Props = {
  cheat: CheatView;
  onInput: (input: CheatInput) => void;
  onHotkey: (key: string, accelerator: string | null) => void;
};

const fmt = (v: number | null) => (v === null ? "—" : Number.isInteger(v) ? String(v) : v.toFixed(2));

export function CheatCard({ cheat, onInput, onHotkey }: Props) {
  const { def, state } = cheat;
  const [draft, setDraft] = useState(String(def.default ?? def.min ?? 0));
  const [slider, setSlider] = useState<number>(def.default ?? def.min ?? 0);

  // Mentre no l'arrossegues, l'slider mostra el valor real de la memòria
  const [dragging, setDragging] = useState(false);
  useEffect(() => {
    if (!dragging && state.value !== null) setSlider(state.value);
  }, [state.value, dragging]);

  const commitSlider = () => {
    setDragging(false);
    onInput({ kind: "set", value: slider });
  };

  return (
    <article
      className={`flex flex-col gap-2 rounded-xl border bg-panel p-4 transition ${
        state.active ? "border-accent/60 shadow-[0_0_0_1px_var(--color-accent)]/20" : "border-line"
      }`}
    >
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <h3 className="font-medium">{def.name}</h3>
          {def.description && <p className="mt-0.5 text-sm text-muted">{def.description}</p>}
        </div>
        {def.kind === "toggle" && (
          <Switch checked={state.active} label={def.name} onChange={(on) => onInput({ kind: "toggle", on })} />
        )}
        {def.kind === "button" && (
          <Button variant="primary" onClick={() => onInput({ kind: "press" })}>
            Executa
          </Button>
        )}
      </div>

      {def.kind === "slider" && (
        <div className="flex items-center gap-3">
          <input
            type="range"
            min={def.min}
            max={def.max}
            step={def.step ?? 1}
            value={slider}
            aria-label={def.name}
            onChange={(e) => {
              setDragging(true);
              setSlider(Number(e.target.value));
            }}
            onPointerUp={commitSlider}
            onKeyUp={commitSlider}
            className="flex-1 accent-[var(--color-accent)]"
          />
          <span className="w-14 text-right font-mono text-sm">{fmt(slider)}</span>
        </div>
      )}

      {def.kind === "value" && (
        <form
          className="flex items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            const n = Number(draft);
            if (draft.trim() !== "" && Number.isFinite(n)) onInput({ kind: "set", value: n });
          }}
        >
          <input
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            inputMode="decimal"
            aria-label={`Nou valor de ${def.name}`}
            className="w-40 rounded-lg border border-line bg-panel-2 px-2 py-1.5 font-mono text-sm outline-none focus:border-accent"
          />
          <Button type="submit">Aplica</Button>
        </form>
      )}

      <footer className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted">
        {def.target.type !== "bytes" && (
          <span>
            Valor actual <span className="font-mono text-ink">{fmt(state.value)}</span>
          </span>
        )}
        {(def.kind === "toggle" || def.kind === "button") && (
          <span className="flex items-center gap-1">
            Drecera <HotkeyInput value={cheat.hotkey} label={def.name} error={cheat.hotkeyError} onChange={(a) => onHotkey(def.id, a)} />
          </span>
        )}
        {def.kind === "slider" && (
          <>
            <span className="flex items-center gap-1">
              Baixa
              <HotkeyInput value={cheat.hotkeyDecrease} label={`${def.name} (baixa)`} onChange={(a) => onHotkey(`${def.id}:decrease`, a)} />
            </span>
            <span className="flex items-center gap-1">
              Puja
              <HotkeyInput value={cheat.hotkeyIncrease} label={`${def.name} (puja)`} onChange={(a) => onHotkey(`${def.id}:increase`, a)} />
            </span>
          </>
        )}
        {state.address && <span className="selectable ml-auto font-mono">{state.address}</span>}
      </footer>

      {(state.error || cheat.hotkeyError) && (
        <p role="alert" className="text-xs text-bad">
          {state.error ?? cheat.hotkeyError}
        </p>
      )}
    </article>
  );
}
