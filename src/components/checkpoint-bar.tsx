import { useEffect, useState } from "react";

import { ConfirmDialog } from "@/components/confirm-dialog";
import { Button } from "@/components/ui/button";
import { rpc } from "@/lib/rpc";
import { toast } from "@/stores/useToastStore";
import type { Checkpoint } from "@/types/board";

interface CheckpointBarProps {
  /** The task whose worktree checkpoints these are. */
  taskId: string;
  /** Called after a restore, so the diff can reload. */
  onRestored: () => void;
}

/** Save / restore worktree checkpoints for a task under review. */
export function CheckpointBar({ taskId, onRestored }: CheckpointBarProps) {
  const [checkpoints, setCheckpoints] = useState<Checkpoint[]>([]);
  const [restoring, setRestoring] = useState<Checkpoint | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    rpc
      .listCheckpoints(taskId)
      .then((result) => setCheckpoints(result.checkpoints))
      .catch(() => setCheckpoints([]));
  }, [taskId]);

  async function save() {
    if (busy) return;
    setBusy(true);
    try {
      const result = await rpc.createCheckpoint(
        taskId,
        `Checkpoint ${checkpoints.length + 1}`,
      );
      setCheckpoints((prev) => [...prev, result.checkpoint]);
    } catch (err) {
      toast.error(`Could not save the checkpoint: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  }

  async function confirmRestore() {
    const checkpoint = restoring;
    if (!checkpoint) return;
    setBusy(true);
    try {
      await rpc.restoreCheckpoint(checkpoint.id);
      onRestored();
    } catch (err) {
      toast.error(`Could not restore the checkpoint: ${String(err)}`);
    } finally {
      setBusy(false);
      setRestoring(null);
    }
  }

  return (
    <div className="flex flex-wrap items-center gap-1.5 border-b px-3 py-1.5">
      <Button
        size="xs"
        variant="secondary"
        onClick={() => void save()}
        disabled={busy}
      >
        Save checkpoint
      </Button>
      {checkpoints.map((checkpoint) => (
        <Button
          key={checkpoint.id}
          size="xs"
          variant="ghost"
          onClick={() => setRestoring(checkpoint)}
          disabled={busy}
          title="Restore the worktree to this checkpoint"
        >
          ↺ {checkpoint.label}
        </Button>
      ))}
      {checkpoints.length === 0 && (
        <span className="text-xs text-muted-foreground">
          No checkpoints yet.
        </span>
      )}

      <ConfirmDialog
        open={restoring !== null}
        title="Restore this checkpoint?"
        description={
          restoring
            ? `Reset the worktree to "${restoring.label}". Everything done ` +
              "since is discarded."
            : ""
        }
        confirmLabel="Restore"
        busyLabel="Restoring…"
        busy={busy}
        onConfirm={() => void confirmRestore()}
        onOpenChange={(open) => {
          if (!open && !busy) setRestoring(null);
        }}
      />
    </div>
  );
}
