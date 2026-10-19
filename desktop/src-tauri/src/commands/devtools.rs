/// Opens the webview's developer tools from the frontend's own F12 /
/// Ctrl+Shift+I handler.
///
/// Opening is idempotent, so the app's handler and WebView2's built-in
/// accelerator can both end up calling it without the second call closing
/// what the first one opened. (Closing is done from the devtools panel
/// itself.)
///
/// Only meaningful with `app.security.devtools` enabled in
/// `tauri.conf.json`: without it the webview is created with devtools
/// disabled and this is a no-op — which is exactly why F12 did nothing in
/// release builds.
#[tauri::command]
pub fn open_devtools(window: tauri::WebviewWindow) {
    window.open_devtools();
}
