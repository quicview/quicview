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

## Phase 2 — Transport Integration

- [ ] QV-015: Integrate QuicRTC for QUIC transport
- [ ] QV-016: Integrate QuicSignal for session encryption
- [ ] QV-017: Implement QUIC stream multiplexing (video + input + control)
- [ ] QV-018: Implement relay fallback via QuicRTC relay

## Phase 3 — Real Capture & Display

- [ ] QV-019: Windows screen capture (DXGI Desktop Duplication)
- [ ] QV-020: Linux screen capture (PipeWire / X11)
- [ ] QV-021: macOS screen capture (ScreenCaptureKit)
- [ ] QV-022: Windows virtual display driver (IddCx)
- [ ] QV-023: Linux virtual display (evdi / virtual framebuffer)
- [ ] QV-024: GPU-accelerated rendering (wgpu surface)

## Phase 4 — Codec

- [ ] QV-025: H.264 hardware encoding (NVENC / QSV / AMF)
- [ ] QV-026: H.264 hardware decoding
- [ ] QV-027: VP9 software fallback
- [ ] QV-028: Adaptive bitrate based on network quality

## Phase 5 — Input & UX

- [ ] QV-029: Windows input injection (SendInput)
- [ ] QV-030: Linux input injection (evdev / uinput)
- [ ] QV-031: macOS input injection (CGEvent)
- [ ] QV-032: Clipboard sync
- [ ] QV-033: Multi-monitor drag-and-drop
- [ ] QV-034: Audio streaming

## Phase 6 — IoT & Extension

- [ ] QV-035: Raspberry Pi headless client
- [ ] QV-036: Display layout management (multi-RPi wall)
- [ ] QV-037: Auto-discovery on LAN (mDNS)
- [ ] QV-038: Power management (sleep/wake virtual displays)

## Phase 7 — Production

- [ ] QV-039: TUI dashboard for quicview-cli
- [ ] QV-040: Prometheus metrics endpoint
- [ ] QV-041: Configuration file support (TOML)
- [ ] QV-042: Graceful shutdown & reconnection
- [ ] QV-043: Security audit
- [ ] QV-044: Documentation site
- [ ] QV-045: Publish to crates.io
- [ ] QV-046: Release automation
