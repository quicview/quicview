# QuicView Architecture Refactor — Issues & Plan

## Overview

QuicView is being simplified from a **P2P architecture** (inherited from RustDesk) to a pure **client-server architecture**. No peer discovery, no rendezvous, no relay servers — just direct connections over QUIC (primary) and TCP/TLS (fallback).

---

## Codebase Structure Analysis

### Current Crate Layout

```
crates/
├── bridge/       # Flutter-free contracts (ClientLauncher, ServerLauncher traits)
├── cli/          # CLI entry point (clap-based commands)
├── client/       # Headless client core + HTTP UI (MJPEG, input, clipboard)
├── common/       # ⚠️ LEGACY — RustDesk P2P code (rendezvous, LAN, KCP, etc.)
├── config/       # YAML config schema (QuicViewConfig)
├── plugin/       # Plugin framework (unused)
├── proto/        # TCP/TLS probe, QUICVIEW/1 handshake, address parsing
├── server/       # Minimal HTTP health server (stub)
└── transport/    # ✅ QUIC control channel (quinn-based, JSON/protobuf framing)

libs/
├── clipboard/    # Clipboard utilities
├── enigo/        # Input injection (upstream fork)
├── hbb_common/   # ⚠️ LEGACY — RustDesk shared lib (rendezvous protos, config, etc.)
├── portable/     # Portable mode support
├── remote_printer/
├── scrap/        # Screen capture (platform-specific)
└── virtual_display/
```

### What's GOOD (Keep & Enhance)

| Crate | Component | Status |
|-------|-----------|--------|
| `transport/` | `quic_ctrl` module | ✅ **Core of new arch** — QUIC control channel with JSON/protobuf framing, TLS modes (insecure, system, pin, TOFU), reconnect with backoff |
| `proto/` | `probe_tcp`, `probe_tls`, `handshake_tcp`, `handshake_tls` | ✅ Keep — useful for health checks and initial handshake |
| `proto/` | `parse_host_port`, `gen_nonce_hex`, `hmac_nonce_hex` | ✅ Keep — utility functions |
| `client/` | `core::Client` | ✅ Keep — event-driven client core with QUIC ctrl integration |
| `client/` | `http_ui` module | ✅ Keep — HTTP API for UI (MJPEG, input, clipboard, SSE) |
| `client/` | `capture::macos` | ✅ Keep — macOS screen capture via ScreenCaptureKit |
| `server/` | `HealthSvc`, `run_health_server` | ✅ Keep — HTTP health/readiness endpoints |
| `config/` | `QuicViewConfig` | ✅ Keep — needs schema update |
| `bridge/` | `ClientLauncher`, `ServerLauncher` traits | ✅ Keep — clean interface for launchers |
| `libs/scrap/` | Screen capture implementations | ✅ Keep (platform-specific capture) |
| `libs/clipboard/` | Clipboard utilities | ✅ Keep |

### What's LEGACY (Remove or Gut)

| Location | Component | Reason to Remove |
|----------|-----------|------------------|
| `crates/common/` | `rendezvous_mediator.rs` (853 lines) | P2P registration/discovery via ID server |
| `crates/common/` | `lan.rs` (345 lines) | UDP broadcast LAN peer discovery |
| `crates/common/` | `hbbs_http.rs` | HTTP API for RustDesk ID server |
| `crates/common/` | `kcp_stream.rs` (152 lines) | KCP-over-UDP stream wrapper (P2P optimization) |
| `crates/common/` | `port_forward.rs` | Port forwarding via relay |
| `crates/common/` | `custom_server.rs` | Custom ID server config |
| `crates/common/` | Most of `lib.rs` | Re-exports P2P modules |
| `libs/hbb_common/` | `rendezvous_proto` | Protobuf schemas for ID/relay servers |
| `libs/hbb_common/` | `udp.rs`, `socket_client.rs` (partial) | Raw UDP socket utilities for P2P |
| `libs/hbb_common/` | `config.rs` (partial) | `RENDEZVOUS_PORT`, relay config, etc. |

### Dependency Flow (Current)

