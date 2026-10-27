import { useEffect, useState } from "react";
import { ConfirmDialog } from "../components/ui/ConfirmDialog";
import { Placeholder } from "../components/ui/Placeholder";
import { LibraryPage } from "../features/library/LibraryPage";
import { SettingsPage } from "../features/settings/SettingsPage";
import { TrainerPage } from "../features/trainer/TrainerPage";
import { useSession } from "../stores/session";

const PAGES = [
  { id: "library", label: "Biblioteca" },
  { id: "trainer", label: "Trainer" },
  { id: "scanner", label: "Escàner" },
  { id: "settings", label: "Configuració" },
] as const;

type PageId = (typeof PAGES)[number]["id"];

export default function App() {
  const [page, setPage] = useState<PageId>("library");
  const { info, pending, lastDetach, confirmPending, cancelPending } = useSession();

  // En adjuntar-se a un joc, anem directament a la seva pantalla
  useEffect(() => {
    if (info) setPage("trainer");
  }, [info?.pid]);

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
            onClick={() => setPage(p.id)}
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
          <span className={`size-2 rounded-full ${info ? "bg-ok" : "bg-muted"}`} />
          {info ? (
            <span>
              Adjuntat a <strong>{info.name}</strong> <span className="font-mono text-muted">PID {info.pid}</span>
            </span>
          ) : (
            <span className="text-muted">{lastDetach ?? "Cap joc adjuntat"}</span>
          )}
        </header>
        <main className="min-h-0 flex-1 overflow-hidden p-5">
          {page === "library" && <LibraryPage />}
          {page === "trainer" && <TrainerPage />}
          {page === "scanner" && (
            <Placeholder title="Escàner de memòria" phase={5}>
              Cerca de valors (exacte, canviat, augmentat…) i desar troballes com a trucs nous.
            </Placeholder>
          )}
          {page === "settings" && <SettingsPage />}
        </main>
      </div>

      {pending && (
        <ConfirmDialog
          title="Anti-cheat en marxa al sistema"
          message={pending.message}
          confirmLabel={`Adjunta igualment a ${pending.name}`}
          onConfirm={() => void confirmPending()}
          onCancel={cancelPending}
        />
      )}
    </div>
  );
}
