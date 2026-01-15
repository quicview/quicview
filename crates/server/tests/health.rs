use server::{run, ServerHandle};
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
async fn health_endpoint_ok() {
    let cfg = make_cfg();
    let ServerHandle { addr, .. } = run(&cfg).await.expect("start health server");

    // Connect raw TCP and do HTTP/1 request using hyper client-conn
    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let io = TokioIo::new(stream);
    let (mut sender, conn) = http1::handshake::<_, Empty<Bytes>>(io).await.expect("handshake");
    tokio::spawn(async move { let _ = conn.await; });

    let req = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .header("host", "localhost")
        .body(Empty::new())
        .unwrap();
    let resp = sender.send_request(req).await.expect("send");
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ready_endpoint_ok() {
    let cfg = make_cfg();
    let ServerHandle { addr, .. } = run(&cfg).await.expect("start server");

    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let io = TokioIo::new(stream);
    let (mut sender, conn) = http1::handshake::<_, Empty<Bytes>>(io).await.expect("handshake");
    tokio::spawn(async move { let _ = conn.await; });

    let req = Request::builder()
        .method(Method::GET)
        .uri("/ready")
        .header("host", "localhost")
        .body(Empty::new())
        .unwrap();
    let resp = sender.send_request(req).await.expect("send");
    assert_eq!(resp.status(), StatusCode::OK);
}