```
┌──────────────────────────────────────────────────────────────────┐
│                           CLI                                     │
│  (clap commands: client, ctrl-server, launch-server, etc.)       │
└───────────────────────┬──────────────────────────────────────────┘
                        │
        ┌───────────────┴───────────────┐
        │                               │
        ▼                               ▼
┌───────────────┐               ┌───────────────┐
│    client     │               │    server     │
│  (core, http) │               │   (health)    │
└───────┬───────┘               └───────┬───────┘
        │                               │
        ├───────────────┬───────────────┤
        ▼               ▼               ▼
┌───────────────┐ ┌───────────────┐ ┌───────────────┐
│   transport   │ │    proto      │ │    config     │
│ (QUIC ctrl)   │ │ (TCP/TLS)     │ │ (YAML schema) │
└───────────────┘ └───────────────┘ └───────────────┘
        │               │
        └───────┬───────┘
                ▼
        ┌───────────────┐
        │    bridge     │
        │  (contracts)  │
        └───────────────┘
                │
                ▼
        ┌───────────────────────────────────────┐
        │           libs/hbb_common              │
        │  (protos, tcp, config, etc.)          │
        │  ⚠️ Contains P2P baggage               │
        └───────────────────────────────────────┘
```

---

## Target Architecture

```
┌──────────────────┐          QUIC (primary)             ┌──────────────────┐
│                  │ ◀────────────────────────────────▶ │                  │
│   QuicView       │    • Control channel (JSON/proto)   │   QuicView       │
│   Client         │    • Screen streaming               │   Server         │
│                  │    • Input injection                │   (Host Machine) │
│   (Viewer)       │    • Clipboard sync                 │                  │
└──────────────────┘    • File transfer (future)         └──────────────────┘
                              │
                    ────────────────────
                    │ TCP+TLS fallback │
                    ────────────────────
```

### New Crate Roles

| Crate | New Role |
|-------|----------|
| `transport/` | **Primary** — QUIC streams for all data (ctrl, screen, input, clipboard) |
| `proto/` | Protocol definitions, frame codecs, handshake logic |
| `client/` | Direct connection to server, HTTP UI for local control |
| `server/` | Listen on host, stream screen, accept input, health endpoints |
| `config/` | Simplified YAML schema (no rendezvous/relay) |
| `bridge/` | Thin contracts for platform-specific launchers |
| `common/` | **GUTTED** — only platform utils, no P2P code |

---

## Current State (P2P Legacy)

| Component | Purpose | Status |
|-----------|---------|--------|
| `rendezvous_host` | ID registration, peer discovery | **To Remove** |
| `relay_host` | NAT traversal fallback relay | **To Remove** |
| `hbbs_http` | HTTP API for ID server | **To Remove** |
| `rendezvous_mediator` | Peer connection negotiation | **To Remove** |
| `lan.rs` | LAN peer discovery | **To Remove** |
| `kcp_stream` | KCP-over-UDP for P2P | **To Remove** |

---

## Target State (Client-Server)

```
┌──────────────────┐          TCP/TLS or QUIC            ┌──────────────────┐
│                  │ ◀────────────────────────────────▶ │                  │
│   QuicView       │    • Screen streaming (MJPEG/H264)  │   QuicView       │
│   Client         │    • Input injection                │   Server         │
│                  │    • Clipboard sync                 │   (Host Machine) │
│   (Viewer)       │    • File transfer (future)       │                  │
└──────────────────┘                                    └──────────────────┘
```

### Connection Model
- **Direct addressing**: Client connects to `server_addr:port` (no ID lookup)
- **Authentication**: Token-based (existing `--auth-token` mechanism)
- **Transport options**:
  - TCP + TLS (simple, widely compatible)
  - QUIC (multiplexed streams, 0-RTT, built-in TLS 1.3)

---

## Issues to Resolve

### Issue #1: Remove Rendezvous/Relay Config
**Priority:** High  
**Files affected:**
- `crates/config/src/lib.rs` — Remove `rendezvous_host`, `relay_host` from `ServerConfig`
- `quicview.yaml`, `examples/quicview.yaml` — Simplify schema
- `crates/common/src/rendezvous_mediator.rs` — Delete or gut
- `crates/common/src/hbbs_http.rs` — Delete
- `crates/common/src/lan.rs` — Delete

