// wayza.com's REST API (/wayza/v0), as the person (their wzs_ session) or as one of their AIs the app runs. Acting as an
// AI, the app sends its own sign-in and names the AI with Wayza-As (design "Your Claude on all your computers", 3 Nov
// 2026): no computer keeps an AI's key. Code that acts as an AI passes "as:<its address>" where a key used to go.

use anyhow::{bail, Result};
use serde_json::{json, Value};

#[derive(Clone)]
pub struct Api {
    pub base: String,
    http: reqwest::Client,
}

impl Api {
    pub fn new(base: &str) -> Self {
        let http = reqwest::Client::builder().user_agent(concat!("WayzaMac/", env!("CARGO_PKG_VERSION"))).build().expect("http client");
        Api { base: base.trim_end_matches('/').to_string(), http }
    }

    pub async fn call(&self, method: reqwest::Method, path: &str, bearer: Option<&str>, body: Option<Value>) -> Result<Value> {
        let mut req = self.http.request(method, format!("{}/wayza/v0{}", self.base, path));
        if let Some(b) = bearer {
            req = signed(req, b)?;
        }
        if let Some(b) = body {
            req = req.json(&b);
        }
        let res = req.send().await?;
        let status = res.status();
        let v: Value = res.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            let msg = v.get("error").and_then(Value::as_str).unwrap_or("wayza.com said no").to_string();
            if status.as_u16() == 401 {
                bail!(Unauthorized(msg));
            }
            bail!("{} ({})", msg, status.as_u16());
        }
        Ok(v)
    }
    pub async fn get(&self, path: &str, bearer: &str) -> Result<Value> {
        self.call(reqwest::Method::GET, path, Some(bearer), None).await
    }
    pub async fn post(&self, path: &str, bearer: Option<&str>, body: Value) -> Result<Value> {
        self.call(reqwest::Method::POST, path, bearer, Some(body)).await
    }
    pub async fn patch(&self, path: &str, bearer: &str, body: Value) -> Result<Value> {
        self.call(reqwest::Method::PATCH, path, Some(bearer), Some(body)).await
    }
    pub async fn put(&self, path: &str, bearer: &str, body: Value) -> Result<Value> {
        self.call(reqwest::Method::PUT, path, Some(bearer), Some(body)).await
    }
    pub async fn delete(&self, path: &str, bearer: &str) -> Result<Value> {
        self.call(reqwest::Method::DELETE, path, Some(bearer), None).await
    }

    // Rooms help as plain text (wayza.com/help/rooms.txt), the same words people read at wayza.com/help.
    // Short timeout: Claude waits on this before answering, so a slow wayza.com must not hold up a turn.
    pub async fn help(&self) -> Result<String> {
        let res = self.http.get(format!("{}/help/rooms.txt", self.base)).timeout(std::time::Duration::from_secs(5)).send().await?;
        if !res.status().is_success() {
            bail!("wayza.com said no ({})", res.status().as_u16());
        }
        Ok(tokio::time::timeout(std::time::Duration::from_secs(5), res.text()).await??)
    }

    // A photo or file's bytes (rooms/{id}/files/{file}), as the AI.
    pub async fn bytes(&self, path: &str, bearer: &str) -> Result<Vec<u8>> {
        let res = signed(self.http.get(format!("{}/wayza/v0{}", self.base, path)), bearer)?.send().await?;
        if !res.status().is_success() {
            bail!("wayza.com said no ({})", res.status().as_u16());
        }
        Ok(res.bytes().await?.to_vec())
    }
    // The app's own session, from the code the browser brought back.
    pub async fn token(&self, code: &str, verifier: &str) -> Result<Value> {
        // me: this app's window shows wayza.me, so its way in is there.
        self.post("/mac/token", None, json!({ "code": code, "code_verifier": verifier, "me": true })).await
    }
    // The AI the person said yes to: its address and a new key.
    // v: 2, so wayza.com hands this app no key: it acts as the AI with its own sign-in.
    pub async fn ai_token(&self, code: &str, verifier: &str) -> Result<Value> {
        self.post("/mac/ai-token", None, json!({ "code": code, "code_verifier": verifier, "v": 2 })).await
    }
}

// The person's session, or for "as:<address>" the person's session plus Wayza-As naming that AI.
fn signed(req: reqwest::RequestBuilder, bearer: &str) -> Result<reqwest::RequestBuilder> {
    match bearer.strip_prefix("as:") {
        Some(ai) => {
            let Some(session) = crate::auth::kept("session") else { bail!(Unauthorized("Sign in again.".into())) };
            Ok(req.bearer_auth(session).header("Wayza-As", ai))
        }
        None => Ok(req.bearer_auth(bearer)),
    }
}

// The session has ended (signed out, a new password, 30 days): sign in again.
#[derive(Debug)]
pub struct Unauthorized(pub String);
impl std::fmt::Display for Unauthorized {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Unauthorized {}

pub fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
