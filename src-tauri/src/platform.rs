// What differs between the Mac and Windows: where Claude Code is and whether it is signed in, the window that signs it
// in, the computer's name, where the log goes, the notification settings, and keeping a file private. The rest of the
// app is the same on both (Windows test build plan, 2 Nov 2026).

use std::path::PathBuf;

pub fn home() -> PathBuf {
    #[cfg(windows)]
    let var = "USERPROFILE";
    #[cfg(not(windows))]
    let var = "HOME";
    PathBuf::from(std::env::var_os(var).unwrap_or_default())
}

// The PATH Claude Code runs with. Mac: the login shell's, since an app opened from Finder gets a bare one and Claude Code
// may be installed through npm or Homebrew. Windows: an app already gets the person's own PATH.
#[cfg(target_os = "macos")]
pub fn login_path() -> String {
    std::process::Command::new("/bin/zsh")
        .args(["-lc", "printf %s \"$PATH\""])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin".into())
}
#[cfg(not(target_os = "macos"))]
pub fn login_path() -> String {
    std::env::var("PATH").unwrap_or_default()
}

// Where Claude Code may be, in order. Windows: claude.exe, from the official installer into %USERPROFILE%\.local\bin
// (code.claude.com/docs/en/setup, read 2 Nov 2026), or on the PATH. npm's claude.cmd isn't run: a .cmd goes through
// cmd.exe, which would read room text in its arguments as its own syntax.
pub fn claude_places(path: &str) -> Vec<PathBuf> {
    let h = home();
    #[cfg(windows)]
    {
        let mut places: Vec<PathBuf> = std::env::split_paths(path).map(|d| d.join("claude.exe")).collect();
        places.push(h.join(".local").join("bin").join("claude.exe"));
        places
    }
    #[cfg(not(windows))]
    {
        let mut places: Vec<PathBuf> = path.split(':').map(|d| PathBuf::from(d).join("claude")).collect();
        places.extend([h.join(".claude/local/claude"), h.join(".local/bin/claude"), "/opt/homebrew/bin/claude".into(), "/usr/local/bin/claude".into()]);
        places
    }
}

// Claude Code's sign-in in the Mac's login Keychain. Windows keeps it only in %USERPROFILE%\.claude\.credentials.json
// (code.claude.com/docs/en/authentication, read 2 Nov 2026), which the caller checks on both.
#[cfg(target_os = "macos")]
pub fn signed_in_keychain() -> bool {
    std::process::Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", "Claude Code-credentials"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
#[cfg(not(target_os = "macos"))]
pub fn signed_in_keychain() -> bool {
    false
}

// Claude Code signs in in its own window, with its own sign-in: Terminal on a Mac, a console window on Windows.
#[cfg(target_os = "macos")]
pub fn open_sign_in(bin: &std::path::Path) {
    let script = format!("tell application \"Terminal\"\nactivate\ndo script \"{} /login\"\nend tell", bin.display().to_string().replace('"', "\\\""));
    let _ = std::process::Command::new("/usr/bin/osascript").args(["-e", &script]).spawn();
}
#[cfg(windows)]
pub fn open_sign_in(bin: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    let _ = std::process::Command::new(bin).arg("/login").current_dir(home()).creation_flags(CREATE_NEW_CONSOLE).spawn();
}
#[cfg(not(any(target_os = "macos", windows)))]
pub fn open_sign_in(_bin: &std::path::Path) {}

// Claude Code runs without a console window popping up on Windows.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub fn quiet(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
pub fn quiet_async(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(target_os = "macos")]
fn raw_computer_name() -> Option<String> {
    std::process::Command::new("/usr/sbin/scutil").args(["--get", "ComputerName"]).output().ok().and_then(|o| String::from_utf8(o.stdout).ok())
}
#[cfg(not(target_os = "macos"))]
fn raw_computer_name() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}
pub fn computer_name() -> Option<String> {
    raw_computer_name()
        .map(|s| s.trim().chars().filter(|c| !c.is_control() && *c != '<' && *c != '>').take(60).collect::<String>())
        .filter(|s| !s.is_empty())
}

// Mac: ~/Library/Logs/Wayza. Windows: %LOCALAPPDATA%\Wayza\Logs.
pub fn log_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| home().join("AppData").join("Local")).join("Wayza").join("Logs")
    }
    #[cfg(not(windows))]
    {
        home().join("Library/Logs/Wayza")
    }
}

// The system's own notification settings for the app.
pub fn notification_settings_url(id: &str) -> String {
    #[cfg(windows)]
    {
        let _ = id;
        "ms-settings:notifications".into()
    }
    #[cfg(not(windows))]
    {
        format!("x-apple.systempreferences:com.apple.Notifications-Settings.extension?id={id}")
    }
}

// A file only this computer's user can read. Mac: mode 0600. Windows: files under the person's own profile folder
// already allow only them (and the system), so it is a plain file there.
pub fn create_private(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)
}
