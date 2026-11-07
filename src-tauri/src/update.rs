// The app updates itself (agreed 5 Nov 2026, MacUpdate1). A while after it opens, and every six hours after that, it
// asks wayza.com/download/mac/latest.json for a newer version; that answer comes from the newest release on this repo
// once its update is signed with the update key (kept on Graham's Mac; its public half is in tauri.conf.json). A newer one downloads and installs quietly; the room page then shows "A new version of
// Wayza is ready." with Restart, and the new version runs from the next start whether or not Restart is pressed.
//
// Only a build whose config has plugins.updater checks, and only on a Mac.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::UpdaterExt;

static READY: AtomicBool = AtomicBool::new(false);

// A newer version is installed and runs once the app restarts.
pub fn ready() -> bool {
    READY.load(Ordering::SeqCst)
}

// Whether this build checks for updates.
pub fn on<R: Runtime>(h: &AppHandle<R>) -> bool {
    cfg!(target_os = "macos") && h.config().plugins.0.contains_key("updater")
}

// Check now and then until a newer version is installed; then say so to the page (on_ready) and stop.
pub async fn watch<R: Runtime>(h: AppHandle<R>, on_ready: impl Fn(&AppHandle<R>)) {
    tokio::time::sleep(Duration::from_secs(60)).await;
    loop {
        match check(&h).await {
            Checked::Ready => {
                on_ready(&h);
                return;
            }
            _ => tokio::time::sleep(Duration::from_secs(6 * 3600)).await,
        }
    }
}

// What one check found.
pub enum Checked {
    // A newer version is installed and runs after a restart.
    Ready,
    // This is the newest version.
    Newest,
    // wayza.com couldn't be asked: most likely no internet.
    NoAnswer,
    // A newer version was found, but downloading it, checking its signature or installing it failed.
    NotInstalled,
}

// One check, never two at once (the timer's and Check for Updates…).
pub async fn check<R: Runtime>(h: &AppHandle<R>) -> Checked {
    static ONE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _one = ONE.lock().await;
    if ready() {
        return Checked::Ready;
    }
    let found = match h.updater() {
        Ok(u) => u.check().await,
        Err(e) => Err(e),
    };
    let u = match found {
        Ok(Some(u)) => u,
        Ok(None) => return Checked::Newest,
        Err(e) => {
            crate::log(&format!("update check failed: {e}"));
            return Checked::NoAnswer;
        }
    };
    match u.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => {
            crate::log(&format!("update {} installed; runs after a restart", u.version));
            READY.store(true, Ordering::SeqCst);
            Checked::Ready
        }
        Err(e) => {
            crate::log(&format!("update {} not installed: {e}", u.version));
            Checked::NotInstalled
        }
    }
}
