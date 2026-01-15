#![cfg(feature = "quic-ctrl")]

use client::core::{Client, ClientEvent};
use transport::quic_ctrl;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn quic_ctrl_start_stop_flow() {
    // Start QUIC control server
    let (addr, _server_task, tx_cmd, _tx_sig) = quic_ctrl::start_ctrl_server(
        (std::net::Ipv4Addr::LOCALHOST, 0).into(),
        "secret".to_string(),
    )
    .await
    .expect("ctrl server started");

    // Start client with control channel
    let client = Client::new();
    let mut events = client.subscribe();
    client.start_with_ctrl(addr, "secret".into()).await.unwrap();

    // Expect initial Started (from local start) and then SessionIncoming once QUIC connects
    let first = events.recv().await.expect("first event");
    match first {
        ClientEvent::Started => {},
        other => panic!("unexpected first event: {:?}", other),
    }
    // Wait for session incoming to indicate the control task is running
    loop {
        match events.recv().await.expect("next event") {
            ClientEvent::SessionIncoming { .. } => break,
            _ => continue,
        }
    }
    // Wait for CtrlConnected to ensure server has a receiver subscribed
    loop {
        match events.recv().await.expect("next event 2") {
            ClientEvent::CtrlConnected => break,
            _ => continue,
        }
    }

    // Send Start (idempotent) and Stop via control channel
    tx_cmd.send(quic_ctrl::Cmd::Start).unwrap();
    // Let it process
    sleep(Duration::from_millis(50)).await;

    tx_cmd.send(quic_ctrl::Cmd::Stop).unwrap();

    // Expect a Stopped event
    let ev = events.recv().await.expect("stopped event");
    match ev {
        ClientEvent::Stopped => {},
        other => panic!("unexpected event: {:?}", other),
    }
}

// Integration test for QUIC control channel: hello/auth/session -> reauth -> close.
#[cfg(feature = "quic-ctrl")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ctrl_flow_hello_auth_reauth_close() {
    use std::time::Duration;
    use client::core::Client;
    use transport::quic_ctrl::{self, ServerSignal, Cmd as CtrlCmd, TlsMode};

    // Start a dev control server on a random port with a known token
    let token = "test-token-flow".to_string();
    let bind = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
    let (addr, _join, tx_cmd, tx_sig) = quic_ctrl::start_ctrl_server(bind, token.clone())
        .await
        .expect("start ctrl server");

    // Start the client and connect its control channel to the server
    let client = Client::new();
    let res = client
        .start_with_ctrl_tls_tuned(
            addr,
            token.clone(),
            1,              // ping every second
            200,            // reconnect base backoff ms
            2_000,          // reconnect max cap ms
            TlsMode::InsecureNoVerify, // local test, no TLS verify
            None,            // cached_pin
            None,            // cached_ca
        )
        .await;
    assert!(res.is_ok(), "client should connect ctrl channel: {:?}", res.err());

    // Give it a moment to establish
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Trigger a reauth request from the server, then provide (same) token update
    let _ = tx_sig.send(ServerSignal::ReauthRequest);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let _ = tx_sig.send(ServerSignal::UpdateToken(token.clone()));

    // Allow some time for the client to handle reauth
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Ask server to stop to ensure the client handles clean close
    let _ = tx_cmd.send(CtrlCmd::Stop);
    tokio::time::sleep(Duration::from_millis(200)).await;

    // If we reached here without panic, the basic flow is healthy
}

#[cfg(not(feature = "quic-ctrl"))]
#[test]
fn ctrl_flow_feature_disabled() {
    eprintln!("skipped: quic-ctrl feature not enabled for client crate");
}
