# Tauri Client

The Tauri app launches the QuicView HTTP UI in-process and opens a native window to it.

Static assets resolution order:
- `QUICVIEW_LEPTOS_DIST` env var pointing to a built `leptos-web/dist` directory
- Packaged resource `leptos-web-dist` (when bundling)
- Dev path: `apps/leptos-web/dist` (relative to this crate)

Dev run:
```sh
cd apps/tauri-client/src-tauri
cargo run
```

Bundle (macOS example):
```sh
cd apps/tauri-client/src-tauri
# Ensure leptos-web has been built: `trunk build --release` (or your chosen build)
# Then build the Tauri bundle
cargo tauri build
```

Note: The app requires macOS Screen Recording and Accessibility permissions for capture and input injection.
