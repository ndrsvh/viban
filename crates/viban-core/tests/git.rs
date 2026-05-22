//! Integration tests for the `git` module, exercised against real temporary
//! git repositories. These need the `git` CLI on PATH.

use std::path::Path;
use std::process::{Command, Stdio};

use tokio::sync::Mutex;
use viban_core::git;

/// Each test here spawns several `git` subprocesses; running many at once has
/// shown intermittent failures under heavy load (file locking on Windows), so
/// the tests take this lock and run one at a time.
static GIT_TEST_LOCK: Mutex<()> = Mutex::const_new(());

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
    std::fs::write(dir.join("file.txt"), "content\n").expect("write file");
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-m", "initial"]);
}

#[tokio::test]
async fn is_git_repo_distinguishes_repos_from_plain_directories() {
    let _guard = GIT_TEST_LOCK.lock().await;
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
    let _guard = GIT_TEST_LOCK.lock().await;
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
    let _guard = GIT_TEST_LOCK.lock().await;
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
    let _guard = GIT_TEST_LOCK.lock().await;
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
    let _guard = GIT_TEST_LOCK.lock().await;
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
