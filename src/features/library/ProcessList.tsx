import { useEffect, useMemo, useState } from "react";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import { useProcesses } from "../../stores/processes";

export function ProcessList() {
  const { list, loading, error, refresh } = useProcesses();
  const [query, setQuery] = useState("");
  const [showSystem, setShowSystem] = useState(false);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return list.filter(
      (p) =>
        // Sense ruta = Windows no ens hi deixa accedir (processos protegits/del sistema)
        (showSystem || p.path !== null) &&
        (!q || p.name.toLowerCase().includes(q) || String(p.pid).includes(q)),
    );
  }, [list, query, showSystem]);

  return (
    <section className="flex min-h-0 flex-1 flex-col rounded-xl border border-line bg-panel">
      <header className="flex flex-wrap items-center gap-3 border-b border-line p-3">
        <h2 className="font-semibold">Processos en execució</h2>
        <span className="text-sm text-muted">
          {visible.length} de {list.length}
        </span>
        <div className="ml-auto flex items-center gap-3">
          <label className="flex items-center gap-2 text-sm text-muted">
            <input type="checkbox" checked={showSystem} onChange={(e) => setShowSystem(e.target.checked)} />
            Processos del sistema
          </label>
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Cerca per nom o PID"
            aria-label="Cerca processos"
            className="w-56 rounded-lg border border-line bg-panel-2 px-3 py-1.5 text-sm outline-none placeholder:text-muted focus:border-accent"
          />
          <Button onClick={() => void refresh()} disabled={loading}>
            {loading ? "Actualitzant…" : "Actualitza"}
          </Button>
        </div>
      </header>

      {error && <p className="border-b border-line bg-bad/10 p-3 text-sm text-bad">{error}</p>}

      <div className="min-h-0 flex-1 overflow-auto">
        <table className="w-full text-sm">
          <thead className="sticky top-0 bg-panel text-left text-xs uppercase text-muted">
            <tr>
              <th className="px-3 py-2 font-medium">Nom</th>
              <th className="px-3 py-2 font-medium">PID</th>
              <th className="px-3 py-2 font-medium">Arq.</th>
              <th className="px-3 py-2 font-medium">Ruta</th>
            </tr>
          </thead>
          <tbody>
            {visible.map((p) => {
              const isTestGame = p.name.toLowerCase() === "test-game.exe";
              return (
                <tr key={p.pid} className={`border-t border-line/60 ${isTestGame ? "bg-accent/5" : "hover:bg-panel-2"}`}>
                  <td className="px-3 py-1.5 font-medium">
                    {p.name} {isTestGame && <Badge tone="accent">joc de prova</Badge>}
                  </td>
                  <td className="px-3 py-1.5 font-mono text-muted">{p.pid}</td>
                  <td className="px-3 py-1.5">
                    <Badge tone={p.arch === "unknown" ? "neutral" : p.arch === "x86" ? "warn" : "ok"}>
                      {p.arch === "unknown" ? "?" : p.arch}
                    </Badge>
                  </td>
                  <td className="selectable max-w-0 truncate px-3 py-1.5 font-mono text-xs text-muted" title={p.path ?? ""}>
                    {p.path ?? "— sense accés"}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {!loading && visible.length === 0 && <p className="p-6 text-center text-sm text-muted">Cap procés coincideix.</p>}
      </div>
    </section>
  );
}
