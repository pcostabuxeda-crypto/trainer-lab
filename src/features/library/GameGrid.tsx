import { useEffect, useMemo, useState } from "react";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import type { RunningGame, Store } from "../../lib/ipc";
import { useGames } from "../../stores/games";
import { useSession } from "../../stores/session";

const STORE_LABEL: Record<Store, string> = { steam: "Steam", epic: "Epic", gog: "GOG", manual: "Manual" };

export function GameGrid() {
  const { games, running, loading, error, start, refresh, addManual, removeManual } = useGames();
  const { info, busy, attach } = useSession();
  const [query, setQuery] = useState("");
  const [onlyTrainer, setOnlyTrainer] = useState(false);

  useEffect(() => void start(), [start]);

  // Procés en marxa de cada joc (el primer, que és el que té trainer si n'hi ha)
  const runningByGame = useMemo(() => {
    const m = new Map<string, RunningGame>();
    for (const r of running) if (r.gameId && !m.has(r.gameId)) m.set(r.gameId, r);
    return m;
  }, [running]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return games
      .filter((g) => (!onlyTrainer || g.trainerId) && (!q || g.name.toLowerCase().includes(q)))
      .sort((a, b) => Number(runningByGame.has(b.id)) - Number(runningByGame.has(a.id)));
  }, [games, query, onlyTrainer, runningByGame]);

  return (
    <section className="flex min-h-0 flex-col rounded-xl border border-line bg-panel">
      <header className="flex flex-wrap items-center gap-3 border-b border-line p-3">
        <h2 className="font-semibold">Jocs</h2>
        <span className="text-sm text-muted">
          {visible.length} de {games.length}
          {running.length > 0 && ` · ${running.length} en marxa`}
        </span>
        <div className="ml-auto flex flex-wrap items-center gap-3">
          <label className="flex items-center gap-2 text-sm text-muted">
            <input type="checkbox" checked={onlyTrainer} onChange={(e) => setOnlyTrainer(e.target.checked)} />
            Només amb trainer
          </label>
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Cerca un joc"
            aria-label="Cerca jocs"
            className="w-48 rounded-lg border border-line bg-panel-2 px-3 py-1.5 text-sm outline-none placeholder:text-muted focus:border-accent"
          />
          <Button onClick={() => void addManual()} disabled={loading}>
            Afegeix .exe
          </Button>
          <Button onClick={() => void refresh()} disabled={loading}>
            {loading ? "Cercant…" : "Torna a buscar"}
          </Button>
        </div>
      </header>

      {error && (
        <p role="alert" className="border-b border-line bg-bad/10 p-3 text-sm text-bad">
          {error}
        </p>
      )}

      {running.some((r) => !r.gameId) && (
        <div className="flex flex-wrap gap-2 border-b border-line p-3">
          <span className="self-center text-sm text-muted">En marxa amb trainer:</span>
          {running
            .filter((r) => !r.gameId)
            .map((r) => (
              <span key={r.pid} className="flex items-center gap-2 rounded-lg border border-ok/50 bg-ok/5 py-1 pr-1 pl-3 text-sm">
                {r.name} <span className="font-mono text-xs text-muted">{r.processName}</span>
                {info?.pid === r.pid ? (
                  <Badge tone="ok">adjuntat</Badge>
                ) : (
                  <Button variant="primary" disabled={busy} onClick={() => void attach(r.pid, r.processName)}>
                    Adjunta
                  </Button>
                )}
              </span>
            ))}
        </div>
      )}

      <div className="grid min-h-0 grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3 overflow-auto p-3">
        {visible.map((g) => {
          const r = runningByGame.get(g.id);
          const attached = r && info?.pid === r.pid;
          return (
            <article
              key={g.id}
              className={`flex flex-col gap-2 rounded-lg border p-3 ${r ? "border-ok/50 bg-ok/5" : "border-line bg-panel-2"}`}
            >
              <div className="flex items-start gap-2">
                <h3 className="min-w-0 flex-1 truncate font-medium" title={g.name}>
                  {g.name}
                </h3>
                <Badge>{STORE_LABEL[g.store]}</Badge>
              </div>
              <p className="selectable truncate font-mono text-[11px] text-muted" title={g.installDir}>
                {g.installDir}
              </p>
              <div className="mt-auto flex flex-wrap items-center gap-2">
                {g.trainerId ? <Badge tone="accent">trainer</Badge> : <Badge>sense trainer</Badge>}
                {r && <Badge tone="ok">en marxa · {r.processName}</Badge>}
                {g.store === "manual" && g.exe && (
                  <button
                    onClick={() => void removeManual(g.exe!)}
                    className="text-xs text-muted underline-offset-2 hover:text-bad hover:underline"
                  >
                    Treu
                  </button>
                )}
                {r &&
                  (attached ? (
                    <Badge tone="ok">adjuntat</Badge>
                  ) : (
                    <Button className="ml-auto" variant="primary" disabled={busy} onClick={() => void attach(r.pid, r.processName)}>
                      Adjunta
                    </Button>
                  ))}
              </div>
            </article>
          );
        })}
        {!loading && visible.length === 0 && (
          <p className="col-span-full p-6 text-center text-sm text-muted">
            {games.length === 0 ? "No s'ha trobat cap joc de Steam, Epic ni GOG. Afegeix-ne un amb «Afegeix .exe»." : "Cap joc coincideix."}
          </p>
        )}
      </div>
    </section>
  );
}
