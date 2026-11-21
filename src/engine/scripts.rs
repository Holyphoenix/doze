use base64::Engine as _;
use chrono::Utc;
use rhai::{Dynamic, Engine, Scope};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::engine::VariableContext;
use crate::models::{
    ExecutionResponse, HttpMethod, KeyValue, RequestBody, RequestItem, ScriptAssertion,
    ScriptLogLine,
};

#[derive(Debug, Clone, Default)]
pub struct ScriptExecutionResult {
    pub assertions: Vec<ScriptAssertion>,
    pub logs: Vec<ScriptLogLine>,
    pub env_updates: BTreeMap<String, String>,
    pub secret_updates: BTreeMap<String, String>,
    pub error: Option<String>,
}

#[derive(Clone)]
struct SharedScriptState {
    phase: String,
    vars: Arc<Mutex<VariableContext>>,
    env_updates: Arc<Mutex<BTreeMap<String, String>>>,
    secret_updates: Arc<Mutex<BTreeMap<String, String>>>,
    assertions: Arc<Mutex<Vec<ScriptAssertion>>>,
    logs: Arc<Mutex<Vec<ScriptLogLine>>>,
}

#[derive(Clone)]
struct EnvHandle(SharedScriptState);

#[derive(Clone)]
struct SecretsHandle(SharedScriptState);

#[derive(Clone)]
struct VarsHandle(SharedScriptState);

#[derive(Clone)]
struct ReqHandle(Arc<Mutex<RequestItem>>);

#[derive(Clone)]
struct ResHandle(Arc<ExecutionResponse>);

