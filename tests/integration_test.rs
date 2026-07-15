use cftctl::config::model::TunnelConfig;
use cftctl::config::mutation::{self, IngressPatch};
use cftctl::config::validation;
use serde_json::json;

#[test]
fn test_realistic_tunnel_parsing() {
    let raw: serde_json::Value = serde_json::from_str(
        include_str!("fixtures/realistic-tunnel.json")
    ).unwrap();
    let config = TunnelConfig::from_value(raw);

    assert_eq!(config.version(), 128);
    assert_eq!(config.ingress_count(), 6);

    let rules = config.ingress_rules();
    assert_eq!(rules.len(), 6);

    assert_eq!(rules[0].hostname.as_deref(), Some("app.example.com"));
    assert_eq!(rules[0].service, "https://backend-internal:443");
    assert_eq!(
        rules[0].origin_request.as_ref().unwrap().origin_server_name.as_deref(),
        Some("app.example.com")
    );

    assert!(rules[5].is_catch_all);
    assert_eq!(rules[5].service, "http_status:404");
}

#[test]
fn test_validate_realistic_tunnel() {
    let raw = serde_json::from_str(
        include_str!("fixtures/realistic-tunnel.json")
    ).unwrap();
    let config = TunnelConfig::from_value(raw);

    let result = validation::validate_config(&config);
    assert!(
        result.is_valid(),
        "realistic tunnel should be valid, got errors: {:?}",
        result.errors
    );
}

#[test]
fn test_validate_realistic_tunnel_https_warnings() {
    let raw = serde_json::from_str(
        include_str!("fixtures/realistic-tunnel.json")
    ).unwrap();
    let config = TunnelConfig::from_value(raw);

    let result = validation::validate_config(&config);
    let https_warnings: Vec<_> = result.warnings.iter()
        .filter(|w| w.contains("HTTPS"))
        .collect();
    assert!(https_warnings.is_empty(), "expected no HTTPS warnings for valid config, got: {:?}", https_warnings);
}

#[test]
fn test_unknown_fields_survive_round_trip() {
    let raw: serde_json::Value = serde_json::from_str(
        include_str!("fixtures/tunnel-with-unknown-fields.json")
    ).unwrap();

    let mut config = TunnelConfig::from_value(raw.clone());

    let patch = IngressPatch {
        service: Some("https://new-backend:9443".to_string()),
        ..Default::default()
    };
    mutation::apply_ingress_patch(&mut config, 0, &patch);

    let rules = config.ingress_rules();
    assert_eq!(rules[0].service, "https://new-backend:9443");

    let raw_after = &config.raw;
    assert_eq!(
        raw_after.get("unknownFutureMetadata").and_then(|v| v.as_str()),
        Some("preserve-me")
    );
    assert_eq!(
        raw_after.pointer("/config/futureTopLevel/something").and_then(|v| v.as_str()),
        Some("important")
    );

    let rule0 = &raw_after["config"]["ingress"][0];
    assert_eq!(rule0["futureField"]["nested"].as_str(), Some("value"));
    assert_eq!(rule0["futureField"]["array"].as_array().unwrap().len(), 3);

    let rule1 = &raw_after["config"]["ingress"][1];
    assert_eq!(rule1["anotherFutureField"].as_str(), Some("should-survive"));

    let catch_all = &raw_after["config"]["ingress"][2];
    assert_eq!(catch_all["futureGlobalSetting"].as_bool(), Some(true));
}

#[test]
fn test_ingress_rules_with_paths() {
    let raw = serde_json::from_str(
        include_str!("fixtures/realistic-tunnel.json")
    ).unwrap();
    let config = TunnelConfig::from_value(raw);

    let rules = config.ingress_rules();
    let api_rule = rules.iter()
        .find(|r| r.hostname.as_deref() == Some("api.example.com"))
        .unwrap();
    assert_eq!(api_rule.path.as_deref(), Some("/v2"));
}

#[test]
fn test_ingress_with_no_origin_request() {
    let raw = serde_json::from_str(
        include_str!("fixtures/realistic-tunnel.json")
    ).unwrap();
    let config = TunnelConfig::from_value(raw);

    let rules = config.ingress_rules();
    let static_rule = rules.iter()
        .find(|r| r.hostname.as_deref() == Some("static.example.com"))
        .unwrap();
    assert!(static_rule.origin_request.is_none());
}

#[test]
fn test_global_origin_request_and_per_rule_coexist() {
    let raw = json!({
        "tunnel_id": "test",
        "config": {
            "ingress": [
                {
                    "id": "1",
                    "hostname": "app.example.com",
                    "service": "https://backend",
                    "originRequest": {
                        "http2Origin": true,
                        "originServerName": "app.example.com"
                    }
                },
                {
                    "service": "http_status:404"
                }
            ],
            "warp-routing": { "enabled": false }
        },
        "version": 1
    });
    let config = TunnelConfig::from_value(raw);

    let rules = config.ingress_rules();
    let app = rules.iter().find(|r| r.hostname.as_deref() == Some("app.example.com")).unwrap();
    let or = app.origin_request.as_ref().unwrap();
    assert_eq!(or.http2_origin, Some(true));
    assert_eq!(or.origin_server_name.as_deref(), Some("app.example.com"));
}

#[test]
fn test_duplicate_hostnames_different_paths() {
    let raw = json!({
        "tunnel_id": "test",
        "config": {
            "ingress": [
                {
                    "hostname": "app.example.com",
                    "service": "https://a",
                    "path": "/api"
                },
                {
                    "hostname": "app.example.com",
                    "service": "https://b",
                    "path": "/web"
                },
                {
                    "service": "http_status:404"
                }
            ],
            "warp-routing": { "enabled": false }
        },
        "version": 1
    });
    let config = TunnelConfig::from_value(raw);
    let result = validation::validate_config(&config);
    assert!(result.is_valid());
}

#[test]
fn test_duplicate_hostnames_same_path() {
    let raw = json!({
        "tunnel_id": "test",
        "config": {
            "ingress": [
                {
                    "hostname": "app.example.com",
                    "service": "https://a",
                    "path": "/api"
                },
                {
                    "hostname": "app.example.com",
                    "service": "https://b",
                    "path": "/api"
                },
                {
                    "service": "http_status:404"
                }
            ],
            "warp-routing": { "enabled": false }
        },
        "version": 1
    });
    let config = TunnelConfig::from_value(raw);
    let result = validation::validate_config(&config);
    assert!(result.is_valid()); // duplicates are warnings
    assert!(result.warnings.iter().any(|e| e.contains("duplicate")));
}

#[test]
fn test_find_ingress_by_hostname() {
    let raw = serde_json::from_str(
        include_str!("fixtures/realistic-tunnel.json")
    ).unwrap();
    let config = TunnelConfig::from_value(raw);

    let (idx, rule) = config.find_ingress("docs.example.com").unwrap();
    assert_eq!(idx, 2);
    assert_eq!(rule.service, "https://traefik");

    assert!(config.find_ingress("nonexistent.example.com").is_none());
}

#[test]
fn test_sha256_stability() {
    let raw: serde_json::Value = serde_json::from_str(
        include_str!("fixtures/realistic-tunnel.json")
    ).unwrap();
    let config1 = TunnelConfig::from_value(raw.clone());
    let config2 = TunnelConfig::from_value(raw);

    let hash1 = config1.sha256();
    let hash2 = config2.sha256();

    assert_eq!(hash1, hash2, "SHA-256 should be stable for the same input");
    assert_eq!(hash1.len(), 64);
}
