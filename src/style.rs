use owo_colors::{OwoColorize, Stream};
use std::io::IsTerminal;

// ── Color Helpers (stdout) ─────────────────────────────────────

/// Blue — primary action, active state
pub fn primary(text: &str) -> String {
    text.if_supports_color(Stream::Stdout, |s| s.blue()).to_string()
}

/// Green — completed, success
pub fn success(text: &str) -> String {
    text.if_supports_color(Stream::Stdout, |s| s.green()).to_string()
}

/// Yellow — blocked, warning
pub fn warning(text: &str) -> String {
    text.if_supports_color(Stream::Stdout, |s| s.yellow()).to_string()
}

/// Red — abandoned, error
#[allow(dead_code)]
pub fn error(text: &str) -> String {
    text.if_supports_color(Stream::Stdout, |s| s.red()).to_string()
}

/// Dim — pending, inactive
pub fn dim(text: &str) -> String {
    text.if_supports_color(Stream::Stdout, |s| s.dimmed()).to_string()
}

/// Cyan — informational
pub fn info(text: &str) -> String {
    text.if_supports_color(Stream::Stdout, |s| s.cyan()).to_string()
}

// ── Color Helpers (stderr) ─────────────────────────────────────

/// Red for stderr errors
pub fn error_stderr(text: &str) -> String {
    text.if_supports_color(Stream::Stderr, |s| s.red()).to_string()
}

/// Yellow for stderr warnings
pub fn warning_stderr(text: &str) -> String {
    text.if_supports_color(Stream::Stderr, |s| s.yellow()).to_string()
}

/// Cyan for stderr info
#[allow(dead_code)]
pub fn info_stderr(text: &str) -> String {
    text.if_supports_color(Stream::Stderr, |s| s.cyan()).to_string()
}

// ── Status Label Styling ───────────────────────────────────────

/// Color a session status label based on its value.
pub fn status_label(status: &str) -> String {
    match status {
        "in_progress" => status.if_supports_color(Stream::Stdout, |s| s.blue()).to_string(),
        "completed" => status.if_supports_color(Stream::Stdout, |s| s.green()).to_string(),
        "blocked" => status.if_supports_color(Stream::Stdout, |s| s.yellow()).to_string(),
        "abandoned" => status.if_supports_color(Stream::Stdout, |s| s.red()).to_string(),
        "pending" => status.if_supports_color(Stream::Stdout, |s| s.dimmed()).to_string(),
        "trashed" => status.if_supports_color(Stream::Stdout, |s| s.dimmed()).to_string(),
        _ => status.to_string(),
    }
}

// ── Emoji Gating ───────────────────────────────────────────────

/// Whether the terminal supports emoji. Disabled when stderr is not a terminal
/// or NO_COLOR is set (since color and emoji are often linked).
pub fn use_emoji() -> bool {
    std::io::stderr().is_terminal()
        && std::env::var_os("NO_COLOR").is_none()
}

/// Return an emoji or ASCII fallback based on terminal support.
pub fn emoji<'a>(emoji: &'a str, fallback: &'a str) -> &'a str {
    if use_emoji() { emoji } else { fallback }
}

// ── Spinner ────────────────────────────────────────────────────

const SPINNER_CHARS: &[u8] = b"|/-\\";

/// Show a simple inline spinner during long operations.
/// Only renders when stderr is a terminal and NO_COLOR is not set.
pub struct Spinner {
    frame: usize,
    message: String,
    enabled: bool,
}

impl Spinner {
    pub fn new(message: &str) -> Self {
        let enabled = std::io::stderr().is_terminal()
            && std::env::var_os("NO_COLOR").is_none();
        let sp = Self { frame: 0, message: message.to_string(), enabled };
        sp.tick();
        sp
    }

    fn tick(&self) {
        if !self.enabled { return; }
        let c = SPINNER_CHARS[self.frame % SPINNER_CHARS.len()] as char;
        let msg = format!("\r{} {}", c, self.message);
        use std::io::Write;
        let _ = write!(std::io::stderr(), "{}", msg);
    }

    pub fn advance(&mut self) {
        self.frame += 1;
        self.tick();
    }

    pub fn set_message(&mut self, msg: &str) {
        self.message = msg.to_string();
        self.tick();
    }

    pub fn done(&self) {
        if !self.enabled { return; }
        use std::io::Write;
        let _ = write!(std::io::stderr(), "\r");
    }
}

/// Color a status icon (used in `ccsm scan`).
pub fn status_icon_styled(icon: &str, status: &str) -> String {
    match status {
        "in_progress" => icon.if_supports_color(Stream::Stdout, |s| s.blue()).to_string(),
        "completed" => icon.if_supports_color(Stream::Stdout, |s| s.green()).to_string(),
        "blocked" => icon.if_supports_color(Stream::Stdout, |s| s.yellow()).to_string(),
        "abandoned" => icon.if_supports_color(Stream::Stdout, |s| s.red()).to_string(),
        "pending" => icon.if_supports_color(Stream::Stdout, |s| s.dimmed()).to_string(),
        "trashed" => icon.if_supports_color(Stream::Stdout, |s| s.dimmed()).to_string(),
        _ => icon.to_string(),
    }
}
