import type { ReactNode } from "react";

const tones = {
  neutral: "border-line text-muted",
  accent: "border-accent/40 text-accent",
  ok: "border-ok/40 text-ok",
  warn: "border-warn/40 text-warn",
  bad: "border-bad/40 text-bad",
};

export function Badge({ tone = "neutral", children }: { tone?: keyof typeof tones; children: ReactNode }) {
  return (
    <span className={`inline-flex items-center rounded-md border px-1.5 py-0.5 font-mono text-[11px] uppercase ${tones[tone]}`}>
      {children}
    </span>
  );
}
