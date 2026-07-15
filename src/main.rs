use clap::{Parser, CommandFactory};
use tracing_subscriber::EnvFilter;

use cftctl::cli::{self, Commands, TunnelCmd, TunnelConfigCmd, IngressCmd, HistoryCmd, DnsCmd};
use cftctl::cloudflare::client::CloudflareClient;
use cftctl::cloudflare::tokens;
use cftctl::commands::{self, interactive, MutateOptions};
use cftctl::config::mutation::IngressPatch;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    let cli = cli::Cli::parse();

    let filter = if cli.verbose {
        EnvFilter::new("cftctl=debug")
    } else {
        EnvFilter::new("cftctl=info")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let token = tokens::resolve_token(cli.token)?;
    let account_id = tokens::resolve_account_id(cli.account)?;
    let client = CloudflareClient::new(token)?;

    match cli.command {
        Commands::Tunnel { cmd } => handle_tunnel(&client, &account_id, cmd, cli.json, cli.no_interactive).await,
        Commands::Ingress { cmd } => handle_ingress(&client, &account_id, cmd, cli.json, cli.yes, cli.dry_run, cli.no_interactive).await,
        Commands::Apply(args) => {
            commands::apply::apply_file(&client, &account_id, &args.file, cli.yes, cli.dry_run).await
        }
        Commands::History { cmd } => handle_history(&client, &account_id, cmd, cli.no_interactive).await,
        Commands::Rollback(args) => {
            let tunnel = resolve_required(
                &client, &account_id, args.tunnel, cli.no_interactive, "tunnel",
            ).await?;
            commands::rollback::rollback(
                &client, &account_id, &tunnel, &args.snapshot, cli.yes, cli.dry_run,
            ).await
        }
        Commands::Dns { cmd } => handle_dns(&client, &account_id, cmd, cli.yes, cli.dry_run, cli.no_interactive).await,
        Commands::Publish(args) => {
            let tunnel = resolve_required(&client, &account_id, args.tunnel, cli.no_interactive, "tunnel").await?;
            let hostname = resolve_or_prompt(args.hostname, cli.no_interactive, || {
                interactive::prompt_hostname(None)
            })?;
            let service = resolve_or_prompt(args.service, cli.no_interactive, || {
                interactive::prompt_service()
            })?;
            commands::dns::publish(
                &client, &account_id, &tunnel, &hostname, &service,
                args.origin_server_name, cli.yes, cli.dry_run,
            ).await
        }
        Commands::Completions(args) => {
            let shell = match args.shell.as_str() {
                "zsh" => clap_complete::Shell::Zsh,
                "bash" => clap_complete::Shell::Bash,
                "fish" => clap_complete::Shell::Fish,
                "elvish" => clap_complete::Shell::Elvish,
                "powershell" => clap_complete::Shell::PowerShell,
                _ => anyhow::bail!("unsupported shell: {}. supported: zsh, bash, fish, elvish, powershell", args.shell),
            };
            let mut cmd = cli::Cli::command();
            let name = cmd.get_name().to_string();
            clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
            Ok(())
        }
        Commands::Complete { shell, args } => {
            handle_complete(&client, &account_id, &shell, &args).await
        }
    }
}

// --- Helpers ---

/// Resolve a tunnel ID from an optional arg. Prompts if missing and TTY.
async fn resolve_required(
    client: &CloudflareClient,
    account_id: &str,
    value: Option<String>,
    no_interactive: bool,
    label: &str,
) -> anyhow::Result<String> {
    if let Some(v) = value {
        return Ok(v);
    }
    if !no_interactive && interactive::is_tty() {
        return interactive::prompt_tunnel(client, account_id).await;
    }
    anyhow::bail!(
        "{label} is required (use --{label} <value> or run in a terminal for interactive selection)"
    );
}

