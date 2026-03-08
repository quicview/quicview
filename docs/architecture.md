# Architecture Overview

QuicView is organized as a Cargo workspace with 9 crates, layered from
low-level wire protocol to high-level CLI.

## Crate Dependency Graph

```
quicview-cli
    └── quicview (facade)
            ├── quicview-capture
            ├── quicview-display
            ├── quicview-input
            ├── quicview-session
            ├── quicview-transport ──► quinn / rustls
            ├── quicview-codec
            └── quicview-protocol
```

## Layer Descriptions

### Layer 1 — Wire Protocol (`quicview-protocol`)

Defines the binary frame format and typed messages that transit the wire:

- **`FrameHeader`** — 24-byte binary header: magic, version, frame type, width,
  height, format, timestamp, sequence number, payload length.
- **`InputEvent`** — Mouse (x, y, button, action), Keyboard (keycode, action,
  modifiers), Scroll (dx, dy).
- **`DisplayInfo`** — Display ID, resolution, position, scale, primary flag.
- **`ControlMessage`** — Ping/Pong, SessionOffer/Answer, DisplayConfig,
  Disconnect, KeepAlive.
- **`ProtocolError`** — Decode/Encode/UnsupportedVersion variants.

### Layer 2 — Codec (`quicview-codec`)

Frame encoding and decoding:

- **`RawCodec`** — Identity codec (pass-through, no compression).
- **`DeltaCodec`** — XOR frame differencing: encodes only pixels that changed
  between consecutive frames.
- **`BitrateController`** — Adaptive quality (0–100). Uses a sliding window
  of frame sizes to compute actual bitrate and adjusts quality toward a target.
- **Pixel conversion** — BGRA ↔ RGBA, BGRA → grayscale.

### Layer 3 — Platform Abstraction

#### Capture (`quicview-capture`)

- **`CaptureSource` trait** — `capture_frame() → Vec<u8>`, `display_info()`.
- **`GdiCaptureSource`** — Windows GDI BitBlt capture (primary display).
- **Stubs** — PipeWire (Linux), ScreenCaptureKit (macOS).
- **`TestCaptureSource`** — Deterministic gradient frames for testing.
- **`DisplayWall`** — Multi-display layout for Raspberry Pi arrays.

#### Display (`quicview-display`)

- **`FrameRenderer` trait** — `render(header, data)`.
- **`LogRenderer`** — Logs frame metadata (testing/headless).
- **`BufferRenderer`** — Stores last frame in memory.
- **`DisplaySurface` trait** — `present(buffer, w, h)`.
- **`MemorySurface`** — In-memory surface for testing.

#### Input (`quicview-input`)

- **`InputInjector` trait** — `inject(event)`.
- **`WindowsInputInjector`** — Win32 SendInput (mouse, keyboard, scroll).
- **`LogInjector`** — Logs events without OS interaction.
- **`InputForwarder`** — Async mpsc channel between capture and injection.
- **`ClipboardProvider` trait** — Read/write clipboard.
- **`AudioCapture` trait** — Audio frame capture (stub: `SilentAudioCapture`).

### Layer 4 — Session (`quicview-session`)

Session lifecycle and authentication:

- **Roles** — `Host` (shares displays), `Viewer` (sees & controls),
  `Extender` (virtual display client).
- **`TokenValidator` trait** — `AcceptAll`, `PresharedKeyValidator`
  (constant-time comparison).
- **`Negotiator`** — State machine:
  `Idle → OfferSent/OfferReceived → Established → Closed`.
- **`MemoryDiscovery`** — LAN device discovery (mDNS planned).
- **`PowerManager`** — Sleep/wake virtual displays.

### Layer 5 — Transport (`quicview-transport`)

QUIC connectivity via quinn + rustls:

- **`SelfSignedCert`** — Generate self-signed TLS certificates (rcgen).
  Provides `fingerprint()` for certificate pinning.
- **`CertFingerprint`** — SHA-256 fingerprint for cert verification.
- **`QuicListener`** — Server endpoint: `bind()`, `accept()`.
- **`QuicConnection`** — Client endpoint: `connect()`, `close()`.
- **`StreamMux`** — Multiplexes typed streams over a single QUIC connection:
  - `Video` (kind=0) — Host → Viewer frame data.
  - `Input` (kind=1) — Viewer → Host input events.
  - `Control` (kind=2) — Bidirectional negotiation, keep-alive.

### Layer 6 — Facade (`quicview`)

Public API surface and operational utilities:

- **`Config`** — TOML configuration (bind address, frame rate, quality).
- **`Metrics`** — Atomic counters (connections, frames, errors, bytes).
- **`init_tracing()`** — Structured logging via `tracing-subscriber`.
- **`ShutdownController`** — Graceful shutdown broadcast.
- **FFI** — C ABI: `quicview_create()`, `quicview_destroy()`,
  `quicview_version()`.

### Layer 7 — CLI (`quicview-cli`)

User-facing binary with subcommands:

| Command | Description |
|---------|-------------|
| `serve` | Share displays (host role) |
| `connect` | View & control remote host |
| `extend` | Extend desktop via virtual display |
| `status` | Print runtime metrics |
| `init` | Generate default `quicview.toml` |

## Data Flow

### Host → Viewer (Screen Sharing)

```
CaptureSource::capture_frame()
    → DeltaCodec::encode()
    → FrameHeader::encode() + payload
    → StreamMux::open(Video)
    → QUIC stream → network
```

### Viewer → Host (Input)

```
InputEvent (mouse/key/scroll)
    → serde_json::to_vec()
    → StreamMux::open(Input)
    → QUIC stream → network
    → InputInjector::inject()
```

### Control Channel

```
ControlMessage (Ping/Pong/SessionOffer/etc.)
    → length-prefixed JSON
    → StreamMux::open(Control)
    → QUIC stream → network
```

## Security Architecture

- **Transport encryption** — TLS 1.3 via rustls (QUIC mandates encryption).
- **Certificate pinning** — SHA-256 fingerprint verification via
  `pinned_client_config`.
- **Session authentication** — Pluggable `TokenValidator` trait (PSK, future:
  JWT, TOTP).
- **Input isolation** — Windows UIPI prevents cross-session injection.
- **Message size limits** — Control messages capped at 1 MiB.

See [Security Audit](SECURITY_AUDIT.md) for the full threat model.
