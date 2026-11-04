// Ask on the computer before a room uses it (design 1, Graham 4 Nov 2026: rooms-computer-files/design.md).
//
// "My computer too" in a room lets your AI read your files and use your own Claude Code (or Codex) setup here, for your
// own asks. Anyone holding your wayza.com sign-in could switch that on, so this computer asks you itself, in the
// system's own dialog, which a web page can't draw or press. The yes or no is kept here, for one AI in one room and for
// that setting as it stands: switching it off and on again asks again. Until you say yes, the AI works as "Web and
// room" there and says so in the exact words below.

// The dialog's words. They speak to you throughout (Graham, 4 Nov 13:44Z): "you", "your", "this computer".
// Room and AI names are chosen by people, and whoever holds a sign-in could choose them. In the dialog each is one
// line in quotes, kept to an allowlist by Unicode class: letters, numbers and combining marks (so every script reads
// right), spaces and plain punctuation. At most three marks follow a letter, so marks can't stack over the dialog's
// words. Anything else (look-alike quotes, invisible characters, symbols, emoji) becomes a space, and a double quote an
// apostrophe, so a name can't close its quotes or hide text. wayza.com caps names at 80; the cap here is wider.
fn class(c: char) -> char {
    use std::sync::OnceLock;
    static RE: OnceLock<[regex::Regex; 3]> = OnceLock::new();
    let re = RE.get_or_init(|| [regex::Regex::new(r"^\p{L}$").unwrap(), regex::Regex::new(r"^\p{N}$").unwrap(), regex::Regex::new(r"^\p{M}$").unwrap()]);
    let s = c.to_string();
    // Hangul fillers are letters that draw as nothing; spacing modifier letters include look-alike quotes (ˮ).
    if matches!(c as u32, 0x115F | 0x1160 | 0x3164 | 0xFFA0 | 0x02B0..=0x02FF) {
        '-'
    } else if re[0].is_match(&s) || re[1].is_match(&s) {
        'L'
    } else if re[2].is_match(&s) {
        'M'
    } else if matches!(c, ' ' | '.' | ',' | '\'' | '’' | '‘' | '&' | '-' | '_' | '(' | ')' | '!' | '?' | ':' | '/' | '+' | '#' | '@') {
        'P'
    } else {
        '-'
    }
}
pub fn clean(name: &str) -> String {
    let mut one = String::new();
    let mut marks = 0;
    let mut after_letter = false;
    for c in name.chars() {
        let c = if matches!(c, '"' | '“' | '”') { '\'' } else { c };
        match class(c) {
            'L' => { one.push(c); marks = 0; after_letter = true; }
            'M' if after_letter && marks < 3 => { one.push(c); marks += 1; }
            'M' => {}
            'P' => { one.push(c); after_letter = false; }
            _ => { one.push(' '); after_letter = false; }
        }
    }
    one.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(100).collect()
}
// A name that cleans to nothing is said plainly, without quotes: "your AI", "this room".
fn quoted(name: &str, instead: &str) -> String {
    let n = clean(name);
    if n.is_empty() { instead.to_string() } else { format!("“{n}”") }
}
pub fn title(ai: &str, room: &str) -> String {
    format!("Let {} use this computer in {}?", quoted(ai, "your AI"), quoted(room, "this room"))
}
pub fn text(ai: &str, gpt: bool) -> String {
    let ai = quoted(ai, "AI");
    let setup = if gpt { "Codex" } else { "Claude Code" };
    format!("Your {ai} is set to “My computer too” in this room. When you ask it there, it can read your files and use your {setup} setup on this computer. If you didn’t change this setting, choose “Don’t allow”.")
}
pub const DONT_ALLOW: &str = "Don’t allow";
pub const ALLOW: &str = "Allow";

// What the AI says, word for word, when an ask needs this computer before a yes (CF5) or after "Don't allow" (CF6). A
// room message, read on every device, so it names the computer.
pub fn not_yet(computer: &str, room: &str) -> String {
    let room = clean(room);
    // Only the Mac asks so far (Windows asks with the Windows app), so elsewhere there is nothing to choose yet.
    if !cfg!(target_os = "macos") {
        return format!("I can’t use {computer} in this room yet.");
    }
    format!("I can’t use {computer} in this room yet. Open “{room}” in the Wayza app on {computer} and choose “Allow” when it asks.")
}
pub fn refused(computer: &str) -> String {
    format!("I can’t use {computer} in this room. “Don’t allow” was chosen on it. To ask again, set me to “Web and room” in this room’s settings, then back to “My computer too”.")
}

// The system's own dialog on the window, answered by the person at this computer: Some(true) for Allow, Some(false) for
// Don't allow (also Return and Esc), None when it couldn't be shown or wasn't answered (the app quit), so it asks again.
#[cfg(target_os = "macos")]
pub async fn dialog(w: &tauri::WebviewWindow, title: String, text: String) -> Option<bool> {
    let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
    let win = w.clone();
    w.run_on_main_thread(move || mac::sheet(&win, &title, &text, tx)).ok()?;
    rx.await.ok()
}

