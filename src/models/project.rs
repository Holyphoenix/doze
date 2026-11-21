use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use uuid::Uuid;

use super::request::{KeyValue, RequestItem};

/// Project-level configuration persisted in `<project>/.doze/doze.toml`.
/// Safe and intended to be committed to source control (Git, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Stable identifier used to bind user-local secrets (in the user directory) to this project
    /// without storing any secrets inside `.doze/`.
    #[serde(default = "new_project_id")]
    pub project_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub default_environment: Option<String>,
    #[serde(default)]
    pub default_headers: Vec<KeyValue>,
    #[serde(default)]
    pub proto_include_dirs: Vec<String>,
    #[serde(default)]
    pub default_timeout_ms: Option<u64>,
    #[serde(default)]
    pub follow_redirects: Option<bool>,
}

fn new_project_id() -> String {
    Uuid::new_v4().to_string()
}

fn default_schema_version() -> u32 {
    1
}

impl ProjectConfig {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            project_id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            schema_version: 1,
            default_environment: Some("dev".to_string()),
            default_headers: Vec::new(),
            proto_include_dirs: Vec::new(),
            default_timeout_ms: None,
            follow_redirects: None,
        }
    }
}

/// Declares a secret variable that this environment expects.
/// Stored in `.doze/environments/<env>.toml` with ONLY the key name and description —
/// the actual secret value is NEVER stored in `.doze/` and always lives in the user directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretKeyDecl {
    pub key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Non-secret environment file persisted in `.doze/environments/<name>.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentFile {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Shared non-secret variables (committed to source control)
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    /// Declared secret variable names (ONLY names/descriptions are stored here;
    /// values are stored exclusively in the user's private secrets file!)
    #[serde(default)]
    pub secret_keys: Vec<SecretKeyDecl>,
}

impl EnvironmentFile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            variables: BTreeMap::new(),
            secret_keys: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FolderMeta {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionNode {
    Folder {
        name: String,
        relative_path: String,
        description: String,
        children: Vec<CollectionNode>,
    },
    Request(RequestItem),
}

/// Fully loaded in-memory representation of a `.doze` project package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DozeProject {
    /// Path to the `.doze` directory itself
    pub doze_dir: PathBuf,
    pub config: ProjectConfig,
    pub environments: Vec<EnvironmentFile>,
    pub collections: Vec<CollectionNode>,
}

impl DozeProject {
    pub fn project_root(&self) -> PathBuf {
        self.doze_dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.doze_dir.clone())
    }

    pub fn find_request_by_id(&self, id: &str) -> Option<RequestItem> {
        find_in_nodes(&self.collections, id)
    }

    pub fn all_requests(&self) -> Vec<RequestItem> {
        let mut out = Vec::new();
        collect_requests(&self.collections, &mut out);
        out
    }

    pub fn all_folder_paths(&self) -> Vec<String> {
        let mut out = vec![String::new()]; // root ""
        collect_folders(&self.collections, &mut out);
        out
    }
}

fn find_in_nodes(nodes: &[CollectionNode], id: &str) -> Option<RequestItem> {
    for node in nodes {
        match node {
            CollectionNode::Request(r) if r.id == id => return Some(r.clone()),
            CollectionNode::Folder { children, .. } => {
                if let Some(found) = find_in_nodes(children, id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

fn collect_requests(nodes: &[CollectionNode], out: &mut Vec<RequestItem>) {
    for node in nodes {
        match node {
            CollectionNode::Request(r) => out.push(r.clone()),
            CollectionNode::Folder { children, .. } => collect_requests(children, out),
        }
    }
}

fn collect_folders(nodes: &[CollectionNode], out: &mut Vec<String>) {
    for node in nodes {
        if let CollectionNode::Folder {
            relative_path,
            children,
            ..
        } = node
        {
            out.push(relative_path.clone());
            collect_folders(children, out);
        }
    }
}
