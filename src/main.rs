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
    text::{Line as TLine, Span, Text},
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
    /// No session active — landing screen.
    Landing,
    /// Live cds PTY.
    Live,
    /// Viewing a session transcript.
    Transcript(Box<TranscriptView>),
}

/// Text input mode for creating a new session.
struct InputState {
    prompt: String,
    buffer: String,
    cursor: usize,
}

impl InputState {
    fn new(prompt: &str) -> Self {
        Self {
            prompt: prompt.to_string(),
            buffer: String::new(),
            cursor: 0,
        }
    }

    fn insert(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.buffer.remove(self.cursor);
        }
    }

    fn value(&self) -> &str {
        self.buffer.trim()
    }
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

    // ── PTY: lazy-spawned on first `n` (new session) ───────────────
    let mut pty: Option<Pty> = None;
    let mut screen: Option<TerminalScreen> = None;
    let mut last_pty_cols: u16 = 0;
    let mut last_pty_rows: u16 = 0;

    // ── Sidebar (merge live sessions + registry entries) ───────────
    let mut sidebar = Sidebar::new();
    sidebar.refresh(
        session::load_all(&sessions_dir, Some(&workspace)).unwrap_or_default(),
        &workspace_registry.sessions,
        &ws_path_str,
    );
    let mut last_session_refresh = Instant::now();

    // ── State ───────────────────────────────────────────────────────
    let mut focus = Focus::Sidebar; // start in sidebar so user sees sessions
    let mut view: ViewMode = ViewMode::Landing;
    let mut input: Option<InputState> = None;

    // ── Main event loop ─────────────────────────────────────────────
    let mut pty_buf = vec![0u8; PTY_READ_BUF];
    let mut running = true;

    while running {
        // ── Drain PTY output ────────────────────────────────────
        if let (Some(ref mut p), Some(ref mut sc)) = (pty.as_mut(), screen.as_mut()) {
            match p.read(&mut pty_buf) {
                Ok(n) if n > 0 => sc.process(&pty_buf[..n]),
                Ok(_) => {}
                Err(e) => {
                    eprintln!("PTY read error: {e}");
                    running = false;
                }
            }
        }

        // ── Refresh sessions & merge into registry ──────────────
        if last_session_refresh.elapsed() >= SESSION_REFRESH_INTERVAL {
            sidebar.refresh(
                session::load_all(&sessions_dir, Some(&workspace)).unwrap_or_default(),
                &workspace_registry.sessions,
                &ws_path_str,
            );
            let _ = workspace_registry
                .merge_live_sessions(&sessions_dir, &ws_path_str);
            let _ = workspace_registry.save(&workspace);
            if let Ok(gr) = registry::GlobalRegistry::build(&sessions_dir) {
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

                    // ── Input mode: capture text for new session ──
                    if let Some(ref mut inp) = input {
                        match key.code {
                            KeyCode::Esc => {
                                input = None;
                            }
                            KeyCode::Enter => {
                                let name = inp.value().to_string();
                                if !name.is_empty() {
                                    // Add entry to workspace registry
                                    workspace_registry.sessions.push(
                                        registry::WorkspaceSession {
                                            session_id: String::new(),
                                            name: name.clone(),
                                            goal: String::new(),
                                            scope: String::new(),
                                            status: registry::SessionStatus::InProgress,
                                            pids: vec![],
                                            tags: vec![],
                                            started: String::new(),
                                            completed: String::new(),
                                        },
                                    );
                                    let _ = workspace_registry.save(&workspace);
                                    sidebar.refresh(
                                        session::load_all(&sessions_dir, Some(&workspace))
                                            .unwrap_or_default(),
                                        &workspace_registry.sessions,
                                        &ws_path_str,
                                    );

                                    // Lazy-spawn PTY if not already running
                                    if pty.is_none() {
                                        let pty_cols =
                                            term_cols * (100 - SIDEBAR_FRACTION) / 100;
                                        let pty_rows = term_rows;
                                        match Pty::spawn(
                                            pty_rows,
                                            pty_cols.max(1),
                                            &workspace,
                                        ) {
                                            Ok(p) => {
                                                screen = Some(TerminalScreen::new(
                                                    pty_rows,
                                                    pty_cols.max(1),
                                                ));
                                                last_pty_cols = pty_cols;
                                                last_pty_rows = pty_rows;
                                                pty = Some(p);
                                                // Link the new session_id to the registry
                                                if let Some(ref spawned) = pty {
                                                    link_spawned_session(
                                                        spawned,
                                                        &sessions_dir,
                                                        &mut workspace_registry,
                                                    );
                                                    let _ = workspace_registry.save(&workspace);
                                                }
                                            }
                                            Err(e) => {
                                                eprintln!("Failed to spawn cds: {e}");
                                            }
                                        }
                                    }

                                    input = None;
                                    view = ViewMode::Live;
                                    focus = Focus::Pty;
                                }
                            }
                            KeyCode::Backspace => inp.backspace(),
                            KeyCode::Char(c) => inp.insert(c),
                            _ => {}
                        }
                        continue;
                    }

                    // ── Transcript mode ─────────────────────────
                    if let ViewMode::Transcript(ref mut tv) = view {
                        match key.code {
                            KeyCode::Esc | KeyCode::Tab => {
                                view = if pty.is_some() {
                                    ViewMode::Live
                                } else {
                                    ViewMode::Landing
                                };
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

                    // ── Landing / Live mode ─────────────────────
                    // Tab toggles focus
                    if key.code == KeyCode::Tab && key.modifiers.is_empty() {
                        focus = match focus {
                            Focus::Sidebar => Focus::Pty,
                            Focus::Pty => Focus::Sidebar,
                        };
                        continue;
                    }

                    // Ctrl+N — new session (from landing or sidebar)
                    if key.code == KeyCode::Char('n')
                        && key.modifiers == KeyModifiers::CONTROL
                        && input.is_none()
                    {
                        input = Some(InputState::new("New session name: "));
                        continue;
                    }

                    match focus {
                        Focus::Sidebar => match key.code {
                            KeyCode::Up | KeyCode::Char('k') => sidebar.select_prev(),
                            KeyCode::Down | KeyCode::Char('j') => sidebar.select_next(),
                            KeyCode::Enter => {
                                if let Some(entry) = sidebar.selected_entry() {
                                    if let Some(s) = &entry.live_session {
                                        // Live session → try transcript replay
                                        let path = transcript::transcript_path(
                                            &home, &s.cwd, &s.session_id,
                                        );
                                        if let Some(p) = path {
                                            match TranscriptView::load(
                                                &p, s.display_name(),
                                            ) {
                                                Ok(tv) => {
                                                    view = ViewMode::Transcript(
                                                        Box::new(tv),
                                                    );
                                                }
                                                Err(_) => {
                                                    view = ViewMode::Transcript(
                                                        Box::new(
                                                            TranscriptView::empty(
                                                                s.display_name(),
                                                            ),
                                                        ),
                                                    );
                                                }
                                            }
                                        } else {
                                            view = ViewMode::Transcript(Box::new(
                                                TranscriptView::empty(
                                                    s.display_name(),
                                                ),
                                            ));
                                        }
                                    } else if entry.is_registry {
                                        // Registry entry: try transcript first
                                        // if session_id is linked, else launch PTY
                                        if let Some(ref sid) = entry.registry_session_id {
                                            let path = transcript::transcript_path(
                                                &home, &entry.cwd, sid,
                                            );
                                            if let Some(p) = path {
                                                match TranscriptView::load(
                                                    &p, &entry.label,
                                                ) {
                                                    Ok(tv) => {
                                                        view = ViewMode::Transcript(
                                                            Box::new(tv),
                                                        );
                                                    }
                                                    Err(_) => {
                                                        view = ViewMode::Transcript(
                                                            Box::new(
                                                                TranscriptView::empty(
                                                                    &entry.label,
                                                                ),
                                                            ),
                                                        );
                                                    }
                                                }
                                            } else {
                                                view = ViewMode::Transcript(Box::new(
                                                    TranscriptView::empty(
                                                        &entry.label,
                                                    ),
                                                ));
                                            }
                                        } else if pty.is_none() {
                                            // No session_id linked → launch fresh PTY
                                            let pc =
                                                term_cols * (100 - SIDEBAR_FRACTION)
                                                    / 100;
                                            let pr = term_rows;
                                            match Pty::spawn(
                                                pr,
                                                pc.max(1),
                                                &workspace,
                                            ) {
                                                Ok(p) => {
                                                    screen =
                                                        Some(TerminalScreen::new(
                                                            pr,
                                                            pc.max(1),
                                                        ));
                                                    last_pty_cols = pc;
                                                    last_pty_rows = pr;
                                                    pty = Some(p);
                                                    if let Some(ref spawned) = pty {
                                                        link_spawned_session(
                                                            spawned,
                                                            &sessions_dir,
                                                            &mut workspace_registry,
                                                        );
                                                        let _ = workspace_registry.save(&workspace);
                                                    }
                                                }
                                                Err(e) => {
                                                    eprintln!(
                                                        "Failed to spawn cds: {e}"
                                                    );
                                                }
                                            }
                                            view = ViewMode::Live;
                                            focus = Focus::Pty;
                                        }
                                        // If pty is already running and no linked
                                        // session_id, just switch to live view
                                        if pty.is_some()
                                            && entry.registry_session_id.is_none()
                                        {
                                            view = ViewMode::Live;
                                            focus = Focus::Pty;
                                        }
                                    }
                                }
                            }
                            KeyCode::Char(' ') => focus = Focus::Pty,
                            _ => {}
                        },
                        Focus::Pty => {
                            if let Some(ref mut p) = pty {
                                if let Some(bytes) = encode_key(key) {
                                    if let Err(e) = p.write(&bytes) {
                                        eprintln!("PTY write error: {e}");
                                        running = false;
                                        break;
                                    }
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
        if let (Some(ref mut p), Some(ref mut sc)) = (pty.as_mut(), screen.as_mut()) {
            if pty_cols != last_pty_cols || pty_rows != last_pty_rows {
                let _ = p.resize(pty_rows, pty_cols.max(1));
                sc.resize(pty_rows, pty_cols.max(1));
                last_pty_cols = pty_cols;
                last_pty_rows = pty_rows;
            }
        }

        // ── Render ──────────────────────────────────────────────
        let current_focus = focus;
        terminal.draw(|f| {
            render_ui(
                f,
                screen.as_ref(),
                &mut sidebar,
                current_focus,
                &view,
                input.as_ref(),
            );
        })?;

        // ── Check child exit ────────────────────────────────────
        if let Some(ref mut p) = pty {
            if matches!(view, ViewMode::Live) && p.try_wait().is_some() {
                view = ViewMode::Landing;
                pty = None;
                screen = None;
            }
        }
    }

    // ── Cleanup ─────────────────────────────────────────────────────
    if let Some(ref mut p) = pty {
        let _ = p.kill();
    }
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

// ── Session linking ──────────────────────────────────────────────────

/// After spawning cds, capture its session_id and link it to the
/// first unlinked registry entry. This ensures transcripts are
/// findable on subsequent launches.
fn link_spawned_session(
    pty: &Pty,
    sessions_dir: &PathBuf,
    registry: &mut registry::WorkspaceRegistry,
) {
    let Some(pid) = pty.child_pid() else { return };
    // Poll briefly for the session file to be written
    for _ in 0..10 {
        if let Some(sid) = session::read_session_id(sessions_dir, pid) {
            // Find LAST unlinked entry (most recently created via Ctrl+N)
            if let Some(entry) = registry
                .sessions
                .iter_mut()
                .rev()
                .find(|e| e.session_id.is_empty() && e.pids.is_empty())
            {
                entry.session_id = sid;
                entry.pids.push(pid);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

// ── Rendering ────────────────────────────────────────────────────────

fn render_ui(
    f: &mut Frame,
    screen: Option<&TerminalScreen>,
    sidebar: &mut Sidebar,
    focus: Focus,
    view: &ViewMode,
    input: Option<&InputState>,
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

    // Right panel
    match view {
        ViewMode::Landing => {
            let msg = vec![
                TLine::raw(""),
                TLine::from(Span::styled(
                    "  cc-tui",
                    Style::default().fg(Color::Rgb(120, 180, 255)),
                )),
                TLine::raw(""),
                TLine::raw("  Choose a session or create a new one."),
                TLine::raw(""),
                TLine::from(Span::styled(
                    "    Ctrl+N    new session",
                    Style::default().fg(Color::Yellow),
                )),
                TLine::from(Span::styled(
                    "    Tab       switch to sidebar",
                    Style::default().fg(Color::DarkGray),
                )),
                TLine::from(Span::styled(
                    "    Ctrl+Q    quit",
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            let landing = Paragraph::new(Text::from(msg))
                .block(
                    Block::bordered()
                        .border_style(Style::default().fg(Color::Rgb(80, 80, 80)))
                        .title_top(" Welcome "),
                );
            f.render_widget(landing, chunks[1]);
        }
        ViewMode::Live => {
            if let Some(sc) = screen {
                let text = sc.render();
                let pty_style = match focus {
                    Focus::Pty => Style::default(),
                    Focus::Sidebar => Style::default().fg(Color::DarkGray),
                };
                f.render_widget(
                    Paragraph::new(text)
                        .style(pty_style)
                        .wrap(Wrap { trim: false }),
                    chunks[1],
                );
            }
        }
        ViewMode::Transcript(tv) => {
            let available = chunks[1].height as usize;
            let text = tv.render(available);
            let widget = Paragraph::new(text)
                .block(
                    Block::bordered()
                        .title_top(format!(" {} ", tv.session_name))
                        .title_bottom(
                            " Esc/Tab → back  │  ↑↓/PgUp/PgDn scroll ",
                        )
                        .border_style(Style::default().fg(Color::Rgb(120, 180, 255))),
                )
                .wrap(Wrap { trim: true })
                .scroll((0, 0));
            f.render_widget(widget, chunks[1]);
        }
    }

    // Input overlay (centered)
    if let Some(inp) = input {
        let ow = 50u16.min(area.width - 4);
        let oh = 5u16;
        let ox = area.x + (area.width - ow) / 2;
        let oy = area.y + (area.height - oh) / 2;
        let overlay_area = ratatui::layout::Rect::new(ox, oy, ow, oh);

        let cursor_pos = inp.prompt.len() + inp.cursor;
        let display = format!("{}{}", inp.prompt, inp.buffer);
        let mut spans: Vec<Span> = display
            .char_indices()
            .map(|(i, c)| {
                if i == cursor_pos {
                    Span::styled(
                        c.to_string(),
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::White),
                    )
                } else {
                    Span::raw(c.to_string())
                }
            })
            .collect();
        // Append cursor if at end
        if cursor_pos >= display.len() {
            spans.push(Span::styled(
                " ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White),
            ));
        }

        let input_widget = Paragraph::new(TLine::from(spans))
            .block(
                Block::bordered()
                    .title(" New Session ")
                    .border_style(Style::default().fg(Color::Rgb(120, 180, 255))),
            );

        f.render_widget(ratatui::widgets::Clear, overlay_area);
        f.render_widget(input_widget, overlay_area);
    }

    // Bottom status bar
    let status = match (input.is_some(), view, focus) {
        (true, _, _) => TLine::from(" NEW SESSION  │  Enter name  │  Esc cancel  │  Enter confirm "),
        (_, ViewMode::Landing, _) => {
            TLine::from(" cc-tui  │  Ctrl+N new  │  Tab sidebar  │  Ctrl+Q quit ")
        }
        (_, ViewMode::Transcript(tv), _) => {
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
        (_, ViewMode::Live, Focus::Pty) => {
            TLine::from(" cds  │  Ctrl+Q quit  │  Tab → sidebar  │  Ctrl+N new ")
        }
        (_, ViewMode::Live, Focus::Sidebar) => {
            TLine::from("◀◀ SIDEBAR ▶▶  │  ↑↓/jk nav  │  Enter replay  │  Ctrl+N new  │  Tab → cds")
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
