#![cfg(feature = "http-ui")]

use client::http_ui;
use client::core;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn http_request(addr: SocketAddr, req: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(req.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await.unwrap();
    let resp = String::from_utf8_lossy(&buf[..n]).into_owned();
    let mut lines = resp.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let body = resp.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (code, body)
}

async fn http_post_json(addr: SocketAddr, path: &str, token: Option<&str>, body: &str) -> (u16, String) {
    let mut headers = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\n",
        path, addr
    );
    if let Some(t) = token {
        headers.push_str(&format!("Authorization: Bearer {}\r\n", t));
    }
    let clen = body.as_bytes().len();
    headers.push_str(&format!("Content-Length: {}\r\nConnection: close\r\n\r\n", clen));
    let req = format!("{}{}", headers, body);
    http_request(addr, &req).await
}

async fn http_request_with_origin(addr: SocketAddr, method: &str, path: &str, origin: &str, extra_headers: &str) -> (u16, String, String) {
    let req = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nOrigin: {}\r\n{}Connection: close\r\n\r\n",
        method, path, addr, origin, extra_headers
    );
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(req.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await.unwrap();
    let resp = String::from_utf8_lossy(&buf[..n]).into_owned();
    let mut lines = resp.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
        let aco = resp
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("access-control-allow-origin:"))
            .map(|s| s.to_string())
            .unwrap_or_default();
    let body = resp.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (code, aco, body)
}

#[tokio::test]
async fn http_status_start_stop_cycle() {
    let client = core::Client::new();
    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let handle = http_ui::serve(bind, client, None, None, None, None, None, None, None).await.unwrap();
    let addr = handle.addr;

    // GET /status should be false initially
    let (code, body) = http_request(
        addr,
        &format!(
            "GET /status HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            addr
        ),
    )
    .await;
    assert_eq!(code, 200);
    let val: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(val["running"], false);

    // POST /start -> 204
    let (code, _body) = http_request(
        addr,
        &format!(
            "POST /start HTTP/1.1\r\nHost: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            addr
        ),
    )
    .await;
    assert_eq!(code, 204);

    // GET /status -> true
    let (code, body) = http_request(
        addr,
        &format!(
            "GET /status HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            addr
        ),
    )
    .await;
    assert_eq!(code, 200);
    let val: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(val["running"], true);

    // POST /stop -> 204
    let (code, _body) = http_request(
        addr,
        &format!(
            "POST /stop HTTP/1.1\r\nHost: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            addr
        ),
    )
    .await;
    assert_eq!(code, 204);

    // GET /status -> false again
    let (code, body) = http_request(
        addr,
        &format!(
            "GET /status HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            addr
        ),
    )
    .await;
    assert_eq!(code, 200);
    let val: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(val["running"], false);

    handle.shutdown().await;
}

#[tokio::test]
async fn cors_allowlist_applies() {
    let client = core::Client::new();
    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let handle = http_ui::serve(bind, client, None, None, None, Some(vec!["http://allowed.local".into()]), None, None, None).await.unwrap();
    let addr = handle.addr;

    // Not allowed origin: no ACAO header
    let (code, aco, _body) = http_request_with_origin(addr, "OPTIONS", "/status", "http://not-allowed.local", "").await;
    assert_eq!(code, 204);
    assert_eq!(aco, "");

    // Allowed origin: ACAO should echo
    let (code, aco, _body) = http_request_with_origin(addr, "OPTIONS", "/status", "http://allowed.local", "").await;
    assert_eq!(code, 204);
    assert!(aco.contains("http://allowed.local"));

    handle.shutdown().await;
}

#[tokio::test]
async fn http_requires_auth_when_configured() {
    let client = core::Client::new();
    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let handle = http_ui::serve(bind, client, Some("secret".into()), None, None, None, None, None, None).await.unwrap();
    let addr = handle.addr;

    // POST /start without auth should be 401
    let (code, _body) = http_request(
        addr,
        &format!(
            "POST /start HTTP/1.1\r\nHost: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            addr
        ),
    )
    .await;
    assert_eq!(code, 401);

    // With Bearer token
    let req = format!(
        "POST /start HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        addr
    );
    let (code, _body) = http_request(addr, &req).await;
    assert_eq!(code, 204);

    handle.shutdown().await;
}

#[tokio::test]
async fn static_files_can_be_served() {
    use std::fs::{create_dir_all, write};
    use std::time::{SystemTime, UNIX_EPOCH};
    // Create a unique temp directory under system temp
    let base = std::env::temp_dir();
    let uniq = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = base.join(format!("quicview_test_static_{}", uniq));
    create_dir_all(&dir).unwrap();
    // Write index.html and a js file
    write(dir.join("index.html"), b"<html><body>ok</body></html>").unwrap();
    write(dir.join("app.js"), b"console.log('ok');").unwrap();

    let client = core::Client::new();
    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let handle = http_ui::serve(bind, client, None, Some(dir.clone()), None, None, None, None, None).await.unwrap();
    let addr = handle.addr;

    // GET /
    let (code, body) = http_request(
        addr,
        &format!(
            "GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            addr
        ),
    )
    .await;
    assert_eq!(code, 200);
    assert!(body.contains("ok"));

    // GET /app.js
    let (code, body) = http_request(
        addr,
        &format!(
            "GET /app.js HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            addr
        ),
    )
    .await;
    assert_eq!(code, 200);
    assert!(body.contains("console.log('ok')"));

    handle.shutdown().await;
}

