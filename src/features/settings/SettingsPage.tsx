import { useEffect, useState } from "react";
import { Button } from "../../components/ui/Button";
import * as ipc from "../../lib/ipc";
import { LogViewer } from "./LogViewer";

export function SettingsPage() {
  const [settings, setSettings] = useState<ipc.Settings | null>(null);
  const [interval, setIntervalMs] = useState("100");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    ipc.getSettings().then((s) => {
      setSettings(s);
      setIntervalMs(String(s.freezeIntervalMs));
    }, (e) => setMsg({ ok: false, text: ipc.errorMessage(e) }));
  }, []);

  const save = async () => {
    if (!settings) return;
    const next = { ...settings, freezeIntervalMs: Number(interval) };
    try {
      await ipc.saveSettings(next);
      setSettings(next);
      setMsg({ ok: true, text: "Desat." });
    } catch (e) {
      setMsg({ ok: false, text: ipc.errorMessage(e) });
    }
  };

  const overrides = Object.entries(settings?.hotkeyOverrides ?? {}).flatMap(([trainer, keys]) =>
    Object.entries(keys ?? {}).map(([key, accel]) => ({ trainer, key, accel })),
  );

  return (
    <div className="flex h-full min-h-0 flex-col gap-4">
      <section className="flex flex-col gap-3 rounded-xl border border-line bg-panel p-4">
        <h2 className="font-semibold">Configuració</h2>
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
