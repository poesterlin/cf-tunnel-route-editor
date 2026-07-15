use serde_json::Value;

/// Wraps the complete Cloudflare tunnel configuration with raw JSON preservation.
/// Mutations operate directly on the JSON tree, preserving unknown fields.
#[derive(Debug, Clone)]
pub struct TunnelConfig {
    pub raw: Value,
}

impl TunnelConfig {
    pub fn from_value(value: Value) -> Self {
        Self { raw: value }
    }

    pub fn to_pretty_string(&self) -> String {
        serde_json::to_string_pretty(&self.raw).unwrap_or_else(|_| "{}".to_string())
    }

    #[allow(dead_code)]
    pub fn tunnel_id(&self) -> Option<&str> {
        self.raw.get("tunnel_id")?.as_str()
    }

    pub fn version(&self) -> i64 {
        self.raw
            .get("version")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
    }

    pub fn ingress_count(&self) -> usize {
        self.raw
            .pointer("/config/ingress")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0)
    }

    pub fn ingress_rules(&self) -> Vec<IngressRuleView> {
        self.raw
            .pointer("/config/ingress")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(parse_ingress_view)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn find_ingress(&self, hostname: &str) -> Option<(usize, IngressRuleView)> {
        self.ingress_rules()
            .into_iter()
            .enumerate()
            .find(|(_, r)| r.hostname.as_deref() == Some(hostname))
    }

    pub fn find_ingress_index(&self, hostname: &str) -> Option<usize> {
        self.raw
            .pointer("/config/ingress")
            .and_then(|v| v.as_array())?
            .iter()
            .position(|rule| rule.get("hostname").and_then(|h| h.as_str()) == Some(hostname))
    }

    #[allow(dead_code)]
    pub fn is_catch_all_at(&self, index: usize) -> bool {
        self.raw
            .pointer("/config/ingress")
            .and_then(|v| v.get(index))
            .map(|rule| {
                rule.get("hostname").is_none() || rule.get("hostname").and_then(|h| h.as_str()).is_none()
                    || rule.get("hostname").and_then(|h| h.as_str()) == Some("")
            })
            .unwrap_or(false)
    }

    pub fn get_ingress_raw(&self, index: usize) -> Option<&Value> {
        self.raw.pointer("/config/ingress")?.get(index)
    }

    pub fn canonical_json(&self) -> Value {
        let mut v = self.raw.clone();
        // Remove result/version/tunnel_id wrappers for SHA-256
        v.as_object_mut().map(|o| {
            o.remove("version");
            o.remove("tunnel_id");
            o.remove("created_at");
            o.remove("source");
        });
        v
    }

    pub fn sha256(&self) -> String {
        let canonical = serde_json::to_string(&self.canonical_json()).unwrap_or_default();
        use sha2::{Digest, Sha256};
        let result = Sha256::digest(canonical.as_bytes());
        format!("{:x}", result)
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IngressRuleView {
    pub id: Option<String>,
    pub hostname: Option<String>,
    pub service: String,
    pub path: Option<String>,
    pub origin_request: Option<OriginRequestView>,
    pub is_catch_all: bool,
    pub raw: Value,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct OriginRequestView {
    pub origin_server_name: Option<String>,
    pub no_tls_verify: Option<bool>,
    pub http_host_header: Option<String>,
    pub http2_origin: Option<bool>,
    pub ca_pool: Option<String>,
    pub connect_timeout: Option<i64>,
    pub tls_timeout: Option<i64>,
    pub tcp_keep_alive: Option<i64>,
    pub no_happy_eyeballs: Option<bool>,
    pub disable_chunked_encoding: Option<bool>,
    pub proxy_type: Option<String>,
}

fn parse_ingress_view(rule: &Value) -> IngressRuleView {
    let hostname = rule.get("hostname").and_then(|h| h.as_str()).map(String::from);
    let service = rule
        .get("service")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let is_catch_all = hostname.is_none() || hostname.as_deref() == Some("");

    let origin_request = rule.get("originRequest").map(|or| OriginRequestView {
        origin_server_name: or
            .get("originServerName")
            .and_then(|v| v.as_str())
            .map(String::from),
        no_tls_verify: or.get("noTLSVerify").and_then(|v| v.as_bool()),
        http_host_header: or
            .get("httpHostHeader")
            .and_then(|v| v.as_str())
            .map(String::from),
        http2_origin: or.get("http2Origin").and_then(|v| v.as_bool()),
        ca_pool: or.get("caPool").and_then(|v| v.as_str()).map(String::from),
        connect_timeout: or.get("connectTimeout").and_then(|v| v.as_i64()),
        tls_timeout: or.get("tlsTimeout").and_then(|v| v.as_i64()),
        tcp_keep_alive: or.get("tcpKeepAlive").and_then(|v| v.as_i64()),
        no_happy_eyeballs: or.get("noHappyEyeballs").and_then(|v| v.as_bool()),
        disable_chunked_encoding: or.get("disableChunkedEncoding").and_then(|v| v.as_bool()),
        proxy_type: or.get("proxyType").and_then(|v| v.as_str()).map(String::from),
    });

    IngressRuleView {
        id: rule.get("id").and_then(|i| i.as_str()).map(String::from),
        hostname,
        service,
        path: rule.get("path").and_then(|p| p.as_str()).map(String::from),
        origin_request,
        is_catch_all,
        raw: rule.clone(),
    }
}
