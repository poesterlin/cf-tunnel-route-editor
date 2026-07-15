use dialoguer::Confirm;

use crate::cloudflare::client::CloudflareClient;
use crate::config::model::TunnelConfig;
use crate::config::mutation::{self, IngressPatch};
use crate::commands::MutateOptions;

pub async fn list_ingress(
    client: &CloudflareClient,
    account_id: &str,
    tunnel_identifier: &str,
    json: bool,
) -> anyhow::Result<()> {
    let tunnel = client.resolve_tunnel(account_id, tunnel_identifier).await?;
    let tunnel_id = tunnel["id"].as_str().unwrap_or(tunnel_identifier);
    let tunnel_name = tunnel["name"].as_str();

    let config_value = client.get_tunnel_config(account_id, tunnel_id).await?;
    let config = TunnelConfig::from_value(config_value);

    if json {
        let rules: Vec<serde_json::Value> = config
            .ingress_rules()
            .iter()
            .map(|r| r.raw.clone())
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "tunnel": tunnel_name,
                "version": config.version(),
                "ingress": rules
            }))?
        );
        return Ok(());
    }

    println!(
        "tunnel: {} (v{})",
        tunnel_name.unwrap_or("<unnamed>"),
        config.version()
    );

    let rules = config.ingress_rules();
    if rules.is_empty() {
        println!("no ingress rules");
        return Ok(());
    }

    for (i, rule) in rules.iter().enumerate() {
        let host = rule.hostname.as_deref().unwrap_or("(catch-all)");
        let svc = &rule.service;
        let or_info = format_origin_request_short(&rule.origin_request);
        println!("  [{i}] {host} -> {svc}{or_info}");
    }

    Ok(())
}

fn format_origin_request_short(
    or: &Option<crate::config::model::OriginRequestView>,
) -> String {
    let or = match or {
        Some(o) => o,
        None => return String::new(),
    };
    let mut parts = Vec::new();
    if let Some(ref name) = or.origin_server_name {
        parts.push(format!("originServerName={name}"));
    }
    if let Some(ref header) = or.http_host_header {
        parts.push(format!("httpHostHeader={header}"));
    }
    if or.no_tls_verify.unwrap_or(false) {
        parts.push("noTLSVerify=true".to_string());
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", parts.join(", "))
    }
}

pub async fn show_ingress(
    client: &CloudflareClient,
    account_id: &str,
    tunnel_identifier: &str,
    hostname: &str,
    json: bool,
) -> anyhow::Result<()> {
    let tunnel = client.resolve_tunnel(account_id, tunnel_identifier).await?;
    let tunnel_id = tunnel["id"].as_str().unwrap_or(tunnel_identifier);

    let config_value = client.get_tunnel_config(account_id, tunnel_id).await?;
    let config = TunnelConfig::from_value(config_value);

    match config.find_ingress(hostname) {
        Some((idx, rule)) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&rule.raw)?);
            } else {
                println!("index: {idx}");
                println!("hostname: {}", rule.hostname.as_deref().unwrap_or("(none)"));
                println!("service: {}", rule.service);
                if let Some(ref or) = rule.origin_request {
                    if let Some(ref name) = or.origin_server_name {
                        println!("originServerName: {name}");
                    }
                    if let Some(val) = or.no_tls_verify {
                        println!("noTLSVerify: {val}");
                    }
                    if let Some(ref header) = or.http_host_header {
                        println!("httpHostHeader: {header}");
                    }
                    if let Some(val) = or.http2_origin {
                        println!("http2Origin: {val}");
                    }
                    if let Some(ref pool) = or.ca_pool {
                        println!("caPool: {pool}");
                    }
                }
            }
        }
        None => {
            anyhow::bail!("no ingress rule found for hostname: {hostname}");
        }
    }

    Ok(())
}