fn build_sandboxed_engine(shared: SharedScriptState) -> Engine {
    let mut engine = Engine::new();
    // Sandboxed execution limits so scripts can never hang the desktop UI
    engine.set_max_operations(50_000);
    engine.set_max_call_levels(32);
    engine.set_max_string_size(1_000_000);
    engine.set_max_array_size(10_000);
    engine.set_max_map_size(10_000);

    // Env API: env.get(k), env.set(k, v)
    engine
        .register_type_with_name::<EnvHandle>("Env")
        .register_fn("get", |h: &mut EnvHandle, key: &str| -> String {
            h.0.vars
                .lock()
                .ok()
                .and_then(|v| v.env_vars.get(key).cloned())
                .unwrap_or_default()
        })
        .register_fn("set", |h: &mut EnvHandle, key: &str, value: Dynamic| {
            let val_str = dynamic_to_string(&value);
            if let Ok(mut v) = h.0.vars.lock() {
                v.env_vars.insert(key.to_string(), val_str.clone());
            }
            if let Ok(mut u) = h.0.env_updates.lock() {
                u.insert(key.to_string(), val_str);
            }
        });

    // Secrets API: secrets.get(k), secrets.set(k, v) — ALWAYS local user-only!
    engine
        .register_type_with_name::<SecretsHandle>("Secrets")
        .register_fn("get", |h: &mut SecretsHandle, key: &str| -> String {
            h.0.vars
                .lock()
                .ok()
                .and_then(|v| v.env_secrets.get(key).cloned())
                .unwrap_or_default()
        })
        .register_fn("set", |h: &mut SecretsHandle, key: &str, value: Dynamic| {
            let val_str = dynamic_to_string(&value);
            if let Ok(mut v) = h.0.vars.lock() {
                v.env_secrets.insert(key.to_string(), val_str.clone());
                v.declared_secret_keys.insert(key.to_string());
            }
            if let Ok(mut u) = h.0.secret_updates.lock() {
                u.insert(key.to_string(), val_str);
            }
        });

    // Layered Vars API: vars.get(k)
    engine
        .register_type_with_name::<VarsHandle>("Vars")
        .register_fn("get", |h: &mut VarsHandle, key: &str| -> String {
            h.0.vars
                .lock()
                .ok()
                .and_then(|v| v.lookup(key))
                .unwrap_or_default()
        });

    // Request API: req.url, req.method, req.body, req.set_url, req.set_header, req.get_header, req.set_query, req.set_body
    engine
        .register_type_with_name::<ReqHandle>("Request")
        .register_get("url", |r: &mut ReqHandle| -> String {
            r.0.lock().map(|g| g.url.clone()).unwrap_or_default()
        })
        .register_fn("set_url", |r: &mut ReqHandle, url: &str| {
            if let Ok(mut g) = r.0.lock() {
                g.url = url.to_string();
            }
        })
        .register_get("method", |r: &mut ReqHandle| -> String {
            r.0.lock()
                .map(|g| g.method.as_str().to_string())
                .unwrap_or_default()
        })
        .register_fn("set_method", |r: &mut ReqHandle, method: &str| {
            if let Ok(mut g) = r.0.lock() {
                g.method = HttpMethod::from_str_lossy(method);
            }
        })
        .register_fn("get_header", |r: &mut ReqHandle, key: &str| -> String {
            r.0.lock()
                .ok()
                .and_then(|g| {
                    g.headers
                        .iter()
                        .find(|h| h.enabled && h.key.eq_ignore_ascii_case(key))
                        .map(|h| h.value.clone())
                })
                .unwrap_or_default()
        })
        .register_fn("set_header", |r: &mut ReqHandle, key: &str, value: Dynamic| {
            let val_str = dynamic_to_string(&value);
            if let Ok(mut g) = r.0.lock() {
                if let Some(existing) = g
                    .headers
                    .iter_mut()
                    .find(|h| h.key.eq_ignore_ascii_case(key))
                {
                    existing.enabled = true;
                    existing.value = val_str;
                } else {
                    g.headers.push(KeyValue::new(key, val_str));
                }
            }
        })
        .register_fn("remove_header", |r: &mut ReqHandle, key: &str| {
            if let Ok(mut g) = r.0.lock() {
                g.headers.retain(|h| !h.key.eq_ignore_ascii_case(key));
            }
        })
        .register_fn("set_query", |r: &mut ReqHandle, key: &str, value: Dynamic| {
            let val_str = dynamic_to_string(&value);
            if let Ok(mut g) = r.0.lock() {
                if let Some(existing) = g.query_params.iter_mut().find(|q| q.key == key) {
                    existing.enabled = true;
                    existing.value = val_str;
                } else {
                    g.query_params.push(KeyValue::new(key, val_str));
                }
            }
        })
        .register_get("body", |r: &mut ReqHandle| -> String {
            r.0.lock()
                .map(|g| match &g.body {
                    RequestBody::Json { content } => content.clone(),
                    RequestBody::Raw { content, .. } => content.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default()
        })
        .register_fn("set_body", |r: &mut ReqHandle, body: &str| {
            if let Ok(mut g) = r.0.lock() {
                match &mut g.body {
                    RequestBody::Raw { content, .. } => *content = body.to_string(),
                    _ => {
                        g.body = RequestBody::Json {
                            content: body.to_string(),
                        }
                    }
                }
            }
        });

    // Response API (post-response scripts)
    engine
        .register_type_with_name::<ResHandle>("Response")
        .register_get("status", |r: &mut ResHandle| -> i64 { r.0.status_code as i64 })
        .register_get("status_text", |r: &mut ResHandle| -> String {
            r.0.status_text.clone()
        })
        .register_get("ok", |r: &mut ResHandle| -> bool { r.0.is_success })
        .register_get("duration_ms", |r: &mut ResHandle| -> i64 {
            r.0.duration_ms as i64
        })
        .register_get("size_bytes", |r: &mut ResHandle| -> i64 {
            r.0.size_bytes as i64
        })
        .register_get("body", |r: &mut ResHandle| -> String { r.0.body.clone() })
        .register_get("url", |r: &mut ResHandle| -> String {
            r.0.resolved_url.clone()
        })
        .register_fn("header", |r: &mut ResHandle, name: &str| -> String {
            r.0.headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case(name))
                .map(|h| h.value.clone())
                .unwrap_or_default()
        })
        .register_fn("json", |r: &mut ResHandle| -> Dynamic {
            serde_json::from_str::<serde_json::Value>(&r.0.body)
                .ok()
                .and_then(|v| rhai::serde::to_dynamic(v).ok())
                .unwrap_or(Dynamic::UNIT)
        });

    // Logging functions
    let logs_info = shared.logs.clone();
    let phase_info = shared.phase.clone();
    engine.register_fn("log", move |msg: Dynamic| {
        if let Ok(mut list) = logs_info.lock() {
            list.push(ScriptLogLine {
                phase: phase_info.clone(),
                level: "info".to_string(),
                message: dynamic_to_string(&msg),
            });
        }
    });

    let logs_warn = shared.logs.clone();
    let phase_warn = shared.phase.clone();
    engine.register_fn("warn", move |msg: Dynamic| {
        if let Ok(mut list) = logs_warn.lock() {
            list.push(ScriptLogLine {
                phase: phase_warn.clone(),
                level: "warn".to_string(),
                message: dynamic_to_string(&msg),
            });
        }
    });

    // Test & assertion functions
    let tests_ref = shared.assertions.clone();
    engine.register_fn("test", move |name: &str, passed: bool| {
        if let Ok(mut list) = tests_ref.lock() {
            list.push(ScriptAssertion {
                name: name.to_string(),
                passed,
                detail: if passed {
                    None
                } else {
                    Some("Assertion evaluated to false".to_string())
                },
            });
        }
    });

    let eq_ref = shared.assertions.clone();
    engine.register_fn(
        "assert_eq",
        move |actual: Dynamic, expected: Dynamic, name: &str| {
            let a_str = dynamic_to_string(&actual);
            let e_str = dynamic_to_string(&expected);
            let passed = a_str == e_str;
            if let Ok(mut list) = eq_ref.lock() {
                list.push(ScriptAssertion {
                    name: name.to_string(),
                    passed,
                    detail: if passed {
                        None
                    } else {
                        Some(format!("expected '{}', got '{}'", e_str, a_str))
                    },
                });
            }
        },
    );

    // Utility built-ins
    engine.register_fn("uuid", || Uuid::new_v4().to_string());
    engine.register_fn("now_ms", || Utc::now().timestamp_millis());
    engine.register_fn("now_iso", || Utc::now().to_rfc3339());
    engine.register_fn("base64_encode", |s: &str| {
        base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
    });
    engine.register_fn("base64_decode", |s: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(s.trim())
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_default()
    });
    engine.register_fn("json_parse", |s: &str| -> Dynamic {
        serde_json::from_str::<serde_json::Value>(s)
            .ok()
            .and_then(|v| rhai::serde::to_dynamic(v).ok())
            .unwrap_or(Dynamic::UNIT)
    });
    engine.register_fn("json_stringify", |val: Dynamic| -> String {
        rhai::serde::from_dynamic::<serde_json::Value>(&val)
            .ok()
            .and_then(|v| serde_json::to_string_pretty(&v).ok())
            .unwrap_or_else(|| val.to_string())
    });

    engine
}

