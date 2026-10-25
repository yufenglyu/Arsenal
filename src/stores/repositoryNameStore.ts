import { create } from "zustand";
import { persist } from "zustand/middleware";

export const useRepositoryNameStore = create<{
  repositoryFirst: boolean;
  toggle: () => void;
}>()(
  persist(
    (set) => ({
      repositoryFirst: false,
      toggle: () => set(state => ({ repositoryFirst: !state.repositoryFirst })),
    }),
    { name: "arsenal-repository-name-format" },
  ),
);