pub async fn set_ingress(
    client: &CloudflareClient,
    account_id: &str,
    tunnel_identifier: &str,
    hostname: &str,
    patch: IngressPatch,
    opts: &MutateOptions,
) -> anyhow::Result<()> {
    if patch.is_empty() {
        anyhow::bail!("no fields specified to patch; nothing to do");
    }

    if patch.no_tls_verify == Some(true) && !opts.allow_insecure_origin {
        anyhow::bail!(
            "setting noTLSVerify requires --allow-insecure-origin flag"
        );
    }

    let tunnel = client.resolve_tunnel(account_id, tunnel_identifier).await?;
    let tunnel_id = tunnel["id"].as_str().unwrap_or(tunnel_identifier);
    let tunnel_name = tunnel["name"].as_str();

    // GET current configuration
    let config_value = client.get_tunnel_config(account_id, tunnel_id).await?;
    let original = TunnelConfig::from_value(config_value.clone());
    let original_version = original.version();
    let original_sha256 = original.sha256();

    // Check version expectation
    if let Some(expected) = opts.expect_version {
        if original_version != expected {
            anyhow::bail!(
                "expected version {expected}, got {original_version}"
            );
        }
    }
    if let Some(ref expected_hash) = opts.expect_sha256 {
        if original_sha256 != *expected_hash {
            anyhow::bail!(
                "expected SHA-256 {expected_hash}, got {original_sha256}"
            );
        }
    }

    // Find the target rule
    let target_index = original
        .find_ingress_index(hostname)
        .ok_or_else(|| anyhow::anyhow!("no ingress rule found for hostname: {hostname}"))?;

    // Check for ambiguous match
    let matches = original
        .ingress_rules()
        .iter()
        .filter(|r| r.hostname.as_deref() == Some(hostname))
        .count();
    if matches > 1 {
        anyhow::bail!(
            "ambiguous match: {matches} ingress rules match hostname {hostname}"
        );
    }

    // Save snapshot
    let snapshot_path = crate::config::snapshot::save_snapshot(
        account_id,
        tunnel_id,
        &original,
        tunnel_name,
        "ingress-set",
    )?;
    eprintln!("snapshot saved: {}", snapshot_path.display());

    // Apply patch in memory
    let mut updated = original.clone();
    mutation::apply_ingress_patch(&mut updated, target_index, &patch);

    // Validate
    let validation = crate::config::validation::validate_config(&updated);
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
    let diff = crate::config::diff::diff_ingress_rules(&original, &updated);
    if diff.trim().is_empty() {
        println!("no changes detected");
        return Ok(());
    }
    println!("{}", diff);

    // Check re-fetch for concurrent changes
    if !opts.dry_run {
        let refetched = client.get_tunnel_config(account_id, tunnel_id).await?;
        let refetched_sha = TunnelConfig::from_value(refetched).sha256();
        if refetched_sha != original_sha256 {
            anyhow::bail!(
                "configuration changed between read and write (SHA-256: {original_sha256} -> {refetched_sha}). Aborting."
            );
        }
    }

    if opts.dry_run {
        println!("[dry-run] would update configuration from version {original_version}");
        return Ok(());
    }

    if !opts.yes {
        let confirmed = Confirm::new()
            .with_prompt("apply these changes?")
            .default(false)
            .interact()?;
        if !confirmed {
            println!("cancelled");
            return Ok(());
        }
    }

    // PUT updated configuration
    let put_result = client
        .put_tunnel_config(account_id, tunnel_id, &updated.raw)
        .await?;

    let new_version = put_result
        .get("result")
        .and_then(|r| r.get("version"))
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);

    println!("configuration updated to version {new_version}");

    // GET verification
    let verified = client.get_tunnel_config(account_id, tunnel_id).await?;
    let verified_config = TunnelConfig::from_value(verified);

    let post_change_validation =
        crate::config::validation::validate_single_rule_change(&original, &verified_config, target_index);
    if !post_change_validation.is_valid() {
        eprintln!("WARNING: verification GET shows unexpected changes:");
        for err in &post_change_validation.errors {
            eprintln!("  error: {err}");
        }
        anyhow::bail!("post-write verification failed - manual inspection recommended");
    }

    println!("verification passed");

    // Save post-write snapshot
    crate::config::snapshot::save_snapshot(
        account_id,
        tunnel_id,
        &verified_config,
        tunnel_name,
        "ingress-set-verified",
    )?;

    Ok(())
}

pub async fn unset_ingress(
    client: &CloudflareClient,
    account_id: &str,
    tunnel_identifier: &str,
    hostname: &str,
    unset_fields: &[String],
    opts: &MutateOptions,
) -> anyhow::Result<()> {
    let mut patch = IngressPatch::default();
    for field in unset_fields {
        match field.as_str() {
            "origin-server-name" | "originServerName" => {
                patch.origin_server_name = Some(None);
            }
            "http-host-header" | "httpHostHeader" => {
                patch.http_host_header = Some(None);
            }
            "no-tls-verify" | "noTLSVerify" => {
                patch.no_tls_verify = Some(false);
            }
            "ca-pool" | "caPool" => {
                patch.ca_pool = Some(None);
            }
            _ => anyhow::bail!("unknown field to unset: {field}"),
        }
    }

    set_ingress(client, account_id, tunnel_identifier, hostname, patch, opts).await
}

