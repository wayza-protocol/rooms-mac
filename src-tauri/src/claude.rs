// The person's own Claude Code, run on this Mac with their own sign-in. The app never installs it and never sees that
// sign-in: it only runs `claude -p` and reads what it says.
//
// In a room, everyone can talk to it, so it can answer and never act on this Mac: no tools at all, no MCP servers, and
// none of the person's own setup (hooks, CLAUDE.md, memory, skills, plugins) that room text could reach or recite.
// --safe-mode keeps their own Claude sign-in, where --bare would need an API key (code.claude.com/docs/en/cli-reference,
// read 1 Nov 2026).

use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
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

// A photo or document for Claude: its media type (image/jpeg, png, gif or webp, application/pdf: bytes in base64) or
// text/plain (the file's words as they are), and the file's name.
pub struct Image {
    pub media_type: String,
    pub name: String,
    pub data: String,
}

// One turn: the prompt in, Claude's answer out. `on_text` gets the answer so far as it grows, for the room's
// "writing" line. `resume` carries on the room's own conversation. With photos, the prompt and the photos go in on
// stdin as one stream-json user message, since a prompt argument can only carry words.
// What Claude may use in this turn, from the room's setting (Graham, 1 Nov 2026) and who asked:
// Words: nothing at all. Web: web search and reading web pages, still none of the person's own setup. Computer: only for
// the person's own ask, in a room they set to "My computer too": their own setup (settings, CLAUDE.md, MCP servers),
// their home folder to read, and anything else only as their own permission rules already allow, never by asking.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tools {
    Words,
    Web,
    Computer,
}

// The room's own tools for this turn (wayza.com's turn pass), as Claude Code calls them. The pass itself is only in the
// MCP config file, never on the command line, where `ps` would show it.
pub const ROOM_TOOLS: [&str; 5] = [
    "mcp__wayza-room__room_read",
    "mcp__wayza-room__room_ask",
    "mcp__wayza-room__room_ask_answer",
    "mcp__wayza-room__room_ask_pass",
    "mcp__wayza-room__room_ask_take_back",
];

// The MCP config file for a turn pass: only the person's Mac user can read it, and it goes when the turn ends.
pub fn write_room_config(path: &PathBuf, base: &str, pass: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let cfg = json!({ "mcpServers": { "wayza-room": { "type": "http", "url": format!("{}/wayza/mcp", base.trim_end_matches('/')), "headers": { "Authorization": format!("Bearer {pass}") } } } });
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    f.write_all(cfg.to_string().as_bytes())?;
    Ok(())
}

pub async fn run(path: &str, cwd: &PathBuf, system: &str, prompt: &str, images: &[Image], tools: Tools, room: Option<&PathBuf>, resume: Option<&str>, mut on_text: impl FnMut(&str)) -> Result<Run> {
    let Some(bin) = find(path) else { bail!("Claude Code isn't on this computer.") };
    std::fs::create_dir_all(cwd)?;
    let mut cmd = Command::new(&bin);
    // Room tools, when wayza.com gave this turn a pass: allowed without asking, like the turn's other tools.
    let room_tools = if room.is_some() { format!(",{}", ROOM_TOOLS.join(",")) } else { String::new() };
    cmd.current_dir(cwd).env("PATH", path).args(["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages"]);
    match tools {
        Tools::Words => {
            cmd.args(["--tools", "", "--disallowedTools", "*"]);
        }
        Tools::Web => {
            cmd.args(["--tools", "WebSearch,WebFetch", "--allowedTools"]).arg(format!("WebSearch,WebFetch{room_tools}"));
        }
        Tools::Computer => {
            // The room's own folder stays the working folder, so the room's conversation carries on; home is added.
            cmd.arg("--allowedTools").arg(format!("Read,Glob,Grep,WebSearch,WebFetch{room_tools}"));
            // Their own MCP servers load as usual; the room's is added.
            if let Some(r) = room {
                cmd.arg(format!("--mcp-config={}", r.display()));
            }
            if let Some(h) = std::env::var_os("HOME") {
                cmd.arg("--add-dir").arg(h);
            }
        }
    }
    cmd.args(["--permission-mode", "dontAsk"]);
    let caps = caps(&bin);
    if tools != Tools::Computer {
        // Newer Claude Code has --safe-mode (none of the person's own setup). Older versions (2.1.141 on Graham's Mac,
        // 1 Nov 2026) don't: there, no settings sources and no hooks instead. --safe-mode also drops the servers
        // --mcp-config names (2.1.316, tried 1 Nov 2026), so a turn with room tools uses the second way too: it loads
        // only the room's server, with no skills, plugins of theirs or slash commands.
        if caps.safe_mode && room.is_none() {
            cmd.arg("--safe-mode");
        } else {
            // Claude Code's own memory stays off too: a "My computer too" turn could write notes there that a turn
            // someone else started would then read (build review 1, item 1; tried on 2.1.316).
            cmd.args(["--setting-sources", "", "--settings", r#"{"disableAllHooks":true,"autoMemoryEnabled":false}"#]);
        }
        cmd.arg("--strict-mcp-config");
        match room {
            Some(r) => cmd.arg(format!("--mcp-config={}", r.display())),
            None => cmd.args(["--mcp-config", r#"{"mcpServers":{}}"#]),
        };
    }
    if caps.permission_prompts {
        cmd.args(["--permission-prompts", "none"]);
    }
    cmd.args(["--disable-slash-commands", "--append-system-prompt", system]);
    if let Some(id) = resume {
        cmd.args(["--resume", id]);
    }
    if images.is_empty() {
        cmd.arg(prompt).stdin(Stdio::null());
    } else {
        cmd.args(["--input-format", "stream-json"]).stdin(Stdio::piped());
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = cmd.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        let mut content = vec![json!({ "type": "text", "text": prompt })];
        content.extend(images.iter().map(|i| match i.media_type.as_str() {
            "application/pdf" => json!({ "type": "document", "title": i.name, "source": { "type": "base64", "media_type": "application/pdf", "data": i.data } }),
            "text/plain" => json!({ "type": "text", "text": format!("The file {} (JSON name), as sent:\n{}", serde_json::to_string(&i.name).unwrap_or_default(), i.data) }),
            _ => json!({ "type": "image", "source": { "type": "base64", "media_type": i.media_type, "data": i.data } }),
        }));
        let line = json!({ "type": "user", "message": { "role": "user", "content": content } }).to_string() + "\n";
        stdin.write_all(line.as_bytes()).await?;
        drop(stdin);
    }
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
