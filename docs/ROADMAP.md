# QuicView Roadmap (August 2025)

This roadmap outlines how QuicView will interoperate at protocol boundaries while keeping a clean separation of concerns, policy controls, and enterprise-grade packaging. It is living documentation and will evolve with implementation feedback.

See also: `docs/ITERATION-2025-08-20.md` for the current 2-week iteration scope targeting M1.

## Project Aim

QuicView provides enterprise-grade remote access with a clean-room, first-party implementation that can interoperate with RustDesk protocols while:

- Keeping client and server responsibilities cleanly separated (no Flutter dependency for server; optional for client UI).
- Running with configuration-first, auditable defaults and feature gates for sensitive capabilities (input, clipboard, file transfer).
- Offering packaging, observability, and policy profiles suitable for regulated environments.
- Maintaining clear licensing boundaries by not vendoring AGPL code and keeping all first-party code Apache-2.0.

## Strategic Objectives

- Build first-party components that speak compatible protocols without copying upstream code.
- Deliver a Flutter-free server (rendezvous/relay orchestration) and a client/agent that can operate headless or with a native UI.
- Provide robust, schema-validated YAML configuration (`crates/config`), surfaced through a simple CLI (`crates/cli`).
- Implement policy enforcement: consent, view-only vs. control, clipboard, file transfer, session limits, and multi-viewer.
- Add enterprise packaging (MSI/PKG/Deb/RPM), logging/metrics, and deployment guidance (MDM, SSO/OIDC integration).

## Milestones & Acceptance Criteria

Milestones are incremental and testable. Where feasible, include cross-platform validation on macOS, Windows, and Linux.

### M0 — Repo hygiene and build green

- What:
  - Workspace builds cleanly (`cargo build --workspace`), example config validates, and CLI shows config.
  - Submodules initialized and pinned (e.g., `libs/hbb_common`).
  - Basic CI checks: Rust toolchain matrix, clippy, fmt.
- Acceptance:
  - Commands:
    - `cargo build --workspace`
    - `cargo run -p cli -- validate -c examples/QuicView.yaml`
    - `cargo run -p cli -- show -c examples/QuicView.yaml`
  - CI: All checks pass on main platforms.

### M1 — Server harness

- What:
  - Server crate runs with config (TLS on/off, binds, health and readiness endpoints).
  - No exec fallback, no vendoring. Clean-room listeners only.
  - Logging and minimal metrics (health, listener status).
- Acceptance:
  - `cargo run -p cli --features server -- launch-server -c examples/QuicView.yaml` starts listeners.
  - `--health` endpoint responds OK; basic logs present.
  - Clients can perform a minimal `DLNK/1` handshake against the listener.

### M2 — Protocol interop hardening

- What:
  - Expand protocol coverage (error handling, timeouts, backoff) for rendezvous/relay analogs.
  - Add more integration tests with protocol-level mocks; avoid upstream binaries or vendored code.
- Acceptance:
  - `cli` probes and handshakes validate success/failure paths reliably across platforms.

### M3 — Endpoint agent (client) headless, view-only

- What:
  - Implement a headless endpoint agent that can accept incoming connections per policy.
  - Honor `client_policy.require_consent` (CLI-/daemon-prompted for now) and default to view-only.
  - Multi-viewer support (concurrent observers) for the same endpoint session.
- Acceptance:
  - `cargo run -p cli --features client -- launch-client -c examples/QuicView.yaml` registers the endpoint and accepts connections.
  - Consent flow enforced; no input injection permitted in view-only mode.
  - E2E test with upstream viewer connecting successfully.

### M4 — Input control and clipboard (gated)

- What:
  - Add optional input control via `enigo` and clipboard integration, behind explicit features and config gates.
  - Implement consent prompts for elevation of capabilities during session start.
  - macOS/Windows/Linux OS permission prompts documented (Screen Recording, Input Monitoring, etc.).
- Acceptance:
  - With `allow_input_control: true`, input injection works after consent.
  - With `allow_clipboard: true`, clipboard sync works; can be disabled at runtime.
  - Negative cases verified (disabled by policy => blocked).

### M5 — Native viewer UI (no Flutter)

