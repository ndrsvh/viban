import { create } from "zustand";

export interface Toast {
  id: number;
  message: string;
}

interface ToastState {
  toasts: Toast[];
  /** Shows an error toast. */
  error: (message: string) => void;
  /** Dismisses a toast by id. */
  dismiss: (id: number) => void;
}

let nextId = 0;

/**
 * Transient error notifications. The single mechanism for surfacing a failure
 * that has no better in-context home — call `toast.error(...)` from anywhere.
 */
export const useToastStore = create<ToastState>((set) => ({
  toasts: [],
  error: (message) => {
    nextId += 1;
    const id = nextId;
    set((state) => ({ toasts: [...state.toasts, { id, message }] }));
  },
  dismiss: (id) =>
    set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
}));

/** Fire a toast outside React — no hook needed. */
export const toast = {
  error: (message: string) => useToastStore.getState().error(message),
};
