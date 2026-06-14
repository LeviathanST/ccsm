mod ansi;
mod pty;
mod registry;
mod session;
mod sidebar;
mod transcript;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Line as TLine,
    widgets::{Block, Paragraph, Wrap},
    Frame,
};

use ansi::TerminalScreen;
use pty::Pty;
use sidebar::Sidebar;
use transcript::TranscriptView;

const PTY_READ_BUF: usize = 8192;
const SESSION_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// Fraction of terminal width given to the sidebar.
const SIDEBAR_FRACTION: u16 = 30;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Sidebar,
    Pty,
}

/// What's shown in the right panel.
enum ViewMode {
    /// Live cds PTY (default).
    Live,
    /// Viewing a session transcript.
    Transcript(Box<TranscriptView>),
}

fn main() -> anyhow::Result<()> {
    // ── Workspace ───────────────────────────────────────────────────
    let workspace = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let workspace = std::fs::canonicalize(&workspace).unwrap_or(workspace);

    // ── Session data paths ──────────────────────────────────────────
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let sessions_dir = PathBuf::from(&home).join(".claude").join("sessions");
    let global_registry_path = PathBuf::from(&home).join(".claude").join("sessions.json");

    // ── Build/refresh global registry (Tier 1) ─────────────────────
    let _ = registry::GlobalRegistry::load_or_build(&global_registry_path, &sessions_dir)
        .and_then(|r| r.save(&global_registry_path));

    // ── Load workspace registry (Tier 2), seed if empty ────────────
    let mut workspace_registry =
        registry::WorkspaceRegistry::load(&workspace).unwrap_or_else(|_| {
            registry::WorkspaceRegistry::empty()
        });
    workspace_registry.seed(registry::WorkspaceRegistry::default_seed());
    let ws_path_str = workspace.to_string_lossy().to_string();
    let _ = workspace_registry.merge_live_sessions(&sessions_dir, &ws_path_str);
    let _ = workspace_registry.save(&workspace);

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

    let pty_cols = term_cols * (100 - SIDEBAR_FRACTION) / 100;
    let pty_rows = term_rows;

    // ── Spawn cds in PTY ────────────────────────────────────────────
    let mut pty = Pty::spawn(pty_rows, pty_cols.max(1), &workspace)?;
    let mut screen = TerminalScreen::new(pty_rows, pty_cols.max(1));
    let mut last_pty_cols = pty_cols;
    let mut last_pty_rows = pty_rows;

    // ── Sidebar ─────────────────────────────────────────────────────
    let mut sidebar = Sidebar::new();
    sidebar.refresh(session::load_all(&sessions_dir, Some(&workspace)).unwrap_or_default());
    let mut last_session_refresh = Instant::now();

    // ── State ───────────────────────────────────────────────────────
    let mut focus = Focus::Pty;
    let mut view: ViewMode = ViewMode::Live;

    // ── Main event loop ─────────────────────────────────────────────
    let mut pty_buf = vec![0u8; PTY_READ_BUF];
    let mut running = true;

    while running {
        // ── Drain PTY output (always, even in transcript mode) ──
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

        // ── Refresh sessions & merge into registry ──────────────
        if last_session_refresh.elapsed() >= SESSION_REFRESH_INTERVAL {
            sidebar.refresh(
                session::load_all(&sessions_dir, Some(&workspace)).unwrap_or_default(),
            );
            // Merge live session data into the workspace registry
            let _ = workspace_registry
                .merge_live_sessions(&sessions_dir, &ws_path_str);
            let _ = workspace_registry.save(&workspace);
            // Rebuild global overview
            if let Ok(gr) =
                registry::GlobalRegistry::build(&sessions_dir)
            {
                let _ = gr.save(&global_registry_path);
            }
            last_session_refresh = Instant::now();
        }

        // ── Handle input ────────────────────────────────────────
        while crossterm::event::poll(Duration::from_millis(1))? {
            match crossterm::event::read()? {
                Event::Key(key) => {
                    // Global: Ctrl+Q always quits
                    if key.code == KeyCode::Char('q')
                        && key.modifiers == KeyModifiers::CONTROL
                    {
                        running = false;
                        break;
                    }

                    // ── Transcript mode: any navigation key, Esc/Tab to exit ──
                    if let ViewMode::Transcript(ref mut tv) = view {
                        match key.code {
                            KeyCode::Esc | KeyCode::Tab => {
                                view = ViewMode::Live;
                                continue;
                            }
                            KeyCode::Up | KeyCode::Char('k') => tv.scroll_up(1),
                            KeyCode::Down | KeyCode::Char('j') => tv.scroll_down(1),
                            KeyCode::PageUp => tv.scroll_up(10),
                            KeyCode::PageDown => tv.scroll_down(10),
                            KeyCode::Home => tv.scroll = 0,
                            _ => {}
                        }
                        continue;
                    }

                    // ── Live mode ───────────────────────────────
                    // Global: Tab toggles focus
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
                            KeyCode::Enter => {
                                // Try to load and view the session transcript
                                if let Some(s) = sidebar.selected_session() {
                                    let path = transcript::transcript_path(
                                        &home, &s.cwd, &s.session_id,
                                    );
                                    if let Some(p) = path {
                                        match TranscriptView::load(&p, s.display_name())
                                        {
                                            Ok(tv) => {
                                                view = ViewMode::Transcript(Box::new(tv));
                                            }
                                            Err(_) => {
                                                view = ViewMode::Transcript(Box::new(
                                                    TranscriptView::empty(s.display_name()),
                                                ));
                                            }
                                        }
                                    } else {
                                        view = ViewMode::Transcript(Box::new(
                                            TranscriptView::empty(s.display_name()),
                                        ));
                                    }
                                }
                            }
                            KeyCode::Char(' ') => focus = Focus::Pty,
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

        // ── Resize PTY ──────────────────────────────────────────
        let pty_cols = term_cols * (100 - SIDEBAR_FRACTION) / 100;
        let pty_rows = term_rows;
        if pty_cols != last_pty_cols || pty_rows != last_pty_rows {
            let _ = pty.resize(pty_rows, pty_cols.max(1));
            screen.resize(pty_rows, pty_cols.max(1));
            last_pty_cols = pty_cols;
            last_pty_rows = pty_rows;
        }

        // ── Render ──────────────────────────────────────────────
        let current_focus = focus;
        terminal.draw(|f| {
            render_ui(f, &screen, &mut sidebar, current_focus, &view);
        })?;

        // ── Check child exit (only quit in live mode) ──────────
        if matches!(view, ViewMode::Live) && pty.try_wait().is_some() {
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
        KeyCode::Tab => None,
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

fn render_ui(
    f: &mut Frame,
    screen: &TerminalScreen,
    sidebar: &mut Sidebar,
    focus: Focus,
    view: &ViewMode,
) {
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

    // Right panel — live PTY or transcript
    match view {
        ViewMode::Live => {
            let text = screen.render();
            let pty_style = match focus {
                Focus::Pty => Style::default(),
                Focus::Sidebar => Style::default().fg(Color::DarkGray),
            };
            f.render_widget(
                Paragraph::new(text).style(pty_style).wrap(Wrap { trim: false }),
                chunks[1],
            );
        }
        ViewMode::Transcript(tv) => {
            let available = chunks[1].height as usize;
            let text = tv.render(available);

            let widget = Paragraph::new(text)
                .block(
                    Block::bordered()
                        .title_top(format!(" {} ", tv.session_name))
                        .title_bottom(" Esc/Tab → back to cds  │  ↑↓/PgUp/PgDn scroll ")
                        .border_style(Style::default().fg(Color::Rgb(120, 180, 255))),
                )
                .wrap(Wrap { trim: true })
                .scroll((0, 0));

            f.render_widget(widget, chunks[1]);
        }
    }

    // Bottom status bar
    let status = match (focus, view) {
        (_, ViewMode::Transcript(tv)) => {
            let pct = if tv.line_count > 0 {
                tv.scroll * 100 / tv.line_count
            } else {
                0
            };
            TLine::from(format!(
                " REPLAY: {}  │  {}/{} lines ({}%)  │  Esc/Tab → back ",
                tv.session_name, tv.scroll, tv.line_count, pct
            ))
        }
        (Focus::Pty, ViewMode::Live) => {
            TLine::from(" cds  │  Ctrl+Q quit  │  Tab → sidebar ")
        }
        (Focus::Sidebar, ViewMode::Live) => {
            TLine::from("◀◀ SIDEBAR ▶▶  │  ↑↓/jk nav  │  Enter → replay  │  Space → cds  │  Tab → cds")
        }
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
