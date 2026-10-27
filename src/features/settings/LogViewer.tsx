import { useEffect, useMemo, useRef, useState } from "react";
import type { LogLevel } from "../../lib/ipc";
import { useLogs } from "../../stores/logs";

const ORDER: LogLevel[] = ["trace", "debug", "info", "warn", "error"];
const COLORS: Record<LogLevel, string> = {
  trace: "text-muted/60",
  debug: "text-muted",
  info: "text-ok",
  warn: "text-warn",
  error: "text-bad",
};

export function LogViewer() {
  const entries = useLogs((s) => s.entries);
  const [min, setMin] = useState<LogLevel>("debug");
  const bottom = useRef<HTMLDivElement>(null);

  const visible = useMemo(() => entries.filter((e) => ORDER.indexOf(e.level) >= ORDER.indexOf(min)), [entries, min]);

  useEffect(() => bottom.current?.scrollIntoView({ block: "end" }), [visible.length]);

  return (
    <section className="flex min-h-0 flex-1 flex-col rounded-xl border border-line bg-panel">
      <header className="flex items-center gap-3 border-b border-line p-3">
        <h2 className="font-semibold">Logs</h2>
        <span className="text-sm text-muted">{visible.length} entrades</span>
        <label className="ml-auto flex items-center gap-2 text-sm text-muted">
          Nivell mínim
          <select
            value={min}
            onChange={(e) => setMin(e.target.value as LogLevel)}
            className="rounded-lg border border-line bg-panel-2 px-2 py-1 text-ink"
          >
            {ORDER.map((l) => (
              <option key={l} value={l}>
                {l}
              </option>
            ))}
          </select>
        </label>
      </header>
      <div className="selectable min-h-0 flex-1 overflow-auto p-3 font-mono text-xs leading-relaxed">
        {visible.map((e, i) => (
          <div key={`${e.ts}-${i}`} className="flex gap-3">
            <span className="shrink-0 text-muted">{new Date(e.ts).toLocaleTimeString()}</span>
            <span className={`w-11 shrink-0 uppercase ${COLORS[e.level]}`}>{e.level}</span>
            <span className="shrink-0 text-muted">{e.target}</span>
            <span className="break-all">{e.message}</span>
          </div>
        ))}
        <div ref={bottom} />
      </div>
    </section>
  );
}
