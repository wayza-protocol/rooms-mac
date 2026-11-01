// Signing the app in, and bringing Claude in, through wayza.com in the person's own browser.
//
// The app is a public client: it has no secret. Each time it makes a PKCE verifier, listens on 127.0.0.1 on a port of
// its own, and opens wayza.com with the verifier's challenge. wayza.com asks the person (MacSignin1, MacConsent1) and
// sends the browser back to http://127.0.0.1:<port>/<path>?code&state. Only this app holds the verifier, so only it
// can trade the code (server: src/wayza/mac.js).

use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

pub fn random(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}

pub fn pkce() -> Pkce {
    let verifier = random(32);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    Pkce { verifier, challenge }
}

// Listens on 127.0.0.1 for wayza.com to send the browser back. `open` gets the port and opens the browser. Returns the
// query wayza.com sent (code, or error=declined), once its state matches ours.
pub async fn round_trip(path: &str, state: &str, open: impl FnOnce(u16) -> Result<()>) -> Result<HashMap<String, String>> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    open(port)?;
    let wait = async {
        loop {
            let (mut sock, _) = listener.accept().await?;
            let mut buf = vec![0u8; 8192];
            // A connection that sends nothing must not hold the sign-in up: 5 seconds, then the next one.
            let n = match tokio::time::timeout(Duration::from_secs(5), sock.read(&mut buf)).await {
                Ok(Ok(n)) => n,
                _ => continue,
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let target = req.lines().next().unwrap_or("").split(' ').nth(1).unwrap_or("").to_string();
            let url = url::Url::parse(&format!("http://127.0.0.1{target}")).ok();
            let ours = url.as_ref().map(|u| u.path() == format!("/{path}")).unwrap_or(false);
            if !ours {
                let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                continue;
            }
            let q: HashMap<String, String> = url.unwrap().query_pairs().into_owned().collect();
            if q.get("state").map(String::as_str) != Some(state) {
                let _ = sock.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                continue;
            }
            let body = page(q.contains_key("code"));
            let head = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len());
            let _ = sock.write_all(head.as_bytes()).await;
            let _ = sock.write_all(body.as_bytes()).await;
            return Ok::<_, anyhow::Error>(q);
        }
    };
    match tokio::time::timeout(Duration::from_secs(15 * 60), wait).await {
        Ok(r) => r,
        Err(_) => bail!("No answer from wayza.com in 15 minutes."),
    }
}

// The browser tab after wayza.com sends it back here. No board shows it: plain words, in the system font (no fetch).
fn page(done: bool) -> String {
    let line = if done { "Done. You can go back to the Wayza app." } else { "Nothing changed. You can go back to the Wayza app." };
    format!(r#"<!doctype html><meta charset="utf-8"><title>Wayza</title><body style="margin:0;height:100vh;display:grid;place-items:center;background:#FAFAF9;font-family:-apple-system,system-ui,sans-serif;color:#111;font-size:17px;font-weight:500">{line}<script>setTimeout(function(){{window.close()}},1500)</script></body>"#)
}

// The app's own secrets, in the login Keychain. Each is read from the Keychain at most once per launch and then held in
// memory: until builds are signed, every new build makes macOS ask for each read that hasn't had "Always Allow".
const SERVICE: &str = "com.wayza.rooms";
static HELD: std::sync::Mutex<Option<std::collections::HashMap<String, Option<String>>>> = std::sync::Mutex::new(None);

fn held<T>(f: impl FnOnce(&mut std::collections::HashMap<String, Option<String>>) -> T) -> T {
    let mut g = HELD.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Default::default))
}
pub fn keep(account: &str, secret: &str) -> Result<()> {
    keyring::Entry::new(SERVICE, account)?.set_password(secret).map_err(|e| anyhow!(e))?;
    held(|h| h.insert(account.to_string(), Some(secret.to_string())));
    Ok(())
}
pub fn kept(account: &str) -> Option<String> {
    // The lock is held through the read, so two first reads at launch make one prompt, not two.
    held(|h| {
        h.entry(account.to_string())
            .or_insert_with(|| keyring::Entry::new(SERVICE, account).ok().and_then(|e| e.get_password().ok()))
            .clone()
    })
}
pub fn forget(account: &str) {
    if let Ok(e) = keyring::Entry::new(SERVICE, account) {
        let _ = e.delete_credential();
    }
    held(|h| h.insert(account.to_string(), None));
}
