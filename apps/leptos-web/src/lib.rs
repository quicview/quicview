use leptos::*;
use leptos::html;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;
#[cfg(target_arch = "wasm32")]
use gloo_timers::future::TimeoutFuture;
#[cfg(not(target_arch = "wasm32"))]
fn spawn_local<F: std::future::Future<Output = ()> + 'static>(_f: F) {}

#[component]
pub fn App() -> impl IntoView {
    // Core signals
    let (status, set_status) = create_signal(String::from("unknown"));
    let (base_url, set_base_url) = create_signal(String::from("http://127.0.0.1:21180"));
    let (token, set_token) = create_signal(String::new());
    // Stream controls
    let (w, set_w) = create_signal(320u32);
    let (h, set_h) = create_signal(180u32);
    let (fps, set_fps) = create_signal(5u32);
    let (q, set_q) = create_signal(70u8);
    // FPS indicator
    let (fps_actual, _set_fps_actual) = create_signal(0u32);
    // Toasts
    #[derive(Clone)]
    struct Toast { msg: String, kind: &'static str }
    let (toasts, set_toasts) = create_signal::<Vec<Toast>>(vec![]);
    let push_toast = move |msg: String, kind: &'static str| set_toasts.update(|v| v.push(Toast{msg, kind}));
    // Clipboard
    let (clip, set_clip) = create_signal(String::new());
    // Remote control
    let (rc_enabled, set_rc_enabled) = create_signal(false);
    // Consent and control status
    let (consent_allowed, set_consent_allowed) = create_signal(false);
    let (ctrl_json, set_ctrl_json) = create_signal(String::from("null"));
    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Default)]
    struct CtrlConfig {
        connected: bool,
        last_disconnect: Option<String>,
        reconnects: u64,
        last_connected_at: Option<u64>,
        last_disconnect_at: Option<u64>,
        last_error: Option<String>,
        last_attempt_at: Option<u64>,
        attempts: u64,
        ping_interval_secs: Option<u64>,
        backoff_base_ms: Option<u64>,
        backoff_max_ms: Option<u64>,
        tls: Option<CtrlTls>,
    }
    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Default)]
    struct CtrlTls { mode: String, sni: Option<String>, pin_sha256_hex: Option<String>, ca_pem_len: Option<usize> }
    #[derive(Clone, Debug, serde::Deserialize)]
    struct CtrlConfigResponse { ctrl: Option<CtrlConfig> }
    let (ctrl_cfg, _set_ctrl_cfg) = create_signal::<Option<CtrlConfig>>(None);
    let img_ref: NodeRef<html::Img> = create_node_ref();
    #[cfg(target_arch = "wasm32")]
    let overlay_ref: NodeRef<html::Div> = create_node_ref();
    #[cfg(target_arch = "wasm32")]
    let (viewer_has_focus, set_viewer_has_focus) = create_signal(false);
    #[cfg(target_arch = "wasm32")]
    let (_disp_w, set_disp_w) = create_signal(0.0f64);
    #[cfg(target_arch = "wasm32")]
    let (_disp_h, set_disp_h) = create_signal(0.0f64);
    // Registry
    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
    struct ServerEntry { name: String, url: String, token: String }
    let (servers, set_servers) = create_signal::<Vec<ServerEntry>>(vec![]);
    let (selected_server, set_selected_server) = create_signal::<Option<usize>>(None);
    // Displays
    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
    struct DisplayInfo { id: u32, x: i32, y: i32, width: u32, height: u32, is_main: bool }
    let (displays, set_displays) = create_signal::<Vec<DisplayInfo>>(vec![]);
    let (selected_display, set_selected_display) = create_signal::<Option<u32>>(None);

    // Load persisted settings (wasm only) and token from hash
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::window;
        if let Some(win) = window() {
            if let Ok(Some(storage)) = win.local_storage() {
                if let Ok(Some(val)) = storage.get_item("quicview.base_url") { if !val.is_empty() { set_base_url.set(val); } }
                if let Ok(Some(val)) = storage.get_item("quicview.token") { if !val.is_empty() { set_token.set(val); } }
                if let Ok(Some(val)) = storage.get_item("quicview.w") { if let Ok(n) = val.parse::<u32>() { set_w.set(n); } }
                if let Ok(Some(val)) = storage.get_item("quicview.h") { if let Ok(n) = val.parse::<u32>() { set_h.set(n); } }
                if let Ok(Some(val)) = storage.get_item("quicview.fps") { if let Ok(n) = val.parse::<u32>() { set_fps.set(n); } }
                if let Ok(Some(val)) = storage.get_item("quicview.q") { if let Ok(n) = val.parse::<u8>() { set_q.set(n); } }
                if let Ok(Some(val)) = storage.get_item("quicview.rc.enabled") { if val == "1" { set_rc_enabled.set(true); } }
                if let Ok(Some(val)) = storage.get_item("quicview.display.id") { if let Ok(n) = val.parse::<u32>() { set_selected_display.set(Some(n)); } }
                if let Ok(Some(val)) = storage.get_item("quicview.servers") { if !val.is_empty() { if let Ok(list) = serde_json::from_str::<Vec<ServerEntry>>(&val) { set_servers.set(list); } } }
                if let Ok(Some(val)) = storage.get_item("quicview.servers.selected") { if let Ok(idx) = val.parse::<usize>() { set_selected_server.set(Some(idx)); } }
            }
            if let Ok(loc) = win.location().hash() {
                if let Some(pos) = loc.find("token=") {
                    let t = &loc[pos + 6..];
                    let t = t.trim_start_matches('#');
                    if !t.is_empty() { if let Ok(decoded) = urlencoding::decode(t) { set_token.set(decoded.to_string()); } }
                }
            }
        }
    }

    // Persist settings on change (wasm only)
    #[cfg(target_arch = "wasm32")]
    {
        use leptos::signal_prelude::*;
        use web_sys::window;
        let _ = create_effect(move |_| {
            if let Some(win) = window() {
                if let Ok(Some(storage)) = win.local_storage() {
                    let _ = storage.set_item("quicview.base_url", &base_url.get());
                    let _ = storage.set_item("quicview.token", &token.get());
                    let _ = storage.set_item("quicview.w", &w.get().to_string());
                    let _ = storage.set_item("quicview.h", &h.get().to_string());
                    let _ = storage.set_item("quicview.fps", &fps.get().to_string());
                    let _ = storage.set_item("quicview.q", &q.get().to_string());
                    let _ = storage.set_item("quicview.rc.enabled", if rc_enabled.get() { "1" } else { "0" });
                    if let Some(id) = selected_display.get() { let _ = storage.set_item("quicview.display.id", &id.to_string()); }
                    if let Ok(json) = serde_json::to_string(&servers.get()) { let _ = storage.set_item("quicview.servers", &json); }
                    if let Some(idx) = selected_server.get() { let _ = storage.set_item("quicview.servers.selected", &idx.to_string()); }
                }
            }
        });
    }

    // Helpers
    let refresh = move || {
        let base = base_url.get_untracked();
        let tok = token.get_untracked();
        spawn_local(async move {
            let url = format!("{}/status", base);
            match fetch_json_auth::<serde_json::Value>(&url, tok.as_deref_filter_empty()).await {
                Ok(v) => {
                    let running = v.get("running").and_then(|x| x.as_bool()).unwrap_or(false);
                    set_status.set(if running { "running".into() } else { "stopped".into() });
                    set_consent_allowed.set(v.get("consent_allowed").and_then(|x| x.as_bool()).unwrap_or(false));
                    if let Some(c) = v.get("ctrl") { set_ctrl_json.set(serde_json::to_string_pretty(c).unwrap_or_else(|_| "null".into())); } else { set_ctrl_json.set("null".into()); }
                }
                Err(_) => {
                    set_status.set("error".into());
                }
            }
        });
    };

    // Kick off an initial refresh and refresh on base_url/token changes
    {
        let refresh_now = refresh.clone();
        create_effect(move |_| {
            // read signals so effect tracks them
            let _ = base_url.get();
            let _ = token.get();
            refresh_now();
        });
    }

    // Periodically poll /ctrl/config (wasm only) and update compact status panel.
    // Uses a simple cancel flag so changing base_url/token stops the prior loop.
    #[cfg(target_arch = "wasm32")]
    {
        use leptos::signal_prelude::*;
        let base_url = base_url.clone();
        let token = token.clone();
        let set_ctrl_cfg = _set_ctrl_cfg.clone();
        let set_ctrl_json = set_ctrl_json.clone();
        create_effect(move |_| {
            // Track base URL and token, restart loop when they change
            let base = base_url.get();
            let tok = token.get();
            let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let c2 = cancelled.clone();
            // When this effect re-runs, drop the old cancelled flag by cloning a new Arc
            spawn_local(async move {
                loop {
                    if c2.load(std::sync::atomic::Ordering::Relaxed) { break; }
                    let url = format!("{}/ctrl/config", base);
                    match fetch_json_auth::<CtrlConfigResponse>(&url, tok.as_deref_filter_empty()).await {
                        Ok(resp) => {
                            if let Some(cfg) = resp.ctrl {
                                set_ctrl_json.set(serde_json::to_string_pretty(&cfg).unwrap_or_else(|_| "null".into()));
                                set_ctrl_cfg.set(Some(cfg));
                            } else {
                                set_ctrl_json.set("null".into());
                                set_ctrl_cfg.set(None);
                            }
                        }
                        Err(_) => {
                            set_ctrl_cfg.set(None);
                        }
                    }
                    TimeoutFuture::new(2000).await;
                }
            });
            on_cleanup(move || {
                cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
            });
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn map_coords(client_x: f64, client_y: f64, img_el: &web_sys::HtmlImageElement, w: u32, h: u32) -> (f64, f64) {
        use wasm_bindgen::JsCast;
        let el: &web_sys::HtmlElement = img_el.unchecked_ref();
        let rect = el.get_bounding_client_rect();
        let rx = client_x - rect.left();
        let ry = client_y - rect.top();
        let disp_w = rect.width().max(1.0);
        let disp_h = rect.height().max(1.0);
        let x = (rx / disp_w) * (w as f64);
        let y = (ry / disp_h) * (h as f64);
        (x, y)
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn map_coords(_client_x: f64, _client_y: f64, _img_el: &(), _w: u32, _h: u32) -> (f64, f64) { (0.0, 0.0) }

    #[cfg(target_arch = "wasm32")]
    let send_mouse = move |payload: serde_json::Value| {
        let base = base_url.get_untracked();
        let tok = token.get_untracked();
        spawn_local(async move {
            let _ = fetch_post_json(&format!("{}/input/mouse", base), tok.as_deref_filter_empty(), payload).await;
        });
    };
    #[cfg(target_arch = "wasm32")]
    let send_key = move |payload: serde_json::Value| {
        let base = base_url.get_untracked();
        let tok = token.get_untracked();
        spawn_local(async move {
            let _ = fetch_post_json(&format!("{}/input/key", base), tok.as_deref_filter_empty(), payload).await;
        });
    };

    let stream_src = move || {
        let base = base_url.get();
        let wv = w.get();
        let hv = h.get();
        let fpsv = fps.get();
        let qv = q.get();
        let mut url = format!("{}/stream.mjpeg?w={}&h={}&fps={}&q={}", base, wv, hv, fpsv, qv);
        let tok = token.get();
        if !tok.is_empty() { url.push_str("&token="); url.push_str(&urlencoding::encode(&tok)); }
        url
    };

    // Registry editor fields
    let (reg_name, set_reg_name) = create_signal(String::new());
    let (reg_url, set_reg_url) = create_signal(String::new());
    let (reg_tok, set_reg_tok) = create_signal(String::new());

    // Overlay (wasm only)
    #[cfg(target_arch = "wasm32")]
    let overlay_view = {
        view!{
            <div
                node_ref=overlay_ref tabindex="0"
                on:click=move |_| {
                    set_viewer_has_focus.set(true);
                    if rc_enabled.get() {
                        use wasm_bindgen::JsCast;
                        if let Some(div) = overlay_ref.get() {
                            let el: &web_sys::Element = div.unchecked_ref();
                            let _ = el.request_pointer_lock();
                        }
                    }
                }
                on:blur=move |_| set_viewer_has_focus.set(false)
                on:pointerdown=move |e| {
                    if !rc_enabled.get() { return; }
                    let Some(img) = img_ref.get() else { return; };
                    let (x, y) = map_coords(e.client_x() as f64, e.client_y() as f64, &img, w.get(), h.get());
                    let btn = match e.button() { 0 => Some("left"), 1 => Some("middle"), 2 => Some("right"), _ => None };
                    let did = selected_display.get();
                    if let Some(b) = btn { send_mouse(serde_json::json!({"x": x, "y": y, "button": b, "down": true, "frame_w": w.get(), "frame_h": h.get(), "display_id": did })); }
                    e.prevent_default();
                }
                on:pointerup=move |e| {
                    if !rc_enabled.get() { return; }
                    let Some(img) = img_ref.get() else { return; };
                    let (x, y) = map_coords(e.client_x() as f64, e.client_y() as f64, &img, w.get(), h.get());
                    let btn = match e.button() { 0 => Some("left"), 1 => Some("middle"), 2 => Some("right"), _ => None };
                    let did = selected_display.get();
                    if let Some(b) = btn { send_mouse(serde_json::json!({"x": x, "y": y, "button": b, "down": false, "frame_w": w.get(), "frame_h": h.get(), "display_id": did })); }
                    e.prevent_default();
                }
                on:pointermove=move |e| {
                    if !rc_enabled.get() { return; }
                    let Some(img) = img_ref.get() else { return; };
                    let (x, y) = map_coords(e.client_x() as f64, e.client_y() as f64, &img, w.get(), h.get());
                    let did = selected_display.get();
                    send_mouse(serde_json::json!({"x": x, "y": y, "frame_w": w.get(), "frame_h": h.get(), "display_id": did }));
                    e.prevent_default();
                }
                on:wheel=move |e| {
                    if !rc_enabled.get() { return; }
                    let did = selected_display.get();
                    send_mouse(serde_json::json!({"wheel_x": e.delta_x(), "wheel_y": e.delta_y(), "frame_w": w.get(), "frame_h": h.get(), "display_id": did }));
                    e.prevent_default();
                }
                on:contextmenu=move |e| { e.prevent_default(); }
                on:keydown=move |e| {
                    if !rc_enabled.get() { return; }
                    set_viewer_has_focus.set(true);
                    let key = e.key();
                    let text = if key.len()==1 { Some(key.clone()) } else { None };
                    send_key(serde_json::json!({ "key": key, "text": text, "down": true }));
                    e.prevent_default();
                }
                on:keyup=move |e| {
                    if !rc_enabled.get() { return; }
                    let key = e.key();
                    let text = if key.len()==1 { Some(key.clone()) } else { None };
                    send_key(serde_json::json!({ "key": key, "text": text, "down": false }));
                    e.prevent_default();
                }
                style=move || {
                    let mut s = String::from("position:absolute; inset:0; outline:none; ");
                    s.push_str(if rc_enabled.get() { "cursor: crosshair;" } else { "cursor: not-allowed;" });
                    if viewer_has_focus.get() { s.push_str(" box-shadow: inset 0 0 0 2px #4caf50AA;"); }
                    s
                }
            ></div>
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    let overlay_view = view!{ <></> };

    // Fetch displays on base URL or token change
    {
        let base_url = base_url.clone();
        let token = token.clone();
        let set_displays = set_displays.clone();
        let set_selected_display = set_selected_display.clone();
        create_effect(move |_| {
            let base = base_url.get();
            let tok = token.get();
            spawn_local(async move {
                let url = format!("{}/displays", base);
                if let Ok(v) = fetch_json_auth::<serde_json::Value>(&url, tok.as_deref_filter_empty()).await {
                    let sel = v.get("selected").and_then(|x| x.as_u64()).map(|n| n as u32);
                    let list: Vec<DisplayInfo> = v.get("displays").and_then(|x| serde_json::from_value(x.clone()).ok()).unwrap_or_default();
                    set_displays.set(list);
                    if let Some(id) = sel { set_selected_display.set(Some(id)); }
                }
            });
        });
    }

    // UI (simplified to avoid nested macro delimiter issues)
    view! {
        <main style="font-family: system-ui, Arial; margin: 24px;">
            <h1>"QuicView Client (Leptos)"</h1>
            <div style="margin-bottom: 12px;">
                <label>"Base URL: "<input value=move || base_url.get() on:input=move |e| set_base_url.set(event_target_value(&e)) size="40"/></label>
                <label style="margin-left: 12px;">"Token: "<input r#type="password" value=move || token.get() on:input=move |e| set_token.set(event_target_value(&e)) size="24"/></label>
                <button style="margin-left: 12px;" on:click=move |_| set_rc_enabled.update(|b| *b = !*b)>
                    { move || if rc_enabled.get() { "Remote Control: ON" } else { "Remote Control: OFF" } }
                </button>
            </div>

            <details style="margin:8px 0;">
                <summary>"Server Registry"</summary>
                <div style="margin-top: 6px;">
                    <div style="margin-bottom: 6px;">
                        <label>"Name: "<input value=move || reg_name.get() on:input=move |e| set_reg_name.set(event_target_value(&e)) size="24"/></label>
                        <label style="margin-left:8px;">"URL: "<input value=move || reg_url.get().or_if_empty(base_url.get()) on:input=move |e| set_reg_url.set(event_target_value(&e)) size="36"/></label>
                        <label style="margin-left:8px;">"Token: "<input value=move || reg_tok.get().or_if_empty(token.get()) on:input=move |e| set_reg_tok.set(event_target_value(&e)) size="24"/></label>
                    </div>
                    <select on:change=move |e| {
                        let idx = event_target_value(&e).parse::<usize>().ok();
                        if let Some(i) = idx {
                            set_selected_server.set(Some(i));
                            if let Some(entry) = servers.get().get(i).cloned() {
                                set_base_url.set(entry.url);
                                set_token.set(entry.token);
                                set_reg_name.set(entry.name);
                                set_reg_url.set(base_url.get());
                                set_reg_tok.set(token.get());
                            }
                        }
                    }>
                        <option value="" selected=move || selected_server.get().is_none()>"— select —"</option>
                        { move || servers.get().into_iter().enumerate().map(|(i, s)| view!{ <option value={i.to_string()} selected=move || selected_server.get()==Some(i)>{s.name.clone()}</option> }).collect_view() }
                    </select>
                    <button style="margin-left: 8px;" on:click=move |_| {
                        let mut name = reg_name.get();
                        if name.is_empty() { name = reg_url.get().or_if_empty(base_url.get()); }
                        let url = reg_url.get().or_if_empty(base_url.get());
                        let tok = reg_tok.get().or_if_empty(token.get());
                        let entry = ServerEntry { name, url, token: tok };
                        set_servers.update(|v| v.push(entry));
                    }>"Add"</button>
                    <button style="margin-left: 8px;" on:click=move |_| {
                        if let Some(i) = selected_server.get() {
                            set_servers.update(|v| {
                                if i < v.len() {
                                    v[i].name = reg_name.get().or_if_empty(reg_url.get().or_if_empty(base_url.get()));
                                    v[i].url = reg_url.get().or_if_empty(base_url.get());
                                    v[i].token = reg_tok.get().or_if_empty(token.get());
                                }
                            });
                        }
                    }>"Save to Selected"</button>
                    <button style="margin-left: 8px;" on:click=move |_| {
                        if let Some(i) = selected_server.get() {
                            set_servers.update(|v| { if i < v.len() { v.remove(i); } });
                            set_selected_server.set(None);
                        }
                    }>"Remove Selected"</button>
                </div>
            </details>

            <details>
                <summary>"Displays"</summary>
                <div style="margin: 6px 0;">
                    <select on:change=move |e| {
                        if let Ok(id) = event_target_value(&e).parse::<u32>() {
                            set_selected_display.set(Some(id));
                            let base = base_url.get_untracked();
                            let tok = token.get_untracked();
                            spawn_local(async move { let _ = fetch_post_json(&format!("{}/displays/select", base), tok.as_deref_filter_empty(), serde_json::json!({"id": id})).await; });
                        }
                    }>
                        <option value="" selected=move || selected_display.get().is_none()>"— select —"</option>
                        { move || displays.get().into_iter().map(|d| {
                            let label = format!("{} ({}x{} @ {},{}){}", d.id, d.width, d.height, d.x, d.y, if d.is_main { " [main]" } else { "" });
                            let id_str = d.id.to_string();
                            let selected = selected_display.get()==Some(d.id);
                            view!{ <option value={id_str} selected=move || selected.clone()>{label}</option> }
                        }).collect_view() }
                    </select>
                    <button style="margin-left: 8px;" on:click=move |_| {
                        let base = base_url.get_untracked();
                        let tok = token.get_untracked();
                        let set_displays = set_displays.clone();
                        let set_selected_display = set_selected_display.clone();
                        spawn_local(async move {
                            let url = format!("{}/displays", base);
                            if let Ok(v) = fetch_json_auth::<serde_json::Value>(&url, tok.as_deref_filter_empty()).await {
                                let sel = v.get("selected").and_then(|x| x.as_u64()).map(|n| n as u32);
                                let list: Vec<DisplayInfo> = v.get("displays").and_then(|x| serde_json::from_value(x.clone()).ok()).unwrap_or_default();
                                set_displays.set(list);
                                if let Some(id) = sel { set_selected_display.set(Some(id)); }
                            }
                        });
                    }>"Refresh"</button>
                </div>
            </details>

            <div style="margin: 12px 0;">
                <label>"W: "<input r#type="number" min="64" max="4096" value=move || w.get().to_string() on:input=move |e| if let Ok(n)=event_target_value(&e).parse(){ set_w.set(n);} /></label>
                <label style="margin-left: 8px;">"H: "<input r#type="number" min="36" max="2160" value=move || h.get().to_string() on:input=move |e| if let Ok(n)=event_target_value(&e).parse(){ set_h.set(n);} /></label>
                <label style="margin-left: 8px;">"FPS: "<input r#type="number" min="1" max="60" value=move || fps.get().to_string() on:input=move |e| if let Ok(n)=event_target_value(&e).parse(){ set_fps.set(n);} /></label>
                <label style="margin-left: 8px;">"Q: "<input r#type="number" min="30" max="95" value=move || q.get().to_string() on:input=move |e| if let Ok(n)=event_target_value(&e).parse(){ set_q.set(n);} /></label>
                <span style="margin-left: 8px; opacity: 0.8;">{"FPS(actual): "} { move || fps_actual.get().to_string() }</span>
                <span style="margin-left: 12px;">
                    <span title="QUIC control channel status and tunables">{"Ctrl:"}</span>
                    <span style=move || {
                        let ok = ctrl_cfg.get().map(|c| c.connected).unwrap_or(false);
                        if ok { String::from("color:#1b5e20; margin-left:6px;") } else { String::from("color:#b71c1c; margin-left:6px;") }
                    }>
                        { move || if ctrl_cfg.get().map(|c| c.connected).unwrap_or(false) { "connected" } else { "disconnected" } }
                    </span>
                    <span style="opacity:.75; margin-left: 8px;">
                        { move || ctrl_cfg.get().and_then(|c| c.ping_interval_secs.map(|p| format!("ping {}s", p))).unwrap_or_else(|| "ping -".into()) }
                        {|| ", "}
                        { move || ctrl_cfg.get().and_then(|c| c.backoff_base_ms.map(|b| format!("backoff {}ms", b))).unwrap_or_else(|| "backoff -".into()) }
                        {|| "/"}
                        { move || ctrl_cfg.get().and_then(|c| c.backoff_max_ms.map(|m| format!("{}ms", m))).unwrap_or_else(|| "-".into()) }
                        {|| ", attempts: "}
                        { move || ctrl_cfg.get().map(|c| c.attempts.to_string()).unwrap_or_else(|| "-".into()) }
                        {|| ", reconnects: "}
                        { move || ctrl_cfg.get().map(|c| c.reconnects.to_string()).unwrap_or_else(|| "-".into()) }
                    </span>
                    <Show when=move || ctrl_cfg.get().and_then(|c| c.last_error.clone()).is_some()>
                        <span style="margin-left: 8px; color:#b71c1c;">
                            { move || ctrl_cfg.get().and_then(|c| c.last_error.clone()).unwrap_or_default() }
                        </span>
                    </Show>
                </span>
                <span style="margin-left: 8px;">
                    <button on:click=move |_| { set_w.set(640); set_h.set(360); set_fps.set(10); set_q.set(65); push_toast("Preset: Low".into(), "info"); }>"Low"</button>
                    <button style="margin-left:6px;" on:click=move |_| { set_w.set(1280); set_h.set(720); set_fps.set(15); set_q.set(70); push_toast("Preset: Medium".into(), "info"); }>"Medium"</button>
                    <button style="margin-left:6px;" on:click=move |_| { set_w.set(1920); set_h.set(1080); set_fps.set(30); set_q.set(80); push_toast("Preset: High".into(), "info"); }>"High"</button>
                </span>
                <button style="margin-left: 8px;" on:click=move |_| refresh() >"Refresh Status"</button>
                <span style="margin-left: 12px;">{"Status: "} { status }</span>
                <span style="margin-left: 12px;">{"Consent: "} { move || if consent_allowed.get() { "allowed" } else { "denied" } }</span>
                <Show when=move || ctrl_cfg.get().and_then(|c| c.tls.clone()).is_some()>
                    <span style="margin-left: 12px; opacity:.8;">
                        {|| "TLS: "}
                        { move || {
                            if let Some(cfg) = ctrl_cfg.get() {
                                if let Some(t) = cfg.tls { 
                                    let mut s = t.mode;
                                    if let Some(pin) = t.pin_sha256_hex { s.push_str(" ("); s.push_str(&short_hex(&pin)); s.push(')'); }
                                    return s;
                                }
                            }
                            String::from("-")
                        }}
                    </span>
                </Show>
                <button style="margin-left: 8px;" on:click=move |_| {
                    let base = base_url.get_untracked();
                    let tok = token.get_untracked();
                    let rf = refresh.clone();
                    spawn_local(async move { let _ = fetch_post(&format!("{}/consent/allow", base), tok.as_deref_filter_empty()).await; rf(); });
                }>
                    "Allow"
                </button>
                <button style="margin-left: 6px;" on:click=move |_| {
                    let base = base_url.get_untracked();
                    let tok = token.get_untracked();
                    let rf = refresh.clone();
                    spawn_local(async move { let _ = fetch_post(&format!("{}/consent/deny", base), tok.as_deref_filter_empty()).await; rf(); });
                }>
                    "Deny"
                </button>
            </div>

            <div style="margin-bottom: 12px;">
                <button on:click=move |_| {
                    let base = base_url.get_untracked();
                    let tok = token.get_untracked();
                    spawn_local(async move { let _ = fetch_post(&format!("{}/start", base), tok.as_deref_filter_empty()).await; });
                }>"Start"</button>
                <button style="margin-left: 8px;" on:click=move |_| {
                    let base = base_url.get_untracked();
                    let tok = token.get_untracked();
                    spawn_local(async move { let _ = fetch_post(&format!("{}/stop", base), tok.as_deref_filter_empty()).await; });
                }>"Stop"</button>
                <details style="margin-left: 12px; display: inline-block;">
                    <summary>"Ctrl Status"</summary>
                    <pre style="background:#f6f8fa; padding:8px; border:1px solid #ddd; max-width: 800px; overflow:auto;">{ ctrl_json }</pre>
                </details>
            </div>

            <div style="margin-bottom: 12px;">
                <button on:click=move |_| {
                    let base = base_url.get_untracked();
                    let tok = token.get_untracked();
                    spawn_local(async move {
                        let url = format!("{}/input/mouse", base);
                        let _ = fetch_post_json(&url, tok.as_deref_filter_empty(), serde_json::json!({ "x": 100.0, "y": 100.0, "button": "left", "down": true })).await;
                        let _ = fetch_post_json(&url, tok.as_deref_filter_empty(), serde_json::json!({ "x": 100.0, "y": 100.0, "button": "left", "down": false })).await;
                    });
                }>"Test Click (100,100)"</button>
                <button style="margin-left: 8px;" on:click=move |_| {
                    let base = base_url.get_untracked();
                    let tok = token.get_untracked();
                    spawn_local(async move {
                        let url = format!("{}/input/key", base);
                        let _ = fetch_post_json(&url, tok.as_deref_filter_empty(), serde_json::json!({ "text": "a" })).await;
                    });
                }>"Type 'a'"</button>
            </div>

            <div style="margin-bottom: 12px;">
                <div><strong>"Clipboard"</strong></div>
                <div>
                    <textarea rows="4" cols="60" value=move || clip.get() on:input=move |e| set_clip.set(event_target_value(&e))></textarea>
                </div>
                <div style="margin-top: 6px;">
                    <button on:click=move |_| {
                        let base = base_url.get_untracked();
                        let tok = token.get_untracked();
                        let set_clip = set_clip.clone();
                        spawn_local(async move {
                            let url = format!("{}/clipboard", base);
                            match fetch_json_auth::<serde_json::Value>(&url, tok.as_deref_filter_empty()).await {
                                Ok(v) => if let Some(t) = v.get("text").and_then(|x| x.as_str()) { set_clip.set(t.to_string()); },
                                Err(_) => {}
                            }
                        });
                    }>"Read"</button>
                    <button style="margin-left: 8px;" on:click=move |_| {
                        let base = base_url.get_untracked();
                        let tok = token.get_untracked();
                        let body = serde_json::json!({"text": clip.get_untracked()});
                        spawn_local(async move { let url = format!("{}/clipboard", base); let _ = fetch_post_json(&url, tok.as_deref_filter_empty(), body).await; });
                    }>"Write"</button>
                </div>
            </div>

        <div style="position: relative; border: 1px solid #ddd; display: inline-block; user-select: none;">
                <img src=stream_src alt="MJPEG stream" style="max-width: 100%; display: block;" node_ref=img_ref
            on:load=move |_| { #[cfg(target_arch = "wasm32")] { use wasm_bindgen::JsCast; if let Some(img) = img_ref.get() { let el: &web_sys::HtmlElement = img.unchecked_ref(); let rect = el.get_bounding_client_rect(); set_disp_w.set(rect.width()); set_disp_h.set(rect.height()); } } }
            on:resize=move |_| { #[cfg(target_arch = "wasm32")] { use wasm_bindgen::JsCast; if let Some(img) = img_ref.get() { let el: &web_sys::HtmlElement = img.unchecked_ref(); let rect = el.get_bounding_client_rect(); set_disp_w.set(rect.width()); set_disp_h.set(rect.height()); } } }
                />
                {overlay_view}
                <Show when=move || !rc_enabled.get()>
                    <div style="position:absolute; inset:0; display:flex; align-items:center; justify-content:center; background: rgba(255,255,255,0.4); pointer-events:none;">
                        <div style="padding:6px 10px; background:#fff; border:1px solid #ccc; border-radius:6px; color:#333;">"Remote Control is OFF"</div>
                    </div>
                </Show>
            </div>
            <div style="position: fixed; right: 12px; bottom: 12px; display: flex; flex-direction: column; gap: 6px;">
                { move || toasts.get().into_iter().enumerate().map(|(_i,t)| {
                    let bg = match t.kind { "error" => "#fbeaea", "info" => "#eef5ff", _ => "#f6f8fa" };
                    let bd = match t.kind { "error" => "#f5c2c7", "info" => "#9ec5fe", _ => "#d0d7de" };
                    view!{ <div style=format!("background:{}; border:1px solid {}; padding:8px 10px; border-radius:6px;", bg, bd)>{t.msg.clone()}</div> }
                }).collect_view() }
            </div>
        </main>
    }
}

// removed unused: refresh_status

#[cfg(target_arch = "wasm32")]
async fn fetch_json<T: for<'de> serde::Deserialize<'de>>(url: &str) -> Result<T, String> {
    use gloo_net::http::Request;
    let res = Request::get(url).send().await.map_err(|e| e.to_string())?;
    if !res.ok() { return Err(format!("status {}", res.status())); }
    res.json().await.map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
async fn fetch_json_auth<T: for<'de> serde::Deserialize<'de>>(url: &str, token: Option<&str>) -> Result<T, String> {
    use gloo_net::http::Request;
    let mut req = Request::get(url);
    if let Some(t) = token { req = req.header("Authorization", &format!("Bearer {}", t)); }
    let res = req.send().await.map_err(|e| e.to_string())?;
    if !res.ok() { return Err(format!("status {}", res.status())); }
    res.json().await.map_err(|e| e.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_json_auth<T: for<'de> serde::Deserialize<'de>>(_url: &str, _token: Option<&str>) -> Result<T, String> {
    Err("not supported outside wasm".into())
}

#[cfg(target_arch = "wasm32")]
async fn fetch_post(url: &str, token: Option<&str>) -> Result<(), String> {
    use gloo_net::http::Request;
    let mut req = Request::post(url);
    if let Some(t) = token {
        req = req.header("Authorization", &format!("Bearer {}", t));
    }
    let res = req.send().await.map_err(|e| e.to_string())?;
    if !res.ok() { return Err(format!("status {}", res.status())); }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_post(_url: &str, _token: Option<&str>) -> Result<(), String> { Err("not supported outside wasm".into()) }

#[cfg(target_arch = "wasm32")]
async fn fetch_post_json(url: &str, token: Option<&str>, body: serde_json::Value) -> Result<(), String> {
    use gloo_net::http::Request;
    let mut req = Request::post(url).header("content-type", "application/json");
    if let Some(t) = token { req = req.header("Authorization", &format!("Bearer {}", t)); }
    let res = req.json(&body).map_err(|e| e.to_string())?.send().await.map_err(|e| e.to_string())?;
    if !res.ok() { return Err(format!("status {}", res.status())); }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_post_json(_url: &str, _token: Option<&str>, _body: serde_json::Value) -> Result<(), String> { Err("not supported outside wasm".into()) }

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn main_js() {
    console_error_panic_hook::set_once();
    leptos::mount_to_body(|| view! { <App/> })
}

// small helpers
trait AsDerefFilterEmpty { fn as_deref_filter_empty(&self) -> Option<&str>; }
trait StringOrEmpty { fn or_if_empty(self, fallback: String) -> String; }
fn short_hex(h: &str) -> String { let s = h.trim(); if s.len() <= 8 { s.to_string() } else { format!("{}…", &s[..8]) } }

impl AsDerefFilterEmpty for String {
    fn as_deref_filter_empty(&self) -> Option<&str> {
        if self.is_empty() { None } else { Some(self.as_str()) }
    }
}

impl StringOrEmpty for String {
    fn or_if_empty(self, fallback: String) -> String {
        if self.is_empty() { fallback } else { self }
    }
}
