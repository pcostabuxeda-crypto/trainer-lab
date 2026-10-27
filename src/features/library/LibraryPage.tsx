import { GameGrid } from "./GameGrid";
import { ProcessList } from "./ProcessList";
import { TrainerList } from "./TrainerList";

export function LibraryPage() {
  return (
    <div className="flex h-full min-h-0 flex-col gap-4 overflow-auto pr-1">
      <GameGrid />
      <TrainerList />
      <details className="rounded-xl border border-line bg-panel">
        <summary className="cursor-pointer p-3 text-sm text-muted">
          Tots els processos (per adjuntar-te a un joc que no surt a la llista)
        </summary>
        <div className="flex h-[420px] flex-col">
          <ProcessList />
        </div>
      </details>
    </div>
  );
}
