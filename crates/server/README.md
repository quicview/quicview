# QuicView Minimal Server (original)

Status: minimal, original TCP server that speaks the `DLNK/1` text handshake and exposes an optional health TCP port.

What this is:
- A small async binary (`src/main.rs`) that listens on `--listen` and replies `DLNK/1 OK` to a `DLNK/1 HELLO` line.
- Optional HMAC auth: pass `--key` and clients must include `auth=hmac(nonce)`.
- Optional `--health` TCP listener that accepts and immediately closes (readiness check).

What this is NOT:
- It is not a launcher and does not embed upstream servers.
- It does not compile any legacy/scrapped modules.

About the scrapped modules under `src/`:
- Files like `audio_service.rs`, `clipboard_service.rs`, etc., are preserved as references but not compiled.
- Cargo is configured with an explicit `[lib]` pointing to `src/quarantine.rs` so `src/lib.rs` is ignored.
- Keep these files for class/method-level reuse. When you re-implement something, move only the pieces you need into original code under Apache-2.0.

Run:
```bash
cargo build -p server
target/debug/server --listen 127.0.0.1:22116 --health 127.0.0.1:23000
```

See `crates/proto` for the protocol primitives used by this server.
