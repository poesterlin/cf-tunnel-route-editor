use serde_json::{json, Value};

use super::client::CloudflareClient;
use super::errors::CloudflareError;

#[derive(Debug)]
#[allow(dead_code)]
pub struct DnsRecord {
    pub id: String,
    pub record_type: String,
    pub name: String,
    pub content: String,
    pub proxied: bool,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum DnsEnsureResult {
    AlreadyExists,
    Created,
    Updated { old_content: String, new_content: String },
    Conflict { record_type: String, content: String },
}

impl CloudflareClient {
    pub async fn get_dns_records(
        &self,
        zone_id: &str,
        name: &str,
        record_type: &str,
    ) -> Result<Vec<DnsRecord>, CloudflareError> {
        let resp = self
            .get(&format!(
                "/zones/{}/dns_records?type={}&name={}",
                zone_id, record_type, name
            ))
            .await?;

        let records = resp
            .get("result")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        let parsed = records
            .iter()
            .map(|r| DnsRecord {
                id: r["id"].as_str().unwrap_or("").to_string(),
                record_type: r["type"].as_str().unwrap_or("").to_string(),
                name: r["name"].as_str().unwrap_or("").to_string(),
                content: r["content"].as_str().unwrap_or("").to_string(),
                proxied: r["proxied"].as_bool().unwrap_or(false),
            })
            .collect();

        Ok(parsed)
    }

    pub async fn create_dns_record(
        &self,
        zone_id: &str,
        name: &str,
        record_type: &str,
        content: &str,
        proxied: bool,
    ) -> Result<Value, CloudflareError> {
        let body = json!({
            "type": record_type,
            "name": name,
            "content": content,
            "proxied": proxied,
            "ttl": 1,
        });
        self.post(&format!("/zones/{}/dns_records", zone_id), &body)
            .await
    }

    pub async fn update_dns_record(
        &self,
        zone_id: &str,
        record_id: &str,
        name: &str,
        record_type: &str,
        content: &str,
        proxied: bool,
    ) -> Result<Value, CloudflareError> {
        let body = json!({
            "type": record_type,
            "name": name,
            "content": content,
            "proxied": proxied,
            "ttl": 1,
        });
        self.put(
            &format!("/zones/{}/dns_records/{}", zone_id, record_id),
            &body,
        )
        .await
    }

    #[allow(dead_code)]
    pub async fn delete_dns_record(
        &self,
        zone_id: &str,
        record_id: &str,
    ) -> Result<Value, CloudflareError> {
        self.delete(&format!("/zones/{}/dns_records/{}", zone_id, record_id))
            .await
    }
}
