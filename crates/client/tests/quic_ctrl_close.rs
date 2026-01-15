#![cfg(feature = "quic-ctrl")]

use client::core::{Client, ClientEvent};
use transport::quic_ctrl;

#[tokio::test]
async fn quic_ctrl_close_flow() {
    // Start QUIC control server
    let (addr, _server_task, _tx_cmd, tx_sig) = quic_ctrl::start_ctrl_server(
        (std::net::Ipv4Addr::LOCALHOST, 0).into(),
        "secret".to_string(),
    )
    .await
    .expect("ctrl server started");

    // Start client with control channel
    let client = Client::new();
    let mut events = client.subscribe();
    client.start_with_ctrl(addr, "secret".into()).await.unwrap();

    // Drain until connected
    loop {
        match events.recv().await.expect("event") {
            ClientEvent::CtrlConnected => break,
            _ => continue,
        }
    }

    // Ask server to close the session
    tx_sig
        .send(quic_ctrl::ServerSignal::Close("test close".into()))
        .unwrap();

    // Expect a disconnect event soon
    let mut got = false;
    for _ in 0..10 {
        match events.recv().await.expect("event2") {
            ClientEvent::CtrlDisconnected { .. } => {
                got = true;
                break;
            }
            _ => continue,
        }
    }
    assert!(got, "expected CtrlDisconnected after server Close");
}