pub async fn add_ingress(
    client: &CloudflareClient,
    account_id: &str,
    tunnel_identifier: &str,
    hostname: &str,
    service: &str,
    patch: Option<IngressPatch>,
    opts: &MutateOptions,
) -> anyhow::Result<()> {
    if !opts.allow_create {
        anyhow::bail!("adding ingress rules requires --allow-create");
    }

    let tunnel = client.resolve_tunnel(account_id, tunnel_identifier).await?;
    let tunnel_id = tunnel["id"].as_str().unwrap_or(tunnel_identifier);
    let tunnel_name = tunnel["name"].as_str();

    // GET current configuration
    let config_value = client.get_tunnel_config(account_id, tunnel_id).await?;
    let original = TunnelConfig::from_value(config_value.clone());
    let original_sha256 = original.sha256();

    // Check for existing rule
    if original.find_ingress(hostname).is_some() {
        anyhow::bail!("ingress rule for {hostname} already exists");
    }

    // Save snapshot
    crate::config::snapshot::save_snapshot(
        account_id,
        tunnel_id,
        &original,
        tunnel_name,
        "ingress-add",
    )?;

    // Apply add
    let mut updated = original.clone();
    mutation::add_ingress_rule(&mut updated, hostname, service);

    if let Some(p) = &patch {
        let new_idx = updated
            .find_ingress_index(hostname)
            .expect("just added rule should exist");
        mutation::apply_ingress_patch(&mut updated, new_idx, p);
    }

    // Validate
    let validation = crate::config::validation::validate_config(&updated);
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
    let diff = crate::config::diff::diff_ingress_rules(&original, &updated);
    println!("{}", diff);

    if opts.dry_run {
        println!("[dry-run] would add ingress rule for {hostname}");
        return Ok(());
    }

    // Re-fetch for concurrent changes
    let refetched = client.get_tunnel_config(account_id, tunnel_id).await?;
    let refetched_sha = TunnelConfig::from_value(refetched).sha256();
    if refetched_sha != original_sha256 {
        anyhow::bail!("configuration changed between read and write. Aborting.");
    }

    if !opts.yes {
        let confirmed = Confirm::new()
            .with_prompt("apply these changes?")
            .default(false)
            .interact()?;
        if !confirmed {
            println!("cancelled");
            return Ok(());
        }
    }

    client
        .put_tunnel_config(account_id, tunnel_id, &updated.raw)
        .await?;
    println!("ingress rule added for {hostname}");

    Ok(())
}

pub async fn remove_ingress(
    client: &CloudflareClient,
    account_id: &str,
    tunnel_identifier: &str,
    hostname: &str,
    opts: &MutateOptions,
) -> anyhow::Result<()> {
    if !opts.allow_delete {
        anyhow::bail!("removing ingress rules requires --allow-delete");
    }

    let tunnel = client.resolve_tunnel(account_id, tunnel_identifier).await?;
    let tunnel_id = tunnel["id"].as_str().unwrap_or(tunnel_identifier);
    let tunnel_name = tunnel["name"].as_str();

    // GET current configuration
    let config_value = client.get_tunnel_config(account_id, tunnel_id).await?;
    let original = TunnelConfig::from_value(config_value.clone());
    let original_sha256 = original.sha256();

    // Find target
    let target_index = original
        .find_ingress_index(hostname)
        .ok_or_else(|| anyhow::anyhow!("no ingress rule found for hostname: {hostname}"))?;

    // Save snapshot
    crate::config::snapshot::save_snapshot(
        account_id,
        tunnel_id,
        &original,
        tunnel_name,
        "ingress-remove",
    )?;

    // Apply remove
    let mut updated = original.clone();
    mutation::remove_ingress_rule(&mut updated, target_index);

    // Validate
    let validation = crate::config::validation::validate_config(&updated);
    if !validation.is_valid() {
        eprintln!("validation errors:");
        for err in &validation.errors {
            eprintln!("  error: {err}");
        }
        anyhow::bail!("configuration validation failed");
    }

    // Show diff
    let diff = crate::config::diff::diff_ingress_rules(&original, &updated);
    println!("{}", diff);

    if opts.dry_run {
        println!("[dry-run] would remove ingress rule for {hostname}");
        return Ok(());
    }

    let refetched = client.get_tunnel_config(account_id, tunnel_id).await?;
    let refetched_sha = TunnelConfig::from_value(refetched).sha256();
    if refetched_sha != original_sha256 {
        anyhow::bail!("configuration changed between read and write. Aborting.");
    }

    if !opts.yes {
        let confirmed = Confirm::new()
            .with_prompt("apply these changes?")
            .default(false)
            .interact()?;
        if !confirmed {
            println!("cancelled");
            return Ok(());
        }
    }

    client
        .put_tunnel_config(account_id, tunnel_id, &updated.raw)
        .await?;
    println!("ingress rule removed for {hostname}");

    Ok(())
}
