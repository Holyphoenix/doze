use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

use crate::models::{
    AuthConfig, CollectionNode, EnvironmentFile, FolderMeta, HttpMethod, KeyValue, ProjectConfig,
    RequestBody, RequestItem, DozeProject, SecretKeyDecl,
};
use crate::storage::paths::sanitize_filename;

#[derive(Debug, Error)]
pub enum ProjectStoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error in {path}: {source}")]
    TomlDe {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("Invalid .doze project: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, ProjectStoreError>;

const DOZE_GITIGNORE: &str = r#"# Doze safety guard — secrets and local overrides must NEVER be committed.
# User secrets are stored in your OS user configuration directory, not here.
*.secrets.toml
secrets/
local/
.history/
"#;

/// Initialize a brand new `.doze` package directory with source-control friendly defaults.
pub fn init_project(doze_dir: &Path, project_name: &str, with_samples: bool) -> Result<DozeProject> {
    fs::create_dir_all(doze_dir)?;
    fs::create_dir_all(doze_dir.join("environments"))?;
    fs::create_dir_all(doze_dir.join("requests"))?;

    let gitignore_path = doze_dir.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(&gitignore_path, DOZE_GITIGNORE)?;
    }

    let config = ProjectConfig::new(project_name);
    save_project_config(doze_dir, &config)?;

    // Starter dev environment (non-secret vars + declared secret key placeholder)
    let mut dev = EnvironmentFile::new("dev");
    dev.description = "Local / development environment (safe to commit to git)".to_string();
    dev.variables
        .insert("base_url".to_string(), "https://httpbin.org".to_string());
    dev.variables
        .insert("ws_url".to_string(), "wss://echo.websocket.events".to_string());
    dev.variables
        .insert("rpc_url".to_string(), "https://cloudflare-eth.com".to_string());
    dev.variables
        .insert("grpc_url".to_string(), "http://127.0.0.1:50051".to_string());
    dev.secret_keys.push(SecretKeyDecl {
        key: "api_token".to_string(),
        description: "Personal API bearer token (stored in your user directory, never in .doze)".to_string(),
    });
    save_environment(doze_dir, &dev)?;

    if with_samples {
        seed_sample_requests(doze_dir)?;
    }

    load_project(doze_dir)
}

fn seed_sample_requests(doze_dir: &Path) -> Result<()> {
    // 1. REST GET
    let mut get_req = RequestItem::new_rest("Get JSON Sample", HttpMethod::Get, "{{base_url}}/get");
    get_req.description = "Sample REST GET request using {{base_url}} environment variable".to_string();
    get_req.query_params.push(KeyValue::new("client", "doze"));
    get_req.query_params.push(KeyValue::new("ts", "{{$timestamp}}"));
    save_request(doze_dir, "", &mut get_req)?;

    // 2. REST POST
    let mut post_req = RequestItem::new_rest("Echo POST JSON", HttpMethod::Post, "{{base_url}}/post");
    post_req.description = "Sample REST POST with JSON body and Bearer secret placeholder".to_string();
    post_req.auth = AuthConfig::Bearer {
        token: "{{api_token}}".to_string(),
    };
    post_req.body = RequestBody::Json {
        content: "{\n  \"service\": \"doze-demo\",\n  \"request_id\": \"{{$uuid}}\",\n  \"ok\": true\n}".to_string(),
    };
    save_request(doze_dir, "", &mut post_req)?;

    // 3. WebSocket folder & sample
    create_folder(doze_dir, "websocket", "WebSocket", "Realtime WebSocket endpoints")?;
    let mut ws_req = RequestItem::new_websocket("Echo WebSocket", "{{ws_url}}");
    ws_req.description = "Connect and exchange live text/JSON frames over WebSocket".to_string();
    save_request(doze_dir, "websocket", &mut ws_req)?;

    // 4. RPC folder & samples (JSON-RPC + gRPC)
    create_folder(doze_dir, "rpc", "RPC Services", "JSON-RPC 2.0 and gRPC service calls")?;
    let mut jrpc = RequestItem::new_jsonrpc(
        "JSON-RPC Block Number",
        "{{rpc_url}}",
        "eth_blockNumber",
    );
    jrpc.description = "Standard JSON-RPC 2.0 call over HTTPS".to_string();
    save_request(doze_dir, "rpc", &mut jrpc)?;

    let mut grpc = RequestItem::new_grpc("Echo gRPC Unary", "{{grpc_url}}");
    grpc.description = "Dynamic gRPC call — pick any .proto file to inspect services & invoke methods".to_string();
    grpc.grpc.service = "echo.EchoService".to_string();
    grpc.grpc.method = "SayHello".to_string();
    grpc.grpc.payload_json = "{\n  \"message\": \"Hello from Doze!\"\n}".to_string();
    save_request(doze_dir, "rpc", &mut grpc)?;

    Ok(())
}

/// Load an existing `.doze` package directory into memory.
pub fn load_project(doze_dir: &Path) -> Result<DozeProject> {
    let manifest_path = doze_dir.join("doze.toml");
    if !manifest_path.exists() {
        return Err(ProjectStoreError::Invalid(format!(
            "No doze.toml found in {}",
            doze_dir.display()
        )));
    }

    let raw_manifest = fs::read_to_string(&manifest_path)?;
    let config: ProjectConfig = toml::from_str(&raw_manifest).map_err(|e| ProjectStoreError::TomlDe {
        path: manifest_path.display().to_string(),
        source: e,
    })?;

    // Load environments
    let mut environments = Vec::new();
    let env_dir = doze_dir.join("environments");
    if env_dir.exists() {
        let mut entries: Vec<PathBuf> = fs::read_dir(&env_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension().and_then(|e| e.to_str()) == Some("toml")
                    && !p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .ends_with(".secrets.toml")
            })
            .collect();
        entries.sort();

        for path in entries {
            let content = fs::read_to_string(&path)?;
            let env: EnvironmentFile =
                toml::from_str(&content).map_err(|e| ProjectStoreError::TomlDe {
                    path: path.display().to_string(),
                    source: e,
                })?;
            environments.push(env);
        }
    }

    // Load collections tree
    let req_root = doze_dir.join("requests");
    let collections = if req_root.exists() {
        load_collection_dir(&req_root, &req_root)?
    } else {
        Vec::new()
    };

    Ok(DozeProject {
        doze_dir: doze_dir.to_path_buf(),
        config,
        environments,
        collections,
    })
}

