import { create } from "zustand";
import * as ipc from "../lib/ipc";

type State = {
  settings: ipc.Settings | null;
  load: () => Promise<void>;
  /** Desa només els camps que canvien; retorna l'error si n'hi ha. */
  update: (patch: Partial<ipc.Settings>) => Promise<string | null>;
};

export const useSettings = create<State>((set, get) => ({
  settings: null,
  load: async () => {
    try {
      set({ settings: await ipc.getSettings() });
    } catch {
      // Sense backend (p. ex. al navegador) es queden els valors per defecte de la UI
    }
  },
  update: async (patch) => {
    const current = get().settings ?? (await ipc.getSettings());
    const next = { ...current, ...patch };
    try {
      await ipc.saveSettings(next);
      set({ settings: next });
      return null;
    } catch (e) {
      return ipc.errorMessage(e);
    }
  },
}));

/** Mode avançat: adreces, eines de depuració i escàner tècnic. */
export const useAdvanced = () => useSettings((s) => s.settings?.advancedMode ?? false);