**New config schema:**
```yaml
server:
  host: "0.0.0.0"                   # Bind address
  port: 21116                       # Listen port
  tls:
    enabled: true
    cert_path: "/path/to/cert.pem"
    key_path: "/path/to/key.pem"
  auth_token: "secure-token"        # Or use env var

client_policy:
  require_consent: false
  allow_input_control: true
  allow_clipboard: true
  allow_file_transfer: false
```

---

### Issue #2: Simplify Transport Layer
**Priority:** High  
**Files affected:**
- `crates/transport/src/lib.rs` — Focus on TCP+TLS and QUIC only
- `libs/hbb_common/` — Evaluate what's still needed

**Transport options to keep:**
1. **QUIC** (quinn) — **Primary transport**, NAT-friendly, multiplexed streams, built-in TLS 1.3
2. **TCP + TLS** (rustls) — Fallback for restrictive networks

**Transport to remove:**
- KCP-over-UDP (raw UDP wrapper, not needed — QUIC is superior)
- Raw UDP relay protocols (QUIC already handles this better)

**Why QUIC over raw UDP:**
- Connection IDs survive NAT rebinding
- Built-in congestion control and reliability
- Multiplexed streams (screen, input, clipboard on one connection)
- 0-RTT reconnection
- Encrypted by default (TLS 1.3)

---

### Issue #3: Simplify Server Binary
**Priority:** Medium  
**Files affected:**
- `crates/server/src/lib.rs`
- `crates/cli/src/main.rs` — `launch-server` command

**Current server does:**
- Listen for incoming connections
- Authenticate with token
- Stream screen captures
- Accept input injection
- Handle clipboard

**Remove:**
- Any ID registration logic
- Relay/forwarding code
- Heartbeat to rendezvous

---

### Issue #4: Simplify Client Connection
**Priority:** Medium  
**Files affected:**
- `crates/client/src/lib.rs`
- `crates/client/src/io_loop.rs`
- `crates/cli/src/main.rs` — `client` command

**Client should:**
```
quicview client --server 192.168.1.100:21116 --token <auth-token>
```

**Remove:**
- ID-based connection (`--peer-id`)
- Rendezvous lookup
- Relay fallback logic
- NAT traversal attempts

---

### Issue #5: Update CLI Interface
**Priority:** Medium  

**Current commands to simplify:**
| Command | Current | Proposed |
|---------|---------|----------|
| `client` | `--peer-id`, `--ctrl-addr` | `--server <host:port>` |
| `launch-server` | Complex flags | `--listen <addr>`, `--token` |
| `probe-client` | Uses rendezvous | Direct health check |

**Commands to remove:**
- Any rendezvous-related subcommands
- Relay configuration

---

### Issue #6: Clean Up hbb_common
**Priority:** Low (defer)  
**Notes:**
- Large upstream dependency with lots of P2P code
- Evaluate what's actually used:
  - ✅ `tcp.rs` — Keep (TCP utilities)
  - ✅ `protobuf` schemas — Keep (message framing)
  - ✅ `config.rs` — Partially keep
  - ❌ Rendezvous protos — Remove
  - ❌ Relay protos — Remove

---

### Issue #7: Update Documentation
**Priority:** Low  
**Files:**
- `README.md` — Update architecture description
- `docs/ROADMAP.md` — Reflect simplified model
- Config examples — New schema

---

## Migration Path

### Phase 1: Config & Types (Non-breaking)
1. Add new `listen_addr` field alongside old fields
2. Deprecate `rendezvous_host`, `relay_host` (warn if present)
3. Update `QuicViewConfig` struct

### Phase 2: Server Simplification
1. Server listens directly on `listen_addr`
2. Remove rendezvous registration
3. Keep existing stream/input/clipboard handlers

### Phase 3: Client Simplification  
1. Client connects directly to `--server` address
2. Remove ID lookup, relay fallback
3. Simplify TLS modes (keep: system roots, pin, insecure-for-dev)

### Phase 4: Cleanup
1. Delete dead code (rendezvous_mediator, hbbs_http, lan)
2. Trim hbb_common imports
3. Update all docs

---

## Open Questions

1. **QUIC vs TCP+TLS as default?**
   - QUIC has advantages: NAT-friendly, multiplexing, 0-RTT, connection migration
   - TCP+TLS is simpler but less performant for real-time streaming
   - Recommendation: **QUIC default**, TCP+TLS fallback for UDP-blocked networks