/// Resolve a value or prompt. If no_interactive or not TTY, error.
fn resolve_or_prompt<F>(
    value: Option<String>,
    no_interactive: bool,
    prompt_fn: F,
) -> anyhow::Result<String>
where
    F: FnOnce() -> anyhow::Result<String>,
{
    if let Some(v) = value {
        return Ok(v);
    }
    if !no_interactive && interactive::is_tty() {
        return prompt_fn();
    }
    anyhow::bail!("value is required (provide as argument or run in a terminal for interactive input)");
}

// --- Command handlers ---

async fn handle_tunnel(
    client: &CloudflareClient, account_id: &str, cmd: TunnelCmd, json: bool, no_interactive: bool,
) -> anyhow::Result<()> {
    match cmd {
        TunnelCmd::List => commands::tunnel::list_tunnels(client, account_id, json).await,
        TunnelCmd::Config { cmd } => match cmd {
            TunnelConfigCmd::Get { tunnel } => {
                let id = resolve_required(client, account_id, tunnel, no_interactive, "tunnel").await?;
                commands::tunnel::get_config(client, account_id, &id, json).await
            }
        },
    }
}

async fn handle_ingress(
    client: &CloudflareClient, account_id: &str, cmd: IngressCmd,
    json: bool, yes: bool, dry_run: bool, no_interactive: bool,
) -> anyhow::Result<()> {
    match cmd {
        IngressCmd::List { tunnel } => {
            let id = resolve_required(client, account_id, tunnel, no_interactive, "tunnel").await?;
            commands::ingress::list_ingress(client, account_id, &id, json).await
        }
        IngressCmd::Show { tunnel, hostname } => {
            let id = resolve_required(client, account_id, tunnel, no_interactive, "tunnel").await?;
            let hn = resolve_or_prompt(hostname, no_interactive, || {
                tokio::runtime::Handle::current().block_on(async {
                    interactive::prompt_ingress_rule(client, account_id, &id, "show").await
                })
            })?;
            commands::ingress::show_ingress(client, account_id, &id, &hn, json).await
        }
        IngressCmd::Set(args) => {
            let tunnel = resolve_required(client, account_id, args.tunnel, no_interactive, "tunnel").await?;
            let hostname = resolve_or_prompt(args.hostname, no_interactive, || {
                tokio::runtime::Handle::current().block_on(async {
                    interactive::prompt_ingress_rule(client, account_id, &tunnel, "set").await
                })
            })?;

            // Build patch from CLI flags
            let mut patch = IngressPatch::default();
            patch.service = args.service;
            patch.origin_server_name = args.origin_server_name.map(Some);
            patch.no_tls_verify = args.no_tls_verify;
            patch.http_host_header = args.http_host_header.map(Some);
            patch.http2_origin = args.http2_origin;
            patch.ca_pool = args.ca_pool.map(Some);

            // If no patch fields set, go interactive
            if patch.is_empty() && !no_interactive && interactive::is_tty() {
                patch = interactive::prompt_ingress_patch_interactive()?;
            }

            let opts = MutateOptions {
                yes, dry_run,
                allow_create: false, allow_delete: false,
                allow_insecure_origin: args.allow_insecure_origin,
                expect_version: args.expect_version,
                expect_sha256: args.expect_sha256,
            };
            commands::ingress::set_ingress(
                client, account_id, &tunnel, &hostname, patch, &opts,
            ).await
        }
        IngressCmd::Unset(args) => {
            let tunnel = resolve_required(client, account_id, args.tunnel, no_interactive, "tunnel").await?;
            let hostname = resolve_or_prompt(args.hostname, no_interactive, || {
                tokio::runtime::Handle::current().block_on(async {
                    interactive::prompt_ingress_rule(client, account_id, &tunnel, "unset").await
                })
            })?;

            let mut unset_fields = Vec::new();
            if args.origin_server_name { unset_fields.push("origin-server-name".to_string()); }
            if args.http_host_header { unset_fields.push("http-host-header".to_string()); }
            if args.no_tls_verify { unset_fields.push("no-tls-verify".to_string()); }
            if args.ca_pool { unset_fields.push("ca-pool".to_string()); }
            if unset_fields.is_empty() { anyhow::bail!("no fields specified to unset"); }

            let opts = MutateOptions {
                yes, dry_run,
                allow_create: false, allow_delete: false,
                allow_insecure_origin: args.allow_insecure_origin,
                expect_version: args.expect_version,
                expect_sha256: args.expect_sha256,
            };
            commands::ingress::unset_ingress(
                client, account_id, &tunnel, &hostname, &unset_fields, &opts,
            ).await
        }
        IngressCmd::Add(args) => {
            let tunnel = resolve_required(client, account_id, args.tunnel, no_interactive, "tunnel").await?;
            let hostname = resolve_or_prompt(args.hostname, no_interactive, || {
                interactive::prompt_hostname(None)
            })?;
            let service = resolve_or_prompt(args.service, no_interactive, || {
                interactive::prompt_service()
            })?;

            // If --allow-create not set and interactive, prompt
            let allow_create = if args.allow_create {
                true
            } else if !no_interactive && interactive::is_tty() {
                dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("create new ingress rule?")
                    .default(false)
                    .interact()?
            } else {
                false
            };

            let patch = if args.origin_server_name.is_some()
                || args.no_tls_verify.is_some()
                || args.http_host_header.is_some()
            {
                let mut p = IngressPatch::default();
                p.origin_server_name = args.origin_server_name.map(Some);
                p.no_tls_verify = args.no_tls_verify;
                p.http_host_header = args.http_host_header.map(Some);
                Some(p)
            } else {
                if !no_interactive && interactive::is_tty() {
                    eprintln!("\nconfigure origin request settings?");
                    match interactive::prompt_ingress_patch_fields(&["service"]) {
                        Ok(p) if !p.is_empty() => Some(p),
                        _ => None,
                    }
                } else {
                    None
                }
            };

            let opts = MutateOptions {
                yes, dry_run,
                allow_create,
                allow_delete: false,
                allow_insecure_origin: args.allow_insecure_origin,
                expect_version: None, expect_sha256: None,
            };
            commands::ingress::add_ingress(
                client, account_id, &tunnel, &hostname, &service, patch, &opts,
            ).await
        }
        IngressCmd::Remove(args) => {
            let tunnel = resolve_required(client, account_id, args.tunnel, no_interactive, "tunnel").await?;
            let hostname = resolve_or_prompt(args.hostname, no_interactive, || {
                tokio::runtime::Handle::current().block_on(async {
                    interactive::prompt_ingress_rule(client, account_id, &tunnel, "remove").await
                })
            })?;

            let allow_delete = if args.allow_delete {
                true
            } else if !no_interactive && interactive::is_tty() {
                dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("delete this ingress rule?")
                    .default(false)
                    .interact()?
            } else {
                false
            };

            let opts = MutateOptions {
                yes, dry_run,
                allow_create: false, allow_delete,
                allow_insecure_origin: false,
                expect_version: None, expect_sha256: None,
            };
            commands::ingress::remove_ingress(client, account_id, &tunnel, &hostname, &opts).await
        }
    }
}

