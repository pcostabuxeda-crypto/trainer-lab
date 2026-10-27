import { useEffect, useState } from "react";
import { Button } from "../../components/ui/Button";
import * as ipc from "../../lib/ipc";
import type { CheatDef, Locator, ScanHit, ScanSummary } from "../../lib/ipc";
import { slug, uniqueId } from "../../lib/slug";
import { useSession } from "../../stores/session";

/**
 * Assistent per crear un truc sense saber res de memòria:
 * què vols canviar → quin valor tens → canvia'l al joc → (repetir) → prova-ho → com el vols.
 * Per sota fa servir el mateix escàner que el mode avançat.
 */

type Step =
  | { id: "what" }
  | { id: "first" }
  | { id: "refine"; count: number }
  // `kept`: després d'escriure, el valor s'ha mantingut a la memòria (si no, el joc l'ha
  // recalculat i aquella adreça és una còpia). `now`: el valor rellegit.
  | { id: "verify"; candidates: ScanHit[]; index: number; tested: boolean; kept?: boolean; now?: number | null }
  | { id: "how"; hit: ScanHit; locator: Locator | null }
  | { id: "done"; text: string };

type Mode = "freeze" | "value" | "add";

const PRESETS = ["Vida", "Munició", "Diners", "Energia", "Experiència"];
/** Amb tan pocs candidats, ja els provem un per un. */
const FEW = 5;

/** Valor de la pantalla → valor a la memòria (p. ex. 47,70 → 4770 si el joc guarda cèntims). */
const toRaw = (hit: ScanHit, n: number) => (hit.scale === 1 ? n : Math.round(n * hit.scale));
const FREE_KEYS = ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12"];

