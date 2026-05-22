//! Resolves where viban stores its per-project data — deliberately *outside*
//! the project folder, so cloud-sync clients (OneDrive, Dropbox) cannot lock
//! the database. See ADR-0003.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The data directory for `workspace`: `<base>/projects/<key>/`, where `base`
/// is `data_dir_override` when given, otherwise the OS local data directory
/// under `viban/`, and `key` is a stable hash of the workspace path.
pub fn project_data_dir(data_dir_override: Option<&Path>, workspace: &Path) -> Result<PathBuf> {
    let base = match data_dir_override {
        Some(dir) => dir.to_path_buf(),
        None => dirs::data_local_dir()
            .context("cannot resolve the OS data directory")?
            .join("viban"),
    };
    Ok(base.join("projects").join(project_key(workspace)))
}

/// A stable, filesystem-safe key for a workspace path. Uses FNV-1a so the key
/// is identical across runs and toolchain versions.
fn project_key(workspace: &Path) -> String {
    let canonical = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in canonical.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_data_dir_is_under_the_override_base() {
        let base = Path::new("/tmp/vibandata");
        let dir = project_data_dir(Some(base), Path::new("/some/project")).expect("dir");
        assert!(dir.starts_with(base));
        assert!(dir.to_string_lossy().contains("projects"));
    }

    #[test]
    fn project_key_is_stable_and_distinct() {
        let a1 = project_data_dir(Some(Path::new("/d")), Path::new("/proj/a")).expect("a");
        let a2 = project_data_dir(Some(Path::new("/d")), Path::new("/proj/a")).expect("a");
        let b = project_data_dir(Some(Path::new("/d")), Path::new("/proj/b")).expect("b");
        assert_eq!(a1, a2, "the same workspace always maps to the same dir");
        assert_ne!(a1, b, "different workspaces map to different dirs");
    }
}
