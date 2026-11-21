use doze::engine::{curl, grpc, VariableContext};
use doze::models::{
    AuthConfig, EnvironmentFile, HttpMethod, KeyValue, ProjectSecretsStore, RequestBody,
    RequestItem, SecretKeyDecl, UserSettings,
};
use doze::storage::{project_store, user_store};
use tempfile::tempdir;

#[test]
fn project_package_never_persists_secrets_into_doze_dir() {
    let tmp = tempdir().unwrap();
    let doze_dir = tmp.path().join(".doze");
    let user_secrets_file = tmp.path().join("user_secrets.toml");

    let proj = project_store::init_project(&doze_dir, "Payments Service", true).unwrap();
    assert!(doze_dir.join("doze.toml").exists());
    assert!(doze_dir.join(".gitignore").exists());
    assert!(doze_dir.join("environments").join("dev.toml").exists());

    // Add a variable AND a secret key declaration with an actual secret value
    let mut env = EnvironmentFile::new("staging");
    env.variables
        .insert("base_url".to_string(), "https://staging.example.com".to_string());
    // Even if someone accidentally put a secret key inside variables, save_environment strips it!
    env.variables
        .insert("db_api_secret".to_string(), "SUPER_SENSITIVE_VALUE".to_string());
    env.secret_keys.push(SecretKeyDecl {
        key: "db_api_secret".to_string(),
        description: "Staging API secret".to_string(),
    });
    project_store::save_environment(&doze_dir, &env).unwrap();

    // Save the actual secret value strictly to user secrets file
    let mut secrets = ProjectSecretsStore::new(&proj.config.project_id);
    secrets.set_secret("staging", "db_api_secret", "SUPER_SENSITIVE_VALUE".to_string());
    user_store::save_project_secrets_to(&secrets, &user_secrets_file).unwrap();

    // Verify .doze/environments/staging.toml declares the key name, NEVER the value
    let env_toml = std::fs::read_to_string(doze_dir.join("environments").join("staging.toml")).unwrap();
    assert!(env_toml.contains("base_url"));
    assert!(env_toml.contains("db_api_secret"));
    assert!(
        !env_toml.contains("SUPER_SENSITIVE_VALUE"),
        "Secret value must NEVER be stored inside .doze!"
    );

    // Reload project + user secrets and verify resolution works
    let reloaded = project_store::load_project(&doze_dir).unwrap();
    let loaded_secrets =
        user_store::load_project_secrets_from(&reloaded.config.project_id, &user_secrets_file)
            .unwrap();
    let staging = reloaded
        .environments
        .iter()
        .find(|e| e.name == "staging")
        .unwrap();

    let ctx = VariableContext::build(&UserSettings::default(), Some(staging), Some(&loaded_secrets));
    assert_eq!(ctx.interpolate("{{base_url}}/v1"), "https://staging.example.com/v1");
    assert_eq!(ctx.interpolate("Bearer {{db_api_secret}}"), "Bearer SUPER_SENSITIVE_VALUE");
    assert_eq!(ctx.interpolate_masked("Bearer {{db_api_secret}}"), "Bearer ••••••");
}

#[test]
fn variable_precedence_and_url_resolution() {
    let mut user = UserSettings::default();
    user.global_variables
        .insert("host".to_string(), "global.example.com".to_string());
    user.global_variables
        .insert("global_only".to_string(), "yes".to_string());

    let mut env = EnvironmentFile::new("dev");
    env.variables
        .insert("host".to_string(), "dev.example.com".to_string());
    env.secret_keys.push(SecretKeyDecl {
        key: "token".to_string(),
        description: String::new(),
    });

    let mut secrets = ProjectSecretsStore::new("proj-1");
    secrets.set_secret("dev", "token", "s3cr3t".to_string());

    let ctx = VariableContext::build(&user, Some(&env), Some(&secrets));

    assert_eq!(ctx.lookup("host").as_deref(), Some("dev.example.com"));
    assert_eq!(ctx.lookup("global_only").as_deref(), Some("yes"));
    assert_eq!(ctx.lookup("token").as_deref(), Some("s3cr3t"));
    assert!(ctx.lookup("$uuid").unwrap().len() >= 32);

    let url = ctx.resolve_url(
        "https://{{host}}/users/:id",
        &[KeyValue::new("id", "42")],
        &[KeyValue::new("active", "{{global_only}}")],
        None,
        false,
    );
    assert_eq!(url, "https://dev.example.com/users/42?active=yes");

    let missing = ctx.find_unresolved("https://{{host}}/{{missing_var}}");
    assert_eq!(missing, vec!["missing_var".to_string()]);
}

