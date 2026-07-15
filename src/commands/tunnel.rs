use crate::cloudflare::client::CloudflareClient;
use crate::config::model::TunnelConfig;

pub async fn list_tunnels(
    client: &CloudflareClient,
    account_id: &str,
    json: bool,
) -> anyhow::Result<()> {
    let tunnels = client.get_tunnels(account_id).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!(tunnels))?);
        return Ok(());
    }

    if tunnels.is_empty() {
        println!("no tunnels found");
        return Ok(());
    }

    println!("{:<36}  {:<30}  {:<12}  {:<20}", "ID", "NAME", "STATUS", "CREATED");
    println!("{}", "-".repeat(105));
    for t in &tunnels {
        let id = t["id"].as_str().unwrap_or("unknown");
        let name = t["name"].as_str().unwrap_or("unknown");
        let status = t["status"].as_str().unwrap_or("unknown");
        let created = t["created_at"].as_str().unwrap_or("unknown");
        // Truncate to fit
        let created_short = &created[..created.len().min(19)];
        println!("{:<36}  {:<30}  {:<12}  {:<20}", id, name, status, created_short);
    }

    Ok(())
}

pub async fn get_config(
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
        println!("{}", config.to_pretty_string());
        return Ok(());
    }

    println!("tunnel: {} ({})", tunnel_name.unwrap_or("<unnamed>"), tunnel_id);
    println!("version: {}", config.version());
    println!();

    let rules = config.ingress_rules();
    for (i, rule) in rules.iter().enumerate() {
        println!("--- rule {} ---", i);
        println!("  hostname: {}", rule.hostname.as_deref().unwrap_or("(catch-all)"));
        println!("  service:  {}", rule.service);
        if let Some(ref or) = rule.origin_request {
            if let Some(ref name) = or.origin_server_name {
                println!("  originServerName: {}", name);
            }
            if let Some(val) = or.no_tls_verify {
                println!("  noTLSVerify: {}", val);
            }
            if let Some(ref header) = or.http_host_header {
                println!("  httpHostHeader: {}", header);
            }
            if let Some(val) = or.http2_origin {
                println!("  http2Origin: {}", val);
            }
            if let Some(ref pool) = or.ca_pool {
                println!("  caPool: {}", pool);
            }
        }
        println!();
    }

    Ok(())
}
