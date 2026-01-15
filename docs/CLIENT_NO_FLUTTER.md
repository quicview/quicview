# Client without Flutter: Feasibility and Plan

Goal: Ship a QuicView client that is Flutter-free via our own first-party engine, with optional protocol interop for testing.

## Why feasible
- Core components are Rust:
  - Networking: KCP, relay/rendezvous flows
  - Screen capture/encode: `libs/scrap` and codecs
  - Input control: `enigo`, OS-specific services
  - Clipboard, file transfer: Rust crates
- Desktop GUI in upstream already uses native technologies (e.g., `sciter-rs`, `tao`, `tray-icon`) behind feature flags; Flutter is not mandatory for desktop.
- We can build a headless client + tray/consent prompts using Rust-only crates.

## Architecture options (Flutter-free)
- Headless agent + tray UI:
  - Service/daemon runs the client engine.
  - A small tray app (using `tao` + `tray-icon`) handles status and consent prompts.
- Native viewer window:
  - Use `tao/winit` + an immediate-mode GUI (`egui`) to render remote frames decoded by Rust engine; or reuse upstream Sciter-based viewer (no Flutter).
- Strict policy path (no GUI):
  - In locked-down environments, operate via CLI + MDM prompts, requiring consent or pre-approved policy without any GUI.

## Plan & Milestones
1) M0: Headless client prototype (no GUI)
- Accept/Initiate connections, apply `client_policy` (view-only, input, clipboard, file transfer).
- CLI-driven consent for prototyping.

2) M1: Tray + consent prompts
- Small tray app with status, start/stop.
- Native consent dialogs per policy.

3) M2: Viewer window
- Render remote desktop in a native window (no Flutter); keyboard/mouse capture; clipboard and file transfer controls.

4) M3: Packaging & OS permissions
- macOS: Screen Recording, Input Monitoring prompts; Linux/Windows equivalents.
- Codesigning/notarization guidance.

## Reuse vs vendor
- Do not vendor upstream AGPL code; implement the needed engine pieces first-party.
- Keep GUI minimal and native to avoid Flutter.
- Maintain `bridge` contracts so `client` crate stays independent of UI tech.

## Risks & considerations
- Platform permission UX, hardware acceleration, and performance tuning.
- Feature parity for file transfer and multi-monitor support.
- Testing matrix across macOS, Windows, Linux.

## Next steps
- Implement minimal client engine pieces in `crates/client` and wire `client::Client` to use them.
- Add a `client/native` feature and begin a small tray application.
- Provide a headless “service mode” for server-like operation on endpoints.