- What:
  - Provide a simple native viewer window (e.g., `tao`/`winit` + `egui`) to render remote frames.
  - Keyboard/mouse capture, multi-monitor awareness, basic scaling/fit.
- Acceptance:
  - A `viewer` mode connects to an endpoint and displays its desktop with acceptable latency.
  - Basic UX: connect by ID, toggle view-only/control (subject to policy), select monitor.

### M6 — Packaging, policy profiles, and observability

- What:
  - Packaging templates: Windows MSI (WiX/Burn), macOS PKG/DMG, Linux Deb/RPM.
  - Policy profiles (RBAC-ready) and admin packs (config sets for common enterprise modes).
  - Logging/metrics integration (OpenTelemetry-friendly), audit event stream (session start/stop, consent, capability grants).
- Acceptance:
  - Installers produce working services/shortcuts with default safe policy.
  - Metrics/health endpoints discoverable; audit logs emitted and parsable.

### M7 — Identity and fleet integrations

- What:
  - SSO/OIDC guidance and optional integration for operator authentication.
  - MDM/Intune/Jamf scripts for enrollment and secret/config rotation.
  - License-/key-based server admission control (optional shared secret).
- Acceptance:
  - Documentation + sample configs validated in a demo environment.
  - Gate checks enforced server-side when configured.

## Technical Tracks (Cross-cutting)

- Licensing & Compliance
  - Keep all first-party code Apache-2.0; do not vendor AGPL code. Interoperate only at protocol boundaries.
  - Preserve upstream notices when vendoring; prefer git submodules where possible.

- Security Hardening
  - TLS everywhere by default, proper certificates, and SNI/CA pinning options.
  - Session policy enforcement, rate limits, and feature gates.
  - Secure default config (require consent; data exchange disabled unless explicitly enabled).

- Observability
  - Structured logs with consistent fields; log levels set via config/env.
  - Health/readiness endpoints and basic Prometheus metrics.

- Testing & CI
  - Unit tests for config, bridge contracts, and adapters.
  - Integration tests for handshake flows (feature-gated to include upstream).
  - Platform tests for OS-specific permissions (documented; partial automation where possible).

- Packaging & Distribution
  - Reproducible builds, pinned toolchains, and supply chain notes.
  - OS-specific entitlement/permission documentation and signing guidance.

## Feature/Crate Mapping

- `crates/config`: YAML schema, validation, defaults; ensure round-trip and schema doc generation.
- `crates/bridge`: Contracts/interfaces (if reintroduced), shared error types.
- `crates/server`: Server harness and first-party services.
- `crates/client`: Endpoint agent and optional viewer; add `native` feature for non-Flutter UI.
- `crates/cli`: Commands: `validate`, `show`, `launch-server`, `launch-client`, plus `health` as needed.

## Risks & Mitigations

- API drift with upstream
  - Mitigation: pin submodules/commits; maintain minimal adapters; run periodic sync.

- Platform permission friction (especially macOS)
  - Mitigation: document entitlements; add first-run prompts/tests; provide MDM recipes.

- Scope creep on UI/feature parity
  - Mitigation: keep viewer minimal; focus on policy and orchestration; escalate advanced UX later.

- License contamination
  - Mitigation: do not vendor AGPL code; keep all first-party code Apache-2.0.

## Versioning & Releases

- Use semantic versioning for the workspace crates where applicable.
- Tag milestones (e.g., `v0.1.0-m1`) with release notes that map to acceptance criteria.
- Provide prebuilt artifacts for tagged releases once packaging (M6) is available.

## Quick Commands

```bash
# Build all
cargo build --workspace

# Validate and show config
cargo run -p cli -- validate -c examples/QuicView.yaml
cargo run -p cli -- show -c examples/QuicView.yaml

# Server
cargo run -p cli --features server -- launch-server -c examples/QuicView.yaml

# Client/agent (headless)
cargo run -p cli --features client -- launch-client -c examples/QuicView.yaml
```

Note: Run these commands from inside the `QuicView/` directory. If you're already in `QuicView/`, do not `cd QuicView` again.

---

For background and component inventory, see `docs/UPSTREAM_LIBS.md` and `docs/CLIENT_NO_FLUTTER.md`.
