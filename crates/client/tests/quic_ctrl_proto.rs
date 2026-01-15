#![cfg(all(feature = "quic-ctrl"))]

use transport::quic_ctrl as tc;
use transport::quic_ctrl::CtrlClientConfig;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn quic_ctrl_proto_end_to_end_flow() {
    // Start server
    let bind = "127.0.0.1:0".parse().unwrap();
    let (addr, join, tx_cmd, tx_sig) = tc::start_ctrl_server(bind, "tok".into()).await.unwrap();

    // Start client with insecure TLS (dev/local), default cfg
    let cfg = CtrlClientConfig::default();
    let (_h, mut rx) = tc::run_ctrl_client(addr, "tok".into(), cfg).await.unwrap();

    // Connected
    let ev = timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap();
    matches!(ev, tc::CtrlEvent::Connected);

    // Push a command and see it arrive
    let _ = tx_cmd.send(tc::Cmd::Start);
    let ev = timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap();
    match ev { tc::CtrlEvent::Command(tc::Cmd::Start) => {}, other => panic!("unexpected: {:?}", other) }

    // Server asks for reauth, then client should emit AuthRenewed after ack
    let _ = tx_sig.send(tc::ServerSignal::ReauthRequest);
    let ev = timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap();
    // We may receive a Ping Liveness before AuthRenewed; loop until we see AuthRenewed
    let mut got_auth = matches!(ev, tc::CtrlEvent::AuthRenewed);
    let start = std::time::Instant::now();
    while !got_auth && start.elapsed() < Duration::from_secs(3) {
        if let Ok(Some(next)) = timeout(Duration::from_millis(500), rx.recv()).await { got_auth = matches!(next, tc::CtrlEvent::AuthRenewed); }
    }
    assert!(got_auth, "did not observe AuthRenewed");

    // Server emits an error; client should surface it as CtrlEvent::Error
    let _ = tx_sig.send(tc::ServerSignal::Error { code: "E_TEST".into(), message: "oops".into() });
    // Allow either Liveness ping or Error first
    let mut saw_error = false;
    let start = std::time::Instant::now();
    while !saw_error && start.elapsed() < Duration::from_secs(3) {
        if let Ok(Some(ev)) = timeout(Duration::from_millis(500), rx.recv()).await { if let tc::CtrlEvent::Error(s) = ev { if s.contains("E_TEST") { saw_error = true; } } }
    }
    assert!(saw_error, "did not observe error from server");

    // Close
    let _ = tx_sig.send(tc::ServerSignal::Close("bye".into()));
    // We expect a disconnect soon after
    let mut saw_disc = false;
    let start = std::time::Instant::now();
    while !saw_disc && start.elapsed() < Duration::from_secs(3) {
        if let Ok(Some(ev)) = timeout(Duration::from_millis(500), rx.recv()).await { saw_disc = matches!(ev, tc::CtrlEvent::Disconnected(_)); }
    }
    assert!(saw_disc, "did not observe disconnect after close");

    // Cleanup
    join.abort();
}
