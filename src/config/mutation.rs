use serde_json::{json, Value};

use super::model::TunnelConfig;

/// Describes a patch to an existing ingress rule.
/// Only fields that are explicitly set will be changed.
/// Fields set to `None` in a `Some` wrapper will be cleared.
#[derive(Debug, Clone, Default)]
pub struct IngressPatch {
    pub service: Option<String>,
    pub origin_server_name: Option<Option<String>>,
    pub no_tls_verify: Option<bool>,
    pub http_host_header: Option<Option<String>>,
    pub http2_origin: Option<bool>,
    pub ca_pool: Option<Option<String>>,
}

impl IngressPatch {
    pub fn is_empty(&self) -> bool {
        self.service.is_none()
            && self.origin_server_name.is_none()
            && self.no_tls_verify.is_none()
            && self.http_host_header.is_none()
            && self.http2_origin.is_none()
            && self.ca_pool.is_none()
    }
}

pub fn apply_ingress_patch(config: &mut TunnelConfig, index: usize, patch: &IngressPatch) {
    let needs_origin_request = patch.origin_server_name.is_some()
        || patch.no_tls_verify.is_some()
        || patch.http_host_header.is_some()
        || patch.http2_origin.is_some()
        || patch.ca_pool.is_some();

    // First, ensure originRequest exists if needed
    if needs_origin_request {
        let or_path = format!("/config/ingress/{}/originRequest", index);
        if config.raw.pointer(&or_path).is_none() {
            let rule_path = format!("/config/ingress/{}", index);
            if let Some(rule) = config.raw.pointer_mut(&rule_path) {
                if rule.get("originRequest").is_none() {
                    rule["originRequest"] = json!({});
                }
            }
        }
    }

    // Now mutate the rule
    let path = format!("/config/ingress/{}", index);
    let rule = config
        .raw
        .pointer_mut(&path)
        .expect("ingress rule should exist");

    if let Some(ref service) = patch.service {
        rule["service"] = json!(service);
    }

    match &patch.origin_server_name {
        None => {}
        Some(None) => {
            remove_origin_request_field(rule, "originServerName");
        }
        Some(Some(name)) => {
            rule["originRequest"]["originServerName"] = json!(name);
        }
    }

    match &patch.http_host_header {
        None => {}
        Some(None) => {
            remove_origin_request_field(rule, "httpHostHeader");
        }
        Some(Some(header)) => {
            rule["originRequest"]["httpHostHeader"] = json!(header);
        }
    }

    if let Some(val) = patch.no_tls_verify {
        rule["originRequest"]["noTLSVerify"] = json!(val);
    }

    if let Some(val) = patch.http2_origin {
        rule["originRequest"]["http2Origin"] = json!(val);
    }

    match &patch.ca_pool {
        None => {}
        Some(None) => {
            remove_origin_request_field(rule, "caPool");
        }
        Some(Some(pool)) => {
            rule["originRequest"]["caPool"] = json!(pool);
        }
    }
}

fn remove_origin_request_field(rule: &mut Value, field: &str) {
    if let Some(obj) = rule
        .get_mut("originRequest")
        .and_then(|or| or.as_object_mut())
    {
        obj.remove(field);
        if obj.is_empty() {
            rule.as_object_mut().map(|r| r.remove("originRequest"));
        }
    }
}

pub fn add_ingress_rule(config: &mut TunnelConfig, hostname: &str, service: &str) {
    let template = json!({
        "hostname": hostname,
        "service": service,
        "originRequest": {}
    });

    let ingress = config
        .raw
        .pointer_mut("/config/ingress")
        .and_then(|arr| arr.as_array_mut())
        .expect("ingress array should exist");

    // Find next available numeric ID
    let max_id = ingress
        .iter()
        .filter_map(|r| r.get("id").and_then(|i| i.as_str()))
        .filter_map(|s| s.parse::<u32>().ok())
        .max()
        .unwrap_or(0);

    let mut rule = template.clone();
    rule["id"] = json!((max_id + 1).to_string());

    let catch_all_idx = ingress.iter().rposition(|r| {
        r.get("hostname").is_none()
            || r.get("hostname").and_then(|h| h.as_str()) == Some("")
    });

    match catch_all_idx {
        Some(idx) => {
            ingress.insert(idx, rule);
        }
        None => {
            ingress.push(rule);
        }
    }

    // Ensure catch-all is last
    ensure_catch_all_last(config);
}

pub fn remove_ingress_rule(config: &mut TunnelConfig, index: usize) {
    let ingress = config
        .raw
        .pointer_mut("/config/ingress")
        .and_then(|arr| arr.as_array_mut())
        .expect("ingress array should exist");

    if index < ingress.len() {
        ingress.remove(index);
    }
}