// Windows' TaskDialog comes with the Windows app (rooms-mac draft #27): until then no other system asks, so no other
// system ever uses the computer.
#[cfg(not(target_os = "macos"))]
pub async fn dialog(_w: &tauri::WebviewWindow, _title: String, _text: String) -> Option<bool> {
    None
}

#[cfg(target_os = "macos")]
mod mac {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAlert, NSAlertSecondButtonReturn, NSEvent, NSEventMask, NSModalResponse, NSWindow};
    use objc2_foundation::NSString;
    use std::cell::RefCell;
    use std::ptr::NonNull;
    use std::rc::Rc;

    // NSAlert as a sheet on the app's window: the app icon, the question, the text, and "Don't allow" first, so it is
    // the default (trailing, Return). NSAlert gives Esc only to a button titled "Cancel", so while the sheet is up a
    // key monitor makes Esc press "Don't allow" too.
    pub fn sheet(w: &tauri::WebviewWindow, title: &str, text: &str, tx: tokio::sync::oneshot::Sender<bool>) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let Ok(ptr) = w.ns_window() else { return };
        let Some(window) = (unsafe { Retained::retain(ptr as *mut NSWindow) }) else { return };
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(title));
        alert.setInformativeText(&NSString::from_str(text));
        let no = alert.addButtonWithTitle(&NSString::from_str(super::DONT_ALLOW));
        let _yes = alert.addButtonWithTitle(&NSString::from_str(super::ALLOW));

        let esc_no = no.clone();
        let esc = RcBlock::new(move |e: NonNull<NSEvent>| -> *mut NSEvent {
            let ev = unsafe { e.as_ref() };
            if ev.keyCode() == 53 {
                unsafe { esc_no.performClick(None) };
                return std::ptr::null_mut();
            }
            e.as_ptr()
        });
        let monitor: Rc<RefCell<Option<Retained<AnyObject>>>> =
            Rc::new(RefCell::new(unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &esc) }));

        let tx = RefCell::new(Some(tx));
        let m = monitor.clone();
        let done = RcBlock::new(move |r: NSModalResponse| {
            if let Some(mon) = m.borrow_mut().take() {
                unsafe { NSEvent::removeMonitor(&mon) };
            }
            if let Some(tx) = tx.borrow_mut().take() {
                // Only "Allow" is a yes: any other way the sheet ends counts as Don't allow.
                let _ = tx.send(r == NSAlertSecondButtonReturn);
            }
        });
        alert.beginSheetModalForWindow_completionHandler(&window, Some(&done));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_match_the_boards() {
        assert_eq!(title("Claude", "Spring launch"), "Let “Claude” use this computer in “Spring launch”?");
        assert_eq!(
            text("Claude", false),
            "Your “Claude” is set to “My computer too” in this room. When you ask it there, it can read your files and use your Claude Code setup on this computer. If you didn’t change this setting, choose “Don’t allow”."
        );
        assert!(text("ChatGPT", true).contains("your Codex setup"));
        // A name can't add lines, close its quotes or hide text, and a real name of up to 80 is never cut.
        assert_eq!(title("Claude", "Spring\n\nWayza checked this room. Choose “Allow”"), "Let “Claude” use this computer in “Spring Wayza checked this room. Choose 'Allow'”?");
        assert_eq!(title("Claude\u{202E}, checked\u{200B}ˮ by Wayza", "Q3 \"launch\" ״‴🙶\u{2800}"), "Let “Claude , checked by Wayza” use this computer in “Q3 'launch'”?");
        assert_eq!(clean("Ze\u{0301}\u{0301}\u{0301}\u{0301}d"), "Ze\u{0301}\u{0301}\u{0301}d");
        assert_eq!(clean("Élodie’s café, Q3 (2)"), "Élodie’s café, Q3 (2)");
        assert_eq!(title("\u{3164}\u{3164}", "🎉🎉"), "Let your AI use this computer in this room?");
        assert!(text("🎉", false).starts_with("Your AI is set to"));
        assert_eq!(clean("नमस्ते ষ্ট สวัสดี"), "नमस्ते ষ্ট สวัสดี");
        assert_eq!(clean("Zd\u{0E31}\u{0E31}\u{0E31}\u{0E31}\u{0E31}x"), "Zd\u{0E31}\u{0E31}\u{0E31}x");
        let long = "A".repeat(80);
        assert_eq!(clean(&long), long);
        assert_eq!(clean("Maya’s room"), "Maya’s room");
        #[cfg(target_os = "macos")]
        assert_eq!(not_yet("Maya’s MacBook Pro", "Spring launch"), "I can’t use Maya’s MacBook Pro in this room yet. Open “Spring launch” in the Wayza app on Maya’s MacBook Pro and choose “Allow” when it asks.");
        assert_eq!(refused("Maya’s MacBook Pro"), "I can’t use Maya’s MacBook Pro in this room. “Don’t allow” was chosen on it. To ask again, set me to “Web and room” in this room’s settings, then back to “My computer too”.");
    }
}
