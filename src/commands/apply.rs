use crate::cloudflare::client::CloudflareClient;
use crate::config::model::TunnelConfig;
use crate::config::validation;
use crate::config::diff;
use anyhow::Context;
use dialoguer::Confirm;
use std::path::Path;

pub async fn apply_file(
    client: &CloudflareClient,
    account_id: &str,
    file_path: &Path,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(file_path)
        .with_context(|| format!("failed to read file: {}", file_path.display()))?;

    let new_config_value: serde_json::Value =
        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse JSON from {}", file_path.display()))?;

    let tunnel_id = new_config_value
        .get("tunnel_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("JSON file must contain a 'tunnel_id' field"))?;

    // GET current configuration
    let current_value = client.get_tunnel_config(account_id, &tunnel_id).await?;
    let current = TunnelConfig::from_value(current_value.clone());
    let current_sha256 = current.sha256();

    let proposed = TunnelConfig::from_value(new_config_value);

    // Validate
    let validation = validation::validate_config(&proposed);
    if !validation.is_valid() {
        eprintln!("validation errors:");
        for err in &validation.errors {
            eprintln!("  error: {err}");
        }
        anyhow::bail!("configuration validation failed");
    }
    for warn in &validation.warnings {
        eprintln!("  warning: {warn}");
    }

    // Show diff
    let d = diff::diff_configs(&current, &proposed);
    println!("{}", d);

    if dry_run {
        println!("[dry-run] would apply configuration from {}", file_path.display());
        return Ok(());
    }

    // Re-fetch check
    let refetched = client.get_tunnel_config(account_id, &tunnel_id).await?;
    let refetched_sha = TunnelConfig::from_value(refetched).sha256();
    if refetched_sha != current_sha256 {
        anyhow::bail!(
            "configuration changed between read and write. Aborting."
        );
    }

    if !yes {
        let confirmed = Confirm::new()
            .with_prompt("apply these changes?")
            .default(false)
            .interact()?;
        if !confirmed {
            println!("cancelled");
            return Ok(());
        }
    }

    let put_result = client
        .put_tunnel_config(account_id, &tunnel_id, &proposed.raw)
        .await?;

    let new_version = put_result
        .get("result")
        .and_then(|r| r.get("version"))
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);

    println!("configuration updated to version {new_version}");

    Ok(())
}
