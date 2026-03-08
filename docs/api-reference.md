# API Reference

Public types, traits, and functions exported by each QuicView crate.

---

## `quicview-protocol`

Wire-level types shared by all crates.

### Structs

| Type | Fields | Description |
|------|--------|-------------|
| `FrameHeader` | `magic`, `version`, `frame_type`, `width`, `height`, `format`, `timestamp_us`, `sequence`, `payload_len` | 24-byte binary frame header |
| `MouseEvent` | `x: i32`, `y: i32`, `button: Option<(MouseButton, KeyAction)>` | Mouse position + optional button |
| `KeyEvent` | `keycode: u32`, `action: KeyAction`, `modifiers: u32` | Keyboard key press/release |
| `ScrollEvent` | `dx: f32`, `dy: f32` | Scroll delta |
| `DisplayInfo` | `id`, `width`, `height`, `x`, `y`, `scale`, `primary` | Display metadata |
| `SessionOffer` | `displays`, `capabilities` | Host → Viewer negotiation offer |

### Enums

| Type | Variants | Description |
|------|----------|-------------|
| `InputEvent` | `Mouse`, `Key`, `Scroll` | Tagged input event |
| `ControlMessage` | `Ping`, `Pong`, `SessionOffer`, `SessionAnswer`, `DisplayConfig`, `Disconnect`, `KeepAlive` | Control channel message |
| `FrameType` | `Raw`, `Delta`, `Key` | Frame encoding type |
| `PixelFormat` | `Bgra8`, `Rgba8`, `Grayscale8` | Pixel data format |
| `MouseButton` | `Left`, `Right`, `Middle`, `Back`, `Forward` | Mouse button |
| `KeyAction` | `Press`, `Release` | Key/button action |
| `ProtocolError` | `Decode`, `Encode`, `UnsupportedVersion` | Protocol-level error |

---

## `quicview-codec`

### Traits

```rust
pub trait FrameEncoder: Send {
    fn encode(&mut self, frame: &[u8], header: &FrameHeader) -> Vec<u8>;
}

pub trait FrameDecoder: Send {
    fn decode(&mut self, data: &[u8], header: &FrameHeader) -> Vec<u8>;
}
```

### Structs

| Type | Description |
|------|-------------|
| `RawCodec` | Identity codec — no compression |
| `DeltaCodec` | XOR frame differencing encoder/decoder |
| `BitrateController` | Adaptive quality controller (target bps, quality 0–100) |

### Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `bgra_to_rgba` | `(data: &mut [u8])` | In-place BGRA → RGBA conversion |
| `rgba_to_bgra` | `(data: &mut [u8])` | In-place RGBA → BGRA conversion |
| `bgra_to_grayscale` | `(data: &[u8]) → Vec<u8>` | BGRA → 8-bit grayscale |

---

## `quicview-capture`

### Traits

```rust
pub trait CaptureSource: Send {
    fn capture_frame(&mut self) -> Result<Vec<u8>, CaptureError>;
    fn display_info(&self) -> DisplayInfo;
}
```

### Structs

| Type | Platform | Description |
|------|----------|-------------|
| `GdiCaptureSource` | Windows | GDI BitBlt screen capture |
| `PipeWireCaptureSource` | Linux | Stub — architecture ready |
| `ScreenCaptureKitSource` | macOS | Stub — architecture ready |
| `TestCaptureSource` | All | Deterministic gradient frames |
| `DisplayWall` | All | Multi-display layout manager |

---

## `quicview-display`

### Traits

```rust
pub trait FrameRenderer: Send {
    fn render(&mut self, header: &FrameHeader, data: &[u8]) -> Result<(), DisplayError>;
}

pub trait DisplaySurface: Send {
    fn present(&mut self, buffer: &[u8], width: u32, height: u32) -> Result<(), DisplayError>;
}
```

### Structs

| Type | Description |
|------|-------------|
| `LogRenderer` | Logs frame metadata via tracing |
| `BufferRenderer` | Stores last frame in memory |
| `MemorySurface` | In-memory display surface for testing |

---

## `quicview-input`

### Traits

```rust
pub trait InputInjector: Send {
    fn inject(&mut self, event: &InputEvent) -> Result<(), InputError>;
    fn inject_batch(&mut self, events: &[InputEvent]) -> Result<(), InputError>;
}

pub trait ClipboardProvider: Send {
    fn read(&self) -> Result<String, InputError>;
    fn write(&mut self, text: &str) -> Result<(), InputError>;
}

pub trait AudioCapture: Send {
    fn capture_audio(&mut self) -> Result<Vec<u8>, InputError>;
}
```

