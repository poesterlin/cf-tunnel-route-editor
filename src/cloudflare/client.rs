use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use tracing::debug;

use super::errors::CloudflareError;

pub(crate) const API_BASE: &str = "https://api.cloudflare.com/client/v4";

#[derive(Clone)]
pub struct CloudflareClient {
    pub(crate) client: reqwest::Client,
    token: SecretString,
}

impl CloudflareClient {
    pub fn new(token: SecretString) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(format!("cftctl/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self { client, token })
    }

    pub(crate) fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let bearer = format!("Bearer {}", self.token.expose_secret());
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&bearer).expect("token should be valid header value"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers
    }

    pub async fn get(&self, path: &str) -> Result<Value, CloudflareError> {
        let url = format!("{}{}", API_BASE, path);
        debug!("GET {}", url);
        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;
        Self::handle_response(resp).await
    }

    pub async fn put(&self, path: &str, body: &Value) -> Result<Value, CloudflareError> {
        let url = format!("{}{}", API_BASE, path);
        debug!("PUT {}", url);
        let resp = self
            .client
            .put(&url)
            .headers(self.auth_headers())
            .json(body)
            .send()
            .await?;
        Self::handle_response(resp).await
    }

    pub async fn post(&self, path: &str, body: &Value) -> Result<Value, CloudflareError> {
        let url = format!("{}{}", API_BASE, path);
        debug!("POST {}", url);
        let resp = self
            .client
            .post(&url)
            .headers(self.auth_headers())
            .json(body)
            .send()
            .await?;
        Self::handle_response(resp).await
    }

    #[allow(dead_code)]
    pub async fn delete(&self, path: &str) -> Result<Value, CloudflareError> {
        let url = format!("{}{}", API_BASE, path);
        debug!("DELETE {}", url);
        let resp = self
            .client
            .delete(&url)
            .headers(self.auth_headers())
            .send()
            .await?;
        Self::handle_response(resp).await
    }

    async fn handle_response(resp: reqwest::Response) -> Result<Value, CloudflareError> {
        let status = resp.status().as_u16();
        let body: Value = resp.json().await?;

        if !(200..300).contains(&status) {
            let errors = body
                .get("errors")
                .and_then(|e| e.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| e.get("message").and_then(|m| m.as_str()))
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_else(|| "unknown error".to_string());

            return Err(CloudflareError::Api {
                status,
                message: errors,
            });
        }

        Ok(body)
    }
}
