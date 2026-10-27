import { create } from "zustand";
import * as ipc from "../lib/ipc";

type State = {
  games: ipc.GameEntry[];
  running: ipc.RunningGame[];
  loading: boolean;
  error: string | null;
  start: () => Promise<void>;
  refresh: () => Promise<void>;
  addManual: () => Promise<void>;
  removeManual: (path: string) => Promise<void>;
};

let started = false;

export const useGames = create<State>((set) => {
  // Les tres accions que tornen la llista de jocs comparteixen la gestió d'errors
  const run = async (f: () => Promise<ipc.GameEntry[] | null>) => {
    set({ loading: true, error: null });
    try {
      const games = await f();
      set(games ? { games, loading: false } : { loading: false });
    } catch (e) {
      set({ loading: false, error: ipc.errorMessage(e) });
    }
  };

  return {
    games: [],
    running: [],
    loading: false,
    error: null,
    start: async () => {
      if (started) return;
      started = true;
      await ipc.onRunningGames((running) => set({ running }));
      ipc.runningGames().then((running) => set({ running }), () => {});
      await run(() => ipc.listGames());
    },
    refresh: () => run(() => ipc.listGames(true)),
    addManual: () => run(ipc.addManualGame),
    removeManual: (path) => run(() => ipc.removeManualGame(path)),
  };
});
