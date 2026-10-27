/** Pantalla encara no implementada: diu en quina fase arriba. */
export function Placeholder({ title, phase, children }: { title: string; phase: number; children: string }) {
  return (
    <div className="rounded-xl border border-dashed border-line p-8 text-center">
      <p className="text-sm text-muted">Fase {phase}</p>
      <h2 className="mt-1 text-lg font-semibold">{title}</h2>
      <p className="mx-auto mt-2 max-w-md text-sm text-muted">{children}</p>
    </div>
  );
}
