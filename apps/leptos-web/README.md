# QuicView Leptos Web App (skeleton)

This folder will host the Leptos client UI. We’ll build a Tauri desktop wrapper that serves this web app from the local HTTP agent.

Planned features inspired by RustDesk’s Flutter UI:
- Server list & quick-connect panel
- Remote screen viewer with overlay controls (input capture, display selector)
- Clipboard sync panel (bi-directional)
- Session status, consent indicator, and control-channel metrics
- Settings: FPS, quality, display selection, auth token, CORS

Next steps:
1. Scaffold a Leptos CSR app with a simple status page hitting `GET /status` and consent buttons.
2. Wire stream viewer (<img src="/stream.mjpeg?...">) with controls for `w/h/fps/q`.
3. Add input capture overlay and keyboard handling.
4. Build and copy `dist/` here; CLI `--static-dir` or Tauri will serve it.
