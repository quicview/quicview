# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] — 2025-06-07

### Added
- 8-crate workspace: protocol, codec, capture, display, input, session, facade, cli
- Wire protocol: frame headers (24-byte BE), input events, display metadata, control messages
- Raw codec with pixel format conversion (BGRA↔RGBA, RGBA→RGB)
- Capture source trait with virtual display management
- Display renderer and surface abstractions
- Input injector and async forwarder
- Session roles (Host/Viewer/Extender), token auth, negotiation state machine
- FFI surface (C ABI: create/destroy/version)
- CLI with `serve`, `connect`, `extend` subcommands
- CI: check, test (3 OS), clippy, fmt, doc, audit
- Integration tests and benchmarks
