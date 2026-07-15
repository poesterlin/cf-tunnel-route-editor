use serde_json::Value;

use super::client::CloudflareClient;
use super::errors::CloudflareError;

impl CloudflareClient {
    pub async fn get_tunnels(
        &self,
        account_id: &str,
    ) -> Result<Vec<Value>, CloudflareError> {
        let resp = self
            .get(&format!("/accounts/{}/cfd_tunnel", account_id))
            .await?;

        let tunnels = resp
            .get("result")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(tunnels)
    }

    pub async fn get_tunnel_by_name(
        &self,
        account_id: &str,
        name: &str,
    ) -> Result<Option<Value>, CloudflareError> {
        let tunnels = self.get_tunnels(account_id).await?;
        Ok(tunnels
            .into_iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some(name)))
    }

    pub async fn get_tunnel_by_id(
        &self,
        account_id: &str,
        tunnel_id: &str,
    ) -> Result<Option<Value>, CloudflareError> {
        match self
            .get(&format!(
                "/accounts/{}/cfd_tunnel/{}",
                account_id, tunnel_id
            ))
            .await
        {
            Ok(resp) => Ok(Some(resp.get("result").cloned().unwrap_or(Value::Null))),
            Err(CloudflareError::Api { status, .. }) if status == 404 => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn get_tunnel_config(
        &self,
        account_id: &str,
        tunnel_id: &str,
    ) -> Result<Value, CloudflareError> {
        let resp = self
            .get(&format!(
                "/accounts/{}/cfd_tunnel/{}/configurations",
                account_id, tunnel_id
            ))
            .await?;

        let result = resp
            .get("result")
            .cloned()
            .ok_or_else(|| CloudflareError::NotFound("tunnel configuration".to_string()))?;

        Ok(result)
    }

    pub async fn put_tunnel_config(
        &self,
        account_id: &str,
        tunnel_id: &str,
        config: &Value,
    ) -> Result<Value, CloudflareError> {
        self.put(
            &format!(
                "/accounts/{}/cfd_tunnel/{}/configurations",
                account_id, tunnel_id
            ),
            config,
        )
        .await
    }

    pub async fn resolve_tunnel(
        &self,
        account_id: &str,
        identifier: &str,
    ) -> anyhow::Result<Value> {
        if is_uuid(identifier) {
            self.get_tunnel_by_id(account_id, identifier)
                .await?
                .ok_or_else(|| anyhow::anyhow!("tunnel not found: {}", identifier))
        } else {
            self.get_tunnel_by_name(account_id, identifier)
                .await?
                .ok_or_else(|| anyhow::anyhow!("tunnel not found: {}", identifier))
        }
        .map_err(|e| anyhow::anyhow!("{}", e))
    }
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36 && s.chars().filter(|&c| c == '-').count() == 4
}
