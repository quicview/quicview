# Upstream libraries (reference only)

This doc inventories upstream RustDesk libraries under `rustdesk/libs/` that may inform our protocol understanding and future clean-room work. We do not vendor AGPL code. Any functionality we adopt must be reimplemented first-party under Apache-2.0.

Status legend:
- Ready: can be added now with low risk
- Caution: builds/platform specifics to consider
- Defer: not needed for core remote desktop engine or is packaging-only

## Core dependency

- `hbb_common` (Reference)
  - Purpose: shared types, protobufs, IO, crypto, configuration helpers used across the stack.
  - Integration: already added as submodule at `libs/hbb_common` for reference only. Do not link into first-party crates unless license-compatible and necessary.
  - Notes: many other libs depend on it; enabling this first is foundational.

## Functional building blocks

- `scrap` (Caution)
  - Purpose: screen capture; provides frames for encoding/transport.
  - Platform notes:
    - macOS: uses CoreGraphics; should build without extra system packages.
    - Linux: Wayland support gated behind `wayland` (requires GStreamer/dbus). X11 path typically OK without extra features.
    - Windows: uses D3D11 (via `winapi`).
  - Build notes: has build-deps (`bindgen`, `pkg-config` when `linux-pkg-config` feature is used); pulls git deps (nokhwa/hwcodec) when features enabled.
  - Suggestion: vendor later behind feature `capture` that enables optional dep `scrap`.

- `enigo` (Caution)
  - Purpose: input injection (keyboard/mouse control).
  - Platform notes: macOS requires Accessibility permission; Windows needs appropriate privileges; Linux varies.
  - Build notes: depends on git repos `rdev` and `The-Fat-Controller`; path to `hbb_common`.
  - Suggestion: vendor later behind feature `input` that enables optional dep `enigo`.

- `clipboard` (Caution)
  - Purpose: clipboard integration incl. file-paste helpers.
  - Platform notes:
    - macOS: uses `objc2` and `cacao` (git), needs pasteboard APIs.
    - Linux: optional X11/FUSE path via features; otherwise minimal.
  - Build notes: `build.rs` with `cc`; several optional dependencies via features.
  - Suggestion: vendor later behind feature `clipboard` that enables optional dep `clipboard`.

- `virtual_display` (Caution)
  - Purpose: virtual display support; experimental and platform-specific.
  - Platform notes: likely not available on macOS; check driver/kernel requirements on Windows/Linux.
  - Suggestion: defer until we define a concrete need; guard behind feature `virtual-display`.

- `remote_printer` (Caution, Windows-only)
  - Purpose: remote printing.
  - Platform notes: `target_os = "windows"` only; depends on `winapi` and `hbb_common`.
  - Suggestion: guard behind feature `remote-printer` and platform cfg; add only if Windows printing is in-scope.

## Packaging/auxiliary

- `rustdesk-portable-packer` (Defer)
  - Purpose: Windows packaging helper; not needed for core engine.
  - Suggestion: skip for now; revisit under packaging roadmap.

## Recommended gating

Avoid adding features that would link AGPL or incompatible code. Keep experimental references out of the build. Mirror any future first-party capabilities with explicit config and feature gates (e.g., allow_input_control, allow_clipboard).

## Suggested next steps

1) Decide the first capability to unlock:
   - Headless view-only client → `scrap` (capture) only.
   - Control support → add `enigo` (input) behind explicit policy gate.
   - Data exchange → add `clipboard` behind explicit policy gate.
2) Reimplement needed pieces clean-room in `crates/proto`, `crates/server`, and `crates/client`.

This staged approach keeps builds predictable and aligns capabilities with policy and licensing.
