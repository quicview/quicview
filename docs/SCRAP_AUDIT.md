# QuicView Scrap Audit (RustDesk extraction)

Purpose: document what was copied (scraped) from RustDesk into QuicView, where it lives now, the license status, and our reorganization plan so the codebase stays clean and compliant.

Status date: 2025-08-19

## Summary

- New code (Apache-2.0): `crates/config`, `crates/bridge`, `crates/server` (stub), `crates/client` (stub), `crates/cli`.
- Vendored upstream: none. We do not vendor AGPL code. Interop, if any, occurs only at protocol boundaries during testing.
- Scraped file set: none. Do not introduce vendored AGPL modules into the workspace.

## Detected scraped modules (not compiled currently)

The directory `crates/common/src` may contain legacy RustDesk-like modules:
- `rendezvous_mediator.rs`, `client.rs`, `server.rs`, `core_main.rs`, `ipc.rs`, `hbbs_http.rs`, `kcp_stream.rs`, `clipboard.rs`, `clipboard_file.rs`, `virtual_display_manager.rs`, `ui_*.rs`, `tray.rs`, `updater.rs`, etc.

These appear to mirror RustDesk's monorepo modules. To avoid license and maintenance issues, we will not build this crate as-is. Instead, we will:
- Do not move or vendor AGPL modules. If functionality is needed, implement it clean-room under Apache-2.0.

## Organization plan

1) Keep core QuicView code Apache-2.0; avoid introducing AGPL-licensed code.
2) Do not copy upstream code into the repository; if needed, interact through documented network protocols.
   - Retain original copyright/license notices.
   - Add a small adapter module to integrate with QuicView types.
4) Remove/stage-out the bulk-copied `crates/common` once we’ve migrated any needed parts.

## Mapping table (planned)

- (removed)

## Next steps

- Minimal functional path:
  - Ensure `cli health`, `cli probe-client`, and `cli client-handshake` work against our own listeners.
  - Integration tests should spin up only first-party components or use protocol-level mocks; no vendored code.
- Remove `crates/common` from repo or keep it quarantined with a README stating it’s not used.

## Compliance notes

- (removed)
- Top-level workspace remains Apache-2.0 unless noted.
- Submodules maintain their own license under their directories.

