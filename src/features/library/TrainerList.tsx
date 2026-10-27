import { useEffect } from "react";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import { useTrainers } from "../../stores/trainers";

/** Definicions de trainers carregades i errors de validació. */
export function TrainerList() {
  const { catalog, error, start, reload } = useTrainers();
  useEffect(() => void start(), [start]);

  return (
    <section className="rounded-xl border border-line bg-panel">
      <header className="flex flex-wrap items-center gap-3 border-b border-line p-3">
        <h2 className="font-semibold">Trainers</h2>
        <span className="text-sm text-muted">{catalog?.trainers.length ?? 0} definicions</span>
        <Button className="ml-auto" onClick={() => void reload()}>
          Recarrega
        </Button>
      </header>
      {error && <p className="p-3 text-sm text-bad">{error}</p>}
      <ul className="divide-y divide-line/60">
        {catalog?.trainers.map((t) => (
          <li key={t.id} className="flex flex-wrap items-center gap-3 px-3 py-2 text-sm">
            <span className="font-medium">{t.name}</span>
            <span className="font-mono text-xs text-muted">{t.executable}</span>
            <span className="text-muted">{t.cheatCount} trucs</span>
            {t.online && <Badge tone="bad">online · bloquejat</Badge>}
            <Badge tone={t.source === "user" ? "accent" : "neutral"}>{t.source === "user" ? "teu" : "exemple"}</Badge>
          </li>
        ))}
      </ul>
      {catalog && catalog.errors.length > 0 && (
        <div className="border-t border-line p-3">
          <h3 className="mb-1 text-sm font-semibold text-bad">Definicions amb errors</h3>
          <ul className="selectable flex flex-col gap-2 text-xs">
            {catalog.errors.map((e) => (
              <li key={e.path}>
                <p className="font-mono text-muted">{e.path}</p>
                <ul className="ml-4 list-disc text-bad">
                  {e.errors.map((m) => (
                    <li key={m}>{m}</li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>
        </div>
      )}
      {catalog && (
        <p className="border-t border-line p-3 text-xs text-muted">
          Les teves definicions van a <span className="selectable font-mono text-ink">{catalog.userDir}</span>. Es
          recarreguen soles quan les deses.
        </p>
      )}
    </section>
  );
}
