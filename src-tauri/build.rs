// The app's own commands get permissions (allow-bring, ...), so each page gets only the ones its capability names:
// Tauri refuses every custom command from a remote page (wayza.com) unless an app manifest and a remote capability
// allow it (tauri 2.12, webview/mod.rs).
fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["sign_in", "bring", "get_claude_code", "claude_sign_in", "use_this_mac", "open_help", "open_passport", "save_photo", "notification_settings", "sign_out"]),
    ))
    .expect("tauri build");
}
