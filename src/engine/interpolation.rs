use chrono::Utc;
use rand::Rng;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

use crate::models::{EnvironmentFile, KeyValue, ProjectSecretsStore, UserSettings};

/// Layered variable resolver with strict separation between shared project variables
/// and user-local secrets.
///
/// Precedence (highest to lowest):
/// 1. Built-in dynamic variables (`$timestamp`, `$isoTimestamp`, `$uuid`, `$randomInt`)
/// 2. Active environment user-local secrets (from user directory)
/// 3. Active environment shared variables (from `.doze/environments/<env>.toml`)
/// 4. Global user variables (from user `settings.toml`)
#[derive(Debug, Clone, Default)]
pub struct VariableContext {
    pub global_vars: BTreeMap<String, String>,
    pub env_vars: BTreeMap<String, String>,
    pub env_secrets: BTreeMap<String, String>,
    pub declared_secret_keys: BTreeSet<String>,
}

impl VariableContext {
    pub fn build(
        user_settings: &UserSettings,
        active_env: Option<&EnvironmentFile>,
        secrets_store: Option<&ProjectSecretsStore>,
    ) -> Self {
        let mut ctx = Self {
            global_vars: user_settings.global_variables.clone(),
            env_vars: BTreeMap::new(),
            env_secrets: BTreeMap::new(),
            declared_secret_keys: BTreeSet::new(),
        };

        if let Some(env) = active_env {
            ctx.env_vars = env.variables.clone();
            for decl in &env.secret_keys {
                ctx.declared_secret_keys.insert(decl.key.clone());
            }
            if let Some(store) = secrets_store {
                ctx.env_secrets = store.get_env_secrets(&env.name);
            }
        }
        ctx
    }

    pub fn lookup(&self, key: &str) -> Option<String> {
        match key {
            "$timestamp" => return Some(Utc::now().timestamp().to_string()),
            "$isoTimestamp" => return Some(Utc::now().to_rfc3339()),
            "$uuid" | "$guid" => return Some(Uuid::new_v4().to_string()),
            "$randomInt" => {
                let n: u32 = rand::rng().random_range(0..100_000);
                return Some(n.to_string());
            }
            _ => {}
        }

        if let Some(v) = self.env_secrets.get(key) {
            if !v.is_empty() {
                return Some(v.clone());
            }
        }
        if let Some(v) = self.env_vars.get(key) {
            return Some(v.clone());
        }
        if let Some(v) = self.global_vars.get(key) {
            return Some(v.clone());
        }
        None
    }

    pub fn is_secret_key(&self, key: &str) -> bool {
        self.declared_secret_keys.contains(key) || self.env_secrets.contains_key(key)
    }

    /// Interpolates all `{{var}}` placeholders in `input`.
    /// Unknown variables are left intact as `{{var}}`.
    pub fn interpolate(&self, input: &str) -> String {
        self.interpolate_inner(input, false)
    }

    /// Interpolates `{{var}}` for UI display while masking any secret values as `••••••`.
    pub fn interpolate_masked(&self, input: &str) -> String {
        self.interpolate_inner(input, true)
    }

    fn interpolate_inner(&self, input: &str, mask_secrets: bool) -> String {
        if !input.contains("{{") {
            return input.to_string();
        }
        let mut out = String::with_capacity(input.len());
        let mut rest = input;

        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after_open = &rest[start + 2..];
            if let Some(end) = after_open.find("}}") {
                let key = after_open[..end].trim();
                if let Some(val) = self.lookup(key) {
                    if mask_secrets && self.is_secret_key(key) {
                        out.push_str("••••••");
                    } else {
                        out.push_str(&val);
                    }
                } else {
                    out.push_str("{{");
                    out.push_str(&after_open[..end]);
                    out.push_str("}}");
                }
                rest = &after_open[end + 2..];
            } else {
                out.push_str(&rest[start..]);
                return out;
            }
        }
        out.push_str(rest);
        out
    }

    /// Finds any `{{var}}` keys in `input` that currently have no resolved value.
    pub fn find_unresolved(&self, input: &str) -> Vec<String> {
        let mut missing = Vec::new();
        let mut rest = input;
        while let Some(start) = rest.find("{{") {
            let after = &rest[start + 2..];
            if let Some(end) = after.find("}}") {
                let key = after[..end].trim();
                if !key.is_empty() && self.lookup(key).is_none() && !missing.iter().any(|k| k == key)
                {
                    missing.push(key.to_string());
                }
                rest = &after[end + 2..];
            } else {
                break;
            }
        }
        missing
    }

    /// Resolves URL with both `{{var}}` interpolation and `:path_param` substitution + query params.
    pub fn resolve_url(
        &self,
        raw_url: &str,
        path_params: &[KeyValue],
        query_params: &[KeyValue],
        extra_query: Option<(&str, &str)>,
        mask_secrets: bool,
    ) -> String {
        let mut resolved = if mask_secrets {
            self.interpolate_masked(raw_url)
        } else {
            self.interpolate(raw_url)
        };

        // Substitute `:param` path variables
        for p in path_params.iter().filter(|p| p.enabled && !p.key.trim().is_empty()) {
            let val = if mask_secrets {
                self.interpolate_masked(&p.value)
            } else {
                self.interpolate(&p.value)
            };
            let needle = format!(":{}", p.key.trim());
            resolved = resolved.replace(&needle, &val);
        }

        // Append enabled query parameters
        let mut pairs: Vec<(String, String)> = query_params
            .iter()
            .filter(|q| q.enabled && !q.key.trim().is_empty())
            .map(|q| {
                let k = if mask_secrets {
                    self.interpolate_masked(&q.key)
                } else {
                    self.interpolate(&q.key)
                };
                let v = if mask_secrets {
                    self.interpolate_masked(&q.value)
                } else {
                    self.interpolate(&q.value)
                };
                (k, v)
            })
            .collect();

        if let Some((ek, ev)) = extra_query {
            pairs.push((ek.to_string(), ev.to_string()));
        }

        if pairs.is_empty() {
            return resolved;
        }

        if let Ok(mut parsed) = url::Url::parse(&resolved) {
            {
                let mut q = parsed.query_pairs_mut();
                for (k, v) in pairs {
                    q.append_pair(&k, &v);
                }
            }
            parsed.to_string()
        } else {
            let sep = if resolved.contains('?') { '&' } else { '?' };
            let qs = pairs
                .into_iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            format!("{}{}{}", resolved, sep, qs)
        }
    }
}
