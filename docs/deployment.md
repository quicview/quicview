# Deployment Guide

Production deployment configuration for QuicView.

---

## System Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| OS | Windows 10 / Linux 5.15+ / macOS 13+ | Windows 11 / Ubuntu 22.04 |
| CPU | 2 cores | 4+ cores |
| RAM | 512 MB | 2+ GB |
| Network | 10 Mbps | 100+ Mbps |
| Rust | stable (edition 2024) | latest stable |

## Building for Release

```bash
cargo build --workspace --release
```

The binary is at `target/release/quicview-cli` (or `.exe` on Windows).

## Configuration

### Generate Default Config

```bash
quicview-cli init
```

Creates `quicview.toml`:

```toml
bind_addr = "127.0.0.1:4433"
frame_rate = 30
quality = 80
```

### Configuration Options

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `bind_addr` | String | `"127.0.0.1:4433"` | Address to bind (host mode) |
| `frame_rate` | u32 | `30` | Target frames per second |
| `quality` | u32 | `80` | Encoding quality (0–100) |

---

## Security Configuration

### Certificate Pinning (Recommended)

When a host starts, it prints its certificate fingerprint:

```
INFO server certificate fingerprint: ab:cd:12:34:56:78:...
```

Share this fingerprint out-of-band (e.g. printed on first start, config
file, QR code) and configure the viewer to validate it. This prevents
man-in-the-middle attacks on untrusted networks.

### Pre-Shared Key Authentication

Use `PresharedKeyValidator` to require a shared secret:

```rust
use quicview_session::{PresharedKeyValidator, SessionToken};

let validator = PresharedKeyValidator::new(b"my-secret-key".to_vec());
let token = SessionToken::new(b"my-secret-key".to_vec());
assert!(validator.validate(&token).is_ok());
```

### Network Security

For production deployments:

1. **Bind to specific interfaces** — Avoid `0.0.0.0` unless necessary.
2. **Firewall rules** — Only allow port 4433 (or your configured port) from
   known IP ranges.
3. **Connection limits** — Configure quinn's `concurrent_connections` to
   prevent resource exhaustion.
4. **VPN/WireGuard** — Consider running QuicView over a VPN for additional
   network-layer security.

---

## Running as a Service

### systemd (Linux)

```ini
[Unit]
Description=QuicView Host
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/quicview-cli --config /etc/quicview/quicview.toml serve
Restart=on-failure
RestartSec=5
User=quicview
Group=quicview

[Install]
WantedBy=multi-user.target
```

### Windows Service

Use [NSSM](https://nssm.cc/) or `sc.exe`:

```powershell
nssm install QuicView "C:\Program Files\QuicView\quicview-cli.exe" `
    "--config" "C:\ProgramData\QuicView\quicview.toml" "serve"
nssm set QuicView AppStdout "C:\ProgramData\QuicView\logs\stdout.log"
nssm set QuicView AppStderr "C:\ProgramData\QuicView\logs\stderr.log"
```

---

## Docker

```dockerfile
FROM rust:latest AS builder
WORKDIR /app
COPY . .
RUN cargo build --workspace --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/quicview-cli /usr/local/bin/
EXPOSE 4433/udp
ENTRYPOINT ["quicview-cli"]
CMD ["serve", "--bind", "0.0.0.0:4433"]
```

```bash
docker build -t quicview .
docker run -p 4433:4433/udp quicview
```

---

## Monitoring

### Metrics

QuicView tracks the following metrics atomically:

| Metric | Description |
|--------|-------------|
| `total_connections` | Cumulative connection count |
| `active_connections` | Currently connected peers |
| `frames_sent` | Total frames transmitted |
| `frames_received` | Total frames received |
| `bytes_sent` | Total bytes transmitted |
| `bytes_received` | Total bytes received |
| `errors` | Total error count |

Check metrics via CLI:

```bash
quicview-cli status
```

### Structured Logging

QuicView uses `tracing` for structured logging. Set the log level via
the `RUST_LOG` environment variable:

```bash
RUST_LOG=info quicview-cli serve
RUST_LOG=quicview=debug,quinn=warn quicview-cli serve
```

---

## Troubleshooting

### Connection Refused

- Verify the host is running and bound to the correct address.
- Check firewall rules allow UDP on the configured port.
- Ensure QUIC (UDP) is not blocked by corporate firewalls.

### High Latency

- Use `Ping`/`Pong` timestamps to measure round-trip time.
- Check network for congestion (QUIC handles congestion control
  automatically).
- Reduce `quality` in config for lower bandwidth usage.

### Screen Capture Failures

- **Windows:** Ensure the process has desktop access (not running as a
  service in Session 0 without interactive desktop).
- **Linux:** PipeWire capture requires a running PipeWire session.
- **macOS:** ScreenCaptureKit requires Screen Recording permission.
