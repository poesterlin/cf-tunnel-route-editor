use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Cloudflare Tunnel configuration editor — missing args will prompt interactively
#[derive(Parser)]
#[command(name = "cftctl", version, about, long_about = None)]
pub struct Cli {
    /// Cloudflare API token
    #[arg(
        global = true,
        short = 'T',
        long,
        env = "CFTCTL_TOKEN",
        hide_env_values = true
    )]
    pub token: Option<String>,

    /// Cloudflare account ID
    #[arg(global = true, short = 'A', long, env = "CFTCTL_ACCOUNT")]
    pub account: Option<String>,

    /// Output as JSON
    #[arg(global = true, long)]
    pub json: bool,

    /// Verbose debug output
    #[arg(global = true, short, long)]
    pub verbose: bool,

    /// Skip confirmation prompts
    #[arg(global = true, long)]
    pub yes: bool,

    /// Preview changes without applying
    #[arg(global = true, long)]
    pub dry_run: bool,

    /// Force non-interactive mode (fail on missing args instead of prompting)
    #[arg(global = true, long)]
    pub no_interactive: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Manage Cloudflare tunnels
    Tunnel {
        #[command(subcommand)]
        cmd: TunnelCmd,
    },

    /// Manage tunnel ingress rules
    Ingress {
        #[command(subcommand)]
        cmd: IngressCmd,
    },

    /// Apply a tunnel configuration from a JSON file
    Apply(ApplyArgs),

    /// Manage configuration snapshots
    History {
        #[command(subcommand)]
        cmd: HistoryCmd,
    },

    /// Rollback to a previous snapshot
    Rollback(RollbackArgs),

    /// Manage DNS records for tunnels
    Dns {
        #[command(subcommand)]
        cmd: DnsCmd,
    },

    /// Add ingress rule and create DNS record in one operation
    Publish(PublishArgs),

    /// Generate shell completions
    Completions(CompletionArgs),

    /// Internal: dynamic shell completion handler
    #[command(hide = true)]
    Complete {
        shell: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum TunnelCmd {
    /// List all tunnels
    List,

    /// Manage tunnel configuration
    Config {
        #[command(subcommand)]
        cmd: TunnelConfigCmd,
    },
}

#[derive(Subcommand)]
pub enum TunnelConfigCmd {
    /// Get a tunnel's current configuration (prompts if tunnel omitted)
    Get {
        /// Tunnel name or UUID
        tunnel: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum IngressCmd {
    /// List all ingress rules for a tunnel (prompts if tunnel omitted)
    List {
        /// Tunnel name or UUID
        tunnel: Option<String>,
    },

    /// Show detailed information about a specific ingress rule
    Show {
        /// Tunnel name or UUID
        tunnel: Option<String>,
        /// Hostname of the ingress rule
        hostname: Option<String>,
    },

    /// Update fields of an existing ingress rule (interactive if no patch flags given)
    Set(SetIngressArgs),

    /// Clear/remove specific fields from an ingress rule
    Unset(UnsetIngressArgs),

    /// Add a new ingress rule (interactive if fields omitted)
    Add(AddIngressArgs),

    /// Remove an ingress rule (prompts if tunnel/hostname omitted)
    Remove(RemoveIngressArgs),
}

#[derive(Args)]
pub struct SetIngressArgs {
    /// Tunnel name or UUID
    #[arg(short = 't', long)]
    pub tunnel: Option<String>,
    /// Hostname of the ingress rule to update
    #[arg(short = 'n', long)]
    pub hostname: Option<String>,

    /// New backend service URL
    #[arg(long)]
    pub service: Option<String>,

    /// Set originServerName for TLS verification
    #[arg(long = "origin-server-name")]
    pub origin_server_name: Option<String>,

    /// Don't verify origin TLS certificate
    #[arg(long = "no-tls-verify")]
    pub no_tls_verify: Option<bool>,

    /// Set httpHostHeader for the origin request
    #[arg(long = "http-host-header")]
    pub http_host_header: Option<String>,

    /// Enable http2Origin
    #[arg(long = "http2-origin")]
    pub http2_origin: Option<bool>,

    /// Set CA pool for origin certificate verification
    #[arg(long = "ca-pool")]
    pub ca_pool: Option<String>,

    /// Confirm unsafe origin certificate settings
    #[arg(long)]
    pub allow_insecure_origin: bool,

    /// Expected config version for safety
    #[arg(long)]
    pub expect_version: Option<i64>,

    /// Expected SHA-256 for safety
    #[arg(long)]
    pub expect_sha256: Option<String>,
}

#[derive(Args)]
pub struct UnsetIngressArgs {
    /// Tunnel name or UUID
    #[arg(short = 't', long)]
    pub tunnel: Option<String>,
    /// Hostname of the ingress rule
    #[arg(short = 'n', long)]
    pub hostname: Option<String>,

    /// Remove originServerName
    #[arg(long = "origin-server-name")]
    pub origin_server_name: bool,
    /// Remove httpHostHeader
    #[arg(long = "http-host-header")]
    pub http_host_header: bool,
    /// Remove noTLSVerify
    #[arg(long = "no-tls-verify")]
    pub no_tls_verify: bool,
    /// Remove caPool
    #[arg(long = "ca-pool")]
    pub ca_pool: bool,
    /// Confirm unsafe origin certificate settings
    #[arg(long)]
    pub allow_insecure_origin: bool,
    /// Expected config version for safety
    #[arg(long)]
    pub expect_version: Option<i64>,
    /// Expected SHA-256 for safety
    #[arg(long)]
    pub expect_sha256: Option<String>,
}

#[derive(Args)]
pub struct AddIngressArgs {
    /// Tunnel name or UUID
    #[arg(short = 't', long)]
    pub tunnel: Option<String>,
    /// Hostname for the new rule
    #[arg(short = 'n', long)]
    pub hostname: Option<String>,

    /// Backend service URL
    #[arg(long)]
    pub service: Option<String>,
    /// Set originServerName for TLS verification
    #[arg(long = "origin-server-name")]
    pub origin_server_name: Option<String>,
    /// Don't verify origin TLS certificate
    #[arg(long = "no-tls-verify")]
    pub no_tls_verify: Option<bool>,
    /// Set httpHostHeader for the origin request
    #[arg(long = "http-host-header")]
    pub http_host_header: Option<String>,
    /// Confirm unsafe origin certificate settings
    #[arg(long)]
    pub allow_insecure_origin: bool,
    /// Required: acknowledge creation
    #[arg(long)]
    pub allow_create: bool,
}

#[derive(Args)]
pub struct RemoveIngressArgs {
    /// Tunnel name or UUID
    #[arg(short = 't', long)]
    pub tunnel: Option<String>,
    /// Hostname of the rule to remove
    #[arg(short = 'n', long)]
    pub hostname: Option<String>,

    /// Required: acknowledge deletion
    #[arg(long)]
    pub allow_delete: bool,
}

#[derive(Args)]
pub struct ApplyArgs {
    /// Path to JSON configuration file
    #[arg(long = "file")]
    pub file: PathBuf,
}

#[derive(Subcommand)]
pub enum HistoryCmd {
    /// List available snapshots (prompts if tunnel omitted)
    List {
        /// Tunnel name or UUID
        tunnel: Option<String>,
    },
}

#[derive(Args)]
pub struct RollbackArgs {
    /// Snapshot filename to restore
    pub snapshot: String,

    /// Tunnel name or UUID
    #[arg(short = 't', long)]
    pub tunnel: Option<String>,
}

#[derive(Subcommand)]
pub enum DnsCmd {
    /// Create or verify a CNAME DNS record for a tunnel route
    #[command(name = "ensure-tunnel-route")]
    EnsureTunnelRoute(EnsureTunnelRouteArgs),
}

#[derive(Args)]
pub struct EnsureTunnelRouteArgs {
    /// Tunnel name or UUID
    #[arg(short = 't', long)]
    pub tunnel: Option<String>,
    /// Hostname for the DNS record
    #[arg(short = 'n', long)]
    pub hostname: Option<String>,
    /// Cloudflare zone ID
    #[arg(long)]
    pub zone: Option<String>,
}

#[derive(Args)]
pub struct PublishArgs {
    /// Tunnel name or UUID
    #[arg(short = 't', long)]
    pub tunnel: Option<String>,
    /// Hostname for the new service
    #[arg(short = 'n', long)]
    pub hostname: Option<String>,
    /// Backend service URL
    #[arg(long)]
    pub service: Option<String>,
    /// Set originServerName for TLS verification
    #[arg(long = "origin-server-name")]
    pub origin_server_name: Option<String>,
}

#[derive(Args)]
pub struct CompletionArgs {
    /// Shell type (zsh, bash, fish, elvish, powershell)
    pub shell: String,
}
