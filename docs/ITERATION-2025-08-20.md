# QuicView Iteration Plan — 2025-08-20 → 2025-09-03

Iteration goal: Achieve Milestone M1 (Server harness — transitional) with a minimal, testable server harness, health endpoint, and CLI wiring. Keep scope tight; land small PRs frequently. No upstream exec-fallback.

## Objectives (DoD)

- Server crate starts and binds listeners from config; emits structured logs.
- Health endpoint responds OK; readiness indicates listener status.
- CLI command `launch-server` wires to the server crate (no Flutter deps).
- Readiness: `/ready` turns OK once our listeners are actually accepting TCP connections.
- CI runs unit tests + a tiny integration test that boots the harness and probes health.

## Stories & Tasks

1) Server crate baseline
- Create `crates/server` scaffolding (if not present) with:
  - Config struct inputs (reusing `crates/config`).
  - Tokio runtime startup, address bind (TCP) from config.
  - Health endpoint: `GET /health` returns 200 OK with JSON `{ "status": "ok" }`.
- Add structured logging via `tracing` + `tracing-subscriber` with env filter.
- Tests: unit test for config mapping; integration test for health endpoint using a random free port.

2) CLI wiring
- Extend `crates/cli` `launch-server` to call into `server::run(cfg)`.
- Flags: `--dry-run` (parse and print resolved config, don’t bind), `--health-port` override.
- Tests: clap parsing smoke test; e2e test starts server on ephemeral port and hits `/health`.

3) Exec fallback (removed)
 - Drop all references and features; focus on first-party listeners only.

4) Observability & readiness
- Add a simple readiness probe endpoint `/ready` that returns 200 once listeners are bound.
- Emit startup summary log: bound addresses and features enabled.
- CI: mark `nextest` run partition for integration tests; gate on readiness.

6) Graceful shutdown [Done]
- CLI traps `Ctrl+C`/`SIGTERM` and triggers server shutdown.
- Health server stops accepting.
- Test `crates/server/tests/shutdown.rs` validates health accept loop stops.

5) CI additions (small)
- Add an integration-test job (Ubuntu) that runs the health probe test only.

## Deliverables
- New/updated crates: `crates/server`, `crates/cli` wiring.
- Tests: `crates/server/tests/health.rs`, CLI parsing tests.
- Docs: update `examples/QuicView.yaml` fields for server binds and notes on exec fallback env.

## Out of Scope (defer)
- In-process upstream adapters (`upstream_compat`) — tracked by M2.
- GUI/viewer work — tracked by M3/M5.
- Packaging and SSO — M6/M7.

## Risks & Mitigations
- Upstream binaries missing on CI → Skip exec-fallback tests unless paths are explicitly provided; keep core harness tests independent.
- Port conflicts → Use OS-assigned ports (bind to 127.0.0.1:0) in tests; discover actual port at runtime.
- Cross-platform differences → Start with Ubuntu + macOS; document Windows caveats; avoid platform-specific syscalls.

## Validation
- Local:
```bash
cd QuicView
cargo build --workspace
cargo run -p cli --features server -- launch-server -c examples/QuicView.yaml --dry-run
cargo run -p cli --features server -- launch-server -c examples/QuicView.yaml &
curl -fsS http://127.0.0.1:<port>/health | jq .
```

- CI: root workflows already exist and are path-scoped to `QuicView/**`.
  - “QuicView CI”: fmt, clippy (pedantic), build on Ubuntu + macOS.
  - “QuicView Advanced Checks”: nextest + cargo-deny.

## Tracking
- Open PRs per story; keep them <300 LOC where possible.
- Link PRs to M1 in the roadmap; ensure each has tests and succinct docs.
