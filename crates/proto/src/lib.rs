use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtoError {
    #[error("tcp connect {addr}: {source}")]
    TcpConnect {
        addr: String,
        #[source]
        source: std::io::Error,
    },
    #[error("timeout after {0} ms")]
    Timeout(u64),
    #[error("tls handshake {addr}: {source}")]
    TlsHandshake {
        addr: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("invalid sni: {0}")]
    InvalidSni(String),
    #[error("io error during {ctx}: {source}")]
    Io {
        ctx: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("protocol error: {0}")]
    Protocol(&'static str),
    #[error("invalid address: {0}")]
    InvalidAddress(String),
}

/// Attempt a TCP connect to `host:port` within `timeout_ms`.
///
/// # Errors
/// Returns an error on connect failure or timeout.
pub async fn probe_tcp(host: &str, port: u16, timeout_ms: u64) -> Result<(), ProtoError> {
    let addr = format!("{host}:{port}");
    let dur = Duration::from_millis(timeout_ms);
    let fut = tokio::net::TcpStream::connect(addr.clone());
    match tokio::time::timeout(dur, fut).await {
        Ok(Ok(_s)) => Ok(()),
        Ok(Err(e)) => Err(ProtoError::TcpConnect { addr, source: e }),
        Err(_) => Err(ProtoError::Timeout(timeout_ms)),
    }
}

#[cfg(feature = "tls-client")]
/// Attempt a TLS connect to `host:port` within `timeout_ms`.
/// Optionally override SNI and provide custom CAs in addition to system roots.
///
/// # Errors
/// Returns an error on connect failure, TLS handshake failure, invalid SNI, or timeout.
pub async fn probe_tls(
    host: &str,
    port: u16,
    timeout_ms: u64,
    sni: Option<&str>,
    ca_pem: Option<&[u8]>,
) -> Result<(), ProtoError> {
    use rustls::pki_types::ServerName;
    use std::sync::Arc;
    use tokio_rustls::{rustls, TlsConnector};

    let addr = format!("{host}:{port}");
    let dur = Duration::from_millis(timeout_ms);
    // Build a root store from system certs plus optional CA bundle
    let mut root_store = rustls::RootCertStore::empty();
    match rustls_native_certs::load_native_certs() {
        Ok(certs) => {
            for cert in certs {
                let _ = root_store.add(cert);
            }
        }
        Err(_e) => {}
    }
    if let Some(pem) = ca_pem {
        let mut cursor = std::io::Cursor::new(pem);
        for c in rustls_pemfile::certs(&mut cursor).flatten() {
            let _ = root_store.add(c);
        }
    }
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));

    let tcp = match tokio::time::timeout(dur, tokio::net::TcpStream::connect(addr.clone())).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(ProtoError::TcpConnect { addr, source: e }),
        Err(_) => return Err(ProtoError::Timeout(timeout_ms)),
    };

    let sni_val = sni.unwrap_or(host).to_string();
    let server_name =
        ServerName::try_from(sni_val.clone()).map_err(|_| ProtoError::InvalidSni(sni_val))?;

    match tokio::time::timeout(dur, connector.connect(server_name, tcp)).await {
        Ok(Ok(_tls)) => Ok(()),
        Ok(Err(e)) => Err(ProtoError::TlsHandshake {
            addr,
            source: Box::new(e),
        }),
        Err(_) => Err(ProtoError::Timeout(timeout_ms)),
    }
}

#[cfg(not(feature = "tls-client"))]
#[allow(clippy::unused_async, clippy::missing_errors_doc)]
pub async fn probe_tls(
    _host: &str,
    _port: u16,
    _timeout_ms: u64,
    _sni: Option<&str>,
    _ca_pem: Option<&[u8]>,
) -> Result<(), ProtoError> {
    Err(ProtoError::InvalidSni("tls-client feature disabled".into()))
}

pub const HELLO_PFX: &str = "DLNK/1 HELLO"; // e.g. DLNK/1 HELLO nonce=<hex> [auth=<hex>]\n
pub const OK: &[u8] = b"DLNK/1 OK\n";

/// Parse `host[:port]` including IPv6 forms like `[::1]:21116`.
/// Returns (host, port) where host has no brackets. If no port, uses `default_port`.
#[must_use]
pub fn parse_host_port(input: &str, default_port: u16) -> (String, u16) {
    // IPv6 with brackets
    if let Some(rest) = input.strip_prefix('[') {
        if let Some((h, tail)) = rest.split_once(']') {
            if let Some(tail) = tail.strip_prefix(':') {
                if let Ok(p) = tail.parse::<u16>() {
                    return (h.to_string(), p);
                }
            }
            return (h.to_string(), default_port);
        }
    }
    // IPv4 or hostname
    if let Some((h, p)) = input.rsplit_once(':') {
        if p.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(port) = p.parse::<u16>() {
                return (h.to_string(), port);
            }
        }
    }
    (input.to_string(), default_port)
}

