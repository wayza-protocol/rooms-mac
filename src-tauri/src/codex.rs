// The person's own ChatGPT, through the Codex they installed on this computer, with their own ChatGPT sign-in (Bring
// Codex into rooms, agreed by Graham 2 Nov 2026). The app never installs Codex and never sees that sign-in: it only runs
// `codex exec` and reads what it says.
//
// Rooms get a Codex home of their own (CODEX_HOME, in the app's folder), with its own ChatGPT sign-in, done once by
// Codex's own `codex login`. Codex always sends the global AGENTS.md in its home to the model, even with
// --ignore-user-config (tried on 0.166.0, 2 Nov 2026), and in a room anyone can talk to it, so the person's own Codex
// instructions, skills and settings must never be in reach. The room home has none of them, and keeps room
// conversations out of their own Codex history too.
//
// The room's tools for a turn (wayza.com's turn pass) reach Codex as an MCP server whose Bearer token is in an
// environment variable, never on the command line, where `ps` would show it.

use crate::claude::{Image, Run, SignedOut, State, Tools};
use anyhow::{bail, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub const OPENAI_CODEX: &str = "https://github.com/openai/codex#quickstart";

pub fn find(path: &str) -> Option<PathBuf> {
    let h = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let mut places: Vec<PathBuf> = path.split(':').map(|d| PathBuf::from(d).join("codex")).collect();
    places.extend([h.join(".local/bin/codex"), h.join(".npm-global/bin/codex"), "/opt/homebrew/bin/codex".into(), "/usr/local/bin/codex".into()]);
    // Last, the Codex the ChatGPT app for Mac carries inside itself (ChatGPT 26.1027, codex-cli 0.166, 2 Nov 2026): many
    // people have Codex only through that app, which puts it on no PATH. One the person installed themselves comes first.
    for apps in [PathBuf::from("/Applications"), h.join("Applications")] {
        places.push(apps.join("ChatGPT.app/Contents/Resources/codex-cli/bin/codex"));
    }
    places.into_iter().find(|p| p.is_file())
}

// Signed in for rooms: `codex login status` in the rooms' own Codex home says so (exit 0).
fn signed_in(bin: &Path, home: &Path, path: &str) -> bool {
    std::process::Command::new(bin)
        .args(["login", "status"])
        .env("CODEX_HOME", home)
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn state(path: &str, home: &Path) -> State {
    match find(path) {
        None => State::Missing,
        Some(b) if !signed_in(&b, home, path) => State::SignedOut,
        Some(_) => State::Ready,
    }
}

// The Terminal command that signs Codex in for rooms, with Codex's own ChatGPT sign-in page. Single-quoted for the shell.
pub fn login_command(bin: &Path, home: &Path) -> String {
    let q = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', "'\\''"));
    format!("CODEX_HOME={} {} login", q(home), q(bin))
}

// The turn's room tools, as Codex calls them (wayza-room's five; there's no post tool, the app posts the answer).
const ROOM_TOOLS: [&str; 5] = ["room_read", "room_ask", "room_ask_answer", "room_ask_pass", "room_ask_take_back"];

// A string as a TOML value for -c (a JSON string is a valid TOML basic string).
fn toml(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

pub struct Room<'a> {
    pub url: String, // wayza.com's /wayza/mcp
    pub pass: &'a str,
}

// One turn: the prompt in, ChatGPT's answer out, whole (codex exec gives no words as they come, so the room sees no
// growing bubble: a named gap). `resume` carries on the room's own conversation. Photos go in as files Codex is given with -i;
// text files as their words; PDFs aren't read yet (a named gap).
pub async fn run(path: &str, home: &Path, cwd: &PathBuf, system: &str, prompt: &str, images: &[Image], tools: Tools, room: Option<Room<'_>>, resume: Option<&str>) -> Result<Run> {
    let Some(bin) = find(path) else { bail!("Codex isn't on this computer.") };
    std::fs::create_dir_all(cwd)?;
    std::fs::create_dir_all(home)?;
    let mut cmd = Command::new(&bin);
    cmd.current_dir(cwd).env("PATH", path).env("CODEX_HOME", home).env_remove("OPENAI_API_KEY");
    cmd.args(["exec", "--json", "--skip-git-repo-check", "--ignore-user-config", "--ignore-rules", "-C"]).arg(cwd);
    // None of the person's own setup, and nothing beyond answering: no apps, plugins, hooks, memories, helpers or goals.
    for f in ["apps", "plugins", "hooks", "memories", "multi_agent", "goals"] {
        cmd.args(["--disable", f]);
    }
    cmd.args(["-c", "skills.include_instructions=false", "-c", "approval_policy=\"never\""]);
    match tools {
        Tools::Words => {
            cmd.args(["-s", "read-only", "--disable", "shell_tool", "--disable", "unified_exec", "--disable", "view_image", "-c", "web_search=\"disabled\""]);
        }
        Tools::Web => {
            cmd.args(["-s", "read-only", "--disable", "shell_tool", "--disable", "unified_exec", "-c", "web_search=\"live\""]);
        }
        // Only for the person's own ask, in a room set to "Everything on my computer": Codex may read their files (its
        // read-only sandbox: it can run commands that read, never change anything, and has no network but web search).
        Tools::Computer => {
            cmd.args(["-s", "read-only", "-c", "web_search=\"live\""]);
        }
    }
    if let Some(r) = &room {
        cmd.env("WAYZA_ROOM_PASS", r.pass);
        cmd.args(["-c", &format!("mcp_servers.wayza-room.url={}", toml(&r.url))]);
        cmd.args(["-c", "mcp_servers.wayza-room.bearer_token_env_var=\"WAYZA_ROOM_PASS\""]);
        cmd.args(["-c", &format!("mcp_servers.wayza-room.enabled_tools=[{}]", ROOM_TOOLS.iter().map(|t| toml(t)).collect::<Vec<_>>().join(","))]);
    }
    cmd.args(["-c", &format!("developer_instructions={}", toml(system))]);
    // Photos as files for this turn only, in the room's folder; they go when the turn ends.
    let mut files: Vec<PathBuf> = Vec::new();
    let mut words = String::from(prompt);
    for (n, i) in images.iter().enumerate() {
        match i.media_type.as_str() {
            "text/plain" => words.push_str(&format!("\n\nThe file {} (JSON name), as sent:\n{}", serde_json::to_string(&i.name).unwrap_or_default(), i.data)),
            "application/pdf" => words.push_str(&format!("\n\n(The PDF {} was sent too; you can't open PDFs in rooms yet, so say so if it matters.)", serde_json::to_string(&i.name).unwrap_or_default())),
            t => {
                use base64::Engine as _;
                let ext = t.rsplit('/').next().unwrap_or("png");
                let f = cwd.join(format!(".photo-{n}.{ext}"));
                if let Ok(b) = base64::engine::general_purpose::STANDARD.decode(&i.data) {
                    if std::fs::write(&f, b).is_ok() {
                        files.push(f);
                    }
                }
            }
        }
    }
    if let Some(id) = resume {
        cmd.args(["resume", id]);
    }
    for f in &files {
        cmd.arg("-i").arg(f);
    }
    cmd.arg(words).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let out = run_child(cmd).await;
    for f in files {
        let _ = std::fs::remove_file(f);
    }
    out
}

async fn run_child(mut cmd: Command) -> Result<Run> {
    let mut child = cmd.spawn()?;
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let (mut text, mut session, mut failed): (Option<String>, Option<String>, Option<String>) = (None, None, None);
    while let Some(line) = lines.next_line().await? {
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        match v["type"].as_str() {
            Some("thread.started") => session = v["thread_id"].as_str().map(str::to_string),
            Some("item.completed") if v["item"]["type"] == "agent_message" => text = v["item"]["text"].as_str().map(str::to_string),
            Some("turn.failed") => failed = v["error"]["message"].as_str().map(str::to_string).or(Some("Codex stopped.".into())),
            Some("error") => failed = v["message"].as_str().map(str::to_string).or(failed),
            _ => {}
        }
    }
    let status = child.wait().await?;
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        use tokio::io::AsyncReadExt;
        let _ = e.read_to_string(&mut err).await;
    }
    let why = failed.clone().unwrap_or_default() + " " + err.lines().filter(|l| !l.starts_with("WARNING")).last().unwrap_or("");
    // Its ChatGPT sign-in for rooms has ended: the room shows it signed out (MacSignedOutCodex1).
    if text.is_none() && (why.contains("401") || why.contains("Unauthorized") || why.to_lowercase().contains("not logged in") || why.contains("codex login")) {
        bail!(SignedOut(why.trim().to_string()));
    }
    if let Some(f) = failed.filter(|_| text.is_none()) {
        bail!("{}", f);
    }
    if !status.success() && text.is_none() {
        bail!("Codex stopped: {}", why.trim());
    }
    Ok(Run { text: text.unwrap_or_default().trim().to_string(), session })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toml_strings_escape() {
        assert_eq!(toml("a\"b\nc"), "\"a\\\"b\\nc\"");
    }
    #[test]
    fn login_command_quotes_paths() {
        let c = login_command(Path::new("/opt/homebrew/bin/codex"), Path::new("/Users/jo's/Library/Application Support/x"));
        assert_eq!(c, "CODEX_HOME='/Users/jo'\\''s/Library/Application Support/x' '/opt/homebrew/bin/codex' login");
    }
}