export function CheatWizard({ onDone }: { onDone: () => void }) {
  const { info } = useSession();
  const [step, setStep] = useState<Step>({ id: "what" });
  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [testValue, setTestValue] = useState("999");
  const [mode, setMode] = useState<Mode>("freeze");
  const [amount, setAmount] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);
  const [existing, setExisting] = useState<ipc.TrainerView | null>(null);

  // Trucs que ja té el joc: per no repetir ids ni dreceres
  useEffect(() => {
    if (info) ipc.trainerView().then(setExisting, () => setExisting(null));
  }, [info?.pid, step.id === "how"]);

  if (!info) {
    return (
      <Card title="Crea un truc">
        <p className="text-sm text-muted">Primer obre el joc des de la Biblioteca i espera que l'app s'hi connecti.</p>
      </Card>
    );
  }

  const parse = (s: string) => {
    const n = Number(s.replace(",", "."));
    return s.trim() !== "" && Number.isFinite(n) ? n : null;
  };

  const run = async (f: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await f();
    } catch (e) {
      setError(ipc.errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  /** Després de cada cerca: si en queden pocs, passem a provar-los. */
  const afterScan = async (summary: ScanSummary) => {
    const { count, readFailures, truncated } = summary;
    setWarning([
      truncated ? "Hi havia més coincidències de les que podem guardar. Si no trobem el valor, torna a començar quan tinguis una quantitat menys habitual (evita 0 o 1)." : "",
      readFailures ? `No s'han pogut rellegir ${readFailures} candidats; el joc pot haver mogut o alliberat aquestes dades.` : "",
    ].filter(Boolean).join(" ") || null);
    if (count === 0) {
      setError(
        "Cap candidat coincideix amb el valor. Les coincidències inicials no confirmaven que fossin les dades que buscàvem. Comprova que el número no estigui abreujat (K, M…) i torna a començar. El joc també pot guardar-lo en una altra escala o haver-lo mogut.",
      );
      setStep({ id: "refine", count: 0 });
    } else if (count <= FEW) {
      const candidates = await ipc.scanResults(0, FEW);
      setStep({ id: "verify", candidates, index: 0, tested: false });
    } else {
      setStep({ id: "refine", count });
    }
    setValue("");
  };

  const start = () =>
    run(async () => {
      const n = parse(value);
      if (n === null) throw new Error("Escriu el número que veus al joc.");
      const s = await ipc.scanFirstAuto(n);
      await afterScan(s);
    });

  const refine = (filter: ipc.ScanFilter) =>
    run(async () => {
      const s = await ipc.scanNext(filter);
      await afterScan(s);
    });

  const restart = async () => {
    await ipc.scanReset().catch(() => {});
    setError(null);
    setWarning(null);
    setValue("");
    setStep({ id: "first" });
  };

  /** El candidat és bo: decidim com es trobarà la propera vegada. */
  const accept = (hit: ScanHit) =>
    run(async () => {
      let locator: Locator | null = null;
      if (hit.moduleOffset) {
        const [module, offset] = hit.moduleOffset.split("+");
        locator = { module, offset };
      } else {
        const [best] = await ipc.findPointers(hit.address);
        if (best) locator = { module: best.module, offset: best.moduleOffset, pointers: [best.pointerOffset] };
      }
      setAmount(hit.valueType.startsWith("f") ? "1" : "100");
      setStep({ id: "how", hit, locator });
    });

  const save = (hit: ScanHit, locator: Locator | null) =>
    run(async () => {
      const type = hit.valueType;
      const n = mode === "add" ? parse(amount) : parse(value) ?? hit.value ?? hit.previous;
      if (n === null) throw new Error("Escriu un número.");
      if (!locator) {
        // Sense camí estable: només per a aquesta partida
        if (mode === "add") throw new Error("Sense un camí estable, només es pot congelar o fixar el valor per a aquesta partida.");
        if (mode === "freeze") await ipc.freezeAddress(hit.address, type, toRaw(hit, n));
        else await ipc.scanWrite(hit.address, type, toRaw(hit, n));
        setStep({ id: "done", text: "Fet! Funcionarà fins que tanquis el joc." });
        return;
      }
      const cheats = existing?.cheats ?? [];
      const usedKeys = new Set(cheats.flatMap((c) => [c.hotkey, c.hotkeyIncrease, c.hotkeyDecrease]).filter(Boolean));
      const hotkey = mode === "value" ? undefined : FREE_KEYS.find((k) => !usedKeys.has(k));
      const id = uniqueId(slug(name), cheats.map((c) => c.def.id));
      // Textos neutres: el gènere de la paraula no el sabem («Vida», «Diners», «Munició»…)
      const cheatName = mode === "freeze" ? `${name} sense límit` : mode === "add" ? `+${n} ${name.toLowerCase()}` : name;
      const base = { id, name: cheatName, target: { address: locator, type }, hotkey };
      const cheat: CheatDef =
        mode === "freeze"
          ? { ...base, kind: "toggle", on: { freeze: toRaw(hit, n) }, off: "restore" }
          : mode === "add"
            ? { ...base, kind: "button", action: { add: toRaw(hit, n) } }
            : { ...base, kind: "value", default: toRaw(hit, n) };
      await ipc.saveCheat(cheat);
      await ipc.scanReset().catch(() => {});
      setStep({ id: "done", text: `Desat! «${cheatName}»${hotkey ? ` amb la tecla ${hotkey}` : ""}. Funcionarà també quan tornis a obrir el joc.` });
    });

  const input = (props: { value: string; onChange: (v: string) => void; onEnter: () => void; placeholder?: string }) => (
    <input
      autoFocus
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
      onKeyDown={(e) => e.key === "Enter" && !busy && props.onEnter()}
      inputMode="decimal"
      placeholder={props.placeholder}
      className="w-40 rounded-lg border border-line bg-panel-2 px-3 py-2 font-mono text-lg outline-none focus:border-accent"
    />
  );

  const thing = name.toLowerCase() || "el valor";

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4">
      <Progress step={step.id} />

      {step.id === "what" && (
        <Card title="Què vols modificar?">
          <div className="flex flex-wrap gap-2">
            {PRESETS.map((p) => (
              <button
                key={p}
                onClick={() => setName(p)}
                className={`rounded-full border px-3 py-1 text-sm ${name === p ? "border-accent text-accent" : "border-line text-muted hover:text-ink"}`}
              >
                {p}
              </button>
            ))}
          </div>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="O escriu-ho: Pocions, Fusta…"
            className="rounded-lg border border-line bg-panel-2 px-3 py-2 text-sm outline-none focus:border-accent"
          />
          <p className="text-sm text-muted">Buscarem automàticament números enters i amb decimals. Necessitem un valor numèric visible al joc.</p>
          <Next disabled={!name.trim()} onClick={() => setStep({ id: "first" })}>
            Comença
          </Next>
        </Card>
      )}

      {step.id === "first" && (
        <Card title={`${name}: quant en tens ara?`}>
          <p className="text-sm text-muted">Mira el número al joc i escriu-lo tal qual.</p>
          <div className="flex items-center gap-3">
            {input({ value, onChange: setValue, onEnter: () => void start(), placeholder: "100" })}
            <Next disabled={busy} onClick={() => void start()}>
              {busy ? "Buscant…" : "Busca"}
            </Next>
          </div>
        </Card>
      )}

      {step.id === "refine" && step.count > 0 && (
        <Card title="Ara fes que canviï al joc">
          <p className="text-sm text-muted">
            Hi ha <strong className="text-ink">{step.count.toLocaleString("ca")}</strong> llocs possibles. Juga una mica
            perquè {thing} canviï (rep mal, gasta, dispara…) i escriu el valor nou.
          </p>
          <div className="flex items-center gap-3">
            {input({ value, onChange: setValue, onEnter: () => void refineExact(), placeholder: "85" })}
            <Next disabled={busy} onClick={() => void refineExact()}>
              {busy ? "Buscant…" : "Següent"}
            </Next>
          </div>
          <p className="text-xs text-muted">No veus el número exacte?</p>
          <div className="flex flex-wrap gap-2">
            <Button disabled={busy} onClick={() => void refine({ kind: "increased" })}>
              Ha pujat
            </Button>
            <Button disabled={busy} onClick={() => void refine({ kind: "decreased" })}>
              Ha baixat
            </Button>
            <Button disabled={busy} onClick={() => void refine({ kind: "unchanged" })}>
              No ha canviat
            </Button>
          </div>
        </Card>
      )}

      {step.id === "verify" && (() => {
        const hit = step.candidates[step.index];
        const type = hit.valueType;
        const next = async () => {
          // No era aquest: tornem a posar el valor que tenia i provem el següent
          if (step.tested) await ipc.scanWrite(hit.address, type, toRaw(hit, hit.value ?? hit.previous)).catch(() => {});
          if (step.index + 1 < step.candidates.length) setStep({ ...step, index: step.index + 1, tested: false });
          else {
            setError("Cap dels candidats era el bo. Torna a començar.");
            setStep({ id: "refine", count: 0 });
          }
        };
        return (
          <Card title="Gairebé! Provem-ho">
            <p className="text-sm text-muted">
              Candidat {step.index + 1} de {step.candidates.length}. Posarem {thing} a aquest valor i mira si canvia al joc:
            </p>
            <div className="flex items-center gap-3">
              {input({ value: testValue, onChange: setTestValue, onEnter: () => {} })}
              <Button
                variant="primary"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    const n = parse(testValue);
                    if (n === null) throw new Error("Escriu un número.");
                    await ipc.scanWrite(hit.address, type, toRaw(hit, n));
                    // Esperem una mica i rellegim: si el joc ho ha tornat a canviar, aquesta
                    // adreça no és la bona (és una còpia que el joc recalcula cada moment)
                    await new Promise((r) => setTimeout(r, 1200));
                    const now = (await ipc.scanResults(0, FEW)).find((h) => h.address === hit.address)?.value ?? null;
                    const kept = now !== null && Math.abs(now - n) < 0.01;
                    setStep({ ...step, tested: true, kept, now });
                  })
                }
              >
                Prova
              </Button>
            </div>
            {step.tested && !step.kept && (
              <>
                <p className="text-sm text-warn">
                  {step.now === null
                    ? "No hem pogut tornar a llegir aquesta adreça: el joc l'ha alliberada."
                    : `El joc l'ha tornat a posar a ${step.now} al cap d'un segon: és una còpia que el joc recalcula, no la que guarda ${thing}.`}
                </p>
                <Button variant="primary" className="self-start" disabled={busy} onClick={() => void next()}>
                  {step.index + 1 < step.candidates.length ? "Prova el següent" : "D'acord"}
                </Button>
              </>
            )}
            {step.tested && step.kept && (
              <>
                <p className="text-sm">
                  La memòria ja té <strong>{testValue}</strong>. Mira el joc: si encara mostra el número d'abans, fes alguna
                  cosa que el canviï (ven, compra, guanya o perd una mica) i mira si salta a prop de {testValue}.{" "}
                  <span className="text-muted">Molts jocs no tornen a pintar el número fins que canvia per una altra raó.</span>
                </p>
                <div className="flex gap-2">
                  <Button variant="primary" disabled={busy} onClick={() => void accept(hit)}>
                    Sí, ha canviat
                  </Button>
                  <Button disabled={busy} onClick={() => void next()}>
                    No ha canviat
                  </Button>
                </div>
              </>
            )}
          </Card>
        );
      })()}

      {step.id === "how" && (
        <Card title="Trobat! Com vols el truc?">
          {step.hit.scale !== 1 && (
            <p className="text-sm text-muted">
              Aquest joc guarda {thing} en cèntims. Escriu els números com al joc (p. ex. 999,50); a la pantalla de trucs
              el valor actual es veurà multiplicat per {step.hit.scale}.
            </p>
          )}
          {step.locator ? (
            <p className="text-sm text-ok">Funcionarà també quan tornis a obrir el joc.</p>
          ) : (
            <p className="text-sm text-warn">
              Aquest valor canvia de lloc cada partida i no hem trobat com seguir-lo. El truc funcionarà fins que tanquis el
              joc (no es desa). Amb el mode avançat pots provar d'escriure el camí a mà.
            </p>
          )}
          <div className="flex flex-col gap-2">
            {(
              [
                ["freeze", `Que ${thing} no baixi mai`],
                ["value", `Posar-hi el número que vulgui`],
                ["add", `Un botó que en sumi`],
              ] as [Mode, string][]
            ).map(([m, label]) => (
              <label key={m} className="flex items-center gap-2 text-sm">
                <input type="radio" name="mode" checked={mode === m} onChange={() => setMode(m)} /> {label}
              </label>
            ))}
          </div>
          <div className="flex items-center gap-3 text-sm text-muted">
            {mode === "add" ? "Quantitat a sumar" : mode === "freeze" ? "Valor que es mantindrà" : "Valor"}
            {mode === "add"
              ? input({ value: amount, onChange: setAmount, onEnter: () => void save(step.hit, step.locator), placeholder: "100" })
              : input({ value, onChange: setValue, onEnter: () => void save(step.hit, step.locator), placeholder: String(step.hit.previous) })}
          </div>
          <Next disabled={busy} onClick={() => void save(step.hit, step.locator)}>
            {step.locator ? "Desa el truc" : "Activa-ho ara"}
          </Next>
        </Card>
      )}

      {step.id === "done" && (
        <Card title="Llest!">
          <p className="text-sm">{step.text}</p>
          <div className="flex gap-2">
            <Button variant="primary" onClick={onDone}>
              Veure els trucs
            </Button>
            <Button
              onClick={() => {
                setName("");
                setValue("");
                setStep({ id: "what" });
              }}
            >
              Crea'n un altre
            </Button>
          </div>
        </Card>
      )}

      {warning && <p role="status" className="rounded-xl border border-warn/40 p-4 text-sm text-warn">{warning}</p>}

      {error && (
        <div role="alert" className="flex items-center justify-between gap-3 rounded-xl border border-bad/40 bg-bad/5 p-4 text-sm text-bad">
          {error}
          {step.id === "refine" && step.count === 0 && <Button onClick={() => void restart()}>Torna a començar</Button>}
        </div>
      )}

      {step.id !== "what" && step.id !== "done" && (
        <button disabled={busy} onClick={() => void restart().then(() => setStep({ id: "what" }))} className="self-start text-xs text-muted hover:text-ink disabled:opacity-50">
          Cancel·la i comença de nou
        </button>
      )}
    </div>
  );

  function refineExact() {
    const n = parse(value);
    if (n === null) return setError("Escriu el número nou que veus al joc.");
    return refine({ kind: "exact", value: n });
  }
}

function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-4 rounded-xl border border-line bg-panel p-6">
      <h2 className="text-xl font-semibold">{title}</h2>
      {children}
    </section>
  );
}

function Next(props: { disabled?: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <Button variant="primary" className="self-start px-5 py-2" disabled={props.disabled} onClick={props.onClick}>
      {props.children}
    </Button>
  );
}

const STEPS = ["what", "first", "refine", "verify", "how"] as const;
const LABELS = ["Què", "Valor", "Canvia'l", "Prova", "Desa"];

function Progress({ step }: { step: Step["id"] }) {
  const current = step === "done" ? STEPS.length : STEPS.indexOf(step as (typeof STEPS)[number]);
  return (
    <ol className="flex gap-2 text-xs" aria-label="Passos">
      {LABELS.map((l, i) => (
        <li
          key={l}
          aria-current={i === current ? "step" : undefined}
          className={`flex-1 border-t-2 pt-1 ${i < current ? "border-accent text-muted" : i === current ? "border-accent text-ink" : "border-line text-muted"}`}
        >
          {l}
        </li>
      ))}
    </ol>
  );
}
