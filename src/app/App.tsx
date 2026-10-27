import { useEffect } from "react";
import { ConfirmDialog } from "../components/ui/ConfirmDialog";
import { LibraryPage } from "../features/library/LibraryPage";
import { CheatWizard } from "../features/scanner/CheatWizard";
import { ScannerPage } from "../features/scanner/ScannerPage";
import { SettingsPage } from "../features/settings/SettingsPage";
import { TrainerPage } from "../features/trainer/TrainerPage";
import { useGames } from "../stores/games";
import { type PageId, useNav } from "../stores/nav";
import { useSession } from "../stores/session";
import { useAdvanced } from "../stores/settings";

const PAGES: { id: PageId; label: string }[] = [
  { id: "library", label: "Biblioteca" },
  { id: "trainer", label: "Trucs" },
  { id: "scanner", label: "Crea trucs" },
  { id: "settings", label: "Configuració" },
];

export default function App() {
  const { page, go } = useNav();
  const advanced = useAdvanced();
  const { info, busy, pending, lastDetach, attach, confirmPending, cancelPending } = useSession();
  // Joc amb trainer en marxa i encara no adjuntat: el suggerim a la capçalera
  const detected = useGames((s) => s.running.find((r) => r.trainerId));

  // En adjuntar-se a un joc, anem directament als seus trucs
  useEffect(() => {
    if (info) go("trainer");
  }, [info?.pid, go]);

  return (
    <div className="flex h-full">
      <nav className="flex w-52 shrink-0 flex-col gap-1 border-r border-line bg-panel p-3" aria-label="Navegació principal">
        <div className="mb-5 flex items-center gap-2 px-2 pt-1">
          <span className="grid size-7 place-items-center rounded-md bg-accent font-mono text-sm font-bold text-accent-ink">
            TL
          </span>
          <span className="font-semibold tracking-tight">Trainer Lab</span>
        </div>
        {PAGES.map((p) => (
          <button
            key={p.id}
            onClick={() => go(p.id)}
            aria-current={page === p.id ? "page" : undefined}
            className={`rounded-lg px-3 py-2 text-left text-sm transition ${
              page === p.id ? "bg-panel-2 text-ink shadow-[inset_2px_0_0_var(--color-accent)]" : "text-muted hover:text-ink"
            }`}
          >
            {p.label}
          </button>
        ))}
        <p className="mt-auto px-2 text-xs text-muted">Només jocs single-player i offline.</p>
      </nav>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center gap-3 border-b border-line px-5 text-sm" aria-live="polite">
          <span className={`size-2 rounded-full ${info ? "bg-ok" : detected ? "bg-warn" : "bg-muted"}`} />
          {info ? (
            <span>
              Connectat a <strong>{info.trainer?.name ?? info.name}</strong>
              {advanced && <span className="font-mono text-muted"> · PID {info.pid}</span>}
            </span>
          ) : detected ? (
            <>
              <span>
                Joc obert: <strong>{detected.name}</strong>
              </span>
              <button
                disabled={busy}
                onClick={() => void attach(detected.pid, detected.processName)}
                className="rounded-md bg-accent px-2 py-0.5 text-xs font-medium text-accent-ink hover:brightness-110 disabled:opacity-50"
              >
                Connecta
              </button>
            </>
          ) : (
            <span className="text-muted">{lastDetach ?? "Obre un joc des de la Biblioteca"}</span>
          )}
        </header>
        <main className="min-h-0 flex-1 overflow-hidden p-5">
          {page === "library" && <LibraryPage />}
          {page === "trainer" && <TrainerPage />}
          {page === "scanner" && (advanced ? <ScannerPage onSaved={() => go("trainer")} /> : <CheatWizard onDone={() => go("trainer")} />)}
          {page === "settings" && <SettingsPage />}
        </main>
      </div>

      {pending && (
        <ConfirmDialog
          title="Anti-cheat en marxa al sistema"
          message={pending.message}
          confirmLabel={`Connecta igualment a ${pending.name}`}
          onConfirm={() => void confirmPending()}
          onCancel={cancelPending}
        />
      )}
    </div>
  );
}
