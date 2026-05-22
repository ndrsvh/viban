import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import type { Task } from "@/types/board";

interface TaskDialogProps {
  open: boolean;
  /** The task being edited, or `null` to create a new one. */
  task: Task | null;
  /** The column a new task goes into (create mode only). */
  columnId: string | null;
  onOpenChange: (open: boolean) => void;
  /** Called after a successful create/update/delete so the board reloads. */
  onChanged: () => void;
}

/** Create / edit / delete a task. */
export function TaskDialog({
  open,
  task,
  columnId,
  onOpenChange,
  onChanged,
}: TaskDialogProps) {
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) {
      setTitle(task?.title ?? "");
      setDescription(task?.description ?? "");
    }
  }, [open, task]);

  async function save() {
    if (!title.trim() || busy) return;
    setBusy(true);
    try {
      if (task) {
        await invoke("update_task", { taskId: task.id, title, description });
      } else if (columnId) {
        await invoke("create_task", { columnId, title, description });
      }
      onChanged();
      onOpenChange(false);
    } catch (err) {
      console.error(err);
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!task || busy) return;
    setBusy(true);
    try {
      await invoke("delete_task", { taskId: task.id });
      onChanged();
      onOpenChange(false);
    } catch (err) {
      console.error(err);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{task ? "Edit task" : "New task"}</DialogTitle>
          <DialogDescription>
            {task
              ? "Update or delete this task."
              : "Add a task to the column."}
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-3">
          <Input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Task title"
          />
          <Textarea
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            placeholder="Description (optional)"
            rows={4}
          />
        </div>
        <DialogFooter>
          {task && (
            <Button
              variant="destructive"
              onClick={() => void remove()}
              disabled={busy}
            >
              Delete
            </Button>
          )}
          <Button
            onClick={() => void save()}
            disabled={busy || !title.trim()}
          >
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