pub fn ensure_catch_all_last(config: &mut TunnelConfig) {
    let ingress = config
        .raw
        .pointer_mut("/config/ingress")
        .and_then(|arr| arr.as_array_mut());

    let ingress = match ingress {
        Some(arr) => arr,
        None => return,
    };

    if ingress.is_empty() {
        return;
    }

    let last_idx = ingress.len() - 1;
    let last_is_catch_all = ingress[last_idx]
        .get("hostname")
        .map_or(true, |h| h.as_str().map_or(true, |s| s.is_empty()));

    if last_is_catch_all {
        return;
    }

    // Find and move catch-alls to the end
    let mut catch_all_entries: Vec<serde_json::Value> = Vec::new();
    ingress.retain(|r| {
        let is_ca = r.get("hostname")
            .map_or(true, |h| h.as_str().map_or(true, |s| s.is_empty()));
        if is_ca {
            catch_all_entries.push(r.clone());
            false
        } else {
            true
        }
    });

    ingress.append(&mut catch_all_entries);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_config() -> TunnelConfig {
        let raw = json!({
            "tunnel_id": "test-tunnel",
            "config": {
                "ingress": [
                    {
                        "id": "1",
                        "hostname": "app.example.com",
                        "service": "https://backend:443",
                        "originRequest": {
                            "http2Origin": true
                        }
                    },
                    {
                        "service": "http_status:404"
                    }
                ],
                "warp-routing": {
                    "enabled": false
                }
            },
            "version": 42
        });
        TunnelConfig::from_value(raw)
    }

    fn config_with_unknown_fields() -> TunnelConfig {
        let raw = json!({
            "tunnel_id": "test-tunnel",
            "config": {
                "ingress": [
                    {
                        "id": "1",
                        "hostname": "app.example.com",
                        "service": "https://backend:443",
                        "originRequest": {
                            "http2Origin": true
                        },
                        "futureCloudflareField": "should-survive"
                    },
                    {
                        "service": "http_status:404",
                        "someFutureSetting": 12345
                    }
                ],
                "warp-routing": {
                    "enabled": false
                },
                "futureTopField": {"nested": "value"}
            },
            "version": 42
        });
        TunnelConfig::from_value(raw)
    }

    #[test]
    fn test_patch_preserves_unknown_fields() {
        let mut config = config_with_unknown_fields();
        let patch = IngressPatch {
            service: Some("https://new-backend:8443".to_string()),
            ..Default::default()
        };

        apply_ingress_patch(&mut config, 0, &patch);

        let rule = config.get_ingress_raw(0).unwrap();
        assert_eq!(
            rule.get("futureCloudflareField").and_then(|v| v.as_str()),
            Some("should-survive")
        );
        assert_eq!(
            rule.get("service").and_then(|v| v.as_str()),
            Some("https://new-backend:8443")
        );
    }

    #[test]
    fn test_patch_only_changes_requested_field() {
        let mut config = sample_config();
        let _original = config.to_pretty_string();
        let patch = IngressPatch {
            origin_server_name: Some(Some("app.example.com".to_string())),
            ..Default::default()
        };

        apply_ingress_patch(&mut config, 0, &patch);

        let rule = config.get_ingress_raw(0).unwrap();
        // service should be unchanged
        assert_eq!(
            rule.get("service").and_then(|v| v.as_str()),
            Some("https://backend:443")
        );
        // http2Origin should survive
        assert_eq!(
            rule.pointer("/originRequest/http2Origin").and_then(|v| v.as_bool()),
            Some(true)
        );
        // new field should be set
        assert_eq!(
            rule.pointer("/originRequest/originServerName").and_then(|v| v.as_str()),
            Some("app.example.com")
        );
    }

    #[test]
    fn test_patch_multiple_fields() {
        let mut config = sample_config();
        let patch = IngressPatch {
            service: Some("http://new-svc:8080".to_string()),
            origin_server_name: Some(Some("app.example.com".to_string())),
            http_host_header: Some(Some("app.example.com".to_string())),
            ..Default::default()
        };

        apply_ingress_patch(&mut config, 0, &patch);

        let rule = config.get_ingress_raw(0).unwrap();
        assert_eq!(
            rule.get("service").and_then(|v| v.as_str()),
            Some("http://new-svc:8080")
        );
        assert_eq!(
            rule.pointer("/originRequest/originServerName").and_then(|v| v.as_str()),
            Some("app.example.com")
        );
        assert_eq!(
            rule.pointer("/originRequest/httpHostHeader").and_then(|v| v.as_str()),
            Some("app.example.com")
        );
    }

    #[test]
    fn test_patch_unset_field() {
        let mut config = sample_config();
        // First set originServerName
        let set_patch = IngressPatch {
            origin_server_name: Some(Some("app.example.com".to_string())),
            ..Default::default()
        };
        apply_ingress_patch(&mut config, 0, &set_patch);

        // Verify it was set
        let rule = config.get_ingress_raw(0).unwrap();
        assert!(rule.pointer("/originRequest/originServerName").is_some());

        // Now unset it
        let unset_patch = IngressPatch {
            origin_server_name: Some(None),
            ..Default::default()
        };
        apply_ingress_patch(&mut config, 0, &unset_patch);

        let rule = config.get_ingress_raw(0).unwrap();
        assert!(rule.pointer("/originRequest/originServerName").is_none());
        // http2Origin should survive
        assert_eq!(
            rule.pointer("/originRequest/http2Origin").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn test_catch_all_remains_last_after_add() {
        let mut config = sample_config();
        let original_count = config.ingress_count();

        add_ingress_rule(&mut config, "new.example.com", "https://svc");

        assert_eq!(config.ingress_count(), original_count + 1);

        let rules = config.ingress_rules();
        let last = rules.last().unwrap();
        assert!(last.is_catch_all);
        assert_eq!(last.service, "http_status:404");
    }

    #[test]
    fn test_catch_all_remains_last_after_remove() {
        let mut config = sample_config();
        let rules_before = config.ingress_rules();
        assert_eq!(rules_before.last().unwrap().service, "http_status:404");

        remove_ingress_rule(&mut config, 0);

        let rules_after = config.ingress_rules();
        assert_eq!(rules_after.last().unwrap().service, "http_status:404");
    }

    #[test]
    fn test_add_ingress_before_catch_all() {
        let mut config = sample_config();
        add_ingress_rule(&mut config, "middle.example.com", "https://svc");

        let rules = config.ingress_rules();
        // First rule should be app.example.com
        assert_eq!(rules[0].hostname.as_deref(), Some("app.example.com"));
        // Second should be the new one
        assert_eq!(rules[1].hostname.as_deref(), Some("middle.example.com"));
        // Last should be catch-all
        assert!(rules[2].is_catch_all);
    }

    #[test]
    fn test_patch_does_not_change_other_rules() {
        let mut config = sample_config();
        add_ingress_rule(&mut config, "other.example.com", "https://svc2");

        let original_rule_1 = config.get_ingress_raw(1).unwrap().clone();

        let patch = IngressPatch {
            service: Some("https://changed:8443".to_string()),
            ..Default::default()
        };
        apply_ingress_patch(&mut config, 0, &patch);

        // Rule at index 0 should change
        assert_eq!(
            config.get_ingress_raw(0).unwrap().get("service").and_then(|v| v.as_str()),
            Some("https://changed:8443")
        );

        // Rule at index 1 should be unchanged
        let current_rule_1 = config.get_ingress_raw(1).unwrap();
        assert_eq!(
            current_rule_1.get("hostname").and_then(|v| v.as_str()),
            original_rule_1.get("hostname").and_then(|v| v.as_str())
        );
    }

    #[test]
    fn test_empty_patch_is_noop() {
        let mut config = sample_config();
        let patch = IngressPatch::default();
        assert!(patch.is_empty());

        let before = config.to_pretty_string();
        apply_ingress_patch(&mut config, 0, &patch);
        let after = config.to_pretty_string();

        assert_eq!(before, after);
    }

    #[test]
    fn test_patch_no_tls_verify() {
        let mut config = sample_config();
        let patch = IngressPatch {
            no_tls_verify: Some(true),
            ..Default::default()
        };

        apply_ingress_patch(&mut config, 0, &patch);

        let rule = config.get_ingress_raw(0).unwrap();
        assert_eq!(
            rule.pointer("/originRequest/noTLSVerify").and_then(|v| v.as_bool()),
            Some(true)
        );
        // Existing http2Origin should survive
        assert_eq!(
            rule.pointer("/originRequest/http2Origin").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn test_new_ingress_gets_numeric_id() {
        let mut config = sample_config();
        add_ingress_rule(&mut config, "new.example.com", "https://svc");

        let rules = config.ingress_rules();
        let new_rule = rules.iter().find(|r| r.hostname.as_deref() == Some("new.example.com")).unwrap();
        let id = new_rule.raw.get("id").and_then(|id| id.as_str()).unwrap();
        assert!(id.parse::<u32>().is_ok());
    }

    #[test]
    fn test_ids_larger_than_255_work() {
        let raw = json!({
            "tunnel_id": "test-tunnel",
            "config": {
                "ingress": [
                    {
                        "id": "254",
                        "hostname": "app.example.com",
                        "service": "https://backend"
                    },
                    {
                        "id": "255",
                        "hostname": "other.example.com",
                        "service": "https://other"
                    },
                    {
                        "service": "http_status:404"
                    }
                ],
                "warp-routing": { "enabled": false }
            },
            "version": 42
        });
        let mut config = TunnelConfig::from_value(raw);

        add_ingress_rule(&mut config, "new.example.com", "https://svc");

        let rules = config.ingress_rules();
        let new_rule = rules.iter().find(|r| r.hostname.as_deref() == Some("new.example.com")).unwrap();
        let id: u32 = new_rule.raw.get("id").and_then(|id| id.as_str()).unwrap().parse().unwrap();
        assert_eq!(id, 256);
    }
}
