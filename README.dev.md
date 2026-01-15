# QuicView Development Notes

This workspace separates client and server responsibilities for enterprise deployments and harnesses existing RustDesk code where appropriate (not a mere wrapper/launcher).

Crates:

- `config`: shared config models and YAML loader.
- `bridge`: Flutter-free contracts (`ClientLauncher`, `ServerLauncher`) and shared errors.
- `server`: Flutter-free server implementation; first-party, clean-room components only (no upstream exec).
- `client`: client core (headless agent + UI-agnostic API). UI: Leptos (web-first) and Tauri (native wrapper) in separate crates.
- `cli`: CLI to validate/show config and start client/server via feature flags.
// removed: `upstream_compat` vendoring approach has been dropped to keep QuicView Apache-2.0.

The CLI is minimal and policy-driven; the core direction is to keep first-party implementations and avoid shelling out long-term.

## Build

From `quicview/`:

```
cargo build --workspace
```

## Try it

```
# Validate example config
cargo run -p cli -- validate --config examples/quicview.yaml

# Show values for scripting
cargo run -p cli -- show --config examples/quicview.yaml

# Launch server (feature-gated)
cargo run -p cli --features server -- launch-server --config examples/quicview.yaml

# Note: no `upstream` feature. We do not vendor/link AGPL code.

# Launch client HTTP control surface (feature-gated)
# Starts a tiny HTTP server exposing /, /status, /start, /stop
cargo run -p cli --features http-ui -- client --port 0

# In another shell, probe it (replace PORT):
curl -s http://127.0.0.1:PORT/status
curl -X POST http://127.0.0.1:PORT/start
curl -X POST http://127.0.0.1:PORT/stop

# Visit the minimal dashboard:
# http://127.0.0.1:PORT/

## Optional auth and autostart

You can require a bearer token for POST actions and start the core on launch:

```
cargo run -p cli --features http-ui -- client --port 0 --start --auth-token secret
```

Then use:

```
curl -s http://127.0.0.1:PORT/status
curl -X POST -H 'Authorization: Bearer secret' http://127.0.0.1:PORT/start
curl -X POST -H 'Authorization: Bearer secret' http://127.0.0.1:PORT/stop
```

## Build Leptos app and serve statically

You can build the Leptos CSR app and have the client server serve the static files:

```
cargo install trunk wasm-bindgen-cli
cd apps/leptos-web
trunk build --release
cd ../..
cargo run -p cli --features http-ui -- client --port 21180 --start --static-dir apps/leptos-web/dist --open
```

Flags:
- `--static-dir <path>`: serve files from a directory (e.g., `apps/leptos-web/dist`).
- `--open`: open a browser to the HTTP UI after starting.
- `--auth-token <str>`: require Bearer token for POST actions.
- `--allow-external`: allow binding to non-localhost addresses; use with `--auth-token`.
```

## Next steps

- Replace exec fallback with first-party server components in `crates/server` as we advance milestones (Apache-2.0 only).
- Add platform-specific client integration and optional Flutter via `client/flutter` feature.
- Add packaging scripts (MSI/PKG/Deb/RPM) that ship QuicView with policy presets.
// Vendoring upstream code is not part of the plan; prefer protocol interop and clean-room implementations.
