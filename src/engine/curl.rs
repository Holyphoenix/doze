use crate::engine::VariableContext;
use crate::models::{
    ApiKeyLocation, AuthConfig, HttpMethod, KeyValue, Protocol, RequestBody, RequestItem,
};

/// Generates a clean, shareable `curl` command from a RequestItem.
/// Secrets are masked by default (`mask_secrets = true`) so accidental pastes don't leak tokens!
pub fn export_to_curl(req: &RequestItem, vars: &VariableContext, mask_secrets: bool) -> String {
    let interp = |s: &str| {
        if mask_secrets {
            vars.interpolate_masked(s)
        } else {
            vars.interpolate(s)
        }
    };

    let auth_query = match &req.auth {
        AuthConfig::ApiKey {
            key,
            value,
            location: ApiKeyLocation::Query,
        } if !key.trim().is_empty() => Some((interp(key), interp(value))),
        _ => None,
    };

    let url = vars.resolve_url(
        &req.url,
        &req.path_params,
        &req.query_params,
        auth_query
            .as_ref()
            .map(|(k, v)| (k.as_str(), v.as_str())),
        mask_secrets,
    );

    let mut parts = vec!["curl".to_string()];

    let method = match req.protocol {
        Protocol::JsonRpc => "POST",
        _ => req.method.as_str(),
    };
    if method != "GET" {
        parts.push(format!("-X {}", method));
    }
    parts.push(shell_quote(&url));

    for h in req.headers.iter().filter(|h| h.enabled && !h.key.trim().is_empty()) {
        parts.push(format!(
            "-H {}",
            shell_quote(&format!("{}: {}", interp(&h.key), interp(&h.value)))
        ));
    }

    match &req.auth {
        AuthConfig::None => {}
        AuthConfig::Bearer { token } => {
            parts.push(format!(
                "-H {}",
                shell_quote(&format!("Authorization: Bearer {}", interp(token)))
            ));
        }
        AuthConfig::Basic { username, password } => {
            parts.push(format!(
                "-u {}",
                shell_quote(&format!("{}:{}", interp(username), interp(password)))
            ));
        }
        AuthConfig::ApiKey {
            key,
            value,
            location: ApiKeyLocation::Header,
        } => {
            parts.push(format!(
                "-H {}",
                shell_quote(&format!("{}: {}", interp(key), interp(value)))
            ));
        }
        AuthConfig::ApiKey {
            location: ApiKeyLocation::Query,
            ..
        } => {}
    }

    if req.protocol == Protocol::JsonRpc {
        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": interp(&req.jsonrpc.method),
            "params": serde_json::from_str::<serde_json::Value>(&interp(&req.jsonrpc.params_json))
                .unwrap_or_else(|_| serde_json::json!([])),
        });
        parts.push("-H 'Content-Type: application/json'".to_string());
        parts.push(format!("-d {}", shell_quote(&envelope.to_string())));
    } else {
        match &req.body {
            RequestBody::None => {}
            RequestBody::Json { content } => {
                parts.push("-H 'Content-Type: application/json'".to_string());
                parts.push(format!("-d {}", shell_quote(&interp(content))));
            }
            RequestBody::Raw {
                content,
                content_type,
            } => {
                parts.push(format!(
                    "-H {}",
                    shell_quote(&format!("Content-Type: {}", content_type))
                ));
                parts.push(format!("-d {}", shell_quote(&interp(content))));
            }
            RequestBody::FormUrlEncoded { fields } => {
                for f in fields.iter().filter(|f| f.enabled && !f.key.trim().is_empty()) {
                    parts.push(format!(
                        "--data-urlencode {}",
                        shell_quote(&format!("{}={}", interp(&f.key), interp(&f.value)))
                    ));
                }
            }
            RequestBody::Multipart { fields } => {
                for f in fields.iter().filter(|f| f.enabled && !f.key.trim().is_empty()) {
                    let val = if f.is_file {
                        format!("@{}", interp(&f.value))
                    } else {
                        interp(&f.value)
                    };
                    parts.push(format!("-F {}", shell_quote(&format!("{}={}", interp(&f.key), val))));
                }
            }
            RequestBody::Binary { file_path } => {
                parts.push(format!(
                    "--data-binary {}",
                    shell_quote(&format!("@{}", interp(file_path)))
                ));
            }
        }
    }

    parts.join(" \\\n  ")
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Parses a common `curl` command string into a Doze `RequestItem`.
pub fn import_from_curl(curl_cmd: &str) -> Result<RequestItem, String> {
    let cleaned = curl_cmd.replace("\\\r\n", " ").replace("\\\n", " ");
    let tokens = tokenize_shell(&cleaned);
    if tokens.is_empty() {
        return Err("Empty cURL command".to_string());
    }

    let mut method: Option<HttpMethod> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<KeyValue> = Vec::new();
    let mut body_parts: Vec<String> = Vec::new();
    let mut auth = AuthConfig::None;

    let mut i = 0;
    if tokens[0].eq_ignore_ascii_case("curl") {
        i = 1;
    }

    while i < tokens.len() {
        let tok = &tokens[i];
        match tok.as_str() {
            "-X" | "--request" => {
                if let Some(m) = tokens.get(i + 1) {
                    method = Some(HttpMethod::from_str_lossy(m));
                    i += 2;
                    continue;
                }
            }
            "-H" | "--header" => {
                if let Some(h) = tokens.get(i + 1) {
                    if let Some((k, v)) = h.split_once(':') {
                        let key = k.trim();
                        let val = v.trim();
                        if key.eq_ignore_ascii_case("authorization") {
                            if let Some(token) = val.strip_prefix("Bearer ").or_else(|| val.strip_prefix("bearer ")) {
                                auth = AuthConfig::Bearer {
                                    token: token.trim().to_string(),
                                };
                            } else {
                                headers.push(KeyValue::new(key, val));
                            }
                        } else {
                            headers.push(KeyValue::new(key, val));
                        }
                    }
                    i += 2;
                    continue;
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" => {
                if let Some(d) = tokens.get(i + 1) {
                    body_parts.push(d.clone());
                    if method.is_none() {
                        method = Some(HttpMethod::Post);
                    }
                    i += 2;
                    continue;
                }
            }
            "-u" | "--user" => {
                if let Some(creds) = tokens.get(i + 1) {
                    let (u, p) = creds.split_once(':').unwrap_or((creds, ""));
                    auth = AuthConfig::Basic {
                        username: u.to_string(),
                        password: p.to_string(),
                    };
                    i += 2;
                    continue;
                }
            }
            "--url" => {
                if let Some(u) = tokens.get(i + 1) {
                    url = Some(u.clone());
                    i += 2;
                    continue;
                }
            }
            other => {
                if !other.starts_with('-') && url.is_none() {
                    url = Some(other.to_string());
                }
            }
        }
        i += 1;
    }

    let raw_url = url.ok_or_else(|| "Could not find a URL in the cURL command".to_string())?;
    let mut query_params = Vec::new();
    let base_url = if let Ok(mut parsed) = url::Url::parse(&raw_url) {
        for (k, v) in parsed.query_pairs() {
            query_params.push(KeyValue::new(k.to_string(), v.to_string()));
        }
        parsed.set_query(None);
        parsed.to_string()
    } else {
        raw_url
    };

    let mut req = RequestItem::new_rest(
        "Imported cURL",
        method.unwrap_or(HttpMethod::Get),
        base_url,
    );
    req.query_params = query_params;
    req.headers = headers;
    req.auth = auth;

    if !body_parts.is_empty() {
        let joined = body_parts.join("&");
        if serde_json::from_str::<serde_json::Value>(&joined).is_ok() {
            req.body = RequestBody::Json { content: joined };
        } else {
            req.body = RequestBody::Raw {
                content: joined,
                content_type: "text/plain".to_string(),
            };
        }
    }

    Ok(req)
}

fn tokenize_shell(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut quote: Option<char> = None;

    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else if c == '\\' && q == '"' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                } else {
                    current.push(c);
                }
            }
            None => match c {
                '\'' | '"' => quote = Some(c),
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                }
                c if c.is_whitespace() => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                }
                _ => current.push(c),
            },
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}
