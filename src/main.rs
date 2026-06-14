mod ansi;
mod pty;
mod session;
mod sidebar;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Line as TLine,
    widgets::{Paragraph, Wrap},
    Frame,
};

use ansi::TerminalScreen;
use pty::Pty;
use sidebar::Sidebar;

const PTY_READ_BUF: usize = 8192;
const SESSION_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// Fraction of terminal width given to the sidebar.
const SIDEBAR_FRACTION: u16 = 30; // percent

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Sidebar,
    Pty,
}

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
    let mut term_cols = cols.max(1);
    let mut term_rows = rows.max(1);

    // PTY gets only the right-hand panel (100 - SIDEBAR_FRACTION)% of width
    let pty_cols = term_cols * (100 - SIDEBAR_FRACTION) / 100;
    let pty_rows = term_rows;

    // ── Spawn cds in PTY ────────────────────────────────────────────
    let mut pty = Pty::spawn(pty_rows, pty_cols.max(1))?;
    let mut screen = TerminalScreen::new(pty_rows, pty_cols.max(1));
    let mut last_pty_cols = pty_cols;
    let mut last_pty_rows = pty_rows;

    // ── Sidebar setup ───────────────────────────────────────────────
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let sessions_dir = PathBuf::from(home).join(".claude").join("sessions");

    let mut sidebar = Sidebar::new();
    sidebar.refresh(session::load_all(&sessions_dir).unwrap_or_default());
    let mut last_session_refresh = Instant::now();

    // ── Focus state ─────────────────────────────────────────────────
    let mut focus = Focus::Pty;

    // ── Main event loop ─────────────────────────────────────────────
    let mut pty_buf = vec![0u8; PTY_READ_BUF];
    let mut running = true;

    while running {
        // ── Drain PTY output ────────────────────────────────────
        match pty.read(&mut pty_buf) {
            Ok(n) if n > 0 => {
                screen.process(&pty_buf[..n]);
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("PTY read error: {e}");
                running = false;
            }
        }

        // ── Refresh sessions periodically ───────────────────────
        if last_session_refresh.elapsed() >= SESSION_REFRESH_INTERVAL {
            sidebar.refresh(session::load_all(&sessions_dir).unwrap_or_default());
            last_session_refresh = Instant::now();
        }

        // ── Handle input events ─────────────────────────────────
        while crossterm::event::poll(Duration::from_millis(1))? {
            match crossterm::event::read()? {
                Event::Key(key) => {
                    if key.code == KeyCode::Char('q')
                        && key.modifiers == KeyModifiers::CONTROL
                    {
                        running = false;
                        break;
                    }

                    if key.code == KeyCode::Tab && key.modifiers.is_empty() {
                        focus = match focus {
                            Focus::Sidebar => Focus::Pty,
                            Focus::Pty => Focus::Sidebar,
                        };
                        continue;
                    }

                    match focus {
                        Focus::Sidebar => match key.code {
                            KeyCode::Up | KeyCode::Char('k') => sidebar.select_prev(),
                            KeyCode::Down | KeyCode::Char('j') => sidebar.select_next(),
                            KeyCode::Enter => focus = Focus::Pty,
                            _ => {}
                        },
                        Focus::Pty => {
                            if let Some(bytes) = encode_key(key) {
                                if let Err(e) = pty.write(&bytes) {
                                    eprintln!("PTY write error: {e}");
                                    running = false;
                                    break;
                                }
                            }
                        }
                    }
                }
                Event::Resize(w, h) => {
                    term_cols = w.max(1);
                    term_rows = h.max(1);
                }
                Event::Mouse(_) => {}
                _ => {}
            }
        }

        // ── Resize PTY if the panel dimensions changed ──────────
        let pty_cols = term_cols * (100 - SIDEBAR_FRACTION) / 100;
        let pty_rows = term_rows;
        if pty_cols != last_pty_cols || pty_rows != last_pty_rows {
            let _ = pty.resize(pty_rows, pty_cols.max(1));
            screen.resize(pty_rows, pty_cols.max(1));
            last_pty_cols = pty_cols;
            last_pty_rows = pty_rows;
        }

        // ── Render frame ─────────────────────────────────────────
        let current_focus = focus;
        terminal.draw(|f| {
            render_ui(f, &screen, &mut sidebar, current_focus);
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

// ── Key encoding ────────────────────────────────────────────────────

fn encode_key(key: KeyEvent) -> Option<Vec<u8>> {
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && key.modifiers.contains(KeyModifiers::ALT)
    {
        return None;
    }

    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let byte = match c {
                    'a'..='z' => (c as u8) - b'a' + 1,
                    'A'..='Z' => (c as u8) - b'A' + 1,
                    '@' => 0x00,
                    '[' => 0x1b,
                    '\\' => 0x1c,
                    ']' => 0x1d,
                    '^' => 0x1e,
                    '_' => 0x1f,
                    '?' => 0x7f,
                    ' ' => 0x00,
                    _ => return None,
                };
                Some(vec![byte])
            } else if key.modifiers.contains(KeyModifiers::ALT) {
                let mut c_buf = [0u8; 4];
                let cs = c.encode_utf8(&mut c_buf);
                let mut result = vec![0x1b];
                result.extend_from_slice(cs.as_bytes());
                Some(result)
            } else {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                Some(s.as_bytes().to_vec())
            }
        }
        KeyCode::Tab => None, // consumed by focus switching
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::F(n) => encode_fn_key(n),
        _ => None,
    }
}

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

// ── Rendering ────────────────────────────────────────────────────────

fn render_ui(f: &mut Frame, screen: &TerminalScreen, sidebar: &mut Sidebar, focus: Focus) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(SIDEBAR_FRACTION),
            Constraint::Percentage(100 - SIDEBAR_FRACTION),
        ])
        .split(area);

    // Sidebar (left)
    sidebar.render(f, chunks[0]);

    // PTY panel (right) — no border, full area. Focus shown in status bar.
    let text = screen.render();
    let pty_style = match focus {
        Focus::Pty => Style::default(),
        Focus::Sidebar => Style::default().fg(Color::DarkGray),
    };

    let widget = Paragraph::new(text)
        .style(pty_style)
        .wrap(Wrap { trim: false });

    f.render_widget(widget, chunks[1]);

    // Bottom status bar
    let status = match focus {
        Focus::Pty => TLine::from(" cds  │  Ctrl+Q quit  │  Tab → sidebar "),
        Focus::Sidebar => TLine::from("◀◀ SIDEBAR ▶▶  │  ↑↓/jk navigate  │  Enter → cds  │  Tab → cds"),
    };
    let status_area = ratatui::layout::Rect::new(
        area.x,
        area.y + area.height.saturating_sub(1),
        area.width,
        1,
    );
    f.render_widget(
        Paragraph::new(status)
            .style(Style::default().fg(Color::Black).bg(Color::Rgb(180, 180, 180))),
        status_area,
    );
}
