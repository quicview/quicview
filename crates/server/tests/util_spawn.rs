//! Test utility: spawns a tiny TCP listener child using `sh -c` that waits and exits.
//! We avoid relying on external binaries; this uses `/bin/sh` which should be available on Unix.
//! This test module is only used by feature-gated tests.
#[cfg(unix)]
pub fn spawn_tcp_listener_script(port: u16, lifetime_ms: u64) -> (std::process::Command, String) {
    let script = format!(
        "python3 - <<'PY'\nimport socket, time\ns=socket.socket()\ns.bind(('127.0.0.1',{port}))\ns.listen(1)\ntime.sleep({lifetime_ms}/1000.0)\nPY\n"
    );
    let mut cmd = std::process::Command::new("/bin/sh");
    cmd.arg("-c").arg(script);
    (cmd, format!("127.0.0.1:{port}"))
}
