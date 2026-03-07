# QuicView

[![CI](https://github.com/quicview/quicview/actions/workflows/ci.yml/badge.svg)](https://github.com/quicview/quicview/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

**QUIC-native visual streaming runtime** — remote desktop, video calls, and infinite display extension.

QuicView is the visual layer of the [Comquic](https://github.com/comquic) ecosystem, built on top of [QuicRTC](https://github.com/quicrtc/quicrtc) (connectivity) and [QuicSignal](https://github.com/quicsignal/quicsignal) (protocol/encryption).

## Use-cases

| Mode | Description |
|------|-------------|
| **Remote Desktop** | Host shares displays → Viewer sees & controls them (RDP alternative) |
| **Video Call** | Peer-to-peer display + camera streaming |
| **Display Extension** | IoT devices (RPi, etc.) run QuicView as clients to extend your desktop infinitely |

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                     quicview-cli                         │
│            serve · connect · extend                      │
├──────────────────────────────────────────────────────────┤
│                       quicview                           │
│             facade · error · ffi · observability         │
├────────────┬────────────┬────────────┬───────────────────┤
│  capture   │  display   │   input    │     session       │
│  source    │  renderer  │  injector  │     role          │
│  virtual   │  surface   │  forwarder │     auth          │
│  display   │            │            │     negotiation   │
├────────────┴────────────┴────────────┴───────────────────┤
│                    quicview-codec                        │
│          encoder · decoder · pixel conversion            │
├──────────────────────────────────────────────────────────┤
│                   quicview-protocol                      │
│       frame · input · display · message · error          │
└──────────────────────────────────────────────────────────┘
```

## Crates

| Crate | Description |
|-------|-------------|
| `quicview-protocol` | Wire protocol: frame headers, input events, display metadata, control messages |
| `quicview-codec` | Frame encoding/decoding, pixel format conversion |
| `quicview-capture` | Screen capture trait + virtual display creation |
| `quicview-display` | Frame rendering + display surface abstraction |
| `quicview-input` | Input injection + event forwarding |
| `quicview-session` | Roles (Host/Viewer/Extender), auth, display negotiation |
| `quicview` | Facade: re-exports, error composition, FFI, observability |
| `quicview-cli` | Binary: `quicview serve`, `quicview connect`, `quicview extend` |

## Quick Start

```bash
# Build everything
cargo build --workspace

# Run tests
cargo test --workspace

# Host your displays
cargo run -- serve --bind 0.0.0.0:4433

# Connect as viewer
cargo run -- connect --remote 192.168.1.10:4433

# Extend desktop onto RPi
cargo run -- extend --remote rpi.local:4433 --resolution 1920x1080
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