fn dynamic_to_string(d: &Dynamic) -> String {
    if d.is_unit() {
        String::new()
    } else if let Ok(s) = d.clone().into_immutable_string() {
        s.to_string()
    } else {
        d.to_string()
    }
}

/// Runs a request's `pre_request_script` (if non-empty).
/// Can mutate the outgoing `RequestItem` (URL, method, headers, query params, body)
/// as well as set shared `env` vars or local `secrets` in `vars`.
pub fn run_pre_request_script(
    req: &mut RequestItem,
    vars: &mut VariableContext,
) -> ScriptExecutionResult {
    let script = req.pre_request_script.trim().to_string();
    if script.is_empty() {
        return ScriptExecutionResult::default();
    }

    let shared = SharedScriptState {
        phase: "pre".to_string(),
        vars: Arc::new(Mutex::new(vars.clone())),
        env_updates: Arc::new(Mutex::new(BTreeMap::new())),
        secret_updates: Arc::new(Mutex::new(BTreeMap::new())),
        assertions: Arc::new(Mutex::new(Vec::new())),
        logs: Arc::new(Mutex::new(Vec::new())),
    };

    let req_arc = Arc::new(Mutex::new(req.clone()));
    let engine = build_sandboxed_engine(shared.clone());

    let mut scope = Scope::new();
    scope.push("env", EnvHandle(shared.clone()));
    scope.push("secrets", SecretsHandle(shared.clone()));
    scope.push("vars", VarsHandle(shared.clone()));
    scope.push("req", ReqHandle(req_arc.clone()));

    let error = engine
        .eval_with_scope::<Dynamic>(&mut scope, &script)
        .err()
        .map(|e| e.to_string());

    // Apply mutated request & variable context back
    if let Ok(updated_req) = req_arc.lock() {
        *req = updated_req.clone();
    }
    if let Ok(updated_vars) = shared.vars.lock() {
        *vars = updated_vars.clone();
    }

    ScriptExecutionResult {
        assertions: shared.assertions.lock().map(|v| v.clone()).unwrap_or_default(),
        logs: shared.logs.lock().map(|v| v.clone()).unwrap_or_default(),
        env_updates: shared
            .env_updates
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default(),
        secret_updates: shared
            .secret_updates
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default(),
        error,
    }
}

/// Runs a request's `post_response_script` (if non-empty) against `resp`.
/// Supports assertions (`test`, `assert_eq`), console logging (`log`), and extracting
/// values from JSON/headers into shared `env` vars or local-only `secrets`.
pub fn run_post_response_script(
    req: &RequestItem,
    resp: &ExecutionResponse,
    vars: &mut VariableContext,
) -> ScriptExecutionResult {
    let script = req.post_response_script.trim().to_string();
    if script.is_empty() {
        return ScriptExecutionResult::default();
    }

    let shared = SharedScriptState {
        phase: "post".to_string(),
        vars: Arc::new(Mutex::new(vars.clone())),
        env_updates: Arc::new(Mutex::new(BTreeMap::new())),
        secret_updates: Arc::new(Mutex::new(BTreeMap::new())),
        assertions: Arc::new(Mutex::new(Vec::new())),
        logs: Arc::new(Mutex::new(Vec::new())),
    };

    let req_arc = Arc::new(Mutex::new(req.clone()));
    let res_arc = Arc::new(resp.clone());
    let engine = build_sandboxed_engine(shared.clone());

    let mut scope = Scope::new();
    scope.push("env", EnvHandle(shared.clone()));
    scope.push("secrets", SecretsHandle(shared.clone()));
    scope.push("vars", VarsHandle(shared.clone()));
    scope.push("req", ReqHandle(req_arc));
    scope.push("res", ResHandle(res_arc));

    let error = engine
        .eval_with_scope::<Dynamic>(&mut scope, &script)
        .err()
        .map(|e| e.to_string());

    if let Ok(updated_vars) = shared.vars.lock() {
        *vars = updated_vars.clone();
    }

    ScriptExecutionResult {
        assertions: shared.assertions.lock().map(|v| v.clone()).unwrap_or_default(),
        logs: shared.logs.lock().map(|v| v.clone()).unwrap_or_default(),
        env_updates: shared
            .env_updates
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default(),
        secret_updates: shared
            .secret_updates
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default(),
        error,
    }
}
