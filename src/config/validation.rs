use crate::config::model::TunnelConfig;

#[derive(Debug)]
pub struct ValidationResult {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn add_error(&mut self, msg: impl Into<String>) {
        self.errors.push(msg.into());
    }

    pub fn add_warning(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }
}

pub fn validate_config(config: &TunnelConfig) -> ValidationResult {
    let mut result = ValidationResult::new();
    let rules = config.ingress_rules();

    // 1. Must have at least one rule (catch-all)
    if rules.is_empty() {
        result.add_error("configuration has no ingress rules");
        return result;
    }

    // 2. Find catch-all rules
    let catch_all_indices: Vec<usize> = rules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.is_catch_all)
        .map(|(i, _)| i)
        .collect();

    match catch_all_indices.len() {
        0 => result.add_error("no catch-all rule found (must have exactly one)"),
        1 => {
            let idx = catch_all_indices[0];
            if idx != rules.len() - 1 {
                result.add_error(format!(
                    "catch-all rule must be last (found at index {}, should be at {})",
                    idx,
                    rules.len() - 1
                ));
            }
        }
        n => result.add_error(format!(
            "multiple catch-all rules found ({n}), must have exactly one"
        )),
    }

    // 3. Check non-catch-all rules have hostnames
    for (i, rule) in rules.iter().enumerate() {
        if rule.is_catch_all {
            continue;
        }
        if rule.hostname.is_none() || rule.hostname.as_deref() == Some("") {
            result.add_error(format!("ingress rule at index {i} has no hostname"));
        }
    }

    // 4. Check for duplicate hostname/path combinations (warning, not error — pre-existing)
    let mut seen: Vec<(&str, Option<&str>)> = Vec::new();
    for (i, rule) in rules.iter().enumerate() {
        if rule.is_catch_all {
            continue;
        }
        let key = (
            rule.hostname.as_deref().unwrap_or(""),
            rule.path.as_deref(),
        );
        if let Some(prev_idx) = seen.iter().position(|&k| k == key) {
            result.add_warning(format!(
                "duplicate hostname/path combination at indices {prev_idx} and {i}: hostname={}, path={}",
                key.0,
                key.1.unwrap_or("<none>")
            ));
        }
        seen.push(key);
    }

    // 5. Validate service schemes
    for (i, rule) in rules.iter().enumerate() {
        if rule.is_catch_all {
            continue;
        }
        if rule.service == "http_status:404" {
            continue;
        }
        if !rule.service.starts_with("http://")
            && !rule.service.starts_with("https://")
            && !rule.service.starts_with("unix:")
            && !rule.service.starts_with("tcp://")
            && !rule.service.starts_with("ssh://")
            && !rule.service.starts_with("rdp://")
            && !rule.service.starts_with("smb://")
            && !rule.service.starts_with("bastion://")
        {
            result.add_warning(format!(
                "ingress rule at index {i}: service '{}' uses unknown scheme",
                rule.service
            ));
        }
    }

    // 6. HTTPS origins need originServerName, custom CA, or noTLSVerify
    for (i, rule) in rules.iter().enumerate() {
        if rule.is_catch_all {
            continue;
        }
        if rule.service.starts_with("https://") {
            let or = &rule.origin_request;
            let has_origin_server_name = or.as_ref().and_then(|o| o.origin_server_name.as_ref()).is_some();
            let has_no_tls_verify = or.as_ref().and_then(|o| o.no_tls_verify).unwrap_or(false);
            let has_ca_pool = or.as_ref().and_then(|o| o.ca_pool.as_ref()).is_some();

            if !has_origin_server_name && !has_no_tls_verify && !has_ca_pool {
                result.add_warning(format!(
                    "ingress rule at index {i}: HTTPS service '{}' without originServerName, noTLSVerify, or caPool. Certificate validation will fail unless the origin certificate matches the service hostname.",
                    rule.service
                ));
            }
        }
    }

    result
}