/// Like `parse_host_port` but returns a detailed error on malformed input.
///
/// # Errors
/// Returns an error if the input is empty, has malformed IPv6 brackets, or the port is invalid.
pub fn parse_host_port_checked(
    input: &str,
    default_port: u16,
) -> Result<(String, u16), ProtoError> {
    if input.is_empty() {
        return Err(ProtoError::InvalidAddress("empty".into()));
    }
    // Detect probable IPv6 without brackets
    if input.contains(':') && !input.starts_with('[') {
        let colon_count = input.chars().filter(|&c| c == ':').count();
        if colon_count > 1 {
            return Err(ProtoError::InvalidAddress(format!(
                "IPv6 addresses must be wrapped in []: [{input}]:<port>"
            )));
        }
    }
    // Bracketed IPv6
    if let Some(rest) = input.strip_prefix('[') {
        if let Some((h, tail)) = rest.split_once(']') {
            if h.is_empty() {
                return Err(ProtoError::InvalidAddress("empty host".into()));
            }
            if tail.is_empty() {
                return Ok((h.to_string(), default_port));
            }
            let tail = tail
                .strip_prefix(':')
                .ok_or_else(|| ProtoError::InvalidAddress("missing ':' after ']'".into()))?;
            let port: u16 = tail
                .parse()
                .map_err(|_| ProtoError::InvalidAddress(format!("invalid port: {tail}")))?;
            return Ok((h.to_string(), port));
        }
        return Err(ProtoError::InvalidAddress("missing closing ']'".into()));
    }
    // IPv4/hostname
    if let Some((h, p)) = input.rsplit_once(':') {
        if h.is_empty() {
            return Err(ProtoError::InvalidAddress("empty host".into()));
        }
        if !p.chars().all(|c| c.is_ascii_digit()) {
            return Err(ProtoError::InvalidAddress(format!("invalid port: {p}")));
        }
        let port: u16 = p
            .parse()
            .map_err(|_| ProtoError::InvalidAddress(format!("invalid port: {p}")))?;
        return Ok((h.to_string(), port));
    }
    Ok((input.to_string(), default_port))
}

/// Generate a 16-byte random nonce, return as hex string.
#[must_use]
pub fn gen_nonce_hex() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Compute HMAC-SHA256 over "nonce=<hex>" with the provided shared key bytes.
///
/// # Panics
/// This function will not panic for valid inputs; HMAC-SHA256 accepts keys of any size.
#[must_use]
pub fn hmac_nonce_hex(nonce_hex: &str, key: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(format!("nonce={nonce_hex}").as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Minimal custom handshake over plain TCP: send "DLNK/1 HELLO\n" and expect "DLNK/1 OK\n".
/// Minimal custom handshake over plain TCP: send "DLNK/1 HELLO\n" and expect "DLNK/1 OK\n".
///
/// # Errors
/// Returns an error on connect, timeout, or protocol mismatch.
pub async fn handshake_tcp(
    host: &str,
    port: u16,
    timeout_ms: u64,
    auth_key: Option<&[u8]>,
) -> Result<(), ProtoError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let addr = format!("{host}:{port}");
    let dur = Duration::from_millis(timeout_ms);
    let mut stream =
        match tokio::time::timeout(dur, tokio::net::TcpStream::connect(addr.clone())).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => return Err(ProtoError::TcpConnect { addr, source: e }),
            Err(_) => return Err(ProtoError::Timeout(timeout_ms)),
        };
    let nonce = gen_nonce_hex();
    let hello = if let Some(key) = auth_key {
    let auth = hmac_nonce_hex(&nonce, key);
    format!("{HELLO_PFX} nonce={nonce} auth={auth}\n")
    } else {
    format!("{HELLO_PFX} nonce={nonce}\n")
    };
    tokio::time::timeout(dur, stream.write_all(hello.as_bytes()))
        .await
        .map_err(|_| ProtoError::Timeout(timeout_ms))
        .and_then(|r| {
            r.map_err(|e| ProtoError::Io {
                ctx: "write",
                source: e,
            })
        })?;
    let mut buf = [0u8; 32];
    let n = tokio::time::timeout(dur, stream.read(&mut buf))
        .await
        .map_err(|_| ProtoError::Timeout(timeout_ms))
        .and_then(|r| {
            r.map_err(|e| ProtoError::Io {
                ctx: "read",
                source: e,
            })
        })?;
    if n == 0 {
        return Err(ProtoError::Protocol("eof"));
    }
    if &buf[..n] == OK {
        return Ok(());
    }
    Err(ProtoError::Protocol("unexpected response"))
}