async fn handle_history(
    client: &CloudflareClient, account_id: &str, cmd: HistoryCmd, no_interactive: bool,
) -> anyhow::Result<()> {
    match cmd {
        HistoryCmd::List { tunnel } => {
            let id = resolve_required(client, account_id, tunnel, no_interactive, "tunnel").await?;
            commands::rollback::history_list(client, account_id, &id).await
        }
    }
}

async fn handle_dns(
    client: &CloudflareClient, account_id: &str, cmd: DnsCmd,
    yes: bool, dry_run: bool, no_interactive: bool,
) -> anyhow::Result<()> {
    match cmd {
        DnsCmd::EnsureTunnelRoute(args) => {
            let tunnel = resolve_required(client, account_id, args.tunnel, no_interactive, "tunnel").await?;
            let hostname = resolve_or_prompt(args.hostname, no_interactive, || {
                interactive::prompt_hostname(None)
            })?;
            commands::dns::ensure_tunnel_route(
                client, account_id, &tunnel, &hostname, args.zone, yes, dry_run,
            ).await
        }
    }
}

async fn handle_complete(
    client: &CloudflareClient,
    account_id: &str,
    _shell: &str,
    args: &[String],
) -> anyhow::Result<()> {
    // cftctl complete zsh tunnel <partial>
    // cftctl complete zsh hostname <tunnel-value> <partial>
    if args.is_empty() {
        return Ok(());
    }

    let mode = &args[0];
    let current = if args.len() >= 2 { args.last().unwrap().as_str() } else { "" };

    match mode.as_str() {
        "tunnel" => {
            complete_tunnel_names(client, account_id, current).await
        }
        "hostname" => {
            if args.len() < 3 {
                return Ok(());
            }
            let tunnel = &args[1];
            complete_hostnames_by_tunnel(client, account_id, tunnel, current).await
        }
        _ => Ok(()),
    }
}

