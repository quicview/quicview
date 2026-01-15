use server::run;
use config::{QuicViewConfig, ServerConfig, ClientPolicy};
use hyper::{Request, Method, StatusCode};
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use http_body_util::Empty;
use bytes::Bytes;

fn make_cfg() -> QuicViewConfig {
    QuicViewConfig {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 21116,
            auth_token: None,
            tls_config: None,
            health_bind: Some("127.0.0.1".into()),
            health_port: Some(0),
            ..Default::default()
        },
        client_policy: ClientPolicy::default(),
    }
}

#[tokio::test]
async fn shutdown_stops_accepting() {
    let cfg = make_cfg();
    let handle = run(&cfg).await.expect("start server");
    let addr = handle.addr;

    // Smoke GET /health
    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let io = TokioIo::new(stream);
    let (mut sender, conn) = http1::handshake::<_, Empty<Bytes>>(io).await.expect("handshake");
    tokio::spawn(async move { let _ = conn.await; });
    let req = Request::builder().method(Method::GET).uri("/health").header("host", "localhost").body(Empty::new()).unwrap();
    let resp = sender.send_request(req).await.expect("send");
    assert_eq!(resp.status(), StatusCode::OK);

    // Shutdown
    handle.shutdown().await;

    // After shutdown, connecting should fail within a short timeout
    let res = tokio::time::timeout(std::time::Duration::from_millis(300), tokio::net::TcpStream::connect(addr)).await;
    assert!(matches!(res, Ok(Err(_)) | Err(_)), "expected connect to fail after shutdown, got: {:?}", res);
}
