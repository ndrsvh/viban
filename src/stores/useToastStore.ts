import { create } from "zustand";

export interface Toast {
  id: number;
  kind: "error" | "info";
  message: string;
}

interface ToastState {
  toasts: Toast[];
  /** Shows an error (red) toast. */
  error: (message: string) => void;
  /** Shows an informational (neutral) toast. */
  info: (message: string) => void;
  /** Dismisses a toast by id. */
  dismiss: (id: number) => void;
}

let nextId = 0;

/**
 * Transient notifications. The single mechanism for surfacing something with
 * no better in-context home — call `toast.error(...)` / `toast.info(...)`
 * from anywhere.
 */
export const useToastStore = create<ToastState>((set) => {
  const push = (kind: Toast["kind"], message: string) => {
    nextId += 1;
    const id = nextId;
    set((state) => ({ toasts: [...state.toasts, { id, kind, message }] }));
  };
  return {
    toasts: [],
    error: (message) => push("error", message),
    info: (message) => push("info", message),
    dismiss: (id) =>
      set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
  };
});

/** Fire a toast outside React — no hook needed. */
export const toast = {
  error: (message: string) => useToastStore.getState().error(message),
  info: (message: string) => useToastStore.getState().info(message),
};
