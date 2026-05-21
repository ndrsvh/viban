import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import type { Session } from "@/types/session";

interface SessionSidebarProps {
  sessions: Session[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
}

/** The left rail: a "New session" button above the list of past sessions. */
export function SessionSidebar({
  sessions,
  selectedId,
  onSelect,
  onNew,
}: SessionSidebarProps) {
  return (
    <aside className="flex h-screen w-64 flex-col border-r">
      <div className="border-b p-2">
        <Button className="w-full" size="sm" onClick={onNew}>
          New session
        </Button>
      </div>
      <ScrollArea className="flex-1">
        <div className="flex flex-col gap-1 p-2">
          {sessions.length === 0 && (
            <p className="px-2 py-1 text-xs text-muted-foreground">
              No sessions yet.
            </p>
          )}
          {sessions.map((session) => (
            <button
              key={session.id}
              type="button"
              onClick={() => onSelect(session.id)}
              className={cn(
                "truncate rounded-md px-2 py-1.5 text-left text-sm transition-colors",
                session.id === selectedId
                  ? "bg-accent text-accent-foreground"
                  : "hover:bg-accent/50",
              )}
            >
              {session.title}
            </button>
          ))}
        </div>
      </ScrollArea>
    </aside>
  );
}
