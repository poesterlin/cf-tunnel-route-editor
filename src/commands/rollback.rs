use dialoguer::Confirm;

use crate::cloudflare::client::CloudflareClient;
use crate::config::model::TunnelConfig;
use crate::config::diff;
use crate::config::snapshot;

pub async fn history_list(
    client: &CloudflareClient,
    account_id: &str,
    tunnel_identifier: &str,
) -> anyhow::Result<()> {
    let tunnel = client.resolve_tunnel(account_id, tunnel_identifier).await?;
    let tunnel_id = tunnel["id"].as_str().unwrap_or(tunnel_identifier);

    let snapshots = snapshot::list_snapshots(account_id, tunnel_id)?;

    if snapshots.is_empty() {
        println!("no snapshots found for tunnel {tunnel_id}");
        return Ok(());
    }

    println!("snapshots for tunnel {tunnel_id}:");
    println!(
        "{:<40}  {:<10}  {:<20}  {:<20}  {:<64}",
        "FILENAME", "VERSION", "TIMESTAMP", "OPERATION", "SHA-256"
    );
    println!("{}", "-".repeat(165));
    for s in &snapshots {
        let short_hash = &s.sha256[..s.sha256.len().min(16)];
        println!(
            "{:<40}  v{:<9}  {:<20}  {:<20}  {}...",
            s.filename, s.version, s.timestamp, s.operation, short_hash
        );
    }

    Ok(())
}

pub async fn rollback(
    client: &CloudflareClient,
    account_id: &str,
    tunnel_identifier: &str,
    snapshot_filename: &str,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let tunnel = client.resolve_tunnel(account_id, tunnel_identifier).await?;
    let tunnel_id = tunnel["id"].as_str().unwrap_or(tunnel_identifier);

    let snap = snapshot::load_snapshot(account_id, tunnel_id, snapshot_filename)?;

    // Get current config
    let current_value = client.get_tunnel_config(account_id, tunnel_id).await?;
    let current = TunnelConfig::from_value(current_value.clone());
    let current_sha256 = current.sha256();

    // Build rollback config
    let mut rollback = current.clone();
    rollback.raw["config"] = snap.config.clone();

    // Show diff
    let d = diff::diff_configs(&current, &rollback);
    println!("{}", d);

    // Validate snapshot config
    let validation = crate::config::validation::validate_config(&rollback);
    if !validation.is_valid() {
        eprintln!("validation errors in snapshot:");
        for err in &validation.errors {
            eprintln!("  error: {err}");
        }
        anyhow::bail!("snapshot configuration is invalid");
    }
    for warn in &validation.warnings {
        eprintln!("  warning: {warn}");
    }

    if dry_run {
        println!(
            "[dry-run] would rollback to snapshot {} (v{})",
            snapshot_filename, snap.cloudflare_version
        );
        return Ok(());
    }

    // Re-fetch check
    let refetched = client.get_tunnel_config(account_id, tunnel_id).await?;
    let refetched_sha = TunnelConfig::from_value(refetched).sha256();
    if refetched_sha != current_sha256 {
        anyhow::bail!("configuration changed between read and write. Aborting.");
    }

    if !yes {
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "rollback to snapshot {} (v{})?",
                snapshot_filename, snap.cloudflare_version
            ))
            .default(false)
            .interact()?;
        if !confirmed {
            println!("cancelled");
            return Ok(());
        }
    }

    let put_result = client
        .put_tunnel_config(account_id, tunnel_id, &rollback.raw)
        .await?;

    let new_version = put_result
        .get("result")
        .and_then(|r| r.get("version"))
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);

    println!("rolled back to version {new_version} (from v{})", snap.cloudflare_version);

    Ok(())
}
