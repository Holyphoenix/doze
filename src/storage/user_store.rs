use std::fs;
use std::path::Path;
use thiserror::Error;

use crate::models::{HistoryEntry, ProjectSecretsStore, UserSettings};
use crate::storage::paths;

#[derive(Debug, Error)]
pub enum UserStoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, UserStoreError>;

pub fn load_user_settings() -> Result<UserSettings> {
    load_user_settings_from(&paths::user_settings_path())
}

pub fn load_user_settings_from(path: &Path) -> Result<UserSettings> {
    if !path.exists() {
        let settings = UserSettings::default();
        let _ = save_user_settings_to(&settings, path);
        return Ok(settings);
    }
    let content = fs::read_to_string(path)?;
    let settings: UserSettings = toml::from_str(&content)?;
    Ok(settings)
}

pub fn save_user_settings(settings: &UserSettings) -> Result<()> {
    save_user_settings_to(settings, &paths::user_settings_path())
}

pub fn save_user_settings_to(settings: &UserSettings, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = format!(
        "# Doze User-Level Settings (Private to your OS user account)\n# This file is stored outside any project repository.\n\n{}",
        toml::to_string_pretty(settings)?
    );
    fs::write(path, body)?;
    Ok(())
}

/// Loads user-local secrets for a specific `project_id`.
/// Secrets live strictly in `<user_config_dir>/secrets/<project_id>.toml` and NEVER in `.doze/`.
pub fn load_project_secrets(project_id: &str) -> Result<ProjectSecretsStore> {
    load_project_secrets_from(project_id, &paths::project_secrets_path(project_id))
}

pub fn load_project_secrets_from(project_id: &str, path: &Path) -> Result<ProjectSecretsStore> {
    if !path.exists() {
        return Ok(ProjectSecretsStore::new(project_id));
    }
    let content = fs::read_to_string(path)?;
    let mut store: ProjectSecretsStore = toml::from_str(&content)?;
    if store.project_id.is_empty() {
        store.project_id = project_id.to_string();
    }
    Ok(store)
}

pub fn save_project_secrets(store: &ProjectSecretsStore) -> Result<()> {
    save_project_secrets_to(store, &paths::project_secrets_path(&store.project_id))
}

pub fn save_project_secrets_to(store: &ProjectSecretsStore, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = format!(
        "# Doze User-Local Secrets (NEVER committed to source control)\n# Bound to project_id = \"{}\"\n\n{}",
        store.project_id,
        toml::to_string_pretty(store)?
    );
    fs::write(path, body)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

pub fn load_project_history(project_id: &str) -> Result<Vec<HistoryEntry>> {
    let path = paths::project_history_path(project_id);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    let entries: Vec<HistoryEntry> = serde_json::from_str(&content)?;
    Ok(entries)
}

pub fn save_project_history(project_id: &str, entries: &[HistoryEntry]) -> Result<()> {
    let path = paths::project_history_path(project_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(entries)?;
    fs::write(path, json)?;
    Ok(())
}
