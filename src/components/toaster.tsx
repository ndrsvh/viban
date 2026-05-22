import { X } from "lucide-react";

import { useToastStore } from "@/stores/useToastStore";

/** Fixed-position stack of dismissible error toasts. Rendered once, globally. */
export function Toaster() {
  const toasts = useToastStore((state) => state.toasts);
  const dismiss = useToastStore((state) => state.dismiss);

  if (toasts.length === 0) return null;

  return (
    <div className="fixed right-4 bottom-4 z-50 flex w-80 flex-col gap-2">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          role="alert"
          className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive shadow-md"
        >
          <span className="flex-1 break-words">{toast.message}</span>
          <button
            type="button"
            aria-label="Dismiss"
            className="shrink-0 rounded p-0.5 hover:bg-destructive/20"
            onClick={() => dismiss(toast.id)}
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      ))}
    </div>
  );
}
