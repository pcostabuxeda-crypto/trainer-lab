import { create } from "zustand";
import * as ipc from "../lib/ipc";

type State = {
  catalog: ipc.CatalogView | null;
  error: string | null;
  start: () => Promise<void>;
  reload: () => Promise<void>;
};

let started = false;

export const useTrainers = create<State>((set) => ({
  catalog: null,
  error: null,
  start: async () => {
    if (started) return;
    started = true;
    const load = () => ipc.listTrainers().then((catalog) => set({ catalog, error: null }), (e) => set({ error: ipc.errorMessage(e) }));
    await ipc.onTrainersChanged(() => void load());
    await load();
  },
  reload: async () => {
    try {
      set({ catalog: await ipc.reloadTrainers(), error: null });
    } catch (e) {
      set({ error: ipc.errorMessage(e) });
    }
  },
}));
