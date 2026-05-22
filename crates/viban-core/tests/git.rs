//! Integration tests for the `git` module, exercised against real temporary
//! git repositories. These need the `git` CLI on PATH.

use std::path::Path;
use std::process::{Command, Stdio};

use viban_core::git;

/// Runs `git args` in `dir`, panicking on failure.
fn run_git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git should run");
    assert!(status.success(), "git {args:?} failed");
}

/// Creates a git repo with a single commit at `dir`.
fn init_repo(dir: &Path) {
    run_git(dir, &["init"]);
    run_git(dir, &["config", "user.email", "test@viban.dev"]);
    run_git(dir, &["config", "user.name", "viban test"]);
    // Keep line endings byte-stable across platforms.
    run_git(dir, &["config", "core.autocrlf", "false"]);
    std::fs::write(dir.join("file.txt"), "content\n").expect("write file");
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-m", "initial"]);
}

#[tokio::test]
async fn is_git_repo_distinguishes_repos_from_plain_directories() {
    let repo = tempfile::tempdir().expect("tempdir");
    let plain = tempfile::tempdir().expect("tempdir");
    init_repo(repo.path());

    assert!(git::is_git_repo(repo.path()).await, "a repo is a repo");
    assert!(
        !git::is_git_repo(plain.path()).await,
        "a plain directory is not a repo"
    );
}

#[tokio::test]
async fn ensure_gitignored_creates_appends_and_dedups() {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    let gitignore = root.join(".gitignore");

    // No .gitignore yet — it is created with the entry.
    git::ensure_gitignored(root, ".viban/")
        .await
        .expect("first call");
    let contents = std::fs::read_to_string(&gitignore).expect("read");
    assert!(contents.lines().any(|line| line.trim() == ".viban/"));

    // Idempotent: a second call does not duplicate the entry.
    git::ensure_gitignored(root, ".viban/")
        .await
        .expect("second call");
    let contents = std::fs::read_to_string(&gitignore).expect("read");
    assert_eq!(
        contents.matches(".viban/").count(),
        1,
        "entry must not be duplicated"
    );

    // A different entry is appended; existing content is preserved.
    git::ensure_gitignored(root, "node_modules/")
        .await
        .expect("third call");
    let contents = std::fs::read_to_string(&gitignore).expect("read");
    assert!(contents.contains(".viban/"));
    assert!(contents.contains("node_modules/"));
}

#[tokio::test]
async fn ensure_gitignored_appends_cleanly_without_a_trailing_newline() {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    // An existing file with no trailing newline.
    std::fs::write(root.join(".gitignore"), "target").expect("write");

    git::ensure_gitignored(root, ".viban/")
        .await
        .expect("append");

    let contents = std::fs::read_to_string(root.join(".gitignore")).expect("read");
    assert!(contents.lines().any(|line| line.trim() == "target"));
    assert!(contents.lines().any(|line| line.trim() == ".viban/"));
}

#[tokio::test]
async fn worktree_add_remove_and_branch_delete_round_trip() {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    init_repo(root);

    let worktree = root.join("wt");
    git::worktree_add(root, &worktree, "viban/feature")
        .await
        .expect("worktree add");
    assert!(
        worktree.join("file.txt").is_file(),
        "the worktree is checked out"
    );

    git::worktree_remove(root, &worktree, true)
        .await
        .expect("worktree remove");
    assert!(!worktree.exists(), "the worktree directory is gone");

    git::branch_delete(root, "viban/feature")
        .await
        .expect("branch delete");
    let output = Command::new("git")
        .args(["branch", "--list", "viban/feature"])
        .current_dir(root)
        .output()
        .expect("git branch");
    assert!(
        String::from_utf8_lossy(&output.stdout).trim().is_empty(),
        "the branch was deleted"
    );
}

#[tokio::test]
async fn worktree_remove_without_force_fails_on_uncommitted_changes() {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    init_repo(root);

    let worktree = root.join("wt");
    git::worktree_add(root, &worktree, "viban/dirty")
        .await
        .expect("worktree add");
    std::fs::write(worktree.join("file.txt"), "uncommitted change").expect("write");

    // git refuses to remove a dirty worktree unless forced.
    assert!(
        git::worktree_remove(root, &worktree, false).await.is_err(),
        "a dirty worktree must not be removed without force"
    );
    // With force it succeeds.
    git::worktree_remove(root, &worktree, true)
        .await
        .expect("forced removal");
    assert!(!worktree.exists());
}

#[tokio::test]
async fn worktree_diff_reports_modified_added_and_deleted_files() {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    run_git(root, &["init"]);
    run_git(root, &["config", "user.email", "test@viban.dev"]);
    run_git(root, &["config", "user.name", "viban test"]);
    run_git(root, &["config", "core.autocrlf", "false"]);
    std::fs::write(root.join("keep.txt"), "original\n").expect("write");
    std::fs::write(root.join("remove.txt"), "to be deleted\n").expect("write");
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "initial"]);

    std::fs::write(root.join("keep.txt"), "changed\n").expect("write");
    std::fs::write(root.join("new.txt"), "brand new\n").expect("write");
    std::fs::remove_file(root.join("remove.txt")).expect("remove");

    let diffs = git::worktree_diff(root).await.expect("diff");
    assert_eq!(diffs.len(), 3, "three files changed");

    let find = |name: &str| {
        diffs
            .iter()
            .find(|diff| diff.path == name)
            .unwrap_or_else(|| panic!("missing {name}"))
    };

    let modified = find("keep.txt");
    assert_eq!(modified.status, git::FileStatus::Modified);
    assert_eq!(modified.old_text, "original\n");
    assert_eq!(modified.new_text, "changed\n");

    let added = find("new.txt");
    assert_eq!(added.status, git::FileStatus::Added);
    assert!(added.old_text.is_empty());
    assert_eq!(added.new_text, "brand new\n");

    let deleted = find("remove.txt");
    assert_eq!(deleted.status, git::FileStatus::Deleted);
    assert_eq!(deleted.old_text, "to be deleted\n");
    assert!(deleted.new_text.is_empty());
}

