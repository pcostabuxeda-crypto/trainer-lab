import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import * as ipc from "../../lib/ipc";
import type { CheatInput, TrainerView, VersionStatus } from "../../lib/ipc";
import { useNav } from "../../stores/nav";
import { useSession } from "../../stores/session";
import { useAdvanced } from "../../stores/settings";
import { CheatCard } from "./CheatCard";
import { DebugPanel } from "./DebugPanel";

const VERSION: Record<VersionStatus, { tone: "ok" | "warn" | "neutral"; text: string }> = {
  verified: { tone: "ok", text: "versió compatible" },
  unverified: { tone: "warn", text: "versió no verificada" },
  unknown: { tone: "neutral", text: "sense info de versió" },
};

export function TrainerPage() {
  const { info, lastDetach, detach } = useSession();
  const [view, setView] = useState<TrainerView | null>(null);
  const [toast, setToast] = useState<{ ok: boolean; text: string } | null>(null);
  const [temps, setTemps] = useState<string[]>([]);
  const advanced = useAdvanced();
  const go = useNav((s) => s.go);
  const viewRef = useRef(view);
  viewRef.current = view;

  const load = useCallback(() => ipc.trainerView().then(setView, (e) => setToast({ ok: false, text: ipc.errorMessage(e) })), []);

  useEffect(() => {
    if (!info) return setView(null);
    void load();
    ipc.listFrozen().then((k) => setTemps(k.filter((x) => x.startsWith("temp:"))), () => {});
    // Valors en directe: cada 500 ms mentre aquesta pantalla és oberta
    const timer = setInterval(() => {
      ipc.cheatStates().then(
        (states) => setView((v) => v && { ...v, cheats: v.cheats.map((c, i) => ({ ...c, state: states[i] ?? c.state })) }),
        () => {},
      );
    }, 500);
    const unlisten = ipc.onHotkeyFired((e) => {
      const name = viewRef.current?.cheats.find((c) => c.def.id === e.cheatId)?.def.name ?? e.cheatId;
      setToast({ ok: e.ok, text: e.ok ? `Drecera: ${name}` : `${name}: ${e.message}` });
    });
    return () => {
      clearInterval(timer);
      void unlisten.then((f) => f());
    };
  }, [info?.pid, load]);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 3000);
    return () => clearTimeout(t);
  }, [toast]);

  const groups = useMemo(() => {
    const m = new Map<string, TrainerView["cheats"]>();
    for (const c of view?.cheats ?? []) m.set(c.def.category ?? "General", [...(m.get(c.def.category ?? "General") ?? []), c]);
    return [...m];
  }, [view]);

  const onInput = async (id: string, input: CheatInput) => {
    try {
      const state = await ipc.setCheat(id, input);
      setView((v) => v && { ...v, cheats: v.cheats.map((c) => (c.def.id === id ? { ...c, state } : c)) });
    } catch (e) {
      setToast({ ok: false, text: ipc.errorMessage(e) });
      void load();
    }
  };

  const onHotkey = async (key: string, accelerator: string | null) => {
    try {
      await ipc.setHotkey(key, accelerator);
    } catch (e) {
      setToast({ ok: false, text: ipc.errorMessage(e) });
    }
    void load();
  };

  if (!info) {
    return (
      <div className="rounded-xl border border-dashed border-line p-8 text-center">
        <h2 className="text-lg font-semibold">Cap joc adjuntat</h2>
        <p className="mx-auto mt-2 max-w-md text-sm text-muted">
          {lastDetach ? `${lastDetach}. ` : ""}Obre un joc des de la Biblioteca amb «Juga»: quan s'obri, l'app s'hi connectarà sola.
        </p>
      </div>
    );
  }

  const version = view ? VERSION[view.version.status] : null;

  return (
    <div className="flex h-full min-h-0 flex-col gap-4">
      <div className="flex flex-wrap items-center gap-4 rounded-xl border border-line bg-panel p-4">
        <div>
          <h2 className="text-lg font-semibold">{view?.name ?? info.name}</h2>
          {advanced && (
            <p className="font-mono text-xs text-muted">
              {info.name} · PID {info.pid} · {info.arch}
            </p>
          )}
        </div>
        {version && (
          <span title={view?.version.sha256 ? `SHA-256 ${view.version.sha256}` : undefined}>
            <Badge tone={version.tone}>
              {version.text}
              {view?.version.label ? ` · ${view.version.label}` : ""}
            </Badge>
          </span>
        )}
        {info.warnings.length > 0 && <Badge tone="warn">avís acceptat: {info.warnings.join(", ")}</Badge>}
        <Button className="ml-auto" variant="primary" onClick={() => go("scanner")}>
          + Crea un truc
        </Button>
        <Button onClick={() => void detach()}>Desconnecta</Button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto pr-1">
        {view ? (
          groups.map(([category, cheats]) => (
            <section key={category} className="mb-5">
              <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted">{category}</h3>
              <div className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-3">
                {cheats.map((c) => (
                  <CheatCard
                    key={c.def.id}
                    cheat={c}
                    onInput={(input) => void onInput(c.def.id, input)}
                    onHotkey={(k, a) => void onHotkey(k, a)}
                  />
                ))}
              </div>
            </section>
          ))
        ) : (
          <div className="mb-4 rounded-xl border border-dashed border-line p-6 text-center">
            <p className="text-sm text-muted">Aquest joc encara no té trucs.</p>
            <Button className="mt-3" variant="primary" onClick={() => go("scanner")}>
              Crea el primer truc
            </Button>
          </div>
        )}

        {temps.length > 0 && (
          <section className="mb-5 rounded-xl border border-warn/40 bg-panel p-4">
            <h3 className="text-sm font-semibold">Trucs temporals</h3>
            <p className="mb-2 text-xs text-muted">Actius fins que tanquis el joc. No es desen.</p>
            <ul className="flex flex-col gap-1">
              {temps.map((k) => (
                <li key={k} className="flex items-center justify-between gap-2 text-sm">
                  <span className="font-mono text-xs text-muted">{advanced ? k : "valor congelat"}</span>
                  <Button onClick={() => void ipc.unfreeze(k).then(() => setTemps((t) => t.filter((x) => x !== k)))}>Atura</Button>
                </li>
              ))}
            </ul>
          </section>
        )}

        {advanced && (
        <details className="mt-2 rounded-xl border border-line bg-panel" open={!view}>
          <summary className="cursor-pointer p-3 text-sm text-muted">Eines de depuració (llegir/escriure a mà)</summary>
          <div className="flex min-h-[420px] flex-col p-3 pt-0">
            <DebugPanel />
          </div>
        </details>
        )}
      </div>

      {toast && (
        <div
          role="status"
          className={`fixed right-5 bottom-5 rounded-lg border px-4 py-2 text-sm shadow-lg ${
            toast.ok ? "border-accent/40 bg-panel text-ink" : "border-bad/40 bg-panel text-bad"
          }`}
        >
          {toast.text}
        </div>
      )}
    </div>
  );
}
