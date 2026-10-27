import { useCallback, useEffect, useState } from "react";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import * as ipc from "../../lib/ipc";
import type { ScanFilter, ScanHit, ScanSummary, ValueType } from "../../lib/ipc";
import { useSession } from "../../stores/session";
import { SaveCheatDialog } from "./SaveCheatDialog";

const TYPES: ValueType[] = ["i32", "f32", "i64", "f64", "i16", "u8", "u16", "u32", "u64", "i8", "decimal"];
const FILTERS: { id: ScanFilter["kind"]; label: string }[] = [
  { id: "exact", label: "Valor exacte" },
  { id: "decreased", label: "Ha disminuït" },
  { id: "increased", label: "Ha augmentat" },
  { id: "changed", label: "Ha canviat" },
  { id: "unchanged", label: "No ha canviat" },
];
/** Amb pocs resultats, els valors es refresquen sols. */
const LIVE_LIMIT = 100;

export function ScannerPage({ onSaved }: { onSaved: () => void }) {
  const { info } = useSession();
  const [type, setType] = useState<ValueType>("i32");
  const [value, setValue] = useState("");
  const [filter, setFilter] = useState<ScanFilter["kind"]>("exact");
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [hits, setHits] = useState<ScanHit[]>([]);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [saving, setSaving] = useState<ScanHit | null>(null);

  const loadHits = useCallback(() => ipc.scanResults(0, LIVE_LIMIT).then(setHits, () => {}), []);

  // Si canvies de joc, l'escaneig anterior ja no serveix
  useEffect(() => {
    setSummary(null);
    setHits([]);
  }, [info?.pid]);

  useEffect(() => {
    if (!summary || summary.count === 0 || summary.count > LIVE_LIMIT) return;
    const t = setInterval(() => void loadHits(), 1000);
    return () => clearInterval(t);
  }, [summary, loadHits]);

  const run = async (f: () => Promise<ScanSummary>) => {
    setBusy(true);
    setMsg(null);
    try {
      const s = await f();
      setSummary(s);
      await loadHits();
    } catch (e) {
      setMsg({ ok: false, text: ipc.errorMessage(e) });
    } finally {
      setBusy(false);
    }
  };

  const numberValue = () => {
    const n = Number(value.replace(",", "."));
    if (value.trim() === "" || !Number.isFinite(n)) {
      setMsg({ ok: false, text: "Escriu el valor que veus al joc." });
      return null;
    }
    return n;
  };

  const first = () => {
    const n = numberValue();
    if (n !== null) void run(() => ipc.scanFirst(type, n));
  };

  const next = () => {
    if (filter === "exact") {
      const n = numberValue();
      if (n !== null) void run(() => ipc.scanNext({ kind: "exact", value: n }));
    } else void run(() => ipc.scanNext({ kind: filter } as ScanFilter));
  };

  const reset = async () => {
    await ipc.scanReset().catch(() => {});
    setSummary(null);
    setHits([]);
    setMsg(null);
  };

  const write = async (h: ScanHit) => {
    const n = numberValue();
    if (n === null) return;
    try {
      const r = await ipc.scanWrite(h.address, type, n);
      setMsg({ ok: true, text: `Escrit ${r.value} a ${r.address}` });
      void loadHits();
    } catch (e) {
      setMsg({ ok: false, text: ipc.errorMessage(e) });
    }
  };

  if (!info) {
    return (
      <div className="rounded-xl border border-dashed border-line p-8 text-center">
        <h2 className="text-lg font-semibold">Escàner de memòria</h2>
        <p className="mx-auto mt-2 max-w-md text-sm text-muted">Primer adjunta't a un joc des de la Biblioteca.</p>
      </div>
    );
  }

  const field = "rounded-lg border border-line bg-panel-2 px-2 py-1.5 text-sm text-ink outline-none focus:border-accent";

  return (
    <div className="flex h-full min-h-0 flex-col gap-4">
      <section className="flex flex-col gap-3 rounded-xl border border-line bg-panel p-4">
        <div className="flex flex-wrap items-end gap-3">
          <label className="flex flex-col gap-1 text-sm text-muted">
            Tipus
            <select value={type} disabled={!!summary} onChange={(e) => setType(e.target.value as ValueType)} className={field}>
              {TYPES.map((t) => (
                <option key={t}>{t}</option>
              ))}
            </select>
          </label>
          {summary && (
            <label className="flex flex-col gap-1 text-sm text-muted">
              Condició
              <select value={filter} onChange={(e) => setFilter(e.target.value as ScanFilter["kind"])} className={field}>
                {FILTERS.map((f) => (
                  <option key={f.id} value={f.id}>
                    {f.label}
                  </option>
                ))}
              </select>
            </label>
          )}
          {(!summary || filter === "exact") && (
            <label className="flex flex-col gap-1 text-sm text-muted">
              Valor
              <input
                value={value}
                onChange={(e) => setValue(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && !busy && (summary ? next() : first())}
                inputMode="decimal"
                placeholder="100"
                className={`${field} w-36 font-mono`}
              />
            </label>
          )}
          {summary ? (
            <>
              <Button variant="primary" disabled={busy} onClick={next}>
                {busy ? "Cercant…" : "Cerca següent"}
              </Button>
              <Button disabled={busy} onClick={() => void reset()}>
                Nova cerca
              </Button>
            </>
          ) : (
            <Button variant="primary" disabled={busy} onClick={first}>
              {busy ? "Cercant…" : "Primera cerca"}
            </Button>
          )}
        </div>
        <p className="text-xs text-muted">
          Busca el valor que veus al joc, canvia'l jugant (rep mal, gasta diners) i refina fins que quedin poques adreces.
          Els decimals es comparen amb una tolerància de 0,01.
        </p>
        {summary && (
          <p className="text-sm" aria-live="polite">
            <strong>{summary.count.toLocaleString("ca")}</strong> resultats · cerca {summary.scans} · {summary.elapsedMs} ms
            {summary.truncated && <span className="text-warn"> · n'hi havia més: refina la cerca</span>}
            {summary.readFailures > 0 && <span className="text-warn"> · {summary.readFailures} candidats no llegibles</span>}
          </p>
        )}
        {msg && (
          <p role="status" className={`text-sm ${msg.ok ? "text-ok" : "text-bad"}`}>
            {msg.text}
          </p>
        )}
      </section>

      {summary && summary.count > 0 && (
        <section className="min-h-0 flex-1 overflow-auto rounded-xl border border-line bg-panel">
          {summary.count > LIVE_LIMIT && (
            <p className="border-b border-line p-3 text-sm text-muted">
              Es mostren els primers {LIVE_LIMIT}. Amb menys resultats, els valors s'actualitzen sols.
            </p>
          )}
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-panel text-left text-xs uppercase text-muted">
              <tr>
                <th className="px-3 py-2 font-medium">Adreça</th>
                <th className="px-3 py-2 font-medium">On</th>
                <th className="px-3 py-2 font-medium">Valor</th>
                <th className="px-3 py-2 font-medium">Anterior</th>
                <th className="px-3 py-2" />
              </tr>
            </thead>
            <tbody>
              {hits.map((h) => (
                <tr key={h.address} className="border-t border-line/60 hover:bg-panel-2">
                  <td className="selectable px-3 py-1.5 font-mono text-xs">{h.address}</td>
                  <td className="px-3 py-1.5">
                    {h.moduleOffset ? <span className="font-mono text-xs">{h.moduleOffset}</span> : <Badge>dinàmica</Badge>}
                  </td>
                  <td className="px-3 py-1.5 font-mono">{h.value ?? "—"}</td>
                  <td className="px-3 py-1.5 font-mono text-muted">{h.previous}</td>
                  <td className="flex justify-end gap-2 px-3 py-1">
                    <Button onClick={() => void write(h)} title="Escriu el valor del camp «Valor» en aquesta adreça">
                      Escriu
                    </Button>
                    <Button variant="primary" onClick={() => setSaving(h)}>
                      Desa com a truc
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}

      {saving && (
        <SaveCheatDialog
          hit={saving}
          valueType={type}
          onClose={() => setSaving(null)}
          onSaved={(path) => {
            setSaving(null);
            setMsg({ ok: true, text: `Truc desat a ${path}` });
            onSaved();
          }}
        />
      )}
    </div>
  );
}
