use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::request::{HttpMethod, Protocol, RequestItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Dark,
    Midnight,
    Light,
}

impl Theme {
    pub fn as_class(&self) -> &'static str {
        match self {
            Theme::Dark => "theme-dark",
            Theme::Midnight => "theme-midnight",
            Theme::Light => "theme-light",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Theme::Dark => "Dark (Default)",
            Theme::Midnight => "Midnight Blue",
            Theme::Light => "Light",
        }
    }
}

/// User-level settings stored strictly in the OS user config directory
/// (e.g. `%APPDATA%\doze\Doze\config\settings.toml` or `~/.config/doze/settings.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserSettings {
    #[serde(default)]
    pub theme: Theme,
    #[serde(default = "default_font_size")]
    pub editor_font_size: u16,
    #[serde(default = "default_timeout_ms")]
    pub default_timeout_ms: u64,
    #[serde(default = "default_true")]
    pub follow_redirects: bool,
    #[serde(default = "default_max_redirects")]
    pub max_redirects: usize,
    #[serde(default = "default_true")]
    pub verify_tls: bool,
    #[serde(default)]
    pub proxy_url: String,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
    #[serde(default)]
    pub recent_projects: Vec<String>,
    #[serde(default)]
    pub last_project_path: Option<String>,
    /// Active environment per project_id (stored locally per user)
    #[serde(default)]
    pub active_environments: BTreeMap<String, String>,
    /// Global user variables available in every project (stored locally in user directory)
    #[serde(default)]
    pub global_variables: BTreeMap<String, String>,
}

fn default_font_size() -> u16 {
    13
}
fn default_timeout_ms() -> u64 {
    30_000
}
fn default_true() -> bool {
    true
}
fn default_max_redirects() -> usize {
    10
}
fn default_user_agent() -> String {
    "Doze/0.1.0".to_string()
}
fn default_history_limit() -> usize {
    100
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            editor_font_size: default_font_size(),
            default_timeout_ms: default_timeout_ms(),
            follow_redirects: true,
            max_redirects: default_max_redirects(),
            verify_tls: true,
            proxy_url: String::new(),
            user_agent: default_user_agent(),
            history_limit: default_history_limit(),
            recent_projects: Vec::new(),
            last_project_path: None,
            active_environments: BTreeMap::new(),
            global_variables: BTreeMap::new(),
        }
    }
}

impl UserSettings {
    pub fn record_recent_project(&mut self, path: &str) {
        self.recent_projects.retain(|p| p != path);
        self.recent_projects.insert(0, path.to_string());
        self.recent_projects.truncate(15);
        self.last_project_path = Some(path.to_string());
    }
}

/// User-private secret store for a single project (`<user_config_dir>/secrets/<project_id>.toml`).
/// Maps `environment_name -> (secret_key -> secret_value)`.
/// NEVER written inside `.doze/`!
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProjectSecretsStore {
    pub project_id: String,
    #[serde(default)]
    pub environments: BTreeMap<String, BTreeMap<String, String>>,
}

impl ProjectSecretsStore {
    pub fn new(project_id: impl Into<String>) -> Self {
        Self {
            project_id: project_id.into(),
            environments: BTreeMap::new(),
        }
    }

    pub fn get_env_secrets(&self, env_name: &str) -> BTreeMap<String, String> {
        self.environments
            .get(env_name)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_secret(&mut self, env_name: &str, key: &str, value: String) {
        self.environments
            .entry(env_name.to_string())
            .or_default()
            .insert(key.to_string(), value);
    }

    pub fn remove_secret(&mut self, env_name: &str, key: &str) {
        if let Some(map) = self.environments.get_mut(env_name) {
            map.remove(key);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub protocol: Protocol,
    pub method: HttpMethod,
    pub resolved_url: String,
    pub status_label: String,
    pub is_success: bool,
    pub duration_ms: u64,
    pub request_snapshot: RequestItem,
}
