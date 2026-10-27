import { useEffect, useState } from "react";
import { Button } from "../../components/ui/Button";
import { Switch } from "../../components/ui/Switch";
import { useSettings } from "../../stores/settings";
import * as ipc from "../../lib/ipc";
import { LogViewer } from "./LogViewer";

export function SettingsPage() {
  const { settings, load, update } = useSettings();
  const [interval, setIntervalMs] = useState("100");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);

  // Les dreceres es canvien des d'una altra pantalla: rellegim en entrar
  useEffect(() => void load(), [load]);
  useEffect(() => {
    if (settings) setIntervalMs(String(settings.freezeIntervalMs));
  }, [settings?.freezeIntervalMs]);

  const toggle = async (patch: Partial<ipc.Settings>) => {
    const err = await update(patch);
    if (err) setMsg({ ok: false, text: err });
  };

  const save = async () => {
    const err = await update({ freezeIntervalMs: Number(interval) });
    setMsg(err ? { ok: false, text: err } : { ok: true, text: "Desat." });
  };

  const live = settings;
  const acknowledged = Object.entries(settings?.acknowledgedWarnings ?? {});
  /** `null` = tots els jocs */
  const forget = async (path: string | null) => {
    const next = { ...(settings?.acknowledgedWarnings ?? {}) };
    if (path === null) for (const k of Object.keys(next)) delete next[k];
    else delete next[path];
    const err = await update({ acknowledgedWarnings: next });
    setMsg(err ? { ok: false, text: err } : { ok: true, text: "Fet: et tornarem a preguntar." });
  };
  const overrides = Object.entries(settings?.hotkeyOverrides ?? {}).flatMap(([trainer, keys]) =>
    Object.entries(keys ?? {}).map(([key, accel]) => ({ trainer, key, accel })),
  );

  return (
    <div className="flex h-full min-h-0 flex-col gap-4">
      <section className="flex flex-col gap-3 rounded-xl border border-line bg-panel p-4">
        <h2 className="font-semibold">Configuració</h2>
        <label className="flex items-center justify-between gap-4 text-sm">
          <span>
            Connecta't sol als jocs que tenen trucs
            <span className="block text-xs text-muted">Quan obres un joc, l'app s'hi connecta al cap de 5 segons.</span>
          </span>
          <Switch label="Connexió automàtica" checked={live?.autoAttach ?? true} onChange={(v) => void toggle({ autoAttach: v })} />
        </label>
        <label className="flex items-center justify-between gap-4 text-sm">
          <span>
            Mode avançat
            <span className="block text-xs text-muted">Mostra adreces, eines de depuració, l'escàner tècnic i tots els processos.</span>
          </span>
          <Switch label="Mode avançat" checked={live?.advancedMode ?? false} onChange={(v) => void toggle({ advancedMode: v })} />
        </label>
        <form
          className="flex flex-wrap items-end gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <label className="flex flex-col gap-1 text-sm text-muted">
            Interval de congelació (ms)
            <input
              type="number"
              min={10}
              max={5000}
              value={interval}
              onChange={(e) => setIntervalMs(e.target.value)}
              className="w-32 rounded-lg border border-line bg-panel-2 px-2 py-1.5 font-mono text-ink outline-none focus:border-accent"
            />
          </label>
          <Button type="submit" variant="primary">
            Desa
          </Button>
          {msg && (
            <span role="status" className={`text-sm ${msg.ok ? "text-ok" : "text-bad"}`}>
              {msg.text}
            </span>
          )}
        </form>
        <p className="text-xs text-muted">
          Més baix = el valor congelat és més estable, però consumeix més CPU. 100 ms va bé per a la majoria de jocs.
        </p>
        {acknowledged.length > 0 && (
          <div>
            <h3 className="mb-1 text-sm font-semibold">Avisos d'anti-cheat acceptats</h3>
            <p className="mb-2 text-xs text-muted">
              En aquests jocs ja vas confirmar que juges offline: l'app s'hi connecta sense tornar-ho a preguntar.
            </p>
            <ul className="flex flex-col gap-1 text-sm">
              {acknowledged.map(([path, warnings]) => (
                <li key={path} className="flex items-center justify-between gap-3">
                  <span className="min-w-0 truncate" title={path}>
                    {path.split("\\").pop()} <span className="text-xs text-muted">· {(warnings ?? []).join(", ")}</span>
                  </span>
                  <Button onClick={() => void forget(path)}>Oblida</Button>
                </li>
              ))}
            </ul>
            <Button className="mt-2" onClick={() => void forget(null)}>
              Oblida-ho tot
            </Button>
          </div>
        )}
        {overrides.length > 0 && (
          <div>
            <h3 className="mb-1 text-sm font-semibold">Dreceres personalitzades</h3>
            <ul className="font-mono text-xs text-muted">
              {overrides.map((o) => (
                <li key={`${o.trainer}/${o.key}`}>
                  {o.trainer} · {o.key} → <span className="text-ink">{o.accel || "(desactivada)"}</span>
                </li>
              ))}
            </ul>
            <p className="mt-1 text-xs text-muted">Es canvien des de la pantalla del trainer, amb el joc adjuntat.</p>
          </div>
        )}
      </section>
      <LogViewer />
    </div>
  );
}
