# Getting Started

This guide walks you through building QuicView, running a host, and
connecting a viewer.

## Prerequisites

- **Rust** stable toolchain (edition 2024)
- **Windows 10+** for screen capture and input injection
- Linux/macOS: builds and tests pass, but capture uses stubs (PipeWire /
  ScreenCaptureKit integration planned)

## Build

```bash
# Clone the repository
git clone https://github.com/quicview/quicview.git
cd quicview

# Build all crates
cargo build --workspace

# Run the full test suite
cargo test --workspace
```

## Host Your Displays

Start QuicView in host mode to share your screen:

```bash
# Default bind on 127.0.0.1:4433
cargo run -p quicview-cli -- serve

# Or specify a bind address
cargo run -p quicview-cli -- serve --bind 0.0.0.0:4433
```

On startup the server prints its **certificate fingerprint**:

```
INFO server certificate fingerprint: ab:cd:12:34:...
INFO 0.0.0.0:4433 host listening — waiting for viewers
```

Share this fingerprint with viewers for secure connections.

## Connect as a Viewer

```bash
cargo run -p quicview-cli -- connect --remote 192.168.1.10:4433
```

The viewer opens a QUIC connection, sends a Ping, and receives a Pong to
verify connectivity.

## Extend Your Desktop (IoT)

Extend your desktop onto a headless device (e.g. Raspberry Pi):

```bash
cargo run -p quicview-cli -- extend --remote rpi.local:4433 --resolution 1920x1080
```

## Configuration File

Generate a default configuration:

```bash
cargo run -p quicview-cli -- init
```

This creates `quicview.toml` with default settings. Pass it explicitly:

```bash
cargo run -p quicview-cli -- --config quicview.toml serve
```

## Status & Metrics

Check runtime metrics:

```bash
cargo run -p quicview-cli -- status
```

## Next Steps

- Read the [Architecture Overview](architecture.md) for crate structure and
  data flow.
- Review the [API Reference](api-reference.md) for type and trait details.
- See the [Deployment Guide](deployment.md) for production configuration.
- Check the [Security Audit](SECURITY_AUDIT.md) for threat model and
  mitigations.
