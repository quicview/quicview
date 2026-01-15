# QuicView Protocol Primitives (`proto`)

Status: experimental, original (no upstream reuse).

## Text Handshake v1 (minimal)

- Client sends a single line and awaits a single-line response.
- Encoding: ASCII/UTF-8 text, newline-terminated (`\n`).

Client -> Server:

```
DLNK/1 HELLO nonce=<hex> [auth=<hex>]
```

- `nonce=<hex>`: hex-encoded 16 random bytes (generated per-connection).
- `auth=<hex>`: optional HMAC-SHA256 over the string `"nonce=<hex>"` using a pre-shared key.

Server -> Client (success):

```
DLNK/1 OK
```

Otherwise the server closes the connection without response.

## API

- `probe_tcp(host, port, timeout_ms) -> Result<()>`: Connect within timeout.
- `probe_tls(host, port, timeout_ms, sni) -> Result<()>`: TLS connect within timeout (feature: `tls-client`).
- `handshake_tcp(host, port, timeout_ms) -> Result<()>`: Send hello and expect OK over TCP.
- `handshake_tls(host, port, timeout_ms, sni) -> Result<()>`: Same over TLS (feature: `tls-client`).
- `parse_host_port(input, default_port) -> (host, port)`: IPv6-aware parser (`[::1]:21116`).
- `gen_nonce_hex() -> String`: 16-byte random nonce as hex.
- `hmac_nonce_hex(nonce_hex, key_bytes) -> String`: HMAC-SHA256 of `"nonce=<hex>"`.

## Errors

`ProtoError` variants:
- `TcpConnect { addr, source }`: Connect failure.
- `Timeout(ms)`: Operation timed out.
- `TlsHandshake { addr, source }`: TLS handshake failure.
- `InvalidSni(str)`: Invalid SNI name.
- `Io { ctx, source }`: I/O read/write failure context.
- `Protocol(str)`: Protocol-level failure (e.g., EOF or unexpected response).

## Example server (for local testing)

A small mock is provided at `examples/handshake_server.rs`. It accepts the v1 hello line and replies `DLNK/1 OK` if:
- No key is configured (`DLNK_KEY` unset), or
- `DLNK_KEY` is set and the `auth` matches `hmac(nonce)`.

Run:

```bash
cargo run --example handshake_server -- 127.0.0.1:21116
# with key
DLNK_KEY="secret" cargo run --example handshake_server -- 127.0.0.1:21116
```

Client (CLI):

```bash
# TCP
cargo run -p cli -- client-handshake --timeout 1500 --config examples/quicview.yaml
# TLS (enable feature and set TLS endpoint in your config)
cargo run -p cli --features tls-client -- client-handshake --timeout 1500 --config examples/quicview.yaml
```