#[test]
fn user_settings_roundtrip() {
    let tmp = tempdir().unwrap();
    let settings_path = tmp.path().join("settings.toml");

    let mut s = UserSettings::default();
    s.default_timeout_ms = 12_500;
    s.verify_tls = false;
    s.global_variables
        .insert("org".to_string(), "acme".to_string());
    s.record_recent_project("/work/api/.doze");

    user_store::save_user_settings_to(&s, &settings_path).unwrap();
    let loaded = user_store::load_user_settings_from(&settings_path).unwrap();

    assert_eq!(loaded.default_timeout_ms, 12_500);
    assert!(!loaded.verify_tls);
    assert_eq!(loaded.global_variables.get("org").map(String::as_str), Some("acme"));
    assert_eq!(loaded.last_project_path.as_deref(), Some("/work/api/.doze"));
}

#[test]
fn curl_export_masks_secrets_and_imports_cleanly() {
    let mut env = EnvironmentFile::new("dev");
    env.variables
        .insert("base_url".to_string(), "https://api.acme.test".to_string());
    env.secret_keys.push(SecretKeyDecl {
        key: "api_key".to_string(),
        description: String::new(),
    });
    let mut secrets = ProjectSecretsStore::new("p");
    secrets.set_secret("dev", "api_key", "live-secret-999".to_string());
    let ctx = VariableContext::build(&UserSettings::default(), Some(&env), Some(&secrets));

    let mut req = RequestItem::new_rest("Create Item", HttpMethod::Post, "{{base_url}}/items");
    req.auth = AuthConfig::Bearer {
        token: "{{api_key}}".to_string(),
    };
    req.body = RequestBody::Json {
        content: "{\"name\":\"bolt\"}".to_string(),
    };

    let masked_cmd = curl::export_to_curl(&req, &ctx, true);
    assert!(masked_cmd.contains("https://api.acme.test/items"));
    assert!(!masked_cmd.contains("live-secret-999"));
    assert!(masked_cmd.contains("••••••"));

    let unmasked_cmd = curl::export_to_curl(&req, &ctx, false);
    assert!(unmasked_cmd.contains("live-secret-999"));

    let imported = curl::import_from_curl(
        "curl -X PUT 'https://api.acme.test/items/7?dry=true' -H 'Authorization: Bearer tok123' -d '{\"ok\":true}'",
    )
    .unwrap();
    assert_eq!(imported.method, HttpMethod::Put);
    assert_eq!(imported.url, "https://api.acme.test/items/7");
    assert_eq!(imported.query_params.len(), 1);
    assert_eq!(imported.query_params[0].key, "dry");
    assert_eq!(
        imported.auth,
        AuthConfig::Bearer {
            token: "tok123".to_string()
        }
    );
}

#[test]
fn grpc_proto_inspection_generates_services_and_sample_json() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir.join("examples").join("demo-service");
    let services =
        grpc::inspect_proto_services(".doze/proto/echo.proto", &[], Some(&project_root))
            .expect("pure-Rust protox inspection should succeed");

    assert_eq!(services.len(), 1);
    assert_eq!(services[0].full_name, "echo.EchoService");
    assert_eq!(services[0].methods.len(), 2);

    let say_hello = services[0]
        .methods
        .iter()
        .find(|m| m.method_name == "SayHello")
        .unwrap();
    let sample: serde_json::Value = serde_json::from_str(&say_hello.sample_payload_json).unwrap();
    assert!(sample.get("message").is_some());
    assert!(sample.get("repeatCount").is_some());
    assert!(sample.get("tags").is_some());
}