async fn complete_hostnames_by_tunnel(
    client: &CloudflareClient,
    account_id: &str,
    tunnel_identifier: &str,
    current: &str,
) -> anyhow::Result<()> {
    let tunnel = match client.resolve_tunnel(account_id, tunnel_identifier).await {
        Ok(t) => t,
        Err(_) => { eprintln!(""); return Ok(()); }
    };
    let tunnel_id = tunnel["id"].as_str().unwrap_or(tunnel_identifier);

    let config = match client.get_tunnel_config(account_id, tunnel_id).await {
        Ok(c) => c,
        Err(_) => { eprintln!(""); return Ok(()); }
    };

    if let Some(rules) = config.pointer("/config/ingress").and_then(|v| v.as_array()) {
        for rule in rules {
            if let Some(hostname) = rule.get("hostname").and_then(|h| h.as_str()) {
                if !hostname.is_empty() && (current.is_empty() || hostname.starts_with(current)) {
                    println!("{hostname}");
                }
            }
        }
    }
    Ok(())
}

async fn complete_tunnel_names(
    client: &CloudflareClient,
    account_id: &str,
    current: &str,
) -> anyhow::Result<()> {
    let tunnels = match client.get_tunnels(account_id).await {
        Ok(t) => t,
        Err(_) => { eprintln!(""); return Ok(()); }
    };

    for t in &tunnels {
        if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
            if current.is_empty() || name.starts_with(current) {
                println!("{name}");
            }
        }
        if let Some(id) = t.get("id").and_then(|i| i.as_str()) {
            if !current.is_empty() && id.starts_with(current) {
                println!("{id}");
            }
        }
    }
    Ok(())
}

async fn complete_hostnames(
    client: &CloudflareClient,
    account_id: &str,
    tokens: &[&str],
    tunnel_flag_idx: Option<usize>,
    current: &str,
) -> anyhow::Result<()> {
    // Resolve tunnel
    let tunnel_id = if let Some(idx) = tunnel_flag_idx {
        if tokens.len() > idx + 1 {
            let name_or_id = tokens[idx + 1];
            match client.resolve_tunnel(account_id, name_or_id).await {
                Ok(t) => t["id"].as_str().unwrap_or("").to_string(),
                Err(_) => { eprintln!(""); return Ok(()); }
            }
        } else {
            return Ok(());
        }
    } else {
        return Ok(());
    };

    let config = match client.get_tunnel_config(account_id, &tunnel_id).await {
        Ok(c) => c,
        Err(_) => { eprintln!(""); return Ok(()); }
    };

    if let Some(rules) = config.pointer("/config/ingress").and_then(|v| v.as_array()) {
        for rule in rules {
            if let Some(hostname) = rule.get("hostname").and_then(|h| h.as_str()) {
                if !hostname.is_empty() && (current.is_empty() || hostname.starts_with(current)) {
                    println!("{hostname}");
                }
            }
        }
    }
    Ok(())
}