2. **Keep CTRL channel (existing QUIC control)?**
   - Currently used for out-of-band signaling
   - May still be useful for multiplexed commands
   - Recommendation: Keep as optional advanced feature

3. **Session resumption?**
   - TLS 1.3 session tickets for fast reconnect
   - QUIC 0-RTT for even faster
   - Recommendation: Implement after core simplification

---

## Task Checklist

- [x] Define new config schema (Issue #1)
- [x] Update `QuicViewConfig` struct
- [ ] Implement direct server listen mode
- [ ] Implement direct client connect mode  
- [x] Remove rendezvous/relay code paths
- [x] Update CLI commands
- [x] Delete dead code files
- [ ] Update documentation
- [ ] Test TCP+TLS flow end-to-end
- [ ] Test QUIC flow end-to-end

---

## Detailed File Actions

### Files to DELETE (P2P Legacy)

| File | Lines | Status |
|------|-------|--------|
| `crates/common/src/rendezvous_mediator.rs` | 853 | ✅ DELETED |
| `crates/common/src/lan.rs` | 345 | ✅ DELETED |
| `crates/common/src/kcp_stream.rs` | 152 | ✅ DELETED |
| `crates/common/src/hbbs_http.rs` | ~100 | ✅ DELETED |
| `crates/common/src/port_forward.rs` | ~200 | ✅ DELETED |
| `crates/common/src/custom_server.rs` | ~50 | ✅ DELETED |

### Files to MODIFY

| File | Action | Status |
|------|--------|--------|
| `crates/common/src/lib.rs` | Remove P2P module imports, keep only platform utils | ✅ DONE |
| `crates/config/src/lib.rs` | New schema: `host`, `port`, `tls`, remove `rendezvous_host`/`relay_host` | ✅ DONE |
| `crates/cli/src/main.rs` | Remove rendezvous commands, simplify `client`/`server` args | ✅ DONE |
| `crates/client/src/lib.rs` | Add direct QUIC connect (not via ctrl channel) | ✅ DONE |
| `crates/server/src/lib.rs` | Add QUIC listener for screen/input streams | ✅ DONE |
| `libs/hbb_common/src/lib.rs` | Audit and remove unused P2P re-exports | ✅ DONE (deprecated) |
| `libs/hbb_common/src/config.rs` | Remove `RENDEZVOUS_PORT`, relay config | ✅ DONE (deprecated) |

### Files to ADD

| File | Purpose | Status |
|------|---------|--------|
| `crates/transport/src/quic_data.rs` | New module for QUIC data streams (screen, input, clipboard) | ✅ DONE |
| `crates/transport/src/tcp_data.rs` | TCP+TLS fallback for data streams | ✅ DONE |
| `crates/server/src/quic_server.rs` | QUIC server listener (quinn Endpoint) | ✅ DONE |
| `crates/client/src/quic_client.rs` | QUIC client connection logic | ✅ DONE |

---

## QUIC Stream Multiplexing Design

```
QUIC Connection
├── Stream 0 (bidirectional): Control channel (existing)
│   ├── Hello/Auth handshake
│   ├── Ping/Pong keepalive
│   └── Commands (Start/Stop/Reauth)
│
├── Stream 1 (server→client): Screen frames
│   └── JPEG/H264 frames with length prefix
│
├── Stream 2 (client→server): Input events
│   └── Mouse/Keyboard events (protobuf or JSON)
│
├── Stream 3 (bidirectional): Clipboard
│   └── Text/file sync requests
│
└── Stream 4+ (future): File transfer, audio, etc.
```

### Framing (All Streams)
```
┌─────────────┬──────────────────────────────┐
│ Length (4B) │ Payload (protobuf or JPEG)   │
└─────────────┴──────────────────────────────┘
```

---

## Priority Order

1. **Phase 1a**: Update config schema (non-breaking, add new fields) ✅ DONE
2. **Phase 1b**: Delete P2P files from `crates/common/` ✅ DONE
3. **Phase 2**: Add QUIC data streams to `transport/` ✅ DONE
4. **Phase 3**: Update server to listen on QUIC ✅ DONE
5. **Phase 4**: Update client to connect via QUIC ✅ DONE
6. **Phase 5**: Gut `hbb_common` dependencies ✅ DONE (deprecated, not deleted)
7. **Phase 6**: Final cleanup and docs ⬜ TODO