#[tokio::test]
async fn live_rest_and_jsonrpc_and_websocket_engines_roundtrip() {
    use futures_util::{SinkExt, StreamExt};
    use doze::engine::{http, jsonrpc, websocket};
    use doze::models::{ProjectConfig, WsConnectionStatus, WsDirection};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // 1. Minimal loopback HTTP/JSON-RPC server
    let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http_addr = http_listener.local_addr().unwrap();
    tokio::spawn(async move {
        for _ in 0..2 {
            if let Ok((mut sock, _)) = http_listener.accept().await {
                let mut buf = vec![0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req_str = String::from_utf8_lossy(&buf[..n]);
                let resp_body = if req_str.contains("\"jsonrpc\"") {
                    r#"{"jsonrpc":"2.0","id":1,"result":{"block":"0x123"}}"#
                } else {
                    r#"{"ok":true,"echo":"hello"}"#
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    resp_body.len(),
                    resp_body
                );
                let _ = sock.write_all(response.as_bytes()).await;
            }
        }
    });

    let ctx = VariableContext::default();
    let proj = ProjectConfig::new("Test");
    let user = UserSettings::default();

    // Test REST GET with query param & auth header
    let mut rest_req = RequestItem::new_rest(
        "Loopback GET",
        HttpMethod::Get,
        format!("http://{}/health", http_addr),
    );
    rest_req.query_params.push(KeyValue::new("v", "1"));
    rest_req.auth = AuthConfig::Bearer {
        token: "test-tok".to_string(),
    };
    let rest_resp = http::execute_rest(&rest_req, &ctx, &proj, &user).await;
    assert_eq!(rest_resp.status_code, 200);
    assert!(rest_resp.is_success);
    assert!(rest_resp.body.contains("\"echo\":\"hello\""));

    // Test JSON-RPC 2.0 POST
    let rpc_req = RequestItem::new_jsonrpc(
        "Loopback RPC",
        format!("http://{}/rpc", http_addr),
        "chain_getBlock",
    );
    let rpc_resp = jsonrpc::execute_jsonrpc(&rpc_req, &ctx, &proj, &user).await;
    assert_eq!(rpc_resp.status_code, 200);
    assert!(rpc_resp.is_success);
    assert!(rpc_resp.body.contains("0x123"));
    assert!(rpc_resp
        .protocol_note
        .as_deref()
        .unwrap_or("")
        .contains("JSON-RPC 2.0 OK"));

    // 2. Minimal loopback WebSocket echo server
    let ws_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws_addr = ws_listener.local_addr().unwrap();
    tokio::spawn(async move {
        if let Ok((stream, _)) = ws_listener.accept().await {
            if let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await {
                if let Some(Ok(msg)) = ws.next().await {
                    let _ = ws.send(msg).await;
                }
            }
        }
    });

    let ws_req = RequestItem::new_websocket("Loopback WS", format!("ws://{}", ws_addr));
    let (handle, mut rx) = websocket::connect_websocket(&ws_req, &ctx, &proj);

    // Wait for Connected
    let mut connected = false;
    while let Some(evt) = rx.recv().await {
        if let websocket::WsEvent::StatusChanged(WsConnectionStatus::Connected) = evt {
            connected = true;
            break;
        }
    }
    assert!(connected, "WebSocket client should connect to loopback server");

    handle.send_text("{\"ping\":true}".to_string());

    let mut received_echo = false;
    while let Some(evt) = rx.recv().await {
        if let websocket::WsEvent::Log(log) = evt {
            if log.direction == WsDirection::Received && log.payload == "{\"ping\":true}" {
                received_echo = true;
                break;
            }
        }
    }
    assert!(received_echo, "WebSocket client should receive echoed frame");
    handle.disconnect();
}

#[test]
fn move_reorder_and_rename_requests_and_folders() {
    let tmp = tempdir().unwrap();
    let doze_dir = tmp.path().join(".doze");
    project_store::init_project(&doze_dir, "Drag Test", false).unwrap();

    project_store::create_folder(&doze_dir, "users", "Users", "").unwrap();

    let mut req_a = RequestItem::new_rest("Alpha", HttpMethod::Get, "{{base_url}}/a");
    let mut req_b = RequestItem::new_rest("Beta", HttpMethod::Get, "{{base_url}}/b");
    project_store::save_request(&doze_dir, "", &mut req_a).unwrap();
    project_store::save_request(&doze_dir, "", &mut req_b).unwrap();

    // Reorder Beta before Alpha at root
    let moved_b = project_store::move_request(&doze_dir, &req_b, "", Some(&req_a.id)).unwrap();
    let loaded = project_store::load_project(&doze_dir).unwrap();
    let root_req_names: Vec<String> = loaded
        .collections
        .iter()
        .filter_map(|n| match n {
            doze::models::CollectionNode::Request(r) => Some(r.name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(root_req_names, vec!["Beta".to_string(), "Alpha".to_string()]);

    // Move Beta into the "users" folder
    let moved_to_users = project_store::move_request(&doze_dir, &moved_b, "users", None).unwrap();
    assert!(moved_to_users
        .relative_path
        .as_deref()
        .unwrap_or("")
        .starts_with("users/"));

    // Rename Beta -> Get User and rename folder Users -> Accounts
    let renamed = project_store::rename_request(&doze_dir, &moved_to_users, "Get User").unwrap();
    assert_eq!(renamed.name, "Get User");
    project_store::rename_folder(&doze_dir, "users", "Accounts").unwrap();

    let reloaded = project_store::load_project(&doze_dir).unwrap();
    let accounts_folder = reloaded.collections.iter().find_map(|n| match n {
        doze::models::CollectionNode::Folder { name, children, .. } if name == "Accounts" => {
            Some(children.clone())
        }
        _ => None,
    });
    let children = accounts_folder.expect("Accounts folder should exist");
    assert_eq!(children.len(), 1);
    match &children[0] {
        doze::models::CollectionNode::Request(r) => assert_eq!(r.name, "Get User"),
        _ => panic!("Expected moved request inside Accounts folder"),
    }
}

#[test]
fn open_or_init_auto_generates_doze_for_plain_directory() {
    let tmp = tempdir().unwrap();
    let repo_dir = tmp.path().join("my-backend-service");
    std::fs::create_dir_all(&repo_dir).unwrap();

    // First open: directory has no .doze yet -> auto-creates it named after the folder
    let (created_proj, was_created) = project_store::open_or_init_project(&repo_dir).unwrap();
    assert!(was_created);
    assert_eq!(created_proj.config.name, "my-backend-service");
    assert!(repo_dir.join(".doze").join("doze.toml").exists());
    assert!(repo_dir.join(".doze").join(".gitignore").exists());

    // Second open: loads the existing .doze package with the same project_id
    let (reopened_proj, was_created_again) = project_store::open_or_init_project(&repo_dir).unwrap();
    assert!(!was_created_again);
    assert_eq!(reopened_proj.config.project_id, created_proj.config.project_id);
}

#[test]
fn startup_restores_last_opened_project_and_falls_back_through_recents() {
    let tmp = tempdir().unwrap();
    let proj_a = tmp.path().join("service-a");
    let proj_b = tmp.path().join("service-b");
    std::fs::create_dir_all(&proj_a).unwrap();
    std::fs::create_dir_all(&proj_b).unwrap();

    let (a, _) = project_store::open_or_init_project(&proj_a).unwrap();
    let (b, _) = project_store::open_or_init_project(&proj_b).unwrap();

    let mut settings = UserSettings::default();
    settings.record_recent_project(&a.doze_dir.to_string_lossy());
    settings.record_recent_project(&b.doze_dir.to_string_lossy());

    // Startup should restore last opened project (service-b)
    let loaded = project_store::load_startup_project(&settings).unwrap();
    assert_eq!(loaded.config.project_id, b.config.project_id);

    // If service-b is deleted, startup falls back to next recent project (service-a)
    std::fs::remove_dir_all(&proj_b).unwrap();
    let fallback = project_store::load_startup_project(&settings).unwrap();
    assert_eq!(fallback.config.project_id, a.config.project_id);
}

#[test]
fn pre_request_and_post_response_scripts_execute_and_sandbox_loops() {
    use doze::engine::scripts;
    use doze::models::{ExecutionResponse, HeaderPair, ScriptRunSummary};

    let mut req = RequestItem::new_rest("Auth Login", HttpMethod::Post, "{{base_url}}/login");
    req.pre_request_script = r#"
        req.set_header("X-Trace-Id", "trace-123");
        req.set_query("mode", "strict");
        env.set("last_run", "ok");
        log("pre-script ran for " + req.method);
    "#
    .to_string();
    req.post_response_script = r#"
        test("Status is 200", res.status == 200);
        let body = res.json();
        assert_eq(body.user.role, "admin", "Role is admin");
        assert_eq(res.header("x-server"), "doze-test", "Header matches");
        secrets.set("api_token", body.access_token);
        env.set("user_id", body.user.id);
        log("extracted token for user " + env.get("user_id"));
    "#
    .to_string();

    let mut vars = VariableContext::default();
    let pre = scripts::run_pre_request_script(&mut req, &mut vars);
    assert!(pre.error.is_none(), "pre-script error: {:?}", pre.error);
    assert!(req
        .headers
        .iter()
        .any(|h| h.key == "X-Trace-Id" && h.value == "trace-123"));
    assert!(req
        .query_params
        .iter()
        .any(|q| q.key == "mode" && q.value == "strict"));
    assert_eq!(pre.env_updates.get("last_run").map(String::as_str), Some("ok"));
    assert_eq!(vars.lookup("last_run").as_deref(), Some("ok"));
    assert_eq!(pre.logs.len(), 1);

    let fake_resp = ExecutionResponse {
        status_code: 200,
        status_text: "200 OK".to_string(),
        is_success: true,
        duration_ms: 42,
        size_bytes: 64,
        content_type: "application/json".to_string(),
        body: r#"{"access_token":"jwt-secret-xyz","user":{"id":"u-9","role":"admin"}}"#.to_string(),
        headers: vec![HeaderPair {
            name: "X-Server".to_string(),
            value: "doze-test".to_string(),
        }],
        trailers: Vec::new(),
        cookies: Vec::new(),
        resolved_url: "https://api.example.com/login".to_string(),
        error: None,
        protocol_note: None,
        scripts: ScriptRunSummary::default(),
    };

    let post = scripts::run_post_response_script(&req, &fake_resp, &mut vars);
    assert!(post.error.is_none(), "post-script error: {:?}", post.error);
    assert_eq!(post.assertions.len(), 3);
    assert!(post.assertions.iter().all(|a| a.passed));
    assert_eq!(
        post.secret_updates.get("api_token").map(String::as_str),
        Some("jwt-secret-xyz")
    );
    assert_eq!(post.env_updates.get("user_id").map(String::as_str), Some("u-9"));
    assert_eq!(vars.lookup("api_token").as_deref(), Some("jwt-secret-xyz"));
    assert!(vars.is_secret_key("api_token"));

    // Verify sandbox prevents infinite loops from hanging the app
    let mut bad_req = RequestItem::new_rest("Infinite Loop", HttpMethod::Get, "http://localhost");
    bad_req.pre_request_script = "loop { }".to_string();
    let bad_res = scripts::run_pre_request_script(&mut bad_req, &mut vars);
    assert!(bad_res.error.is_some(), "infinite loop must be terminated by sandbox");
}
