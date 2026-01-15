#![cfg_attr(all(feature = "desktop", not(debug_assertions)), windows_subsystem = "windows")]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
#[cfg(feature = "desktop")]
use tauri::Manager;
#[cfg(all(feature = "desktop", feature = "tray"))]
use tauri::{SystemTray, CustomMenuItem, SystemTrayMenu, SystemTrayEvent};
#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::io::Write as _;

#[tauri::command]
#[cfg(feature = "desktop")]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(feature = "desktop")]
fn find_static_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    // 1) Allow override via env (useful for packaging or custom paths)
    if let Ok(p) = std::env::var("quicview_LEPTOS_DIST") {
        let pb = PathBuf::from(p);
        if pb.exists() { return Some(pb); }
    }
    // 2) Packaged resources (when bundled): resources/leptos-web-dist/**
    if let Some(res_dir) = app.path_resolver().resolve_resource("leptos-web-dist") {
        if res_dir.exists() { return Some(res_dir); }
    }
    // 3) Dev path from the Tauri crate directory: src-tauri/../../leptos-web/dist
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../leptos-web/dist");
    if dev.exists() { return Some(dev); }
    None
}

#[cfg(feature = "desktop")]
fn send_http_post(addr: SocketAddr, path: String, token: Option<String>) {
    std::thread::spawn(move || {
        use std::io::Write;
        let mut stream = match std::net::TcpStream::connect(addr) { Ok(s) => s, Err(_) => return };
        let mut req = format!("POST {} HTTP/1.1\r\nHost: {}\r\n", path, addr);
        if let Some(t) = token.as_deref() { req.push_str(&format!("Authorization: Bearer {}\r\n", t)); }
        req.push_str("Content-Length: 0\r\nConnection: close\r\n\r\n");
        let _ = stream.write_all(req.as_bytes());
        let _ = stream.flush();
    });
}

#[cfg(feature = "desktop")]
#[derive(Clone)]
struct AppState {
    addr: SocketAddr,
    token: Option<String>,
}

#[cfg(all(feature = "desktop", feature = "tray"))]
fn build_tray() -> SystemTray {
    let open = CustomMenuItem::new("open", "Open");
    let start = CustomMenuItem::new("start", "Start");
    let stop = CustomMenuItem::new("stop", "Stop");
    let runlogin = CustomMenuItem::new("run_at_login", "Run at Login");
    let quit = CustomMenuItem::new("quit", "Quit");
    let menu = SystemTrayMenu::new().add_item(open).add_item(start).add_item(stop).add_item(runlogin).add_item(quit);
    SystemTray::new().with_menu(menu)
}

#[cfg(feature = "desktop")]
fn main() {
    let mut builder = tauri::Builder::new();
    #[cfg(all(feature = "desktop", feature = "tray"))]
    {
        builder = builder
        .system_tray(build_tray())
        .on_system_tray_event(|app, event| match event {
            SystemTrayEvent::MenuItemClick { id, .. } => {
                let state_opt = app.try_state::<AppState>().map(|s| s.clone());
                match id.as_str() {
                    "open" => {
                        if let Some(w) = app.get_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "start" => {
                        if let Some(st) = state_opt { send_http_post(st.addr, "/start".to_string(), st.token.clone()); }
                    }
                    "stop" => {
                        if let Some(st) = state_opt { send_http_post(st.addr, "/stop".to_string(), st.token.clone()); }
                    }
                    "quit" => {
                        std::process::exit(0);
                    }
                    #[cfg(target_os = "macos")]
                    "run_at_login" => {
                        // Toggle run-at-login by creating/removing LaunchAgent plist in ~/Library/LaunchAgents/
                        if let Some(app_handle) = app.get_window("main").map(|w| w.app_handle()) {
                            let bundle_id = app_handle.config().tauri.bundle.identifier.clone();
                            let plist_name = format!("{}.quicview.client.plist", bundle_id.replace('/', "."));
                            let home = std::env::var("HOME").unwrap_or_default();
                            let agents_dir = PathBuf::from(&home).join("Library/LaunchAgents");
                            let plist_path = agents_dir.join(&plist_name);
                            if plist_path.exists() {
                                let _ = fs::remove_file(&plist_path);
                            } else {
                                let _ = fs::create_dir_all(&agents_dir);
                                // Build a simple plist to run the app bundle at login
                                let exec = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("/usr/bin/true"));
                                let plist = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{}</string>
<key>ProgramArguments</key><array><string>{}</string></array>
<key>RunAtLoad</key><true/>
</dict></plist>
"#, bundle_id, exec.display());
                                if let Ok(mut f) = fs::File::create(&plist_path) {
                                    let _ = f.write_all(plist.as_bytes());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        });
    }
    builder
        .on_window_event(|event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event.event() {
                api.prevent_close();
                let _ = event.window().hide();
            }
        })
        .invoke_handler(tauri::generate_handler![get_version])
        .setup(|app| {
            // Start QuicView client HTTP UI in-process
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            // Choose a port (try 21180 then fall back to 0)
            let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 21180);
            let static_dir = find_static_dir(&app.handle());
            if let Some(ref p) = static_dir { eprintln!("tauri: serving static UI from {}", p.display()); }
            let token: Option<String> = None;
            // Start server and possibly retry on port conflict
            let (addr, _handle) = {
                let (addr, handle) = rt.block_on(async move {
                    let client = client::core::Client::new();
            let defaults = client::http_ui::StreamConfig { default_width: 1280, default_height: 720, default_fps: 10, default_quality: 70 };
                    match client::http_ui::serve(bind, client.clone(), token.clone(), static_dir.clone(), Some(defaults), None, None, None, None).await {
                        Ok(h) => (h.addr, h),
                        Err(e) => {
                            eprintln!("failed to bind {}: {} — retrying with random port", bind, e);
                            let mut bind2 = bind; bind2.set_port(0);
                let defaults2 = client::http_ui::StreamConfig { default_width: 1280, default_height: 720, default_fps: 10, default_quality: 70 };
                            let h = client::http_ui::serve(bind2, client, token, static_dir, Some(defaults2), None, None, None, None).await.expect("http ui serve");
                            (h.addr, h)
                        }
                    }
                });
                (addr, handle)
            };

            // Share state for tray actions
            app.manage(AppState { addr, token: None });

            // Create main window pointing to the server URL
            let url = format!("http://{}/", addr);
            tauri::WindowBuilder::new(app, "main", tauri::WindowUrl::External(url.parse().unwrap()))
                .title("QuicView")
                .inner_size(1200.0, 800.0)
                .build()?;
            Ok(())
        })
    .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(not(feature = "desktop"))]
fn main() {
    // No-op binary when desktop feature is disabled; present for workspace compatibility.
    eprintln!("tauri-client binary is disabled; enable with --features desktop");
}
