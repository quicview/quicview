# QuicView Security Audit — QV-043

**Date:** 2025-01-15  
**Scope:** All 9 QuicView workspace crates  
**Status:** Complete

---

## Executive Summary

QuicView uses QUIC (quinn 0.11 / rustls 0.23) for all network transport,
providing TLS 1.3 encryption by default. This audit reviews the codebase for
authentication, encryption, input injection, FFI safety, and denial-of-service
risks.

**Overall rating:** Suitable for trusted-network / development use.
Production deployments should enable certificate pinning and pre-shared key
authentication (both now available).

---

## Findings

### 1. TLS Certificate Verification (Critical → Mitigated)

**File:** `quicview-transport/src/cert.rs`

**Finding:** `SkipServerVerification` accepts any server certificate without
validation. A man-in-the-middle attacker on the network can intercept
connections by presenting an arbitrary certificate.

**Mitigation applied:**
- Added `CertFingerprint` type with SHA-256 fingerprint computation.
- Added `SelfSignedCert::pinned_client_config(fingerprint)` that validates the
  server certificate against a known SHA-256 fingerprint (certificate pinning).
- Added `SelfSignedCert::fingerprint()` so hosts can print their fingerprint
  for out-of-band exchange.
- The server now logs its certificate fingerprint on startup.
- `SkipServerVerification` is retained for development / trusted LAN use but
  now emits a `tracing::warn` when used.
- `FingerprintVerifier` performs real TLS 1.2/1.3 signature verification using
  the ring crypto provider.

**Recommendation:** In production, always use `pinned_client_config` with the
host's fingerprint obtained out-of-band (e.g. printed on first start, shared
via QR code, or stored in config).

---

### 2. Session Authentication (Medium → Acceptable)

**File:** `quicview-session/src/auth.rs`

**Finding:** Two validators ship out-of-box:
- `AcceptAll` — accepts any token (development only).
- `PresharedKeyValidator` — constant-time comparison against a shared secret.

**Assessment:** `AcceptAll` is clearly documented as development-only and uses
the `TokenValidator` trait, so swapping in PSK or custom validators is
straightforward. The `constant_time_eq` implementation correctly uses XOR
accumulation to prevent timing side-channels.

**Status:** No changes needed. Validators are appropriately designed.

---

### 3. Control Message Size Limits (Medium → Fixed)

**File:** `quicview-cli/src/main.rs`

**Finding:** The `handle_viewer` function reads a 4-byte length prefix and
allocates a buffer of that size without any upper bound. A malicious peer can
send `0xFFFFFFFF` as the length, causing a 4 GiB allocation attempt (DoS).

**Mitigation applied:**
- Added `MAX_CONTROL_MESSAGE_SIZE` constant (1 MiB) in `quicview-transport`.
- Both server (`handle_viewer`) and client (`Connect` command) now reject
  messages exceeding this limit before allocating.

---

### 4. Windows Input Injection (Low)

**File:** `quicview-input/src/sendinput.rs`

**Finding:** Uses Win32 `SendInput` API via `windows-sys`. This requires the
process to have UI access (UIPI: User Interface Privilege Isolation). The
`unsafe` blocks are minimal and correct:
- `SendInput` receives valid `INPUT` structs with correct size.
- `GetSystemMetrics` is called with valid metric IDs.
- `mem::zeroed()` on `INPUT` is valid (it's a plain data struct).

Mouse coordinates are normalized to absolute screen coordinates (0–65535
range), preventing injection outside screen bounds.

**Status:** No changes needed. Unsafe usage follows Win32 conventions.

---

### 5. FFI Safety (Low)

**File:** `quicview/src/ffi.rs`

**Finding:** Exposes a minimal C FFI surface:
- `quicview_version()` — returns a static C string literal pointer.
- `quicview_create()` / `quicview_destroy()` — standard `Box::into_raw` /
  `Box::from_raw` pattern with null-pointer check.

**Assessment:** The opaque-handle pattern is correct. `quicview_destroy` checks
for null before dereferencing. The `c"0.1.0"` literal ensures NUL termination.

**Status:** No changes needed.

---

### 6. No Rate Limiting (Informational)

**Finding:** The server accepts connections without rate limiting. Under heavy
load, an attacker could exhaust file descriptors or memory by opening many
connections.

**Recommendation:** For production, consider:
- QUIC-level connection limits via `quinn::ServerConfig::concurrent_connections`.
- Per-IP rate limiting at the application layer.
- OS-level firewall rules (iptables/nftables/Windows Firewall).

---

### 7. No Client Certificate Authentication (Informational)

**Finding:** The server uses `with_no_client_auth()`, meaning it does not
require clients to present a TLS certificate. Authentication happens at the
session layer (via `TokenValidator`).

**Assessment:** This is a deliberate design choice — mutual TLS (mTLS) would
add complexity. Session-layer authentication with PSK or tokens is sufficient
for most use cases.

---

## Threat Model

| Threat                    | Likelihood | Impact | Mitigation                           |
|---------------------------|-----------|--------|--------------------------------------|
| MITM on untrusted network | Medium    | High   | Certificate pinning (`pinned_client_config`) |
| Unauthorized viewer       | Medium    | Medium | PSK token validation                 |
| DoS via large messages    | Low       | Medium | Message size limit (1 MiB)           |
| DoS via connection flood  | Low       | Medium | QUIC-level limits (recommended)      |
| Input injection escape    | Very Low  | Low    | UIPI enforced by Windows             |
| FFI memory corruption     | Very Low  | High   | Opaque handle pattern, null checks   |

---

## Files Reviewed

| Crate               | File           | Risk Level |
|---------------------|----------------|-----------|
| quicview-transport  | cert.rs        | Critical → Mitigated |
| quicview-transport  | connection.rs  | Medium (uses cert.rs) |
| quicview-transport  | listener.rs    | Low |
| quicview-transport  | mux.rs         | Low |
| quicview-input      | injector.rs    | Low |
| quicview-input      | sendinput.rs   | Low |
| quicview-session    | auth.rs        | Medium → Acceptable |
| quicview            | ffi.rs         | Low |
| quicview-cli        | main.rs        | Medium → Fixed |

---

## Conclusion

All critical and medium-severity findings have been addressed:
1. **Certificate pinning** is now available via `SelfSignedCert::pinned_client_config`.
2. **Message size limits** prevent allocation-based DoS.
3. **Session authentication** uses constant-time comparison.
4. **Unsafe code** follows established patterns and is minimal.

The codebase is suitable for deployment on trusted networks today, and with
certificate pinning enabled, on untrusted networks as well.
