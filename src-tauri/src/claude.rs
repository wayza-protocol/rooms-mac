// The person's own Claude Code, run on this Mac with their own sign-in. The app never installs it and never sees that
// sign-in: it only runs `claude -p` and reads what it says.
//
// In a room, everyone can talk to it, so it can answer and never act on this Mac: no tools at all, no MCP servers, and
// none of the person's own setup (hooks, CLAUDE.md, memory, skills, plugins) that room text could reach or recite.
// --safe-mode keeps their own Claude sign-in, where --bare would need an API key (code.claude.com/docs/en/cli-reference,
// read 1 Nov 2026).

use anyhow::{bail, Result};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    Ready,
    Missing,
    SignedOut,
}
impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Ready => "ready",
            State::Missing => "missing",
            State::SignedOut => "signed_out",
        }
    }
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

// The PATH of the person's login shell: an app opened from Finder gets a bare one, and Claude Code may be installed
// through npm or Homebrew.
pub fn login_path() -> String {
    std::process::Command::new("/bin/zsh")
        .args(["-lc", "printf %s \"$PATH\""])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin".into())
}

pub fn find(path: &str) -> Option<PathBuf> {
    let h = home();
    let mut places: Vec<PathBuf> = path.split(':').map(|d| PathBuf::from(d).join("claude")).collect();
    places.extend([h.join(".claude/local/claude"), h.join(".local/bin/claude"), "/opt/homebrew/bin/claude".into(), "/usr/local/bin/claude".into()]);
    places.into_iter().find(|p| p.is_file())
}

// Signed in: Claude Code keeps its sign-in in the login Keychain, or in ~/.claude/.credentials.json.
fn signed_in() -> bool {
    let keychain = std::process::Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", "Claude Code-credentials"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    keychain || home().join(".claude/.credentials.json").is_file()
}

pub fn state(path: &str) -> State {
    match find(path) {
        None => State::Missing,
        Some(_) if !signed_in() => State::SignedOut,
        Some(_) => State::Ready,
    }
}

// Claude Code's sign-in has ended (revoked, expired): it says so only when asked to answer. `claude auth status` can
// still say loggedIn.
#[derive(Debug)]
pub struct SignedOut(pub String);
impl std::fmt::Display for SignedOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for SignedOut {}

// What this Claude Code's command line offers, from its own --help, asked once per launch.
#[derive(Clone, Copy)]
struct Caps {
    safe_mode: bool,
    permission_prompts: bool,
}
fn caps(bin: &PathBuf) -> Caps {
    static CAPS: std::sync::OnceLock<Caps> = std::sync::OnceLock::new();
    *CAPS.get_or_init(|| {
        let help = std::process::Command::new(bin).arg("--help").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        Caps { safe_mode: help.contains("--safe-mode"), permission_prompts: help.contains("--permission-prompts") }
    })
}

pub struct Run {
    pub text: String,
    pub session: Option<String>,
}

// One turn: the prompt in, Claude's answer out. `on_text` gets the answer so far as it grows, for the room's
// "writing" line. `resume` carries on the room's own conversation.
pub async fn run(path: &str, cwd: &PathBuf, system: &str, prompt: &str, resume: Option<&str>, mut on_text: impl FnMut(&str)) -> Result<Run> {
    let Some(bin) = find(path) else { bail!("Claude Code isn't on this Mac.") };
    std::fs::create_dir_all(cwd)?;
    let mut cmd = Command::new(&bin);
    cmd.current_dir(cwd)
        .env("PATH", path)
        .args(["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages"])
        .args(["--tools", "", "--disallowedTools", "*", "--permission-mode", "dontAsk"]);
    // Newer Claude Code has --safe-mode (none of the person's own setup) and --permission-prompts. Older versions
    // (2.1.141 on Graham's Mac, 1 Nov 2026) don't: there, no settings sources and no hooks instead.
    let caps = caps(&bin);
    if caps.safe_mode {
        cmd.arg("--safe-mode");
    } else {
        cmd.args(["--setting-sources", "", "--settings", r#"{"disableAllHooks":true}"#]);
    }
    if caps.permission_prompts {
        cmd.args(["--permission-prompts", "none"]);
    }
    cmd
        .args(["--strict-mcp-config", "--mcp-config", r#"{"mcpServers":{}}"#, "--disable-slash-commands"])
        .args(["--append-system-prompt", system]);
    if let Some(id) = resume {
        cmd.args(["--resume", id]);
    }
    cmd.arg(prompt).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = cmd.spawn()?;
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let (mut text, mut session, mut result) = (String::new(), None, None);
    while let Some(line) = lines.next_line().await? {
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(id) = v.get("session_id").and_then(Value::as_str) {
            session = Some(id.to_string());
        }
        match v.get("type").and_then(Value::as_str) {
            // --include-partial-messages: the answer, a few words at a time.
            Some("stream_event") => {
                let d = &v["event"]["delta"];
                if d["type"] == "text_delta" {
                    if let Some(t) = d["text"].as_str() {
                        text.push_str(t);
                        on_text(&text);
                    }
                }
            }
            Some("result") => {
                if v["is_error"].as_bool() == Some(true) {
                    let why = v["result"].as_str().unwrap_or("Claude Code stopped.");
                    if why.contains("Failed to authenticate") || why.contains("401") {
                        bail!(SignedOut(why.to_string()));
                    }
                    bail!("{}", why);
                }
                result = v["result"].as_str().map(str::to_string);
            }
            _ => {}
        }
    }
    let status = child.wait().await?;
    let text = result.unwrap_or(text).trim().to_string();
    if !status.success() && text.is_empty() {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            use tokio::io::AsyncReadExt;
            let _ = e.read_to_string(&mut err).await;
        }
        bail!("Claude Code stopped: {}", err.lines().last().unwrap_or("no answer"));
    }
    Ok(Run { text, session })
}
