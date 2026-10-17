import { useSyncExternalStore } from "react";
import { reducer, type Action, type AppState } from "./machine";

/** Minimal store around the pure reducer; readable outside React. */
export interface Store {
  getState(): AppState;
  dispatch(a: Action): void;
  subscribe(cb: () => void): () => void;
}

export function createStore(initial: AppState): Store {
  let state = initial;
  const listeners = new Set<() => void>();
  return {
    getState: () => state,
    dispatch(a) {
      const next = reducer(state, a);
      if (next === state) return;
      state = next;
      listeners.forEach((cb) => cb());
    },
    subscribe(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}

export function useAppState(store: Store): AppState {
  return useSyncExternalStore(store.subscribe, store.getState);
}