/// Minimal custom handshake over TLS: same text protocol layered on TLS.
#[cfg(feature = "tls-client")]
///
/// # Errors
/// Returns an error on connect, TLS handshake, timeout, or protocol mismatch.
pub async fn handshake_tls(
    host: &str,
    port: u16,
    timeout_ms: u64,
    sni: Option<&str>,
    auth_key: Option<&[u8]>,
    ca_pem: Option<&[u8]>,
) -> Result<(), ProtoError> {
    use rustls::pki_types::ServerName;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_rustls::{rustls, TlsConnector};

    let addr = format!("{host}:{port}");
    let dur = Duration::from_millis(timeout_ms);

    let mut root_store = rustls::RootCertStore::empty();
    if let Ok(certs) = rustls_native_certs::load_native_certs() {
        for cert in certs {
            let _ = root_store.add(cert);
        }
    }
    if let Some(pem) = ca_pem {
        for c in rustls_pemfile::certs(&mut std::io::Cursor::new(pem)).flatten() {
            let _ = root_store.add(c);
        }
    }
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));

    let tcp = match tokio::time::timeout(dur, tokio::net::TcpStream::connect(addr.clone())).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(ProtoError::TcpConnect { addr, source: e }),
        Err(_) => return Err(ProtoError::Timeout(timeout_ms)),
    };
    let sni_val = sni.unwrap_or(host).to_string();
    let server_name =
        ServerName::try_from(sni_val.clone()).map_err(|_| ProtoError::InvalidSni(sni_val))?;
    let mut tls = match tokio::time::timeout(dur, connector.connect(server_name, tcp)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            return Err(ProtoError::TlsHandshake {
                addr,
                source: Box::new(e),
            })
        }
        Err(_) => return Err(ProtoError::Timeout(timeout_ms)),
    };

    let nonce = gen_nonce_hex();
    let hello = if let Some(key) = auth_key {
    let auth = hmac_nonce_hex(&nonce, key);
    format!("{HELLO_PFX} nonce={nonce} auth={auth}\n")
    } else {
    format!("{HELLO_PFX} nonce={nonce}\n")
    };
    tokio::time::timeout(dur, tls.write_all(hello.as_bytes()))
        .await
        .map_err(|_| ProtoError::Timeout(timeout_ms))
        .and_then(|r| {
            r.map_err(|e| ProtoError::Io {
                ctx: "write",
                source: e,
            })
        })?;
    let mut buf = [0u8; 32];
    let n = tokio::time::timeout(dur, tls.read(&mut buf))
        .await
        .map_err(|_| ProtoError::Timeout(timeout_ms))
        .and_then(|r| {
            r.map_err(|e| ProtoError::Io {
                ctx: "read",
                source: e,
            })
        })?;
    if n == 0 {
        return Err(ProtoError::Protocol("eof"));
    }
    if &buf[..n] == OK {
        return Ok(());
    }
    Err(ProtoError::Protocol("unexpected response"))
}

#[cfg(not(feature = "tls-client"))]
#[allow(clippy::unused_async, clippy::missing_errors_doc)]
pub async fn handshake_tls(
    _host: &str,
    _port: u16,
    _timeout_ms: u64,
    _sni: Option<&str>,
    _auth_key: Option<&[u8]>,
    _ca_pem: Option<&[u8]>,
) -> Result<(), ProtoError> {
    Err(ProtoError::InvalidSni("tls-client feature disabled".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[tokio::test]
    async fn test_handshake_tcp_ok() {
        // Start mock server
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 32];
                let n = sock.read(&mut buf).await.unwrap();
                let line = String::from_utf8_lossy(&buf[..n]);
                if line.starts_with(HELLO_PFX) {
                    let _ = sock.write_all(OK).await;
                }
            }
        });

        let res = handshake_tcp("127.0.0.1", addr.port(), 1000, None).await;
    assert!(res.is_ok(), "handshake should succeed: {res:?}");
    }

    #[test]
    fn test_parse_host_port_ipv6() {
        assert_eq!(
            parse_host_port("[::1]:21116", 1),
            ("::1".to_string(), 21116)
        );
        assert_eq!(
            parse_host_port("[2001:db8::1]", 55),
            ("2001:db8::1".to_string(), 55)
        );
        assert_eq!(
            parse_host_port("id.example", 80),
            ("id.example".to_string(), 80)
        );
        assert_eq!(
            parse_host_port("10.0.0.1:1234", 80),
            ("10.0.0.1".to_string(), 1234)
        );
    }
}
