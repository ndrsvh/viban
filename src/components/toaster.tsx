import { X } from "lucide-react";

import { cn } from "@/lib/utils";
import { useToastStore } from "@/stores/useToastStore";

/** Fixed-position stack of dismissible toasts. Rendered once, globally. */
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
          className={cn(
            "flex items-start gap-2 rounded-md border px-3 py-2 text-sm shadow-md",
            toast.kind === "error"
              ? "border-destructive/30 bg-destructive/10 text-destructive"
              : "border-border bg-card text-card-foreground",
          )}
        >
          <span className="flex-1 break-words">{toast.message}</span>
          <button
            type="button"
            aria-label="Dismiss"
            className={cn(
              "shrink-0 rounded p-0.5",
              toast.kind === "error" ? "hover:bg-destructive/20" : "hover:bg-muted",
            )}
            onClick={() => dismiss(toast.id)}
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      ))}
    </div>
  );
}
