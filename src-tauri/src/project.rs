//! Persisted project selection — which git repository viban operates on.
//!
//! The last opened project path is written to `viban.json` in the OS
//! app-config directory so it is restored on the next launch.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// On-disk shape of the project config file.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ProjectConfig {
    #[serde(default)]
    project_path: Option<String>,
}

/// Resolves the path of viban's config file in the OS app-config directory.
fn config_path(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .context("cannot resolve the app config directory")?;
    Ok(dir.join("viban.json"))
}

/// Loads the remembered project path, if any.
pub fn load(app: &AppHandle) -> Option<String> {
    let path = config_path(app).ok()?;
    let contents = std::fs::read_to_string(path).ok()?;
    let config: ProjectConfig = serde_json::from_str(&contents).ok()?;
    config.project_path
}

/// Persists `project_path` so it is restored on the next launch.
pub fn save(app: &AppHandle, project_path: &str) -> Result<()> {
    let path = config_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("failed to create the config directory")?;
    }
    let config = ProjectConfig {
        project_path: Some(project_path.to_string()),
    };
    let contents = serde_json::to_string_pretty(&config).context("failed to serialize config")?;
    std::fs::write(&path, contents).context("failed to write the config file")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ProjectConfig;

    #[test]
    fn config_parses_a_stored_project_path() {
        let config: ProjectConfig =
            serde_json::from_str(r#"{"project_path":"/home/user/proj"}"#).expect("parse");
        assert_eq!(config.project_path.as_deref(), Some("/home/user/proj"));
    }

    #[test]
    fn config_defaults_to_none_when_the_field_is_absent() {
        let config: ProjectConfig = serde_json::from_str("{}").expect("parse");
        assert!(config.project_path.is_none());
    }

    #[test]
    fn config_treats_an_explicit_null_as_no_project() {
        let config: ProjectConfig =
            serde_json::from_str(r#"{"project_path":null}"#).expect("parse");
        assert!(config.project_path.is_none());
    }

    #[test]
    fn config_round_trips_through_json() {
        let config = ProjectConfig {
            project_path: Some("/p".to_string()),
        };
        let json = serde_json::to_string(&config).expect("serialize");
        let back: ProjectConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.project_path, config.project_path);
    }
}
