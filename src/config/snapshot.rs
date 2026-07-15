use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use anyhow::Context;

use crate::config::model::TunnelConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub tunnel_id: String,
    pub tunnel_name: Option<String>,
    pub config: Value,
    pub cloudflare_version: i64,
    pub retrieved_at: String,
    pub sha256: String,
    pub cli_version: String,
    pub operation: String,
}

impl Snapshot {
    pub fn from_config(
        config: &TunnelConfig,
        tunnel_id: &str,
        tunnel_name: Option<&str>,
        operation: &str,
    ) -> Self {
        let config_json = config
            .raw
            .pointer("/config")
            .cloned()
            .unwrap_or_else(|| config.raw.clone());

        Self {
            tunnel_id: tunnel_id.to_string(),
            tunnel_name: tunnel_name.map(String::from),
            config: config_json,
            cloudflare_version: config.version(),
            retrieved_at: Utc::now().to_rfc3339(),
            sha256: config.sha256(),
            cli_version: env!("CARGO_PKG_VERSION").to_string(),
            operation: operation.to_string(),
        }
    }
}

pub fn snapshot_dir(account_id: &str, tunnel_id: &str) -> PathBuf {
    let base = dirs::state_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cftctl")
        .join(account_id)
        .join(tunnel_id);
    base
}

pub fn save_snapshot(
    account_id: &str,
    tunnel_id: &str,
    config: &TunnelConfig,
    tunnel_name: Option<&str>,
    operation: &str,
) -> anyhow::Result<PathBuf> {
    let snapshot = Snapshot::from_config(config, tunnel_id, tunnel_name, operation);
    let dir = snapshot_dir(account_id, tunnel_id);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create snapshot directory: {:?}", dir))?;

    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%SZ");
    let filename = format!("v{}-{}.json", config.version(), timestamp);
    let path = dir.join(&filename);

    let json_str = serde_json::to_string_pretty(&snapshot)?;
    std::fs::write(&path, json_str)
        .with_context(|| format!("failed to write snapshot: {:?}", path))?;

    Ok(path)
}

pub fn list_snapshots(account_id: &str, tunnel_id: &str) -> anyhow::Result<Vec<SnapshotInfo>> {
    let dir = snapshot_dir(account_id, tunnel_id);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut snapshots = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .with_context(|| format!("failed to read snapshot directory: {:?}", dir))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "json") {
            let content = std::fs::read_to_string(&path)?;
            if let Ok(snapshot) = serde_json::from_str::<Snapshot>(&content) {
                snapshots.push(SnapshotInfo {
                    filename: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    version: snapshot.cloudflare_version,
                    timestamp: snapshot.retrieved_at.clone(),
                    sha256: snapshot.sha256,
                    operation: snapshot.operation,
                });
            }
        }
    }

    snapshots.sort_by(|a, b| b.filename.cmp(&a.filename));
    Ok(snapshots)
}

pub fn load_snapshot(
    account_id: &str,
    tunnel_id: &str,
    filename: &str,
) -> anyhow::Result<Snapshot> {
    let path = snapshot_dir(account_id, tunnel_id).join(filename);
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read snapshot: {:?}", path))?;
    serde_json::from_str(&content).with_context(|| "failed to parse snapshot")
}

#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub filename: String,
    pub version: i64,
    pub timestamp: String,
    pub sha256: String,
    pub operation: String,
}
