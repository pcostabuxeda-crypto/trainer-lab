import { create } from "zustand";
import { getLogs, onLog, type LogEntry } from "../lib/ipc";

const MAX = 2000;

type State = {
  entries: LogEntry[];
  /** Carrega l'historial i s'escolta l'esdeveniment `log`. Es crida un cop, a l'arrencada. */
  start: () => Promise<void>;
};

let started = false;

export const useLogs = create<State>((set) => ({
  entries: [],
  start: async () => {
    if (started) return;
    started = true;
    // Primer escoltem i després llegim l'historial; descartem duplicats per `ts`+missatge
    const pending: LogEntry[] = [];
    let ready = false;
    await onLog((e) => {
      if (!ready) return void pending.push(e);
      set((s) => ({ entries: [...s.entries, e].slice(-MAX) }));
    });
    const history = await getLogs(MAX);
    const seen = new Set(history.map((e) => `${e.ts}|${e.message}`));
    ready = true;
    set({ entries: [...history, ...pending.filter((e) => !seen.has(`${e.ts}|${e.message}`))].slice(-MAX) });
  },
}));