fn load_collection_dir(req_root: &Path, current_dir: &Path) -> Result<Vec<CollectionNode>> {
    let mut folders = Vec::new();
    let mut requests = Vec::new();

    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            let rel = path
                .strip_prefix(req_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");

            let meta_path = path.join("folder.toml");
            let meta = if meta_path.exists() {
                let raw = fs::read_to_string(&meta_path)?;
                toml::from_str::<FolderMeta>(&raw).unwrap_or_else(|_| FolderMeta {
                    name: file_name.clone(),
                    description: String::new(),
                    order: 0,
                })
            } else {
                FolderMeta {
                    name: file_name.clone(),
                    description: String::new(),
                    order: 0,
                }
            };

            let children = load_collection_dir(req_root, &path)?;
            folders.push((
                meta.order,
                meta.name.to_lowercase(),
                CollectionNode::Folder {
                    name: if meta.name.is_empty() {
                        file_name
                    } else {
                        meta.name
                    },
                    relative_path: rel,
                    description: meta.description,
                    children,
                },
            ));
        } else if path.extension().and_then(|e| e.to_str()) == Some("toml")
            && file_name != "folder.toml"
        {
            let content = fs::read_to_string(&path)?;
            let mut req: RequestItem =
                toml::from_str(&content).map_err(|e| ProjectStoreError::TomlDe {
                    path: path.display().to_string(),
                    source: e,
                })?;
            let rel = path
                .strip_prefix(req_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            req.relative_path = Some(rel);
            requests.push(req);
        }
    }

    folders.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    requests.sort_by(|a, b| {
        a.order
            .cmp(&b.order)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let mut nodes: Vec<CollectionNode> = folders.into_iter().map(|(_, _, n)| n).collect();
    nodes.extend(requests.into_iter().map(CollectionNode::Request));
    Ok(nodes)
}

pub fn save_project_config(doze_dir: &Path, config: &ProjectConfig) -> Result<()> {
    fs::create_dir_all(doze_dir)?;
    let body = format!(
        "# Doze Project Package Manifest (.doze/doze.toml)\n# Safe to commit to source control. Secrets are NEVER stored here.\n\n{}",
        toml::to_string_pretty(config)?
    );
    fs::write(doze_dir.join("doze.toml"), body)?;
    Ok(())
}

/// Persists a non-secret environment file into `.doze/environments/<slug>.toml`.
/// Secret values are NEVER accepted or written by this function — only `secret_keys` metadata!
pub fn save_environment(doze_dir: &Path, env: &EnvironmentFile) -> Result<()> {
    let env_dir = doze_dir.join("environments");
    fs::create_dir_all(&env_dir)?;

    // Sanitize: ensure no secret_key accidentally appears inside non-secret `variables`
    let mut clean = env.clone();
    for secret in &clean.secret_keys {
        clean.variables.remove(&secret.key);
    }

    let file_path = env_dir.join(format!("{}.toml", sanitize_filename(&clean.name)));
    let body = format!(
        "# Doze Project Environment: {}\n# Safe to commit to source control.\n# Secret keys listed under [[secret_keys]] have their values stored ONLY in each user's local directory.\n\n{}",
        clean.name,
        toml::to_string_pretty(&clean)?
    );
    fs::write(file_path, body)?;
    Ok(())
}

pub fn delete_environment(doze_dir: &Path, env_name: &str) -> Result<()> {
    let env_dir = doze_dir.join("environments");
    let direct = env_dir.join(format!("{}.toml", sanitize_filename(env_name)));
    if direct.exists() {
        fs::remove_file(direct)?;
        return Ok(());
    }
    // Fallback: scan files to match exact `name`
    if env_dir.exists() {
        for entry in fs::read_dir(&env_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                if let Ok(raw) = fs::read_to_string(&path) {
                    if let Ok(parsed) = toml::from_str::<EnvironmentFile>(&raw) {
                        if parsed.name == env_name {
                            fs::remove_file(path)?;
                            break;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn create_folder(
    doze_dir: &Path,
    relative_folder_path: &str,
    display_name: &str,
    description: &str,
) -> Result<String> {
    let clean_rel: String = relative_folder_path
        .split('/')
        .filter(|s| !s.trim().is_empty())
        .map(sanitize_filename)
        .collect::<Vec<_>>()
        .join("/");

    let target = doze_dir.join("requests").join(&clean_rel);
    fs::create_dir_all(&target)?;

    let meta = FolderMeta {
        name: display_name.to_string(),
        description: description.to_string(),
        order: 0,
    };
    fs::write(target.join("folder.toml"), toml::to_string_pretty(&meta)?)?;
    Ok(clean_rel)
}

pub fn delete_folder(doze_dir: &Path, relative_folder_path: &str) -> Result<()> {
    if relative_folder_path.trim().is_empty() {
        return Err(ProjectStoreError::Invalid(
            "Cannot delete root requests folder".to_string(),
        ));
    }
    let target = doze_dir.join("requests").join(relative_folder_path);
    if target.exists() {
        fs::remove_dir_all(target)?;
    }
    Ok(())
}

/// Save a request inside `.doze/requests/`.
/// If `req.relative_path` is already set, updates that file in place.
/// Otherwise creates a unique `<slug>.toml` inside `folder_rel_path`.
pub fn save_request(
    doze_dir: &Path,
    folder_rel_path: &str,
    req: &mut RequestItem,
) -> Result<PathBuf> {
    let req_root = doze_dir.join("requests");
    fs::create_dir_all(&req_root)?;

    let target_path = if let Some(existing_rel) = &req.relative_path {
        let p = req_root.join(existing_rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        p
    } else {
        let folder_dir = if folder_rel_path.trim().is_empty() {
            req_root.clone()
        } else {
            let d = req_root.join(folder_rel_path);
            fs::create_dir_all(&d)?;
            d
        };

        let base_slug = sanitize_filename(&req.name);
        let mut candidate = folder_dir.join(format!("{}.toml", base_slug));
        let mut suffix = 2u32;
        while candidate.exists() {
            candidate = folder_dir.join(format!("{}_{}.toml", base_slug, suffix));
            suffix += 1;
        }
        let rel = candidate
            .strip_prefix(&req_root)
            .unwrap_or(&candidate)
            .to_string_lossy()
            .replace('\\', "/");
        req.relative_path = Some(rel);
        candidate
    };

    let serialized = toml::to_string_pretty(req)?;
    fs::write(&target_path, serialized)?;
    Ok(target_path)
}

pub fn delete_request(doze_dir: &Path, req: &RequestItem) -> Result<()> {
    if let Some(rel) = &req.relative_path {
        let path = doze_dir.join("requests").join(rel);
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub fn duplicate_request(doze_dir: &Path, original: &RequestItem) -> Result<RequestItem> {
    let mut copy = original.clone();
    copy.id = Uuid::new_v4().to_string();
    copy.name = format!("{} (Copy)", original.name);
    let folder = original
        .relative_path
        .as_deref()
        .and_then(|p| Path::new(p).parent())
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    copy.relative_path = None;
    save_request(doze_dir, &folder, &mut copy)?;
    Ok(copy)
}

/// Returns the parent folder relative path for a request's `relative_path`.
pub fn request_folder(req: &RequestItem) -> String {
    req.relative_path
        .as_deref()
        .and_then(|p| Path::new(p).parent())
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

/// Renames a request in place inside `.doze/requests/`.
pub fn rename_request(doze_dir: &Path, req: &RequestItem, new_name: &str) -> Result<RequestItem> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err(ProjectStoreError::Invalid("Request name cannot be empty".to_string()));
    }
    let mut updated = req.clone();
    updated.name = trimmed.to_string();
    let folder = request_folder(&updated);
    save_request(doze_dir, &folder, &mut updated)?;
    Ok(updated)
}

/// Renames a folder's display name in its `folder.toml`.
pub fn rename_folder(
    doze_dir: &Path,
    relative_folder_path: &str,
    new_display_name: &str,
) -> Result<()> {
    let trimmed = new_display_name.trim();
    if trimmed.is_empty() {
        return Err(ProjectStoreError::Invalid("Folder name cannot be empty".to_string()));
    }
    let target = doze_dir.join("requests").join(relative_folder_path);
    if !target.is_dir() {
        return Err(ProjectStoreError::Invalid(format!(
            "Folder '{}' does not exist",
            relative_folder_path
        )));
    }
    let meta_path = target.join("folder.toml");
    let mut meta = if meta_path.exists() {
        let raw = fs::read_to_string(&meta_path)?;
        toml::from_str::<FolderMeta>(&raw).unwrap_or_default()
    } else {
        FolderMeta::default()
    };
    meta.name = trimmed.to_string();
    fs::write(meta_path, toml::to_string_pretty(&meta)?)?;
    Ok(())
}

/// Moves a request to `target_folder` (empty string `""` = root) and optionally reorders it
/// right before `before_request_id` within `target_folder`.
pub fn move_request(
    doze_dir: &Path,
    req: &RequestItem,
    target_folder: &str,
    before_request_id: Option<&str>,
) -> Result<RequestItem> {
    if before_request_id == Some(req.id.as_str()) {
        return Ok(req.clone());
    }

    let req_root = doze_dir.join("requests");
    let target_dir = if target_folder.trim().is_empty() {
        req_root.clone()
    } else {
        let d = req_root.join(target_folder);
        fs::create_dir_all(&d)?;
        d
    };

    let current_folder = request_folder(req);
    let mut moved = req.clone();

    // Move file on disk if changing folder
    if current_folder != target_folder.trim() {
        let old_abs = req
            .relative_path
            .as_deref()
            .map(|rel| req_root.join(rel));

        moved.relative_path = None;
        save_request(doze_dir, target_folder.trim(), &mut moved)?;

        if let Some(old_path) = old_abs {
            if old_path.exists() {
                let new_abs = moved
                    .relative_path
                    .as_deref()
                    .map(|rel| req_root.join(rel));
                if new_abs.as_ref() != Some(&old_path) {
                    let _ = fs::remove_file(old_path);
                }
            }
        }
    }

    // Load all sibling requests in target_dir, sort by current order + name,
    // then insert `moved` at the requested slot and re-assign deterministic `order` (10, 20, ...).
    let mut siblings: Vec<RequestItem> = Vec::new();
    for entry in fs::read_dir(&target_dir)? {
        let path = entry?.path();
        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_file()
            && path.extension().and_then(|e| e.to_str()) == Some("toml")
            && fname != "folder.toml"
        {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(mut item) = toml::from_str::<RequestItem>(&raw) {
                    let rel = path
                        .strip_prefix(&req_root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    item.relative_path = Some(rel);
                    if item.id != moved.id {
                        siblings.push(item);
                    }
                }
            }
        }
    }

    siblings.sort_by(|a, b| {
        a.order
            .cmp(&b.order)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let insert_idx = before_request_id
        .and_then(|target_id| siblings.iter().position(|r| r.id == target_id))
        .unwrap_or(siblings.len());

    siblings.insert(insert_idx, moved.clone());

    let mut final_moved = moved;
    for (idx, item) in siblings.iter_mut().enumerate() {
        let new_order = ((idx + 1) * 10) as i32;
        if item.order != new_order || item.id == final_moved.id {
            item.order = new_order;
            save_request(doze_dir, target_folder.trim(), item)?;
            if item.id == final_moved.id {
                final_moved = item.clone();
            }
        }
    }

    Ok(final_moved)
}

/// Resolves which project to load on app startup:
/// 1. `settings.last_project_path` if it still exists and is a valid `.doze` package
/// 2. The first valid existing `.doze` package in `settings.recent_projects`
/// 3. The default scratchpad workspace (`ensure_default_workspace`)
pub fn load_startup_project(settings: &crate::models::UserSettings) -> Result<DozeProject> {
    let candidates = settings
        .last_project_path
        .iter()
        .chain(settings.recent_projects.iter());

    for raw in candidates {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let dir = crate::storage::paths::resolve_doze_dir(Path::new(trimmed));
        if dir.join("doze.toml").exists() {
            if let Ok(proj) = load_project(&dir) {
                return Ok(proj);
            }
        }
    }

    ensure_default_workspace()
}

/// Ensures the default scratchpad `.doze` workspace exists and returns it.
pub fn ensure_default_workspace() -> Result<DozeProject> {
    let dir = crate::storage::paths::default_workspace_doze_dir();
    if dir.join("doze.toml").exists() {
        load_project(&dir)
    } else {
        init_project(&dir, "Doze Scratchpad", true)
    }
}

/// Opens an existing `.doze` package inside `dir`, or automatically initializes a new `.doze`
/// package if `dir` does not yet contain one. Returns `(project, was_created)`.
pub fn open_or_init_project(dir: &Path) -> Result<(DozeProject, bool)> {
    let doze_dir = crate::storage::paths::resolve_doze_dir(dir);
    if doze_dir.join("doze.toml").exists() {
        return Ok((load_project(&doze_dir)?, false));
    }

    let project_name = doze_dir
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Doze Project")
        .to_string();

    let project = init_project(&doze_dir, &project_name, false)?;
    Ok((project, true))
}

/// Helper to build an effective variable map for preview/inspection (masking secrets if desired).
pub fn active_env_map(
    project: &DozeProject,
    active_env_name: Option<&str>,
) -> BTreeMap<String, String> {
    if let Some(name) = active_env_name {
        if let Some(env) = project.environments.iter().find(|e| e.name == name) {
            return env.variables.clone();
        }
    }
    BTreeMap::new()
}
