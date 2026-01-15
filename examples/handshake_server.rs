use proto::{parse_host_port, hmac_nonce_hex, OK, HELLO_PFX};
use std::net::SocketAddr;
use tokio::{net::TcpListener, io::{AsyncReadExt, AsyncWriteExt}};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let host = std::env::args().nth(1).unwrap_or("127.0.0.1:21116".into());
    let key = std::env::var("QUICVIEW_KEY").ok();
    let (h, p) = parse_host_port(&host, 21116);
    let bind: SocketAddr = format!("{}:{}", h, p).parse()?;
    let listener = TcpListener::bind(bind).await?;
    eprintln!("listening on {}:{}", h, p);

    loop {
        let (mut sock, peer) = listener.accept().await?;
        let key = key.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 512];
            match sock.read(&mut buf).await {
                Ok(n) if n > 0 => {
                    let line = String::from_utf8_lossy(&buf[..n]);
                    if line.starts_with(HELLO_PFX) {
                        // Parse nonce=<hex> and optional auth=<hex>
                        let mut nonce: Option<String> = None;
                        let mut auth: Option<String> = None;
                        for part in line.split_whitespace() {
                            if let Some(v) = part.strip_prefix("nonce=") { nonce = Some(v.trim_end_matches('\n').to_string()); }
                            if let Some(v) = part.strip_prefix("auth=") { auth = Some(v.trim_end_matches('\n').to_string()); }
                        }
                        let ok = match (nonce, key) {
                            (Some(nonce_hex), Some(key_str)) => {
                                let expected = hmac_nonce_hex(&nonce_hex, key_str.as_bytes());
                                Some(expected) == auth
                            }
                            // If no key configured, accept without auth
                            (Some(_), None) => true,
                            _ => false,
                        };
                        if ok {
                            let _ = sock.write_all(OK).await;
                        }
                    }
                }
                _ => {}
            }
            let _ = sock.shutdown().await;
            eprintln!("peer {} handled", peer);
        });
    }
}
