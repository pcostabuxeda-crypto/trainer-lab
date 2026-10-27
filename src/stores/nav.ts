import { create } from "zustand";

export type PageId = "library" | "trainer" | "scanner" | "settings";

/** Pantalla actual. És global perquè qualsevol pantalla pugui portar a una altra. */
export const useNav = create<{ page: PageId; go: (p: PageId) => void }>((set) => ({
  page: "library",
  go: (page) => set({ page }),
}));
