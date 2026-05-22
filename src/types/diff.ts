/** Mirror of `viban_core::git::FileStatus`. */
export type FileStatus = "added" | "modified" | "deleted";

/** Mirror of `viban_core::git::FileDiff`. */
export interface FileDiff {
  path: string;
  status: FileStatus;
  /** File content at HEAD — empty for an added file. */
  old_text: string;
  /** File content in the worktree — empty for a deleted file. */
  new_text: string;
}
