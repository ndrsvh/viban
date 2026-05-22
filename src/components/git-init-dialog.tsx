import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

interface GitInitDialogProps {
  open: boolean;
  /** True while the confirmed action (init or no-git start) is running. */
  busy: boolean;
  /** Initialize a git repository here, then start the session in a worktree. */
  onConfirm: () => void;
  /** Start the session directly in the project folder, with no git. */
  onWorkWithoutGit: () => void;
  onOpenChange: (open: boolean) => void;
}

/** Asks the user how to start a session when the project folder is not its own
 *  git repository: initialize one (so viban can use task worktrees), or run
 *  the agent directly in the folder without git. */
export function GitInitDialog({
  open,
  busy,
  onConfirm,
  onWorkWithoutGit,
  onOpenChange,
}: GitInitDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Set up git for this project?</DialogTitle>
          <DialogDescription>
            viban runs each task in its own git worktree, which needs this
            folder to be its own git repository. Initialize one here (with an
            initial commit of the current contents), or skip git and let the
            agent work directly in the folder.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={busy}
          >
            Cancel
          </Button>
          <Button variant="secondary" onClick={onWorkWithoutGit} disabled={busy}>
            Work without Git
          </Button>
          <Button onClick={onConfirm} disabled={busy}>
            {busy ? "Initializing…" : "Initialize git"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
