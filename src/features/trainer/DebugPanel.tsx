import { useEffect, useState } from "react";
import { Button } from "../../components/ui/Button";
import * as ipc from "../../lib/ipc";
import type { Locator, ValueType } from "../../lib/ipc";

const TYPES: ValueType[] = ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool"];

// Camps del joc de prova: AOB del magic "TLABANCH" + 8 = punter al Player, + offset del camp
const anchor = (field: string): Locator => ({
  aob: { pattern: "54 4C 41 42 41 4E 43 48", offset: 8 },
  pointers: [field],
});
const PRESETS: { label: string; type: ValueType; locator: Locator }[] = [
  { label: "Vida", type: "i32", locator: anchor("0x0") },
  { label: "Munició", type: "i32", locator: anchor("0x4") },
  { label: "Diners", type: "i64", locator: anchor("0x8") },
  { label: "Velocitat", type: "f32", locator: anchor("0x10") },
];

type Line = { ok: boolean; text: string };

/** Eina de la Fase 2 per provar lectura, escriptura i congelació a mà. */
export function DebugPanel() {
  const [locatorText, setLocatorText] = useState(JSON.stringify(PRESETS[0].locator, null, 2));
  const [type, setType] = useState<ValueType>("i32");
  const [value, setValue] = useState("100");
  const [frozen, setFrozen] = useState<string[]>([]);
  const [lines, setLines] = useState<Line[]>([]);

  const log = (l: Line) => setLines((ls) => [l, ...ls].slice(0, 30));
  const refreshFrozen = () => ipc.listFrozen().then(setFrozen, () => setFrozen([]));
  useEffect(() => void refreshFrozen(), []);

  const run = async (action: "read" | "write" | "freeze") => {
    let locator: Locator;
    try {
      locator = JSON.parse(locatorText);
    } catch {
      return log({ ok: false, text: "El localitzador no és JSON vàlid." });
    }
    const n = Number(value);
    if (action !== "read" && !Number.isFinite(n)) return log({ ok: false, text: "El valor ha de ser un número." });
    try {
      const r =
        action === "read" ? await ipc.debugRead(locator, type) : await ipc.debugWrite(locator, type, n, action === "freeze");
      const verb = { read: "Llegit", write: "Escrit", freeze: "Congelat" }[action];
      log({ ok: true, text: `${verb} ${type} a ${r.address} → ${r.value}` });
      if (action === "freeze") void refreshFrozen();
    } catch (e) {
      log({ ok: false, text: ipc.errorMessage(e) });
    }
  };

  return (
    <section className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-4">
      <div className="flex flex-col gap-3 rounded-xl border border-line bg-panel p-4">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm text-muted">Joc de prova:</span>
          {PRESETS.map((p) => (
            <Button
              key={p.label}
              onClick={() => {
                setLocatorText(JSON.stringify(p.locator, null, 2));
                setType(p.type);
              }}
            >
              {p.label}
            </Button>
          ))}
        </div>
        <label className="flex min-h-0 flex-1 flex-col gap-1 text-sm text-muted">
          Localitzador (mateix format que les definicions)
          <textarea
            value={locatorText}
            onChange={(e) => setLocatorText(e.target.value)}
            spellCheck={false}
            className="selectable min-h-40 flex-1 resize-none rounded-lg border border-line bg-bg p-3 font-mono text-xs text-ink outline-none focus:border-accent"
          />
        </label>
        <div className="flex flex-wrap items-end gap-3">
          <label className="flex flex-col gap-1 text-sm text-muted">
            Tipus
            <select
              value={type}
              onChange={(e) => setType(e.target.value as ValueType)}
              className="rounded-lg border border-line bg-panel-2 px-2 py-1.5 text-ink"
            >
              {TYPES.map((t) => (
                <option key={t}>{t}</option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1 text-sm text-muted">
            Valor
            <input
              value={value}
              onChange={(e) => setValue(e.target.value)}
              inputMode="decimal"
              className="w-32 rounded-lg border border-line bg-panel-2 px-2 py-1.5 font-mono text-ink outline-none focus:border-accent"
            />
          </label>
          <div className="ml-auto flex gap-2">
            <Button onClick={() => void run("read")}>Llegeix</Button>
            <Button onClick={() => void run("write")}>Escriu</Button>
            <Button variant="primary" onClick={() => void run("freeze")}>
              Congela
            </Button>
          </div>
        </div>
      </div>

      <div className="flex min-h-0 flex-col gap-4">
        <div className="rounded-xl border border-line bg-panel p-4">
          <h3 className="mb-2 text-sm font-semibold">Valors congelats</h3>
          {frozen.length === 0 && <p className="text-sm text-muted">Cap.</p>}
          <ul className="flex flex-col gap-1">
            {frozen.map((k) => (
              <li key={k} className="flex items-center justify-between gap-2 font-mono text-xs">
                <span className="selectable">{k}</span>
                <Button onClick={() => void ipc.unfreeze(k).then(refreshFrozen)}>Descongela</Button>
              </li>
            ))}
          </ul>
        </div>
        <div className="min-h-0 flex-1 overflow-auto rounded-xl border border-line bg-panel p-4">
          <h3 className="mb-2 text-sm font-semibold">Resultats</h3>
          <ul className="selectable flex flex-col gap-1 font-mono text-xs">
            {lines.map((l, i) => (
              <li key={lines.length - i} className={l.ok ? "text-ink" : "text-bad"}>
                {l.text}
              </li>
            ))}
          </ul>
        </div>
      </div>
    </section>
  );
}