### Structs

| Type | Platform | Description |
|------|----------|-------------|
| `LogInjector` | All | Logs events without OS interaction |
| `WindowsInputInjector` | Windows | Win32 SendInput API |
| `InputForwarder` | All | Async mpsc channel forwarder |
| `MemoryClipboard` | All | In-memory clipboard for testing |
| `SilentAudioCapture` | All | Returns empty audio frames |

---

## `quicview-session`

### Traits

```rust
pub trait TokenValidator: Send + Sync {
    fn validate(&self, token: &SessionToken) -> Result<(), SessionError>;
}
```

### Structs

| Type | Description |
|------|-------------|
| `SessionToken` | Opaque authentication token (wraps `Vec<u8>`) |
| `AcceptAll` | Validator that accepts every token (development) |
| `PresharedKeyValidator` | Constant-time PSK comparison |
| `Negotiator` | Session state machine |
| `MemoryDiscovery` | LAN device discovery registry |
| `PowerManager` | Virtual display sleep/wake |

### Enums

| Type | Variants | Description |
|------|----------|-------------|
| `Role` | `Host`, `Viewer`, `Extender` | Session participant role |
| `NegotiationState` | `Idle`, `OfferSent`, `OfferReceived`, `Established`, `Closed` | Negotiator state |

---

## `quicview-transport`

### Structs

| Type | Description |
|------|-------------|
| `SelfSignedCert` | Self-signed TLS cert generation (rcgen) |
| `CertFingerprint` | SHA-256 certificate fingerprint for pinning |
| `QuicListener` | Server-side QUIC listener |
| `QuicConnection` | Client-side QUIC connection |
| `StreamMux` | Typed bidirectional stream multiplexer |

### Key Methods

```rust
// Certificate
SelfSignedCert::generate(subject_alt_names: &[&str]) -> Result<Self, TransportError>
SelfSignedCert::fingerprint(&self) -> CertFingerprint
SelfSignedCert::server_config(&self) -> Result<ServerConfig, TransportError>
SelfSignedCert::client_config() -> Result<ClientConfig, TransportError>
SelfSignedCert::pinned_client_config(fingerprint: CertFingerprint) -> Result<ClientConfig, TransportError>

// Listener
QuicListener::bind(addr: SocketAddr, cert: &SelfSignedCert) -> Result<Self, TransportError>
QuicListener::accept(&self) -> Result<Connection, TransportError>
QuicListener::mux(connection: Connection) -> StreamMux

// Connection
QuicConnection::connect(remote: SocketAddr, server_name: &str) -> Result<Self, TransportError>
QuicConnection::mux(&self) -> StreamMux
QuicConnection::close(&self)

// Stream Mux
StreamMux::open(&self, kind: StreamKind) -> Result<(SendStream, RecvStream), TransportError>
StreamMux::accept(&self) -> Result<(StreamKind, SendStream, RecvStream), TransportError>
```

### Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `MAX_CONTROL_MESSAGE_SIZE` | 1,048,576 | Maximum control message payload (1 MiB) |

### Enums

| Type | Variants | Description |
|------|----------|-------------|
| `StreamKind` | `Video(0)`, `Input(1)`, `Control(2)` | QUIC stream type tag |

---

## `quicview` (Facade)

### Structs

| Type | Description |
|------|-------------|
| `Config` | TOML-based configuration |
| `Metrics` | Atomic performance counters |
| `ShutdownController` | Graceful shutdown broadcast |
| `ShutdownSignal` | Receiver for shutdown notification |
| `QuicViewHandle` | Opaque FFI handle |

### Functions

| Function | Description |
|----------|-------------|
| `init_tracing()` | Initialize structured logging |
| `quicview_version() → *const c_char` | FFI: library version |
| `quicview_create() → *mut QuicViewHandle` | FFI: create handle |
| `quicview_destroy(handle)` | FFI: destroy handle |

### Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `VERSION` | `"0.1.0"` | Library version from Cargo.toml |

---

## `quicview-cli`

Binary crate — no public API. See [Getting Started](getting-started.md) for
CLI usage.

### Subcommands

| Command | Description |
|---------|-------------|
| `serve [--bind ADDR]` | Host displays |
| `connect --remote ADDR` | View remote host |
| `extend --remote ADDR [--resolution WxH]` | Extend desktop |
| `status` | Print metrics |
| `init` | Generate default config |
