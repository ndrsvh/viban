import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MergeView } from "@codemirror/merge";
import { EditorState } from "@codemirror/state";
import { EditorView, lineNumbers } from "@codemirror/view";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { Attempt, Task } from "@/types/board";
import type { FileDiff, FileStatus } from "@/types/diff";

interface DiffViewProps {
  /** The task whose worktree changes are under review. */
  task: Task;
  /** Called after the review is committed, rejected, or dismissed. */
  onDone: () => void;
}

const STATUS_LABEL: Record<FileStatus, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
};

const STATUS_CLASS: Record<FileStatus, string> = {
  added: "text-emerald-600",
  modified: "text-amber-600",
  deleted: "text-destructive",
};

/** Reviews a task's worktree changes: a file list plus a merge view, with
 *  accept (commit) and reject (discard) actions. When the task has several
 *  attempts, a selector switches which attempt is reviewed. */
export function DiffView({ task, onDone }: DiffViewProps) {
  const [files, setFiles] = useState<FileDiff[]>([]);
  const [selected, setSelected] = useState(0);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [attempts, setAttempts] = useState<Attempt[]>([]);
  // The session of the attempt currently being reviewed.
  const [activeSession, setActiveSession] = useState<string | null>(
    task.session_id,
  );

  const loadDiff = useCallback(async () => {
    setLoading(true);
    try {
      const result = await invoke<{ files: FileDiff[] }>("git_diff", {
        taskId: task.id,
      });
      setFiles(result.files);
      setSelected(0);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [task.id]);

  useEffect(() => {
    void loadDiff();
    invoke<{ attempts: Attempt[] }>("list_attempts", { taskId: task.id })
      .then((result) => setAttempts(result.attempts))
      .catch(() => setAttempts([]));
  }, [loadDiff, task.id]);

  async function switchAttempt(attemptId: string) {
    const attempt = attempts.find((entry) => entry.id === attemptId);
    if (!attempt) return;
    try {
      await invoke("activate_attempt", { attemptId });
      setActiveSession(attempt.session_id);
      await loadDiff();
    } catch (err) {
      setError(String(err));
    }
  }

  async function accept() {
    if (busy) return;
    setBusy(true);
    try {
      await invoke("git_commit", { taskId: task.id });
      onDone();
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  }

  async function reject() {
    if (busy) return;
    setBusy(true);
    try {
      await invoke("git_restore", { taskId: task.id });
      onDone();
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  }

  const current = files[selected];
  const activeAttemptId =
    attempts.find((entry) => entry.session_id === activeSession)?.id ?? "";

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center justify-between gap-2 border-b px-3 py-1.5">
        <div className="flex min-w-0 items-center gap-2">
          <h1 className="truncate text-sm font-medium">
            Review: {task.title}
          </h1>
          {attempts.length > 1 && (
            <select
              value={activeAttemptId}
              onChange={(event) => void switchAttempt(event.target.value)}
              className="shrink-0 rounded-md border bg-background px-1.5 py-0.5 text-xs"
              aria-label="Attempt"
            >
              {attempts.map((attempt, index) => (
                <option key={attempt.id} value={attempt.id}>
                  Attempt {attempts.length - index}
                </option>
              ))}
            </select>
          )}
        </div>
        <div className="flex shrink-0 gap-2">
          <Button variant="ghost" size="sm" onClick={onDone}>
            ← Board
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => void reject()}
            disabled={busy || loading}
          >
            Reject all
          </Button>
          <Button
            size="sm"
            onClick={() => void accept()}
            disabled={busy || loading || files.length === 0}
          >
            Accept all
          </Button>
        </div>
      </header>

      {error && (
        <p className="border-b bg-destructive/10 px-3 py-1.5 text-xs text-destructive">
          {error}
        </p>
      )}

      <div className="flex flex-1 overflow-hidden">
        <aside className="w-64 shrink-0 overflow-y-auto border-r">
          {loading ? (
            <p className="p-3 text-sm text-muted-foreground">Loading…</p>
          ) : files.length === 0 ? (
            <p className="p-3 text-sm text-muted-foreground">No changes.</p>
          ) : (
            files.map((file, index) => (
              <button
                key={file.path}
                onClick={() => setSelected(index)}
                className={cn(
                  "flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs",
                  index === selected ? "bg-muted" : "hover:bg-muted/50",
                )}
              >
                <span
                  className={cn(
                    "font-mono font-medium",
                    STATUS_CLASS[file.status],
                  )}
                >
                  {STATUS_LABEL[file.status]}
                </span>
                <span className="truncate">{file.path}</span>
              </button>
            ))
          )}
        </aside>

        <main className="flex-1 overflow-hidden">
          {current ? (
            <MergePane key={current.path} file={current} />
          ) : (
            !loading && (
              <p className="p-4 text-sm text-muted-foreground">
                {files.length === 0
                  ? "The worktree has no pending changes."
                  : "Select a file to review."}
              </p>
            )
          )}
        </main>
      </div>
    </div>
  );
}

/** A read-only CodeMirror 6 merge view of one file's old and new content. */
function MergePane({ file }: { file: FileDiff }) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const parent = containerRef.current;
    if (!parent) return;
    const readOnly = [
      lineNumbers(),
      EditorView.editable.of(false),
      EditorState.readOnly.of(true),
      EditorView.lineWrapping,
    ];
    const view = new MergeView({
      a: { doc: file.old_text, extensions: readOnly },
      b: { doc: file.new_text, extensions: readOnly },
      parent,
    });
    return () => view.destroy();
  }, [file]);

  return <div ref={containerRef} className="h-full overflow-auto text-sm" />;
}
