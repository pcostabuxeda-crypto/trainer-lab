import { create } from "zustand";
import { errorMessage, listProcesses, type ProcessInfo } from "../lib/ipc";

type State = {
  list: ProcessInfo[];
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
};

export const useProcesses = create<State>((set) => ({
  list: [],
  loading: false,
  error: null,
  refresh: async () => {
    set({ loading: true, error: null });
    try {
      set({ list: await listProcesses(), loading: false });
    } catch (e) {
      set({ error: errorMessage(e), loading: false });
    }
  },
}));
