// The app updates itself (agreed 5 Nov 2026, MacUpdate1). A while after it opens, and every six hours after that, it
// asks wayza.com/download/mac/latest.json for a newer version; that answer comes from the newest release on this repo,
// signed with the update key. A newer one downloads and installs quietly; the room page then shows "A new version of
// Wayza is ready." with Restart, and the new version runs from the next start whether or not Restart is pressed.
//
// Only a build made with the update key has the updater turned on (the release workflow adds plugins.updater to the
// config). A build without it, like a pull request's, never checks.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::UpdaterExt;

static READY: AtomicBool = AtomicBool::new(false);

// A newer version is installed and runs once the app restarts.
pub fn ready() -> bool {
    READY.load(Ordering::SeqCst)
}

// Whether this build was made with the update key.
pub fn on<R: Runtime>(h: &AppHandle<R>) -> bool {
    cfg!(target_os = "macos") && h.config().plugins.0.contains_key("updater")
}

// Check now and then until a newer version is installed; then say so to the page (on_ready) and stop.
pub async fn watch<R: Runtime>(h: AppHandle<R>, on_ready: impl Fn(&AppHandle<R>)) {
    tokio::time::sleep(Duration::from_secs(60)).await;
    loop {
        match once(&h).await {
            Ok(Some(v)) => {
                crate::log(&format!("update {v} installed; runs after a restart"));
                READY.store(true, Ordering::SeqCst);
                on_ready(&h);
                return;
            }
            Ok(None) => {}
            Err(e) => crate::log(&format!("update check failed: {e}")),
        }
        tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
    }
}

async fn once<R: Runtime>(h: &AppHandle<R>) -> anyhow::Result<Option<String>> {
    let Some(u) = h.updater()?.check().await? else { return Ok(None) };
    u.download_and_install(|_, _| {}, || {}).await?;
    Ok(Some(u.version.clone()))
}
