mod ansi;
mod pty;

use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    widgets::{Paragraph, Wrap},
    Frame,
};

use ansi::TerminalScreen;
use pty::Pty;

const PTY_READ_BUF: usize = 8192;

fn main() -> anyhow::Result<()> {
    // ── Terminal setup ──────────────────────────────────────────────
    let mut stdout = std::io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
    )?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // ── Get initial terminal size ───────────────────────────────────
    let (cols, rows) = crossterm::terminal::size()?;
    let (cols, rows) = (cols.max(1), rows.max(1));

    // ── Spawn Claude in PTY ─────────────────────────────────────────
    let mut pty = Pty::spawn(rows, cols)?;
    let mut screen = TerminalScreen::new(rows, cols);

    // ── Main event loop ─────────────────────────────────────────────
    let mut pty_buf = vec![0u8; PTY_READ_BUF];
    let mut running = true;

    while running {
        // ── Drain PTY output ────────────────────────────────────
        match pty.read(&mut pty_buf) {
            Ok(n) if n > 0 => {
                screen.process(&pty_buf[..n]);
            }
            Ok(_) => {} // No data available
            Err(e) => {
                eprintln!("PTY read error: {e}");
                running = false;
            }
        }

        // ── Handle input events (with timeout for responsiveness) ───
        while crossterm::event::poll(Duration::from_millis(1))? {
            match crossterm::event::read()? {
                Event::Key(key) => {
                    // Quit on Ctrl+Q
                    if key.code == KeyCode::Char('q')
                        && key.modifiers == KeyModifiers::CONTROL
                    {
                        running = false;
                        break;
                    }

                    if let Some(bytes) = encode_key(key) {
                        if let Err(e) = pty.write(&bytes) {
                            eprintln!("PTY write error: {e}");
                            running = false;
                            break;
                        }
                    }
                }
                Event::Resize(w, h) => {
                    if let Err(e) = pty.resize(h, w) {
                        eprintln!("PTY resize error: {e}");
                    }
                    screen.resize(h, w);
                }
                Event::Mouse(_) => {} // Ignore mouse for Phase 1
                _ => {}
            }
        }

        // ── Render frame ─────────────────────────────────────────
        terminal.draw(|f| {
            render_ui(f, &screen);
        })?;

        // ── Check if child exited ────────────────────────────────
        if pty.try_wait().is_some() {
            running = false;
        }
    }

    // ── Cleanup ─────────────────────────────────────────────────────
    let _ = pty.kill();
    crossterm::execute!(
        std::io::stdout(),
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen,
    )?;
    crossterm::terminal::disable_raw_mode()?;

    Ok(())
}

/// Convert a crossterm `KeyEvent` into the byte sequence a terminal program expects.
///
/// This encodes the key as though a real terminal had transmitted it over the wire.
/// Returns `None` for keys we don't handle yet (unrecognized combos are dropped).
fn encode_key(key: KeyEvent) -> Option<Vec<u8>> {
    // Unhandled complex modifier combos
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && key.modifiers.contains(KeyModifiers::ALT)
    {
        return None;
    }

    match key.code {
        // ── Regular characters ──────────────────────────────────────
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+letter → control character (1–26)
                let byte = match c {
                    'a'..='z' => (c as u8) - b'a' + 1,
                    'A'..='Z' => (c as u8) - b'A' + 1,
                    '@' => 0x00,  // Ctrl+@ → NUL
                    '[' => 0x1b,  // Ctrl+[ → ESC
                    '\\' => 0x1c,
                    ']' => 0x1d,
                    '^' => 0x1e,
                    '_' => 0x1f,
                    '?' => 0x7f,
                    ' ' => 0x00,  // Ctrl+Space → NUL
                    _ => return None,
                };
                Some(vec![byte])
            } else if key.modifiers.contains(KeyModifiers::ALT) {
                // Alt+char → ESC prefix + char
                let mut c_buf = [0u8; 4];
                let cs = c.encode_utf8(&mut c_buf);
                let mut result = vec![0x1b];
                result.extend_from_slice(cs.as_bytes());
                Some(result)
            } else {
                // Plain char → UTF-8
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                Some(s.as_bytes().to_vec())
            }
        }

        // ── Whitespace / control ────────────────────────────────────
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),

        // ── Arrow keys ──────────────────────────────────────────────
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),

        // ── Navigation ──────────────────────────────────────────────
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),

        // ── Function keys ───────────────────────────────────────────
        KeyCode::F(n) => encode_fn_key(n),

        _ => None,
    }
}

/// Encode function key F1–F12.
fn encode_fn_key(n: u8) -> Option<Vec<u8>> {
    match n {
        1 => Some(vec![0x1b, b'O', b'P']),
        2 => Some(vec![0x1b, b'O', b'Q']),
        3 => Some(vec![0x1b, b'O', b'R']),
        4 => Some(vec![0x1b, b'O', b'S']),
        5 => Some(vec![0x1b, b'[', b'1', b'5', b'~']),
        6 => Some(b"\x1b[17~".to_vec()),
        7 => Some(b"\x1b[18~".to_vec()),
        8 => Some(b"\x1b[19~".to_vec()),
        9 => Some(b"\x1b[20~".to_vec()),
        10 => Some(b"\x1b[21~".to_vec()),
        11 => Some(b"\x1b[23~".to_vec()),
        12 => Some(b"\x1b[24~".to_vec()),
        _ => None,
    }
}

/// Render the cds PTY output as a full-screen fixed grid with styling.
fn render_ui(f: &mut Frame, screen: &TerminalScreen) {
    let area = f.area();
    let text = screen.render();
    let widget = Paragraph::new(text)
        .wrap(Wrap { trim: false });
    f.render_widget(widget, area);
}
