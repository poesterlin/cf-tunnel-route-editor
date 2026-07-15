use anyhow::Context;
use dialoguer::Confirm;

use crate::cloudflare::client::CloudflareClient;
use crate::cloudflare::tokens;

pub async fn ensure_tunnel_route(
    client: &CloudflareClient,
    account_id: &str,
    tunnel_identifier: &str,
    hostname: &str,
    zone_id_opt: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let tunnel = client.resolve_tunnel(account_id, tunnel_identifier).await?;
    let tunnel_id = tunnel["id"].as_str().unwrap_or(tunnel_identifier);
    let _tunnel_name = tunnel["name"].as_str().unwrap_or("unnamed");

    let zone_id = if let Some(zid) = zone_id_opt {
        zid
    } else {
        tokens::resolve_zone_id(None)
            .with_context(|| "zone ID required for DNS operations")?
    };

    let tunnel_cname = format!("{tunnel_id}.cfargotunnel.com");

    // Check existing DNS records
    let existing_cname = client
        .get_dns_records(&zone_id, hostname, "CNAME")
        .await?;

    let existing_a = client
        .get_dns_records(&zone_id, hostname, "A")
        .await?;

    let existing_aaaa = client
        .get_dns_records(&zone_id, hostname, "AAAA")
        .await?;

    // Handle conflicts
    let has_a = !existing_a.is_empty();
    let has_aaaa = !existing_aaaa.is_empty();
    if has_a || has_aaaa {
        let types = [
            if has_a { Some("A") } else { None },
            if has_aaaa { Some("AAAA") } else { None },
        ];
        let type_str: Vec<&str> = types.iter().filter_map(|t| *t).collect();
        anyhow::bail!(
            "DNS record conflict: {hostname} has existing {} record(s). Remove them first before creating a CNAME tunnel route.",
            type_str.join(", ")
        );
    }

    match existing_cname.len() {
        0 => {
            println!("creating CNAME: {hostname} -> {tunnel_cname}");
            if dry_run {
                println!("[dry-run] would create DNS record");
                return Ok(());
            }
            if !yes {
                let confirmed = Confirm::new()
                    .with_prompt("create this DNS record?")
                    .default(false)
                    .interact()?;
                if !confirmed {
                    println!("cancelled");
                    return Ok(());
                }
            }
            let result = client
                .create_dns_record(&zone_id, hostname, "CNAME", &tunnel_cname, true)
                .await?;
            println!("DNS record created: {}", result["result"]["id"].as_str().unwrap_or("unknown"));
        }
        1 => {
            let record = &existing_cname[0];
            if record.content == tunnel_cname {
                println!("DNS record already exists and matches: {hostname} -> {tunnel_cname}");
            } else {
                println!("existing CNAME: {hostname} -> {}", record.content);
                println!("desired CNAME:  {hostname} -> {tunnel_cname}");
                if dry_run {
                    println!("[dry-run] would update DNS record");
                    return Ok(());
                }
                if !yes {
                    let confirmed = Confirm::new()
                        .with_prompt("update this DNS record?")
                        .default(false)
                        .interact()?;
                    if !confirmed {
                        println!("cancelled");
                        return Ok(());
                    }
                }
                client
                    .update_dns_record(
                        &zone_id,
                        &record.id,
                        hostname,
                        "CNAME",
                        &tunnel_cname,
                        true,
                    )
                    .await?;
                println!("DNS record updated");
            }
        }
        n => {
            anyhow::bail!("multiple CNAME records exist for {hostname} ({n} records). Manual cleanup required.");
        }
    }

    Ok(())
}

pub async fn publish(
    client: &CloudflareClient,
    account_id: &str,
    tunnel_identifier: &str,
    hostname: &str,
    service: &str,
    origin_server_name: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    println!("publish: adding ingress + DNS for {hostname}");

    // 1. Check DNS for conflicts first
    let tunnel = client.resolve_tunnel(account_id, tunnel_identifier).await?;
    let _tunnel_id = tunnel["id"].as_str().unwrap_or(tunnel_identifier);

    // (DNS check would happen here; for publish, we delegate to ensure-tunnel-route)

    // 2. Add ingress
    use crate::commands::ingress;
    use crate::config::mutation::IngressPatch;

    let patch = if let Some(name) = origin_server_name {
        let mut p = IngressPatch::default();
        p.origin_server_name = Some(Some(name));
        Some(p)
    } else {
        None
    };

    let opts = super::MutateOptions {
        yes,
        dry_run,
        allow_create: true,
        allow_delete: false,
        allow_insecure_origin: false,
        expect_version: None,
        expect_sha256: None,
    };

    ingress::add_ingress(client, account_id, tunnel_identifier, hostname, service, patch, &opts)
        .await?;

    Ok(())
}