#[tokio::test]
async fn mjpeg_stream_serves_frames() {
    let client = core::Client::new();
    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let handle = http_ui::serve(bind, client, Some("tok".into()), None, None, None, None, None, None).await.unwrap();
    let addr = handle.addr;

    // Start stream with Authorization header and read until first boundary appears
    let req = format!(
        "GET /stream.mjpeg HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer tok\r\nConnection: close\r\n\r\n",
        addr
    );
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(req.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
    let mut buf = vec![0u8; 8192];
    let mut acc = String::new();
    let mut status_ok = false;
    for _ in 0..10u8 {
        let n = stream.read(&mut buf).await.unwrap();
        if n == 0 { break; }
        acc.push_str(&String::from_utf8_lossy(&buf[..n]));
        if !status_ok {
            // Parse status from accumulated data
            if let Some(line) = acc.split("\r\n").next() {
                if let Some(code) = line.split_whitespace().nth(1).and_then(|s| s.parse::<u16>().ok()) {
                    assert_eq!(code, 200);
                    status_ok = true;
                }
            }
        }
        if acc.contains("--frame") { break; }
        // give the server a brief moment to push the first frame
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let body = acc.split("\r\n\r\n").nth(1).unwrap_or("");
    assert!(body.contains("--frame"));
    assert!(body.contains("Content-Type: image/jpeg"));
}

#[tokio::test]
async fn http_get_status_requires_auth_when_configured() {
    let client = core::Client::new();
    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let handle = http_ui::serve(bind, client, Some("tok".into()), None, None, None, None, None, None).await.unwrap();
    let addr = handle.addr;

    // GET /status without auth should be 401
    let (code, _body) = http_request(
        addr,
        &format!(
            "GET /status HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            addr
        ),
    )
    .await;
    assert_eq!(code, 401);

    // With Bearer token should be 200
    let req = format!(
        "GET /status HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer tok\r\nConnection: close\r\n\r\n",
        addr
    );
    let (code, body) = http_request(addr, &req).await;
    assert_eq!(code, 200);
    let val: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(val.get("running").is_some());

    handle.shutdown().await;
}

#[tokio::test]
async fn input_endpoints_basic() {
    let client = core::Client::new();
    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let handle = http_ui::serve(bind, client, Some("tok".into()), None, None, None, None, None, None).await.unwrap();
    let addr = handle.addr;

    // POST /input/mouse without auth => 401
    let (code, _body) = http_post_json(addr, "/input/mouse", None, "{}").await;
    assert_eq!(code, 401);

    // With auth and minimal body
    let (code, _body) = http_post_json(addr, "/input/mouse", Some("tok"), "{\"x\":100.0,\"y\":100.0}").await;
    assert_eq!(code, 204);

    // Keyboard endpoint
    let (code, _body) = http_post_json(addr, "/input/key", Some("tok"), "{\"text\":\"a\"}").await;
    assert_eq!(code, 204);

    handle.shutdown().await;
}

#[tokio::test]
async fn sse_events_auth_and_stream() {
    let client = core::Client::new();
    let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let handle = http_ui::serve(bind, client, Some("tok".into()), None, None, None, None, None, None).await.unwrap();
    let addr = handle.addr;

    // Unauthorized should be 401
    let (code, _body) = http_request(
        addr,
        &format!(
            "GET /events HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            addr
        ),
    )
    .await;
    assert_eq!(code, 401);

    // Authorized should be 200 and include text/event-stream content type and at least one event chunk
    let req = format!(
        "GET /events HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer tok\r\nConnection: close\r\n\r\n",
        addr
    );
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(req.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
    let mut buf = vec![0u8; 8192];
    let mut acc = String::new();
    let mut status_ok = false;
    for _ in 0..10u8 {
        let n = stream.read(&mut buf).await.unwrap();
        if n == 0 { break; }
        acc.push_str(&String::from_utf8_lossy(&buf[..n]));
        if !status_ok {
            if let Some(line) = acc.split("\r\n").next() {
                if let Some(code) = line.split_whitespace().nth(1).and_then(|s| s.parse::<u16>().ok()) {
                    assert_eq!(code, 200);
                    status_ok = true;
                }
            }
            assert!(acc.to_ascii_lowercase().contains("content-type: text/event-stream"));
        }
        if acc.contains("event: status") || acc.contains(":\n\n") { break; }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let body = acc.split("\r\n\r\n").nth(1).unwrap_or("");
    assert!(body.contains("event: status") || body.contains(":\n\n"));

    handle.shutdown().await;
}
