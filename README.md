# QuicView

Enterprise-grade remote access with direct client-server architecture over QUIC (primary) and TCP/TLS (fallback), implemented as first-party, Apache-2.0-only code.

CI status:

[![QuicView CI](https://github.com/OWNER/REPO/actions/workflows/quicview-ci.yml/badge.svg)](https://github.com/OWNER/REPO/actions/workflows/quicview-ci.yml)
[![QuicView Advanced Checks](https://github.com/OWNER/REPO/actions/workflows/quicview-advanced.yml/badge.svg)](https://github.com/OWNER/REPO/actions/workflows/quicview-advanced.yml)

Replace `OWNER/REPO` with your GitHub org/repo path after pushing this workspace to GitHub.

See also: docs/ROADMAP.md for the current project roadmap and milestones.
Current iteration: docs/ITERATION-2025-08-20.md

## Crate layout

- `config`: YAML config schema and loader.
- `bridge`: Flutter-free contracts to integrate client/server implementations and shared errors.
- `server`: Flutter-free server harness (no upstream exec; first-party only).
- `client`: client core (headless agent + UI-agnostic API) plus an optional HTTP control surface and MJPEG stream. UI plan: web-first with Leptos, then a native wrapper using Tauri.
- (removed) `upstream_compat`: we no longer vendor AGPL code; keep this repo Apache-2.0-only.
- `cli`: a small binary that loads `quicview.yaml` and starts client/server via the bridge and features.

## Submodules

We reference `libs/hbb_common` as a submodule for legacy compatibility during the migration period. P2P code paths are deprecated and will be removed.

- `libs/hbb_common` → `https://github.com/rustdesk/hbb_common` (⚠️ DEPRECATED)

Submodule maintenance (zsh):

```bash
# Initialize after fresh clone
git submodule update --init --recursive

# Pull latest upstream for all submodules
git submodule update --remote --recursive

# Pin to a specific commit (example)
cd libs/hbb_common && git checkout <commit> && cd -
git add libs/hbb_common
git commit -m "chore(submodule): pin hbb_common to <commit>"
```

## Build

- Validate config:
	- `cargo run -p cli -- validate -c examples/quicview.yaml`
- Start server (feature-gated implementation, Flutter-free):
	- `cargo run -p cli --features server -- launch-server -c examples/quicview.yaml`
	- Note: we do not exec or vendor upstream binaries.
- Start client HTTP UI (no config file required):
	- Build Leptos web app once (optional, if serving static UI from the client):
		- `cd quicview/apps/leptos-web && trunk build --release` (needs trunk for web builds)
	- Run the client with HTTP UI and macOS features:
		- `cd quicview`
		- `cargo run -p cli --features http-ui,macos-capture,macos-input,clipboard -- client --start --port 21180 --auth-token your-token --static-dir ./apps/leptos-web/dist`
	- Then open `http://127.0.0.1:21180/` in your browser and enter your token if not provided via URL hash or query.
	- Notes:
		- Clipboard endpoints require the `clipboard` feature.
		- On macOS, grant Accessibility permissions for input injection and Screen Recording for capture.
		- You can omit `--static-dir` to use the built-in minimal HTML for start/stop/status.

Notes:
- We are not a wrapper/launcher: the design is to implement first-party components and interoperate at protocol boundaries.
- The `client` crate has an optional feature `flutter` for GUI integration. The `server` and `bridge` crates are Flutter-free by design.

- Clear separation of client vs. server responsibilities
- Config-driven operation with safe, auditable defaults
- Concurrent multi-viewer sessions for collaborative support
Build the workspace and try the CLI with the example config:

```bash
cd quicview
cargo build --workspace
cargo run -p cli -- --config examples/quicview.yaml validate
cargo run -p cli -- --config examples/quicview.yaml show
```

Makefile shortcuts (macOS/zsh):

```zsh
# Build everything
make build

# Build web UI (requires trunk) and run client serving static assets
make web
make client QUICVIEW_TOKEN=dev-token

# Run Tauri shell
make tauri

# QUIC control server/client (dev)
make ctrl-server QUICVIEW_CTRL_TOKEN=dev-token
make ctrl-client QUICVIEW_CTRL_ADDR=127.0.0.1:4433 QUICVIEW_CTRL_TOKEN=dev-token

# Compute certificate pins (DER SHA-256 hex)
# From PEM
make pin-from-pem PEM=server.pem
# From live server (PORT defaults to 4433; SNI defaults to HOST)
make pin-from-live HOST=ctrl.example.com PORT=4433 SNI=ctrl.example.com
```

Today: the server is a first-party, clean-room implementation; client is a stub until we wire GUI/daemon integration. Config validation and scripting are ready.

### Native Tauri client (optional)

You can run the native shell that hosts the in-process HTTP UI:

1) Build the web UI (optional but recommended for full UI):

```zsh
cd quicview/apps/leptos-web
```

2) Run the Tauri app (macOS):
```zsh
cd quicview
cargo build -p tauri-client
open target/debug/tauri-client.app
```

### QUIC control channel (dev/testing)

You can spin up a simple QUIC control server for local testing (requires building with the `quic-ctrl` feature on the CLI and `quic` on the transport crate):

```zsh
# Terminal 1: start control server (listens on 127.0.0.1:4433 by default)
cargo run -p cli --features quic-ctrl -- ctrl-server --token my-secret

# Interactive commands (type in the server terminal):
#  start | stop | reauth_request | reauth <new-token> | quit

# Terminal 2: run the client with QUIC control enabled
cargo run -p cli --features http-ui,quic-ctrl -- \
	client --ctrl-addr 127.0.0.1:4433 --ctrl-token my-secret --open

# Then open the printed UI URL and check /status or /ctrl/config for ctrl metrics.
```

HTTP UI endpoints of interest:
- `GET /status` → overall status including `ctrl` snapshot and rate metrics.
- `GET /ctrl/config` → just the `ctrl` snapshot, including TLS trust info when using QUIC/TLS.

Notes:
- The window points to `http://127.0.0.1:<port>/`. A fixed port `21180` is attempted first; if busy, a random free port is used.

QUIC/TLS modes for the control channel (client):

- `--ctrl-tls insecure` (default): no certificate validation. For local dev only.
- `--ctrl-tls system` with `--ctrl-sni <name>`: use system roots (plus optional `--ctrl-ca-file <pem>`). Good for public CAs or private CA PEMs.
- `--ctrl-tls pin:<DER_SHA256_HEX>` with `--ctrl-sni <name>`: pin the server certificate DER SHA-256 digest (hex). Strongest when you can distribute pins.
- `--ctrl-tls tofu` with `--ctrl-sni <name>`: Trust On First Use. Provide `--ctrl-tofu-pin-file <path>` to persist the learned pin; subsequent runs will verify it.

Examples:

```zsh
# System roots + SNI (optionally add a custom CA file)
cargo run -p cli --features http-ui,quic-ctrl -- \
	client --ctrl-addr 127.0.0.1:4433 --ctrl-token my-secret \
	--ctrl-tls system --ctrl-sni localhost \
	--ctrl-ca-file ./my-private-ca.pem --open

# Certificate pinning (DER SHA-256 hex)
cargo run -p cli --features http-ui,quic-ctrl -- \
	client --ctrl-addr 10.0.0.5:4433 --ctrl-token prod-token \
	--ctrl-tls pin:2f9a...deadbeef --ctrl-sni ctrl.example.com

# TOFU (Trust On First Use) with a persistent pin cache
cargo run -p cli --features http-ui,quic-ctrl -- \
	client --ctrl-addr ctrl.example.com:4433 --ctrl-token token \
	--ctrl-tls tofu --ctrl-sni ctrl.example.com \
	--ctrl-tofu-pin-file ~/.config/quicview/ctrl.pin
```

UI status: `GET /ctrl/config` now includes a `tls` object when TLS is in use, with fields: `mode`, `sni`, `pin_sha256_hex` (if pin/tofu), and `ca_pem_len` (bytes length if provided).

Compute a certificate pin (DER SHA-256 hex):

- If you already have the server certificate in PEM form (leaf cert):

```zsh
# Outputs lowercase hex of SHA-256 over the DER-encoded leaf certificate
openssl x509 -in server.pem -outform DER | shasum -a 256 | awk '{print $1}'
```

- If you want to fetch the certificate live from a running server (replace host/SNI):

```zsh
# Note: -servername must match the SNI you pass with --ctrl-sni
openssl s_client -connect ctrl.example.com:4433 -servername ctrl.example.com -showcerts < /dev/null \
	2>/dev/null | openssl x509 -outform DER | shasum -a 256 | awk '{print $1}'
```

Use the resulting hex in `--ctrl-tls pin:<HEX>` and set `--ctrl-sni` appropriately.

TOFU pin cache file:

- Provide `--ctrl-tofu-pin-file <path>` to persist the learned pin on first connect.
- The file contains a single lowercase hex string (the DER SHA-256 pin). Example:

```zsh
# After first TOFU run
cat ~/.config/quicview/ctrl.pin
# => e3f1c8...b77a
```

Verifying TLS status via the HTTP UI:

```zsh
# If GET endpoints require auth, add: -H "Authorization: Bearer $QUICVIEW_TOKEN"
curl -s http://127.0.0.1:21180/ctrl/config | jq .tls
```

Custom CA usage:

- When using `--ctrl-tls system`, you can append a private CA with `--ctrl-ca-file <pem>`.
- The CA file may contain one or more PEM-encoded CA certificates. These are added to the platform root store used by rustls for this client.
- Ensure the SNI you pass (`--ctrl-sni`) matches the certificate’s subject/SANs; otherwise verification will fail.

Admin rollout checklist:

- Decide on TLS mode: public CA (system), private CA, pinning, or TOFU.
- If pinning: compute and distribute pins per environment; record rotation runbooks.
- If private CA: distribute the CA PEM and set `--ctrl-ca-file` consistently.
- Set `--ctrl-sni` to the canonical control hostname; keep DNS and certs aligned.
- Configure consent defaults and rate limiting; validate `/status` and `/ctrl/config`.
- On macOS, pre-grant permissions (Accessibility, Screen Recording) via MDM where possible.

Run at Login (macOS):
- The system tray includes a "Run at Login" toggle. On first click, it creates a LaunchAgent plist at `~/Library/LaunchAgents/<bundle>.quicview.client.plist` pointing to the current executable. Clicking again removes it.
- No admin rights are required; this is per-user.
- If the app was not granted screen/input permissions previously, macOS will prompt as needed:
	- System Settings → Privacy & Security → Screen Recording: enable for the app (for screen capture)
	- System Settings → Privacy & Security → Accessibility: enable for the app (for input control)
	- After toggling permissions, quit and relaunch the app to take effect.

- Client/server scope separation and policy boundaries
- YAML-based configuration (`examples/quicview.yaml`)
- CLI for validating and exporting configuration for automation
- Building blocks for policy enforcement (consent, input control, clipboard, file transfer)

## Project Structure

```
quicview/
	Cargo.toml
	crates/
		config/            # Config models + YAML loader
		bridge/            # Flutter-free interfaces for client/server implementations
		server/            # Server implementation (health/ready; no upstream exec)
		client/            # Client implementation (optional Flutter integration)
		# upstream_compat/   # (removed) AGPL vendoring was dropped
		cli/               # CLI for validation and starting client/server
	examples/
		quicview.yaml      # Example configuration
```

## Architecture Overview
	- Apply policy (e.g., view-only, control, clipboard, file transfer)

- Server responsibilities
This separation supports independent deployment, scaling, and auditing of server-side components.

## Concurrent Multi-User
Enterprise patterns:
- Use role-based permissions during collaborative support (primary controller + observers)
- For isolated per-user desktops, pair with OS multi-session/VDI and connect to distinct host sessions/VMs
Example (`examples/quicview.yaml`):

```yaml
	tls: true

client_policy:
	allow_clipboard: false
```

Recommended defaults: require consent; grant control and data exchange only when needed.

## Deployment Models

- Client-only pilot
- Self-hosted server for privacy and sovereignty
- Enable logging/metrics for auditability
- Document data flows (screen, input, clipboard, files) and disable unused features
- Implement first-party client/server features and protocol interop (no vendoring/external exec)
- Opinionated policy profiles and admin packs (RBAC)

## Troubleshooting

- If build issues occur, verify Rust toolchain and platform SDKs
- If connectivity is blocked, check server reachability and firewall rules
- If video/input features fail, check required OS permissions

## Attribution & License

- QuicView workspace code is Apache-2.0 unless otherwise noted.

## Contributing

- Keep changes focused on configuration, orchestration, and enterprise conventions
- For foundational engine-level improvements, contribute to the respective upstream components

---

Questions or suggestions? Open an issue with your target platform and deployment context.