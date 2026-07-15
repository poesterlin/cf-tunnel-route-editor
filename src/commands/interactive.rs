use anyhow::Context;
use dialoguer::{Input, Select, MultiSelect, Confirm, theme::ColorfulTheme};
use std::io::IsTerminal;

use crate::cloudflare::client::CloudflareClient;
use crate::config::mutation::IngressPatch;

pub async fn prompt_tunnel(
    client: &CloudflareClient,
    account_id: &str,
) -> anyhow::Result<String> {
    let tunnels = client.get_tunnels(account_id).await?;
    if tunnels.is_empty() {
        anyhow::bail!("no tunnels found in account");
    }

    let items: Vec<String> = tunnels
        .iter()
        .map(|t| {
            format!(
                "{:<20}  {}",
                t["name"].as_str().unwrap_or("unnamed"),
                t["id"].as_str().unwrap_or("")
            )
        })
        .collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("select tunnel")
        .items(&items)
        .default(0)
        .interact()?;

    Ok(tunnels[selection]["id"].as_str().unwrap().to_string())
}

pub async fn prompt_ingress_rule(
    client: &CloudflareClient,
    account_id: &str,
    tunnel_id: &str,
    action: &str,
) -> anyhow::Result<String> {
    let config = client.get_tunnel_config(account_id, tunnel_id).await?;
    let rules = config
        .pointer("/config/ingress")
        .and_then(|v| v.as_array())
        .context("no ingress rules")?;

    let items: Vec<String> = rules
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let host = r
                .get("hostname")
                .and_then(|h| h.as_str())
                .unwrap_or("(catch-all)");
            let svc = r.get("service").and_then(|s| s.as_str()).unwrap_or("");
            if host.is_empty() {
                format!("[{i}] (catch-all) -> {svc}")
            } else {
                format!("[{i}] {host} -> {svc}")
            }
        })
        .collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("select rule to {action}"))
        .items(&items)
        .default(0)
        .interact()?;

    Ok(rules[selection]
        .get("hostname")
        .and_then(|h| h.as_str())
        .unwrap_or("")
        .to_string())
}

pub fn prompt_ingress_patch_interactive() -> anyhow::Result<IngressPatch> {
    prompt_ingress_patch_fields(&[])
}

/// Interactive multi-select with optional excluded fields
pub fn prompt_ingress_patch_fields(exclude: &[&str]) -> anyhow::Result<IngressPatch> {
    let all_fields = &[
        "service",
        "originServerName",
        "noTLSVerify",
        "httpHostHeader",
        "http2Origin",
        "caPool",
    ];

    let labels: &[&str] = &[
        "service         — backend URL",
        "originServerName — TLS SNI for origin cert verification",
        "noTLSVerify     — skip origin certificate validation",
        "httpHostHeader  — override the Host header sent to origin",
        "http2Origin     — use HTTP/2 to the origin",
        "caPool          — custom CA certificate for origin",
    ];

    let (display_fields, indices): (Vec<&str>, Vec<usize>) = all_fields
        .iter()
        .enumerate()
        .filter(|(_, f)| !exclude.contains(f))
        .map(|(i, _)| (labels[i], i))
        .unzip();

    if display_fields.is_empty() {
        anyhow::bail!("no editable fields available");
    }

    let defaults = vec![false; display_fields.len()];

    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("select fields to change (space to toggle, enter to confirm)")
        .items(&display_fields)
        .defaults(&defaults)
        .interact()?;

    if selections.is_empty() {
        anyhow::bail!("no fields selected");
    }

    let mut patch = IngressPatch::default();

    for &sel_idx in &selections {
        let original_idx = indices[sel_idx];
        match all_fields[original_idx] {
            "service" => {
                let val: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("backend URL (e.g. https://traefik)")
                    .interact_text()?;
                patch.service = Some(val);
            }
            "originServerName" => {
                let val: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("TLS SNI hostname (must match origin cert, leave empty to clear)")
                    .allow_empty(true)
                    .interact_text()?;
                if val.is_empty() {
                    patch.origin_server_name = Some(None);
                } else {
                    patch.origin_server_name = Some(Some(val));
                }
            }
            "noTLSVerify" => {
                let val = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt("skip origin certificate validation?")
                    .default(false)
                    .interact()?;
                patch.no_tls_verify = Some(val);
            }
            "httpHostHeader" => {
                let val: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("override Host header sent to origin (leave empty to clear)")
                    .allow_empty(true)
                    .interact_text()?;
                if val.is_empty() {
                    patch.http_host_header = Some(None);
                } else {
                    patch.http_host_header = Some(Some(val));
                }
            }
            "http2Origin" => {
                let val = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt("use HTTP/2 to origin?")
                    .default(true)
                    .interact()?;
                patch.http2_origin = Some(val);
            }
            "caPool" => {
                let val: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("path to custom CA certificate (leave empty to clear)")
                    .allow_empty(true)
                    .interact_text()?;
                if val.is_empty() {
                    patch.ca_pool = Some(None);
                } else {
                    patch.ca_pool = Some(Some(val));
                }
            }
            _ => unreachable!(),
        }
    }

    Ok(patch)
}

pub fn prompt_hostname(default: Option<&str>) -> anyhow::Result<String> {
    let theme = ColorfulTheme::default();
    let mut input = Input::<String>::with_theme(&theme);
    input = input.with_prompt("hostname");
    if let Some(d) = default {
        input = input.default(d.to_string());
    }
    Ok(input.interact_text()?)
}

pub fn prompt_service() -> anyhow::Result<String> {
    let theme = ColorfulTheme::default();
    Ok(Input::<String>::with_theme(&theme)
        .with_prompt("service URL")
        .default("https://traefik".into())
        .interact_text()?)
}

pub fn prompt_origin_server_name() -> anyhow::Result<Option<String>> {
    let val: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("originServerName (optional, enter to skip)")
        .allow_empty(true)
        .interact_text()?;
    if val.is_empty() {
        Ok(None)
    } else {
        Ok(Some(val))
    }
}

pub fn is_tty() -> bool {
    std::io::stdin().is_terminal()
}