#[tokio::test]
async fn worktree_diff_is_empty_for_a_clean_tree() {
    let repo = tempfile::tempdir().expect("tempdir");
    init_repo(repo.path());
    assert!(git::worktree_diff(repo.path())
        .await
        .expect("diff")
        .is_empty());
}

#[tokio::test]
async fn commit_all_stages_and_clears_pending_changes() {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    init_repo(root);
    std::fs::write(root.join("added.txt"), "new content\n").expect("write");
    std::fs::write(root.join("file.txt"), "edited\n").expect("write");

    git::commit_all(root, "viban: apply task")
        .await
        .expect("commit");

    assert!(
        git::worktree_diff(root).await.expect("diff").is_empty(),
        "nothing is pending after a commit"
    );
    let log = Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(root)
        .output()
        .expect("git log");
    assert!(String::from_utf8_lossy(&log.stdout).contains("viban: apply task"));
}

#[tokio::test]
async fn discard_all_restores_tracked_and_removes_untracked_files() {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    init_repo(root);
    std::fs::write(root.join("file.txt"), "tampered\n").expect("write");
    std::fs::write(root.join("untracked.txt"), "junk\n").expect("write");

    git::discard_all(root).await.expect("discard");

    assert!(
        git::worktree_diff(root).await.expect("diff").is_empty(),
        "the worktree is clean after a discard"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("file.txt")).expect("read"),
        "content\n",
        "tracked files return to HEAD"
    );
    assert!(
        !root.join("untracked.txt").exists(),
        "untracked files are removed"
    );
}

#[tokio::test]
async fn has_head_reflects_whether_the_repo_has_commits() {
    let empty = tempfile::tempdir().expect("tempdir");
    run_git(empty.path(), &["init"]);
    assert!(
        !git::has_head(empty.path()).await,
        "a freshly initialized repo has no HEAD"
    );

    let committed = tempfile::tempdir().expect("tempdir");
    init_repo(committed.path());
    assert!(
        git::has_head(committed.path()).await,
        "a repo with a commit has a HEAD"
    );
}

#[tokio::test]
async fn prepare_repo_initializes_a_plain_folder() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("code.txt"), "hello\n").expect("write");
    assert!(!git::is_git_repo(dir.path()).await);

    git::prepare_repo(dir.path()).await.expect("prepare");

    assert!(
        git::is_git_repo(dir.path()).await,
        "the folder is now a repo"
    );
    assert!(git::has_head(dir.path()).await, "with an initial commit");
    let gitignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap_or_default();
    assert!(
        gitignore.lines().any(|line| line.trim() == ".viban/"),
        ".viban/ is gitignored before the initial commit"
    );
}

#[tokio::test]
async fn prepare_repo_commits_an_empty_repository() {
    let dir = tempfile::tempdir().expect("tempdir");
    run_git(dir.path(), &["init"]);
    std::fs::write(dir.path().join("code.txt"), "hello\n").expect("write");
    assert!(!git::has_head(dir.path()).await);

    git::prepare_repo(dir.path()).await.expect("prepare");
    assert!(
        git::has_head(dir.path()).await,
        "an initial commit was made"
    );
}

#[tokio::test]
async fn prepare_repo_is_a_noop_for_a_ready_repository() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_repo(dir.path());
    let head_before = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir.path())
        .output()
        .expect("rev-parse");

    git::prepare_repo(dir.path()).await.expect("prepare");

    let head_after = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir.path())
        .output()
        .expect("rev-parse");
    assert_eq!(
        head_before.stdout, head_after.stdout,
        "a ready repo gets no new commit"
    );
}

#[tokio::test]
async fn merge_branch_brings_in_the_branch_commits() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    init_repo(root);

    // A worktree branch with a new committed file.
    let worktree = root.join("wt");
    git::worktree_add(root, &worktree, "viban/feature")
        .await
        .expect("worktree add");
    std::fs::write(worktree.join("feature.txt"), "task output\n").expect("write");
    run_git(&worktree, &["add", "."]);
    run_git(&worktree, &["commit", "-m", "task work"]);

    git::merge_branch(root, "viban/feature")
        .await
        .expect("merge");

    assert!(
        root.join("feature.txt").is_file(),
        "the branch's file is merged into the project root"
    );
}

#[tokio::test]
async fn merge_branch_aborts_and_errors_on_conflict() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    init_repo(root);

    // The branch edits file.txt one way.
    let worktree = root.join("wt");
    git::worktree_add(root, &worktree, "viban/feature")
        .await
        .expect("worktree add");
    std::fs::write(worktree.join("file.txt"), "branch version\n").expect("write");
    run_git(&worktree, &["add", "."]);
    run_git(&worktree, &["commit", "-m", "branch edit"]);

    // The project root edits the same file differently.
    std::fs::write(root.join("file.txt"), "root version\n").expect("write");
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "root edit"]);

    assert!(
        git::merge_branch(root, "viban/feature").await.is_err(),
        "a conflicting merge must error"
    );
    assert!(
        !root.join(".git").join("MERGE_HEAD").exists(),
        "the failed merge is aborted, leaving no merge in progress"
    );
}
