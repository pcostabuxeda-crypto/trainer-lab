import { useEffect, useRef, useState } from "react";
import { Button } from "../../components/ui/Button";
import * as ipc from "../../lib/ipc";
import type { CheatDef, Locator, PointerCandidate, ScanHit, ValueType } from "../../lib/ipc";
import { slug } from "../../lib/slug";

type Kind = "freeze" | "value" | "slider" | "add";

const KINDS: { id: Kind; label: string }[] = [
  { id: "freeze", label: "Interruptor: congela un valor" },
  { id: "value", label: "Camp: escriu el valor que vulguis" },
  { id: "slider", label: "Slider: valor dins d'un rang" },
  { id: "add", label: "Botó: suma una quantitat" },
];

/** Locator inicial: estàtic si l'adreça és dins d'un mòdul; si no, buit (cal un punter). */
function initialLocator(hit: ScanHit): Locator | null {
  if (!hit.moduleOffset) return null;
  const [module, offset] = hit.moduleOffset.split("+");
  return { module, offset };
}

type Props = { hit: ScanHit; valueType: ValueType; onClose: () => void; onSaved: (path: string) => void };

/** Editor per desar una troballa de l'escàner com a truc nou. */
export function SaveCheatDialog({ hit, valueType, onClose, onSaved }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    ref.current?.showModal();
  }, []);

  const current = hit.value ?? hit.previous;
  const [name, setName] = useState("");
  const [category, setCategory] = useState("");
  const [kind, setKind] = useState<Kind>("freeze");
  const [num, setNum] = useState({ a: String(current), min: "0", max: String(Math.max(current * 10, 100)), step: "1" });
  const [hotkey, setHotkey] = useState("");
  const [locatorText, setLocatorText] = useState(() => {
    const l = initialLocator(hit);
    return l ? JSON.stringify(l, null, 2) : "";
  });
  const [candidates, setCandidates] = useState<PointerCandidate[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const searchPointers = async () => {
    setError(null);
    try {
      setCandidates(await ipc.findPointers(hit.address));
    } catch (e) {
      setError(ipc.errorMessage(e));
    }
  };

  const save = async () => {
    setError(null);
    let address: Locator;
    try {
      address = JSON.parse(locatorText);
    } catch {
      return setError("El localitzador no és JSON vàlid.");
    }
    const n = (k: keyof typeof num) => Number(num[k]);
    const base = {
      id: slug(name),
      name: name.trim(),
      category: category.trim() || undefined,
      target: { address, type: valueType },
      hotkey: hotkey.trim() || undefined,
    };
    const cheat: CheatDef =
      kind === "freeze"
        ? { ...base, kind: "toggle", on: { freeze: n("a") }, off: "restore" }
        : kind === "value"
          ? { ...base, kind: "value", default: n("a") }
          : kind === "slider"
            ? { ...base, kind: "slider", min: n("min"), max: n("max"), step: n("step"), default: n("a"), freeze: true, hotkey: undefined }
            : { ...base, kind: "button", action: { add: n("a") } };
    setSaving(true);
    try {
      onSaved(await ipc.saveCheat(cheat));
    } catch (e) {
      setError(ipc.errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const field = "rounded-lg border border-line bg-panel-2 px-2 py-1.5 text-sm text-ink outline-none focus:border-accent";
  const numField = (k: keyof typeof num, label: string) => (
    <label className="flex flex-col gap-1 text-sm text-muted">
      {label}
      <input value={num[k]} inputMode="decimal" onChange={(e) => setNum({ ...num, [k]: e.target.value })} className={`${field} w-28 font-mono`} />
    </label>
  );

  return (
    <dialog ref={ref} onCancel={onClose} className="m-auto max-h-[90vh] w-[640px] max-w-[95vw] overflow-auto rounded-xl border border-line bg-panel p-0 text-ink backdrop:bg-black/60">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
        className="flex flex-col gap-4 p-5"
      >
        <h2 className="text-lg font-semibold">Desa com a truc</h2>
        <p className="font-mono text-xs break-all text-muted">
          {hit.address} · {valueType} · valor actual {current}
        </p>

        <div className="flex flex-wrap gap-3">
          <label className="flex flex-1 flex-col gap-1 text-sm text-muted">
            Nom
            <input required value={name} onChange={(e) => setName(e.target.value)} placeholder="Vida infinita" className={field} autoFocus />
          </label>
          <label className="flex flex-col gap-1 text-sm text-muted">
            Categoria
            <input value={category} onChange={(e) => setCategory(e.target.value)} placeholder="Jugador" className={`${field} w-36`} />
          </label>
        </div>

        <fieldset className="flex flex-col gap-1">
          <legend className="mb-1 text-sm text-muted">Tipus</legend>
          {KINDS.map((k) => (
            <label key={k.id} className="flex items-center gap-2 text-sm">
              <input type="radio" name="kind" checked={kind === k.id} onChange={() => setKind(k.id)} /> {k.label}
            </label>
          ))}
        </fieldset>

        <div className="flex flex-wrap items-end gap-3">
          {kind === "freeze" && numField("a", "Valor congelat")}
          {kind === "value" && numField("a", "Valor per defecte")}
          {kind === "add" && numField("a", "Quantitat a sumar")}
          {kind === "slider" && (
            <>
              {numField("min", "Mínim")}
              {numField("max", "Màxim")}
              {numField("step", "Pas")}
              {numField("a", "Inicial")}
            </>
          )}
          {kind !== "value" && kind !== "slider" && (
            <label className="flex flex-col gap-1 text-sm text-muted">
              Drecera (opcional)
              <input value={hotkey} onChange={(e) => setHotkey(e.target.value)} placeholder="F7" className={`${field} w-28 font-mono`} />
            </label>
          )}
        </div>

        <div className="flex flex-col gap-2">
          <span className="text-sm text-muted">Localitzador</span>
          {!hit.moduleOffset && (
            <div className="rounded-lg border border-warn/40 bg-warn/5 p-3 text-sm">
              <p className="text-warn">Aquesta adreça és dinàmica: canviarà quan reiniciïs el joc.</p>
              <p className="mt-1 text-muted">
                Busca una variable global que hi apunti (cerca d'un nivell). Si no en surt cap, caldrà trobar la cadena de
                punters amb una altra eina i escriure-la a mà.
              </p>
              <Button type="button" className="mt-2" onClick={() => void searchPointers()}>
                Busca un punter estàtic
              </Button>
              {candidates && candidates.length === 0 && <p className="mt-2 text-muted">No n'he trobat cap.</p>}
              {candidates && candidates.length > 0 && (
                <ul className="mt-2 flex flex-col gap-1 font-mono text-xs">
                  {candidates.map((c) => (
                    <li key={`${c.module}${c.moduleOffset}`}>
                      <button
                        type="button"
                        className="rounded px-1 text-left hover:bg-panel-2 hover:text-accent"
                        onClick={() =>
                          setLocatorText(JSON.stringify({ module: c.module, offset: c.moduleOffset, pointers: [c.pointerOffset] }, null, 2))
                        }
                      >
                        [{c.module}+{c.moduleOffset}] + {c.pointerOffset}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
          <textarea
            value={locatorText}
            onChange={(e) => setLocatorText(e.target.value)}
            spellCheck={false}
            placeholder='{ "module": "game.exe", "offset": "0x1234", "pointers": ["0x10"] }'
            aria-label="Localitzador en JSON"
            className="selectable h-28 resize-none rounded-lg border border-line bg-bg p-3 font-mono text-xs text-ink outline-none focus:border-accent"
          />
        </div>

        {error && (
          <p role="alert" className="text-sm text-bad">
            {error}
          </p>
        )}

        <div className="flex justify-end gap-2 border-t border-line pt-4">
          <Button type="button" onClick={onClose}>
            Cancel·la
          </Button>
          <Button type="submit" variant="primary" disabled={saving || !name.trim() || !locatorText.trim()}>
            {saving ? "Desant…" : "Desa a la definició del joc"}
          </Button>
        </div>
      </form>
    </dialog>
  );
}
