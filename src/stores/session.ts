import { create } from "zustand";
import * as ipc from "../lib/ipc";

type Pending = { pid: number; name: string; message: string };

type State = {
  info: ipc.SessionInfo | null;
  busy: boolean;
  error: string | null;
  /** Avís d'anti-cheat del sistema que l'usuari ha de confirmar */
  pending: Pending | null;
  /** Motiu de l'última desconnexió (p. ex. "El joc s'ha tancat") */
  lastDetach: string | null;
  start: () => Promise<void>;
  attach: (pid: number, name: string, acknowledge?: boolean) => Promise<boolean>;
  confirmPending: () => Promise<void>;
  cancelPending: () => void;
  detach: () => Promise<void>;
};

let started = false;

export const useSession = create<State>((set, get) => ({
  info: null,
  busy: false,
  error: null,
  pending: null,
  lastDetach: null,

  start: async () => {
    if (started) return;
    started = true;
    await ipc.onSessionState((e) =>
      e.status === "attached"
        ? set({ info: e.info, lastDetach: null })
        : set({ info: null, lastDetach: e.reason }),
    );
    set({ info: await ipc.sessionInfo() });
  },

  attach: async (pid, name, acknowledge = false) => {
    set({ busy: true, error: null, pending: null });
    try {
      // L'esdeveniment `session-state` també ho actualitza; posar-ho aquí evita dependre'n
      set({ busy: false, info: await ipc.attach(pid, acknowledge), lastDetach: null });
      return true;
    } catch (e) {
      const err = ipc.asError(e);
      if (err.code === "ANTICHEAT_WARNING") set({ busy: false, pending: { pid, name, message: err.message } });
      else set({ busy: false, error: err.message });
      return false;
    }
  },

  confirmPending: async () => {
    const p = get().pending;
    if (p) await get().attach(p.pid, p.name, true);
  },

  cancelPending: () => set({ pending: null }),

  detach: async () => {
    await ipc.detach();
    set({ info: null, lastDetach: "Desadjuntat per l'usuari" });
  },
}));
