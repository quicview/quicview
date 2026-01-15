use config::QuicViewConfig;

#[test]
fn loads_example_config() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/quicview.yaml");
    let cfg = QuicViewConfig::load_from_file(&path).expect("should load example config");
    // New config uses tls_config.enabled instead of tls
    assert!(cfg.server.effective_tls_enabled());
    assert!(cfg.client_policy.require_consent);
}
