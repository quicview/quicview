use proto::{handshake_tcp, hmac_nonce_hex, HELLO_PFX, OK};

#[test]
fn hmac_handshake_end_to_end() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };
        // Start server on ephemeral port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        // Spawn the server task
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 1024];
            let n = sock.read(&mut buf).await.unwrap();
            let line = String::from_utf8_lossy(&buf[..n]);
            if line.starts_with(HELLO_PFX) {
                // Extract nonce and auth
                let mut nonce: Option<String> = None;
                let mut auth: Option<String> = None;
                for part in line.split_whitespace() {
                    if let Some(v) = part.strip_prefix("nonce=") {
                        nonce = Some(v.trim_end_matches('\n').to_string());
                    }
                    if let Some(v) = part.strip_prefix("auth=") {
                        auth = Some(v.trim_end_matches('\n').to_string());
                    }
                }
                let ok = match (nonce, auth) {
                    (Some(n), Some(a)) => a == hmac_nonce_hex(&n, b"secret"),
                    _ => false,
                };
                if ok {
                    let _ = sock.write_all(OK).await;
                }
            }
        });

        // Client handshake
    let res = handshake_tcp("127.0.0.1", port, 1500, Some(b"secret")).await;
    assert!(res.is_ok(), "handshake ok: {res:?}");

        // Ensure server task finishes
        let _ = server.await;
    });
}
