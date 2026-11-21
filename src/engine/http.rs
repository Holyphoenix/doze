use base64::Engine as _;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::multipart::{Form, Part};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::engine::VariableContext;
use crate::models::{
    ApiKeyLocation, AuthConfig, CookieInfo, ExecutionResponse, HeaderPair, HttpMethod,
    ProjectConfig, RequestBody, RequestItem, ScriptRunSummary, UserSettings,
};

pub async fn execute_rest(
    req: &RequestItem,
    vars: &VariableContext,
    project: &ProjectConfig,
    user: &UserSettings,
) -> ExecutionResponse {
    let start = Instant::now();

    // Auth query param if API Key in Query
    let auth_query = match &req.auth {
        AuthConfig::ApiKey {
            key,
            value,
            location: ApiKeyLocation::Query,
        } if !key.trim().is_empty() => Some((vars.interpolate(key), vars.interpolate(value))),
        _ => None,
    };

    let resolved_url = vars.resolve_url(
        &req.url,
        &req.path_params,
        &req.query_params,
        auth_query
            .as_ref()
            .map(|(k, v)| (k.as_str(), v.as_str())),
        false,
    );

    if resolved_url.trim().is_empty() {
        return ExecutionResponse::from_error(resolved_url, 0, "Request URL is empty.");
    }

    // Build client with effective settings (Request Override > Project Default > User Settings)
    let timeout_ms = req
        .settings
        .timeout_ms
        .or(project.default_timeout_ms)
        .unwrap_or(user.default_timeout_ms);
    let follow_redirects = req
        .settings
        .follow_redirects
        .or(project.follow_redirects)
        .unwrap_or(user.follow_redirects);
    let verify_tls = req.settings.verify_tls.unwrap_or(user.verify_tls);

    let mut client_builder = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .danger_accept_invalid_certs(!verify_tls)
        .cookie_store(true);

    if follow_redirects {
        client_builder =
            client_builder.redirect(reqwest::redirect::Policy::limited(user.max_redirects));
    } else {
        client_builder = client_builder.redirect(reqwest::redirect::Policy::none());
    }

    if !user.proxy_url.trim().is_empty() {
        match reqwest::Proxy::all(user.proxy_url.trim()) {
            Ok(proxy) => client_builder = client_builder.proxy(proxy),
            Err(e) => {
                return ExecutionResponse::from_error(
                    resolved_url,
                    start.elapsed().as_millis() as u64,
                    format!("Invalid proxy URL '{}': {}", user.proxy_url, e),
                );
            }
        }
    }

    let client = match client_builder.build() {
        Ok(c) => c,
        Err(e) => {
            return ExecutionResponse::from_error(
                resolved_url,
                start.elapsed().as_millis() as u64,
                format!("Failed to build HTTP client: {}", e),
            );
        }
    };

    // Build headers: User-Agent -> Project default headers -> Request headers -> Auth headers
    let mut header_map = HeaderMap::new();
    if !user.user_agent.trim().is_empty() {
        if let Ok(val) = HeaderValue::from_str(user.user_agent.trim()) {
            header_map.insert(USER_AGENT, val);
        }
    }

    for kv in project
        .default_headers
        .iter()
        .chain(req.headers.iter())
        .filter(|h| h.enabled && !h.key.trim().is_empty())
    {
        let k = vars.interpolate(kv.key.trim());
        let v = vars.interpolate(&kv.value);
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(&v),
        ) {
            header_map.insert(name, val);
        }
    }

    // Apply authentication header
    match &req.auth {
        AuthConfig::None => {}
        AuthConfig::Bearer { token } => {
            let t = vars.interpolate(token);
            if !t.is_empty() {
                if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", t)) {
                    header_map.insert(AUTHORIZATION, val);
                }
            }
        }
        AuthConfig::Basic { username, password } => {
            let u = vars.interpolate(username);
            let p = vars.interpolate(password);
            let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", u, p));
            if let Ok(val) = HeaderValue::from_str(&format!("Basic {}", encoded)) {
                header_map.insert(AUTHORIZATION, val);
            }
        }
        AuthConfig::ApiKey {
            key,
            value,
            location: ApiKeyLocation::Header,
        } => {
            let k = vars.interpolate(key.trim());
            let v = vars.interpolate(value);
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(k.as_bytes()),
                HeaderValue::from_str(&v),
            ) {
                header_map.insert(name, val);
            }
        }
        AuthConfig::ApiKey {
            location: ApiKeyLocation::Query,
            ..
        } => {}
    }

    let method = match req.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
        HttpMethod::Patch => reqwest::Method::PATCH,
        HttpMethod::Delete => reqwest::Method::DELETE,
        HttpMethod::Head => reqwest::Method::HEAD,
        HttpMethod::Options => reqwest::Method::OPTIONS,
    };

    let mut builder = client.request(method, &resolved_url).headers(header_map);

    // Attach body
    match &req.body {
        RequestBody::None => {}
        RequestBody::Json { content } => {
            let interpolated = vars.interpolate(content);
            builder = builder
                .header(CONTENT_TYPE, "application/json")
                .body(interpolated);
        }
        RequestBody::Raw {
            content,
            content_type,
        } => {
            let interpolated = vars.interpolate(content);
            let ct = if content_type.trim().is_empty() {
                "text/plain"
            } else {
                content_type.trim()
            };
            builder = builder.header(CONTENT_TYPE, ct).body(interpolated);
        }
        RequestBody::FormUrlEncoded { fields } => {
            let pairs: Vec<(String, String)> = fields
                .iter()
                .filter(|f| f.enabled && !f.key.trim().is_empty())
                .map(|f| (vars.interpolate(&f.key), vars.interpolate(&f.value)))
                .collect();
            builder = builder.form(&pairs);
        }
        RequestBody::Multipart { fields } => {
            let mut form = Form::new();
            for field in fields.iter().filter(|f| f.enabled && !f.key.trim().is_empty()) {
                let key = vars.interpolate(&field.key);
                let val = vars.interpolate(&field.value);
                if field.is_file {
                    match tokio::fs::read(&val).await {
                        Ok(bytes) => {
                            let fname = Path::new(&val)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("file")
                                .to_string();
                            let part = Part::bytes(bytes).file_name(fname);
                            form = form.part(key, part);
                        }
                        Err(e) => {
                            return ExecutionResponse::from_error(
                                resolved_url,
                                start.elapsed().as_millis() as u64,
                                format!("Failed to read multipart file '{}': {}", val, e),
                            );
                        }
                    }
                } else {
                    form = form.text(key, val);
                }
            }
            builder = builder.multipart(form);
        }
        RequestBody::Binary { file_path } => {
            let resolved_path = vars.interpolate(file_path);
            match tokio::fs::read(&resolved_path).await {
                Ok(bytes) => {
                    builder = builder
                        .header(CONTENT_TYPE, "application/octet-stream")
                        .body(bytes);
                }
                Err(e) => {
                    return ExecutionResponse::from_error(
                        resolved_url,
                        start.elapsed().as_millis() as u64,
                        format!("Failed to read binary file '{}': {}", resolved_path, e),
                    );
                }
            }
        }
    }

    match builder.send().await {
        Ok(resp) => {
            let status = resp.status();
            let status_code = status.as_u16();
            let status_text = format!(
                "{} {}",
                status_code,
                status.canonical_reason().unwrap_or("")
            )
            .trim()
            .to_string();

            let headers: Vec<HeaderPair> = resp
                .headers()
                .iter()
                .map(|(k, v)| HeaderPair {
                    name: k.to_string(),
                    value: v.to_str().unwrap_or("<binary>").to_string(),
                })
                .collect();

            let content_type = resp
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("text/plain")
                .to_string();

            let cookies: Vec<CookieInfo> = resp
                .cookies()
                .map(|c| CookieInfo {
                    name: c.name().to_string(),
                    value: c.value().to_string(),
                    domain: c.domain().map(|s| s.to_string()),
                    path: c.path().map(|s| s.to_string()),
                })
                .collect();

            let bytes = match resp.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    return ExecutionResponse::from_error(
                        resolved_url,
                        start.elapsed().as_millis() as u64,
                        format!("Failed reading response body: {}", e),
                    );
                }
            };

            let duration_ms = start.elapsed().as_millis() as u64;
            let size_bytes = bytes.len();
            let body = match String::from_utf8(bytes.to_vec()) {
                Ok(text) => text,
                Err(_) => format!(
                    "<Binary payload: {} bytes>\n\nBase64 preview:\n{}",
                    size_bytes,
                    base64::engine::general_purpose::STANDARD
                        .encode(&bytes[..bytes.len().min(4096)])
                ),
            };

            ExecutionResponse {
                status_code,
                status_text,
                is_success: status.is_success(),
                duration_ms,
                size_bytes,
                content_type,
                body,
                headers,
                trailers: Vec::new(),
                cookies,
                resolved_url,
                error: None,
                protocol_note: None,
                scripts: ScriptRunSummary::default(),
            }
        }
        Err(e) => ExecutionResponse::from_error(
            resolved_url,
            start.elapsed().as_millis() as u64,
            format!("HTTP request failed: {}", e),
        ),
    }
}
