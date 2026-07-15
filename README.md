# cftctl — Cloudflare Tunnel Route Editor

A CLI tool for safely managing Cloudflare Tunnel ingress routes. Edit tunnel configurations interactively or via flags, with transactional safety, snapshots, and live shell completions.

## Features

- **Interactive mode** — Missing arguments prompt for input with fuzzy-friendly selectors (tunnel list, hostname picker, multi-select field editor)
- **Transactional writes** — Every mutation follows: snapshot → validate → diff → re-fetch check → PUT → verify. Nothing is written unless the full chain succeeds.
- **Snapshot history** — Automatic backups stored in `~/.local/state/cftctl/<account>/<tunnel>/` with SHA-256 and timestamp, enabling rollback
- **Diff preview** — See exactly what changes before confirming, using `similar` crate for readable diffs
- **Unknown field preservation** — Round-trips raw Cloudflare API JSON, only modifying the fields you touch. New Cloudflare fields won't be lost.
- **Live shell completions** — Tab-completes tunnel names and hostnames live from the Cloudflare API (zsh)
- **Safety flags** — `--dry-run` to preview, `--yes` to skip prompts for automation, `--expect-version` / `--expect-sha256` for concurrency guard
- **Secret handling** — API tokens never appear in logs (`secrecy::SecretString`)

## Installation

```bash
cargo install --path .
```

Or build and copy manually:

```bash
cargo build --release
cp target/release/cftctl ~/.local/bin/
```

## Configuration

Create a `.env` file or export environment variables:

```bash
CFTCTL_TOKEN=your-cloudflare-api-token
CFTCTL_ACCOUNT=your-account-id
CFTCTL_ZONE=your-zone-id   # for DNS commands
```

The token is resolved from (in order): `--token` flag, `CFTCTL_TOKEN`, `CF_API_TOKEN`, `CF_TOKEN`.

### Required Cloudflare API token permissions

- `Cloudflare Tunnel: Edit`
- `DNS: Edit` (for `dns` and `publish` commands)
- `Account: Read`

## Usage

### List tunnels

```bash
cftctl tunnel list
```

### List ingress rules

```bash
cftctl ingress list -t cluster
```

### Show a specific rule

```bash
cftctl ingress show -t cluster -n docs.example.com
```

### Update a rule (interactive)

```bash
cftctl ingress set -t cluster -n docs.example.com
```

Pick fields from a multi-select menu, then set values interactively.

### Update a rule (CLI flags)

```bash
cftctl ingress set -t cluster -n docs.example.com --no-tls-verify true --origin-server-name docs.example.com
```

### Clear fields from a rule

```bash
cftctl ingress unset -t cluster -n docs.example.com --no-tls-verify --http-host-header
```

### Add a new ingress rule

```bash
cftctl ingress add -t cluster -n app.example.com --service https://traefik --allow-create
```

### Remove an ingress rule

```bash
cftctl ingress remove -t cluster -n app.example.com --allow-delete
```

### Add ingress rule + DNS CNAME in one command

```bash
cftctl publish -t cluster -n app.example.com --service https://traefik
```

Creates the ingress rule and ensures a `CNAME` record pointing to `<tunnel-id>.cfargotunnel.com` exists.

### Apply a full config from JSON

```bash
cftctl apply --file config.json -t cluster
```

### View snapshot history

```bash
cftctl history list -t cluster
```

### Rollback to a previous snapshot

```bash
cftctl rollback <snapshot-filename> -t cluster
```

### Automation flags

```bash
# Preview only, no changes
cftctl ingress set -t cluster -n docs.example.com --no-tls-verify true --dry-run

# Skip confirmation prompt
cftctl ingress set -t cluster -n docs.example.com --no-tls-verify true --yes

# Concurrency guard — fail if server version doesn't match
cftctl ingress set -t cluster -n docs.example.com --no-tls-verify true --expect-version 355

# Force non-interactive (error on missing args)
cftctl ingress list -t cluster --no-interactive
```

## Architecture

```
src/
├── main.rs              # Entry point, command dispatch, interactive fallback
├── cli.rs               # clap CLI definitions
├── lib.rs               # Re-exports
├── cloudflare/
│   ├── client.rs        # HTTP client (GET/PUT/POST/DELETE), token redaction
│   ├── tokens.rs        # Token resolution from env/flags
│   ├── tunnels.rs       # Tunnel listing, config GET/PUT, name resolution
│   ├── dns.rs           # DNS record CRUD
│   └── errors.rs        # API error types
├── config/
│   ├── model.rs         # TunnelConfig wrapping serde_json::Value, typed views
│   ├── mutation.rs      # IngressPatch, add/remove rule, catch-all ordering
│   ├── validation.rs    # Validates config (catch-all, duplicates, HTTPS safety)
│   ├── diff.rs          # Diff generation via `similar`
│   └── snapshot.rs      # Snapshot save/load/list with SHA-256
├── commands/
│   ├── tunnel.rs        # tunnel list / config get
│   ├── ingress.rs       # ingress list/show/set/unset/add/remove
│   ├── apply.rs         # Apply config from JSON file
│   ├── rollback.rs      # Restore from snapshot
│   ├── dns.rs           # DNS ensure-tunnel-route
│   ├── interactive.rs   # dialoguer-based prompts
│   └── mod.rs
└── tests/
    ├── fixtures/        # Test JSON configs
    └── integration_test.rs
```

### Design: Hybrid JSON Model

`TunnelConfig` stores the full Cloudflare API response as a raw `serde_json::Value` and provides typed accessors (`IngressRuleView`, `OriginRequestView`) for display. Mutations are applied directly to the JSON tree, so any fields Cloudflare adds that `cftctl` doesn't know about are preserved across round-trips.

### Design: Transactional Writes

Every mutating command follows:

1. **GET** current config → save snapshot
2. Apply patch to local copy
3. **Validate** the result (catch-all position, duplicates, HTTPS safety)
4. **Diff** against original for user review
5. **Re-fetch** config to check it hasn't changed since step 1
6. **PUT** the merged config
7. **GET** to verify the write succeeded

## Testing

```bash
cargo test
```

35 tests (24 unit + 11 integration) covering validation, mutation, diff, snapshot, and end-to-end config round-trips.

## Tech Stack

| Crate | Purpose |
|---|---|
| `clap` | CLI argument parsing |
| `reqwest` | HTTP client (rustls) |
| `tokio` | Async runtime |
| `serde_json` | Hybrid JSON model |
| `dialoguer` | Interactive prompts |
| `similar` | Config diffs |
| `secrecy` | Token redaction in logs |
| `json-patch` | Patch operations |
| `sha2` | Snapshot integrity |
| `chrono` | Snapshot timestamps |
| `dotenvy` | `.env` file loading |

## License

MIT