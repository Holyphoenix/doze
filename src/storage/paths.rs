use directories::ProjectDirs;
use std::path::{Path, PathBuf};

/// Returns the OS-appropriate user configuration directory for Doze.
/// - Windows: `%APPDATA%\doze\Doze\config`
/// - macOS:   `~/Library/Application Support/dev.doze.Doze`
/// - Linux:   `~/.config/doze`
pub fn user_config_dir() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("dev", "doze", "Doze") {
        proj.config_dir().to_path_buf()
    } else {
        fallback_user_dir().join("config")
    }
}

/// Returns the OS-appropriate user data directory (history, scratchpad workspace).
pub fn user_data_dir() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("dev", "doze", "Doze") {
        proj.data_local_dir().to_path_buf()
    } else {
        fallback_user_dir().join("data")
    }
}

fn fallback_user_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".doze-user")
}

pub fn user_settings_path() -> PathBuf {
    user_config_dir().join("settings.toml")
}

pub fn user_secrets_dir() -> PathBuf {
    user_config_dir().join("secrets")
}

pub fn project_secrets_path(project_id: &str) -> PathBuf {
    user_secrets_dir().join(format!("{}.toml", sanitize_id(project_id)))
}

pub fn user_history_dir() -> PathBuf {
    user_data_dir().join("history")
}

pub fn project_history_path(project_id: &str) -> PathBuf {
    user_history_dir().join(format!("{}.json", sanitize_id(project_id)))
}

/// Default local workspace used when the user hasn't opened a repository `.doze` package yet.
pub fn default_workspace_doze_dir() -> PathBuf {
    user_data_dir().join("default-workspace").join(".doze")
}

/// Given a user-picked directory, normalize it to the `.doze` package path.
/// - If the path itself ends with `.doze`, return it directly.
/// - Otherwise return `<path>/.doze`.
pub fn resolve_doze_dir(selected: &Path) -> PathBuf {
    if selected
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case(".doze"))
    {
        selected.to_path_buf()
    } else {
        selected.join(".doze")
    }
}

pub fn sanitize_id(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "default".to_string()
    } else {
        cleaned
    }
}

pub fn sanitize_filename(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_sep = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_sep = false;
        } else if (ch == '-' || ch == '_' || ch == ' ' || ch == '.' || ch == '/') && !last_sep {
            slug.push('_');
            last_sep = true;
        }
    }
    let trimmed = slug.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed
    }
}
