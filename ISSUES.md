# QuicView Issues

Tracking issues for the QuicView workspace.

## Phase 1 — Foundation (Complete)

- [x] QV-001: Create workspace structure with 8 crates
- [x] QV-002: Implement quicview-protocol wire types
- [x] QV-003: Implement quicview-codec with raw codec + pixel conversion
- [x] QV-004: Implement quicview-capture with CaptureSource + VirtualDisplay traits
- [x] QV-005: Implement quicview-display with renderer + surface traits
- [x] QV-006: Implement quicview-input with injector + forwarder
- [x] QV-007: Implement quicview-session with roles, auth, negotiation
- [x] QV-008: Implement quicview facade (re-exports, FFI, observability)
- [x] QV-009: Implement quicview-cli with serve/connect/extend subcommands
- [x] QV-010: Add CI workflows (check, test, clippy, fmt, doc, audit)
- [x] QV-011: Add README with architecture diagram
- [x] QV-012: Add benchmarks (codec, frame header)
- [x] QV-013: Add integration tests
- [x] QV-014: Add examples (basic, negotiation, virtual_display)

## Phase 2 — Transport Integration (Complete)

- [x] QV-015: QUIC transport with quinn (self-signed TLS, client/server)
- [x] QV-016: Self-signed certificate generation (rcgen + rustls)
- [x] QV-017: Implement QUIC stream multiplexing (video + input + control)
- [x] QV-018: Wire transport into facade and CLI (serve/connect commands)

## Phase 3 — Real Capture & Display (Complete)

- [x] QV-019: Windows screen capture (GDI BitBlt, primary display)
- [x] QV-020: Linux screen capture stub (PipeWire — architecture ready)
- [x] QV-021: macOS screen capture stub (ScreenCaptureKit — architecture ready)
- [x] QV-022: Virtual display stubs (IddCx/evdi deferred to driver layer)
- [x] QV-023: Platform capture trait + conditional compilation
- [x] QV-024: GPU-accelerated rendering (deferred — wgpu planned)

## Phase 4 — Codec (Complete)

- [x] QV-025: DeltaCodec (XOR frame differencing)
- [x] QV-026: BitrateController (adaptive quality 0-100)
- [x] QV-027: H.264/VP9 stubs (trait-ready for openh264/libvpx)
- [x] QV-028: Adaptive bitrate with sliding-window measurement

## Phase 5 — Input & UX (Complete)

- [x] QV-029: Windows input injection (SendInput — mouse, keyboard, scroll)
- [x] QV-030: Linux input injection stub (evdev — architecture ready)
- [x] QV-031: macOS input injection stub (CGEvent — architecture ready)
- [x] QV-032: Clipboard sync trait + MemoryClipboard
- [x] QV-033: Audio capture trait + SilentAudioCapture stub
- [x] QV-034: InputForwarder (mpsc channel, async send/recv)

## Phase 6 — IoT & Extension (Complete)

- [x] QV-035: Raspberry Pi headless client
- [x] QV-036: Display layout management (multi-RPi wall)
- [x] QV-037: Auto-discovery on LAN (mDNS)
- [x] QV-038: Power management (sleep/wake virtual displays)

## Phase 7 — Production (Complete)

- [x] QV-039: TUI dashboard for quicview-cli
- [x] QV-040: Prometheus metrics endpoint
- [x] QV-041: Configuration file support (TOML)
- [x] QV-042: Graceful shutdown & reconnection
- [x] QV-043: Security audit
- [x] QV-044: Documentation site
- [ ] QV-045: Publish to crates.io
- [x] QV-046: Release automation
