use proto::{gen_nonce_hex, hmac_nonce_hex, parse_host_port, parse_host_port_checked};

#[test]
fn parse_host_port_variants() {
    assert_eq!(parse_host_port("[::1]:21116", 1), ("::1".into(), 21116));
    assert_eq!(parse_host_port("id.example", 80), ("id.example".into(), 80));
    assert_eq!(
        parse_host_port_checked("10.0.0.1:21117", 1).unwrap(),
        ("10.0.0.1".into(), 21117)
    );
    assert!(parse_host_port_checked("[::1]", 21116).is_ok());
    assert!(parse_host_port_checked("[::1]:21116", 1).is_ok());
}

#[test]
fn nonce_and_hmac_shapes() {
    let nonce = gen_nonce_hex();
    assert_eq!(nonce.len(), 32); // 16 bytes hex-encoded
    let mac = hmac_nonce_hex(&nonce, b"key");
    assert_eq!(mac.len(), 64); // 32 bytes hex-encoded
}
