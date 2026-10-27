import { Button } from "../../components/ui/Button";
import { useSession } from "../../stores/session";
import { DebugPanel } from "./DebugPanel";

export function TrainerPage() {
  const { info, lastDetach, detach } = useSession();

  if (!info) {
    return (
      <div className="rounded-xl border border-dashed border-line p-8 text-center">
        <h2 className="text-lg font-semibold">Cap joc adjuntat</h2>
        <p className="mx-auto mt-2 max-w-md text-sm text-muted">
          {lastDetach ? `${lastDetach}. ` : ""}Ves a la Biblioteca i prem «Adjunta» al costat del procés del joc.
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-4">
      <div className="flex flex-wrap items-center gap-4 rounded-xl border border-line bg-panel p-4">
        <div>
          <h2 className="text-lg font-semibold">{info.name}</h2>
          <p className="font-mono text-xs text-muted">
            PID {info.pid} · {info.arch} · base {info.mainBase} · {info.moduleCount} mòduls
          </p>
        </div>
        {info.warnings.length > 0 && (
          <p className="text-sm text-warn">Anti-cheat al sistema acceptat: {info.warnings.join(", ")}</p>
        )}
        <Button className="ml-auto" onClick={() => void detach()}>
          Desadjunta
        </Button>
      </div>
      <p className="text-sm text-muted">
        Mode depuració (Fase 2): llegeix, escriu i congela valors a mà. A la Fase 3 aquí sortiran els trucs de la
        definició del joc.
      </p>
      <DebugPanel />
    </div>
  );
}
