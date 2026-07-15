use secrecy::{ExposeSecret, SecretString};
use tracing::debug;

pub fn resolve_token(provided: Option<String>) -> anyhow::Result<SecretString> {
    if let Some(token) = provided {
        let trimmed = token.trim().to_string();
        debug!(
            "using token from CLI (prefix: {})",
            token_prefix(&trimmed)
        );
                return Ok(SecretString::from(trimmed.as_str()));
    }

    for env_var in &["CFTCTL_TOKEN", "CF_API_TOKEN", "CF_TOKEN"] {
        if let Ok(token) = std::env::var(env_var) {
            let trimmed = token.trim().to_string();
            if !trimmed.is_empty() {
                debug!(
                    "using token from {env_var} (prefix: {prefix})",
                    env_var = env_var,
                    prefix = token_prefix(&trimmed)
                );
        return Ok(SecretString::from(trimmed.as_str()));
            }
        }
    }

    anyhow::bail!("no token provided; set --token, CFTCTL_TOKEN, CF_API_TOKEN, or CF_TOKEN")
}

fn token_prefix(token: &str) -> &str {
    if token.starts_with("cfut_") {
        "cfut_"
    } else if token.starts_with("cfat_") {
        "cfat_"
    } else {
        "legacy"
    }
}

pub fn resolve_account_id(provided: Option<String>) -> anyhow::Result<String> {
    if let Some(id) = provided {
        return Ok(id.trim().to_string());
    }

    for env_var in &["CFTCTL_ACCOUNT", "CF_ACCOUNT_ID"] {
        if let Ok(id) = std::env::var(env_var) {
            let trimmed = id.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(trimmed);
            }
        }
    }

    anyhow::bail!(
        "no account ID provided; set --account, CFTCTL_ACCOUNT, or CF_ACCOUNT_ID"
    )
}

pub fn resolve_zone_id(provided: Option<String>) -> anyhow::Result<String> {
    if let Some(id) = provided {
        return Ok(id.trim().to_string());
    }

    for env_var in &["CFTCTL_ZONE", "CF_ZONE_ID"] {
        if let Ok(id) = std::env::var(env_var) {
            let trimmed = id.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(trimmed);
            }
        }
    }

    anyhow::bail!("no zone ID provided; set --zone, CFTCTL_ZONE, or CF_ZONE_ID")
}

#[allow(dead_code)]
pub fn redact_token(err_str: &str, token: &SecretString) -> String {
    err_str.replace(token.expose_secret(), "<REDACTED>")
}