/// Post-mutation validation: checks that only the intended rule was changed
pub fn validate_single_rule_change(
    old: &TunnelConfig,
    new: &TunnelConfig,
    changed_index: usize,
) -> ValidationResult {
    let mut result = ValidationResult::new();

    let old_rules = old.ingress_rules();
    let new_rules = new.ingress_rules();

    if old_rules.len() != new_rules.len() {
        result.add_error(format!(
            "rule count changed from {} to {}",
            old_rules.len(),
            new_rules.len()
        ));
        return result;
    }

    for i in 0..old_rules.len() {
        if i == changed_index {
            continue;
        }
        let old_json = serde_json::to_string(&old_rules[i].raw).unwrap_or_default();
        let new_json = serde_json::to_string(&new_rules[i].raw).unwrap_or_default();
        if old_json != new_json {
            result.add_error(format!(
                "unrelated rule at index {i} was modified"
            ));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_config(ingress: serde_json::Value) -> TunnelConfig {
        let raw = json!({
            "tunnel_id": "test-tunnel",
            "config": {
                "ingress": ingress,
                "warp-routing": { "enabled": false }
            },
            "version": 1
        });
        TunnelConfig::from_value(raw)
    }

    #[test]
    fn test_valid_config_with_catch_all() {
        let config = make_config(json!([
            {"hostname": "app.example.com", "service": "https://backend"},
            {"service": "http_status:404"}
        ]));
        let result = validate_config(&config);
        assert!(result.is_valid(), "expected valid config, got errors: {:?}", result.errors);
    }

    #[test]
    fn test_missing_catch_all() {
        let config = make_config(json!([
            {"hostname": "app.example.com", "service": "https://backend"}
        ]));
        let result = validate_config(&config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.contains("catch-all")));
    }

    #[test]
    fn test_multiple_catch_alls() {
        let config = make_config(json!([
            {"hostname": "app.example.com", "service": "https://backend"},
            {"service": "http_status:404"},
            {"service": "http_status:404"}
        ]));
        let result = validate_config(&config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.contains("multiple")));
    }

    #[test]
    fn test_catch_all_not_last() {
        let config = make_config(json!([
            {"service": "http_status:404"},
            {"hostname": "app.example.com", "service": "https://backend"}
        ]));
        let result = validate_config(&config);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.contains("must be last")));
    }

    #[test]
    fn test_duplicate_hostnames() {
        let config = make_config(json!([
            {"hostname": "app.example.com", "service": "https://a"},
            {"hostname": "app.example.com", "service": "https://b"},
            {"service": "http_status:404"}
        ]));
        let result = validate_config(&config);
        assert!(result.is_valid()); // duplicates are a warning, not an error
        assert!(result.warnings.iter().any(|e| e.contains("duplicate")));
    }

    #[test]
    fn test_rule_without_hostname() {
        let config = make_config(json!([
            {"service": "https://backend"},
            {"service": "http_status:404"}
        ]));
        let result = validate_config(&config);
        // A rule without a hostname IS a catch-all, so having two causes "multiple catch-all" error
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.contains("multiple")));
    }

    #[test]
    fn test_https_without_origin_name_warns() {
        let config = make_config(json!([
            {"hostname": "app.example.com", "service": "https://traefik"},
            {"service": "http_status:404"}
        ]));
        let result = validate_config(&config);
        assert!(result.is_valid());
        assert!(result.warnings.iter().any(|w| w.contains("HTTPS")));
    }

    #[test]
    fn test_https_with_origin_server_name_is_clean() {
        let config = make_config(json!([
            {
                "hostname": "app.example.com",
                "service": "https://traefik",
                "originRequest": {
                    "originServerName": "app.example.com"
                }
            },
            {"service": "http_status:404"}
        ]));
        let result = validate_config(&config);
        assert!(result.is_valid());
        assert!(!result.warnings.iter().any(|w| w.contains("HTTPS")));
    }

    #[test]
    fn test_https_with_no_tls_verify_is_clean() {
        let config = make_config(json!([
            {
                "hostname": "app.example.com",
                "service": "https://traefik",
                "originRequest": {
                    "noTLSVerify": true
                }
            },
            {"service": "http_status:404"}
        ]));
        let result = validate_config(&config);
        assert!(result.is_valid());
        assert!(!result.warnings.iter().any(|w| w.contains("HTTPS")));
    }

    #[test]
    fn test_http_service_is_fine() {
        let config = make_config(json!([
            {"hostname": "app.example.com", "service": "http://backend:80"},
            {"service": "http_status:404"}
        ]));
        let result = validate_config(&config);
        assert!(result.is_valid());
        assert!(!result.warnings.iter().any(|w| w.contains("HTTPS")));
    }

    #[test]
    fn test_single_rule_change_validation_passes() {
        let old = make_config(json!([
            {"hostname": "app.example.com", "service": "https://old"},
            {"hostname": "other.example.com", "service": "https://other"},
            {"service": "http_status:404"}
        ]));
        let new = make_config(json!([
            {"hostname": "app.example.com", "service": "https://new"},
            {"hostname": "other.example.com", "service": "https://other"},
            {"service": "http_status:404"}
        ]));
        let result = validate_single_rule_change(&old, &new, 0);
        assert!(result.is_valid());
    }

    #[test]
    fn test_single_rule_change_detects_unrelated_modification() {
        let old = make_config(json!([
            {"hostname": "app.example.com", "service": "https://old"},
            {"hostname": "other.example.com", "service": "https://other"},
            {"service": "http_status:404"}
        ]));
        let new = make_config(json!([
            {"hostname": "app.example.com", "service": "https://old"},
            {"hostname": "other.example.com", "service": "https://changed"},
            {"service": "http_status:404"}
        ]));
        let result = validate_single_rule_change(&old, &new, 0);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.contains("unrelated")));
    }
}
