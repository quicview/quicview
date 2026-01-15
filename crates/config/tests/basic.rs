use config::{ClientPolicy, QuicViewConfig};
use std::fs;

#[test]
fn client_policy_defaults() {
    let p = ClientPolicy::default();
    assert!(p.require_consent);
    assert!(!p.allow_input_control);
    assert!(!p.allow_clipboard);
    assert!(!p.allow_file_transfer);
}

#[test]
fn load_from_file_roundtrip() {
    let tmp = std::env::temp_dir();
    let file = tmp.join("quicview_test_config.yml");
    let yaml = r#"
server:
  rendezvous_host: "id.example"
  relay_host: "relay.example"
  tls: false
client_policy:
  require_consent: true
  allow_input_control: false
  allow_clipboard: false
  allow_file_transfer: false
"#;
    fs::write(&file, yaml).expect("write test config");
    let cfg = QuicViewConfig::load_from_file(&file).expect("load config");
    assert_eq!(cfg.server.rendezvous_host, "id.example");
    assert_eq!(cfg.server.relay_host, "relay.example");
    assert!(!cfg.server.tls);
}
