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
  /** True while the confirmed initialization is running. */
  busy: boolean;
  onConfirm: () => void;
  onOpenChange: (open: boolean) => void;
}

/** Asks the user to confirm turning a plain project folder into a git
 *  repository, which viban needs before it can create a task worktree. */
export function GitInitDialog({
  open,
  busy,
  onConfirm,
  onOpenChange,
}: GitInitDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Initialize a git repository?</DialogTitle>
          <DialogDescription>
            This project folder is not a git repository. viban runs each task
            in its own git worktree, so it needs one. Initialize git here and
            make an initial commit of the folder&apos;s current contents?
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
          <Button onClick={onConfirm} disabled={busy}>
            {busy ? "Initializing…" : "Initialize git"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
