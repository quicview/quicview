# QuicView Client UI Plan (Leptos + Tauri)

We’re replacing Flutter with a web-first UI using Leptos, then wrapping it natively with Tauri.

## Architecture
- `crates/client`: headless client core (agent) — owns networking/policy state.
- `client-web` (future): Leptos SPA that talks to the agent over a lightweight IPC/HTTP or direct in-process API (WASM builds for browser, or in-process for Tauri).
- `client-tauri` (future): Tauri shell that embeds the Leptos UI for native desktop distribution, wiring menus, tray, and OS permissions.

## Why Leptos first
- Rust-first UI with excellent SSR/CSR options and a clean reactive model.
- Enables browser-based control panels and local UIs with shared components.

## Why Tauri later
- Native packaging, tray/menu integration, and OS-level capabilities.
- Reuse the Leptos front-end inside a lightweight Rust shell.

## Integration Contracts
- The client core exposes:
  - Event stream (status, consent needed, session incoming).
  - Commands (start/stop, grant/revoke capabilities) — to be expanded.
- UI layers implement a `UiAdapter` or subscribe to events and invoke commands.

## Phases
1) Core agent (now): `crates/client` with event API and minimal loop.
2) Web UI prototype: Leptos app (separate crate) that subscribes to events and drives the agent in-process.
3) Native wrapper: Tauri app embedding the Leptos UI; adds tray and permissions.

## Notes
- Keep `client` free of heavy UI deps; Leptos and Tauri stay in separate crates.
- WASM/browser build flow (cargo-leptos) will be added later; not part of the default workspace build.

## Quickstart

Build the web UI and run the client’s HTTP UI (serving static assets):

```zsh
cd quicview/apps/leptos-web
trunk build --release

cd ../../
cargo run -p cli --features http-ui,macos-capture,macos-input,clipboard -- \
  client --start --port 21180 --auth-token dev-token \
  --static-dir ./apps/leptos-web/dist

# Open http://127.0.0.1:21180/
```

Run the Tauri native shell (macOS):

```zsh
cd quicview
cargo build -p tauri-client
open target/debug/tauri-client.app
```

Enable the QUIC control channel and TLS:

See the QUIC/TLS section in the top-level `README.md` for modes (system roots, pinning, TOFU) and examples.

```zsh
# Example: QUIC with pinning
cargo run -p cli --features http-ui,quic-ctrl -- \
  client --ctrl-addr 127.0.0.1:4433 --ctrl-token dev-token \
  --ctrl-tls pin:2f9a...deadbeef --ctrl-sni localhost --open

# Example: QUIC with TOFU (persist to pin file)
cargo run -p cli --features http-ui,quic-ctrl -- \
  client --ctrl-addr ctrl.example.com:4433 --ctrl-token dev-token \
  --ctrl-tls tofu --ctrl-sni ctrl.example.com \
  --ctrl-tofu-pin-file ~/.config/quicview/ctrl.pin --open
```
