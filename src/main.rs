mod ansi;
mod pty;
mod registry;
mod session;
mod sidebar;

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

const PTY_READ_BUF: usize = 8192;
const SESSION_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// Default sidebar width percentage.
const DEFAULT_SIDEBAR_PCT: u16 = 30;
/// Minimum width in columns for the sidebar and PTY panels.
const MIN_PANEL_WIDTH: u16 = 20;

/// Compute the sidebar width respecting minimums.
fn sidebar_width(term_cols: u16, pct: u16) -> u16 {
    let raw = term_cols * pct / 100;
    raw.max(MIN_PANEL_WIDTH)
        .min(term_cols.saturating_sub(MIN_PANEL_WIDTH))
}

/// Compute the PTY width from the remaining space.
fn pty_width(term_cols: u16, pct: u16) -> u16 {
    term_cols.saturating_sub(sidebar_width(term_cols, pct)).max(1)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Sidebar,
    Pty,
    Wizard,
}

/// What's shown in the right panel.
enum ViewMode {
    /// No session active — landing screen.
    Landing,
    /// Live cds PTY.
    Live,
}

/// Topic entry in the creation wizard.
struct Topic {
    name: String,
    goal: String,
}

/// Session creation wizard.
enum CreationWizard {
    /// Pick from upcoming topics or choose "Other..."
    TopicPick {
        topics: Vec<Topic>,
        selected: usize,
    },
    /// Custom topic: type a short name/description
    NameInput {
        buffer: String,
        cursor: usize,
    },
}

/// Gather topic suggestions: pending registry sessions + upcoming phases.
fn gather_topics(registry: &crate::registry::WorkspaceRegistry) -> Vec<Topic> {
    use crate::registry::SessionStatus;
    let mut topics: Vec<Topic> = Vec::new();

    // Pending entries from the registry
    for s in &registry.sessions {
        if s.status == SessionStatus::Pending {
            topics.push(Topic {
                name: s.name.clone(),
                goal: if s.goal.is_empty() {
                    "(no goal set)".into()
                } else {
                    s.goal.clone()
                },
            });
        }
    }

    // Upcoming phases not yet tracked
    let phases = [
        ("phase-4-task-dashboard", "Live task dashboard from ~/.claude/tasks/"),
        ("phase-5-hook-bridges", "Hook bridges: TaskCreated/Completed → file → TUI updates"),
        ("phase-6-token-dashboard", "Token dashboard from stats-cache.json"),
        ("phase-7-polish", "Polish: themes, mouse, resize, scrollback"),
        ("testing-suite", "Integration tests for PTY, sidebar, and registry"),
    ];
    for (name, goal) in phases {
        if !registry.sessions.iter().any(|s| s.name == name) {
            topics.push(Topic { name: name.into(), goal: goal.into() });
        }
    }

    topics
}

fn main() -> anyhow::Result<()> {
    // ── Subcommand dispatch ───────────────────────────────────────
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "setup" {
        return run_setup(&args[0]);
    }
    if args.len() > 1 {
        let cmd = args[1].as_str();
        if cmd == "version" || cmd == "--version" || cmd == "-V" {
            println!("cc-tui {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        match cmd {
            "sessions" | "s" => return run_sessions(),
            "active" | "a" => return run_active(),
            "summary" | "sum" => return run_summary(),
            "show" if args.len() > 2 => return run_show(&args[2]),
            "new" if args.len() > 2 => return run_new(&args[2], args.get(3).map(|s| s.as_str()).unwrap_or("")),
            "start" if args.len() > 2 => return run_status(&args[2], "start"),
            "complete" if args.len() > 2 => return run_status(&args[2], "complete"),
            "block" if args.len() > 2 => return run_status(&args[2], "block"),
            "abandon" if args.len() > 2 => return run_status(&args[2], "abandon"),
            "scope" if args.len() > 3 => return run_set_field(&args[2], "scope", &args[3..].join(" ")),
            "tag" if args.len() > 3 => return run_set_tags(&args[2], &args[3..]),
            _ if cmd == "show" || cmd == "new" || cmd == "start" || cmd == "complete"
                || cmd == "block" || cmd == "abandon" || cmd == "scope" || cmd == "tag" => {
                eprintln!("usage: cc-tui {} <name> [value...]", cmd);
                return Ok(());
            }
            _ => {} // fall through to TUI mode
        }
    }

    // ── Workspace ───────────────────────────────────────────────────
    let workspace = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let workspace = std::fs::canonicalize(&workspace).unwrap_or(workspace);

    // ── Session data paths ──────────────────────────────────────────
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()));
    let sessions_dir = home.join(".claude").join("sessions");
    let global_registry_path = home.join(".claude").join("sessions.json");

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
    let _ = workspace_registry.refresh_from_live(&sessions_dir, &ws_path_str);
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
    );
    let mut last_session_refresh = Instant::now();

    // ── State ───────────────────────────────────────────────────────
    let mut focus = Focus::Sidebar; // start in sidebar so user sees sessions
    let mut pre_wizard_focus = Focus::Sidebar;
    let mut view: ViewMode = ViewMode::Landing;
    let mut wizard: Option<CreationWizard> = None;
    let mut sidebar_pct: u16 = DEFAULT_SIDEBAR_PCT;
    let mut drag_resizing: bool = false;

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
            );
            let _ = workspace_registry
                .refresh_from_live(&sessions_dir, &ws_path_str);
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

                    // ── Wizard mode: topic picker or name input ──
                    if wizard.is_some() {
                        // Esc always cancels wizard
                        if key.code == KeyCode::Esc {
                            wizard = None;
                            focus = pre_wizard_focus;
                            continue;
                        }
                        let mut wiz = wizard.take().unwrap();
                        let consumed = handle_wizard_key(key, &mut wiz, &mut workspace_registry, &workspace, &sessions_dir, &home, &mut sidebar, &mut pty, &mut screen, &mut last_pty_cols, &mut last_pty_rows, &mut view, &mut focus, term_cols, term_rows);
                        // Dismiss wizard only when a session was created:
                        // create_session_from_wizard transitions focus Wizard→Pty.
                        // Don't use view==Live — it's already Live when Ctrl+N opens
                        // wizard from inside a running session, which caused every
                        // non-Enter key to dismiss the picker immediately.
                        if focus == Focus::Pty {
                            wizard = None;
                        } else if matches!(wiz, CreationWizard::TopicPick { .. })
                            && key.code == KeyCode::Enter
                            && !consumed
                        {
                            // Enter on "Other..." → name input
                            wizard = Some(CreationWizard::NameInput { buffer: String::new(), cursor: 0 });
                        } else {
                            wizard = Some(wiz);
                        }
                        continue;
                    }

                    // ── Landing / Live mode ─────────────────────
                    // Tab toggles focus
                    if key.code == KeyCode::Tab && key.modifiers.is_empty() {
                        focus = match focus {
                            Focus::Sidebar => Focus::Pty,
                            Focus::Pty => Focus::Sidebar,
                            Focus::Wizard => Focus::Sidebar,
                        };
                        continue;
                    }

                    // Ctrl+N — new session creation wizard
                    if key.code == KeyCode::Char('n')
                        && key.modifiers == KeyModifiers::CONTROL
                        && wizard.is_none()
                    {
                        pre_wizard_focus = focus;
                        wizard = Some(CreationWizard::TopicPick {
                            topics: gather_topics(&workspace_registry),
                            selected: 0,
                        });
                        focus = Focus::Wizard;
                        continue;
                    }

                    if focus == Focus::Wizard {
                        // Keys reach here if wizard was dismissed externally;
                        // restore sidebar focus. Normale wizard input
                        // is intercepted by the wizard block above.
                        focus = Focus::Sidebar;
                        continue;
                    }

                    match focus {
                        Focus::Wizard => {} // handled by wizard block above
                        Focus::Sidebar => match key.code {
                            KeyCode::Up | KeyCode::Char('k') => sidebar.select_prev(),
                            KeyCode::Down | KeyCode::Char('j') => sidebar.select_next(),
                            KeyCode::Enter => {
                                if let Some(entry) = sidebar.selected_entry() {
                                    if entry.is_separator {
                                        continue; // separators are not actionable
                                    }
                                    if entry.is_trashed {
                                        // Enter on trashed → recover
                                        if let Some(ref sid) = entry.registry_session_id {
                                            workspace_registry.recover(sid, &entry.label);
                                        } else {
                                            workspace_registry.recover("", &entry.label);
                                        }
                                        let _ = workspace_registry.save(&workspace);
                                        sidebar.refresh(
                                            session::load_all(&sessions_dir, Some(&workspace))
                                                .unwrap_or_default(),
                                            &workspace_registry.sessions,
                                        );
                                    } else if let Some(s) = &entry.live_session {
                                        // Live session → resume in cds.
                                        // Prefer registry session_id (user may have manually set
                                        // it to a different transcript) over the live session's.
                                        let sid = entry.registry_session_id.clone()
                                            .filter(|rsid| !rsid.is_empty())
                                            .filter(|rsid| {
                                                let slug = crate::registry::project_slug(&workspace);
                                                home.join(".claude").join("projects")
                                                    .join(&slug).join(format!("{}.jsonl", rsid))
                                                    .exists()
                                            })
                                            .unwrap_or_else(|| s.session_id.clone());
                                        let pc = pty_width(term_cols, sidebar_pct);
                                        let pr = term_rows;
                                        if let Some(ref mut p) = pty {
                                            let _ = p.kill();
                                        }
                                        match Pty::spawn(pr, pc.max(1), &workspace, Some(&sid)) {
                                            Ok(p) => {
                                                if let Some(child_pid) = p.pid() {
                                                    workspace_registry.link_spawn(
                                                        &entry.label, child_pid, Some(&sid));
                                                    let _ = workspace_registry.save(&workspace);
                                                }
                                                screen = Some(TerminalScreen::new(pr, pc.max(1)));
                                                last_pty_cols = pc;
                                                last_pty_rows = pr;
                                                pty = Some(p);
                                            }
                                            Err(e) => eprintln!("PTY spawn error: {e}"),
                                        }
                                        view = ViewMode::Live;
                                        focus = Focus::Pty;
                                    } else if entry.is_registry {
                                        // Promote entry to InProgress (handoff: demote others).
                                        // Must happen BEFORE spawn so merge_live_sessions
                                        // Strategy 2 can link the new live session.
                                        // Match by session_id if available, else by name
                                        // (prefer newest when duplicates exist).
                                        let now = now_iso_ts();
                                        for e in workspace_registry.sessions.iter_mut() {
                                            if e.status == crate::registry::SessionStatus::InProgress
                                                && e.name != entry.label
                                            {
                                                e.status = crate::registry::SessionStatus::Completed;
                                                if e.completed.is_empty() {
                                                    e.completed = now.clone();
                                                }
                                            }
                                        }
                                        // Find index: session_id match first, else newest by name.
                                        let promote_idx: Option<usize> =
                                            if let Some(ref sid) = entry.registry_session_id {
                                                workspace_registry.sessions.iter()
                                                    .position(|e| e.session_id == *sid)
                                            } else {
                                                None
                                            };
                                        let promote_idx = promote_idx.or_else(|| {
                                            workspace_registry.sessions.iter().rev()
                                                .position(|e| e.name == entry.label)
                                                .map(|pos_from_end| {
                                                    workspace_registry.sessions.len() - 1 - pos_from_end
                                                })
                                        });
                                        if let Some(i) = promote_idx {
                                            workspace_registry.sessions[i].status =
                                                crate::registry::SessionStatus::InProgress;
                                            if !workspace_registry.sessions[i].started.is_empty() {
                                                workspace_registry.sessions[i].started.clear();
                                            }
                                        }
                                        let _ = workspace_registry.save(&workspace);

                                        // Try resume if transcript still exists on disk.
                                        let sid = entry.registry_session_id.clone()
                                            .filter(|s| !s.is_empty())
                                            .filter(|s| {
                                                let slug = crate::registry::project_slug(&workspace);
                                                let path = home.join(".claude").join("projects")
                                                    .join(&slug).join(format!("{}.jsonl", s));
                                                path.exists()
                                            });
                                        // If transcript gone, clear stale identity on the
                                        // EXACT entry we selected.
                                        if sid.is_none() {
                                            let clear_idx: Option<usize> =
                                                if let Some(ref esid) = entry.registry_session_id {
                                                    workspace_registry.sessions.iter()
                                                        .position(|e| e.session_id == *esid)
                                                } else {
                                                    None
                                                };
                                            let clear_idx = clear_idx.or_else(|| {
                                                workspace_registry.sessions.iter().rev()
                                                    .position(|e| e.name == entry.label)
                                                    .map(|pos_from_end| {
                                                        workspace_registry.sessions.len() - 1 - pos_from_end
                                                    })
                                            });
                                            if let Some(i) = clear_idx {
                                                workspace_registry.sessions[i].session_id.clear();
                                                workspace_registry.sessions[i].pids.clear();
                                            }
                                            let _ = workspace_registry.save(&workspace);
                                        }

                                        let reg_label = entry.label.clone();
                                        let reg_sid = sid.clone();
                                        // Refresh sidebar immediately so the entry shows as active.
                                        sidebar.refresh(
                                            session::load_all(&sessions_dir, Some(&workspace))
                                                .unwrap_or_default(),
                                            &workspace_registry.sessions,
                                        );

                                        let pc = pty_width(term_cols, sidebar_pct);
                                        let pr = term_rows;
                                        if let Some(ref mut p) = pty {
                                            let _ = p.kill();
                                        }
                                        match Pty::spawn(pr, pc.max(1), &workspace, reg_sid.as_deref()) {
                                            Ok(p) => {
                                                if let Some(child_pid) = p.pid() {
                                                    workspace_registry.link_spawn(
                                                        &reg_label, child_pid, reg_sid.as_deref());
                                                    let _ = workspace_registry.save(&workspace);
                                                }
                                                screen = Some(TerminalScreen::new(pr, pc.max(1)));
                                                last_pty_cols = pc;
                                                last_pty_rows = pr;
                                                pty = Some(p);
                                            }
                                            Err(e) => eprintln!("PTY spawn error: {e}"),
                                        }
                                        view = ViewMode::Live;
                                        focus = Focus::Pty;
                                    }
                                }
                            }
                            KeyCode::Char('d') => {
                                // Trash selected entry
                                if let Some(entry) = sidebar.selected_entry() {
                                    let sid = entry
                                        .registry_session_id
                                        .clone()
                                        .or_else(|| {
                                            entry
                                                .live_session
                                                .as_ref()
                                                .map(|s| s.session_id.clone())
                                        })
                                        .unwrap_or_default();
                                    workspace_registry.trash(&sid, &entry.label);
                                    let _ = workspace_registry.save(&workspace);
                                    sidebar.refresh(
                                        session::load_all(&sessions_dir, Some(&workspace))
                                            .unwrap_or_default(),
                                        &workspace_registry.sessions,
                                    );
                                }
                            }
                            KeyCode::Char('D') => {
                                // Shift+D: permanently clean selected entry
                                if let Some(entry) = sidebar.selected_entry() {
                                    let sid = entry
                                        .registry_session_id
                                        .clone()
                                        .or_else(|| {
                                            entry
                                                .live_session
                                                .as_ref()
                                                .map(|s| s.session_id.clone())
                                        });
                                    let sid = sid.unwrap_or_default();
                                    workspace_registry.clean(&sid, &entry.label, &home, &workspace);
                                    let _ = workspace_registry.save(&workspace);
                                    sidebar.refresh(
                                        session::load_all(&sessions_dir, Some(&workspace))
                                            .unwrap_or_default(),
                                        &workspace_registry.sessions,
                                    );
                                }
                            }
                            KeyCode::Char('C') => {
                                // Shift+C: permanently clean all trashed sessions
                                workspace_registry.clean_all_trashed(&home, &workspace);
                                let _ = workspace_registry.save(&workspace);
                                sidebar.refresh(
                                    session::load_all(&sessions_dir, Some(&workspace))
                                        .unwrap_or_default(),
                                    &workspace_registry.sessions,
                                );
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
                Event::Mouse(mouse) => {
                    use crossterm::event::{MouseButton, MouseEventKind};
                    let sw = sidebar_width(term_cols, sidebar_pct) as u16;
                    let near_divider = mouse.column >= sw.saturating_sub(1)
                        && mouse.column <= sw.saturating_add(1);
                    match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) if near_divider => {
                            drag_resizing = true;
                        }
                        MouseEventKind::Down(MouseButton::Left) => {
                            // Click in sidebar area
                            if mouse.column < sw
                                && mouse.row >= 1
                                && mouse.row < term_rows.saturating_sub(1)
                            {
                                let idx = (mouse.row - 1) as usize;
                                if idx < sidebar.entries.len()
                                    && !sidebar.entries[idx].is_separator
                                {
                                    let was_selected =
                                        sidebar.list_state.selected() == Some(idx);
                                    sidebar.list_state.select(Some(idx));
                                    if was_selected {
                                        // Click on already-selected → Enter
                                        focus = Focus::Sidebar;
                                        // Simulate Enter press by recursing through
                                        // the key handler. Direct Enter action:
                                        if let Some(entry) = sidebar.selected_entry() {
                                            if entry.is_separator {
                                                continue;
                                            }
                                            if entry.is_trashed {
                                                if let Some(ref sid) = entry.registry_session_id {
                                                    workspace_registry.recover(sid, &entry.label);
                                                } else {
                                                    workspace_registry.recover("", &entry.label);
                                                }
                                                let _ = workspace_registry.save(&workspace);
                                                sidebar.refresh(
                                                    session::load_all(&sessions_dir, Some(&workspace))
                                                        .unwrap_or_default(),
                                                    &workspace_registry.sessions,
                                                );
                                            } else if entry.live_session.is_some() {
                                                // Resume live session — same logic as Enter
                                                let sid = entry.registry_session_id.clone()
                                                    .filter(|rsid| !rsid.is_empty())
                                                    .filter(|rsid| {
                                                        let slug = crate::registry::project_slug(&workspace);
                                                        home.join(".claude").join("projects")
                                                            .join(&slug).join(format!("{}.jsonl", rsid))
                                                            .exists()
                                                    })
                                                    .unwrap_or_else(|| {
                                                        entry.live_session.as_ref()
                                                            .map(|s| s.session_id.clone())
                                                            .unwrap_or_default()
                                                    });
                                                let pc = pty_width(term_cols, sidebar_pct);
                                                let pr = term_rows;
                                                if let Some(ref mut p) = pty {
                                                    let _ = p.kill();
                                                }
                                                match Pty::spawn(pr, pc.max(1), &workspace, Some(&sid)) {
                                                    Ok(p) => {
                                                        if let Some(child_pid) = p.pid() {
                                                            workspace_registry.link_spawn(
                                                                &entry.label, child_pid, Some(&sid));
                                                            let _ = workspace_registry.save(&workspace);
                                                        }
                                                        screen = Some(TerminalScreen::new(pr, pc.max(1)));
                                                        last_pty_cols = pc;
                                                        last_pty_rows = pr;
                                                        pty = Some(p);
                                                    }
                                                    Err(e) => eprintln!("PTY spawn error: {e}"),
                                                }
                                                view = ViewMode::Live;
                                                focus = Focus::Pty;
                                            } else if entry.is_registry {
                                                // Promote + spawn — same logic as Enter on registry entry
                                                let now = now_iso_ts();
                                                for e in workspace_registry.sessions.iter_mut() {
                                                    if e.status == crate::registry::SessionStatus::InProgress
                                                        && e.name != entry.label
                                                    {
                                                        e.status = crate::registry::SessionStatus::Completed;
                                                        if e.completed.is_empty() {
                                                            e.completed = now.clone();
                                                        }
                                                    }
                                                }
                                                let promote_idx: Option<usize> =
                                                    if let Some(ref sid) = entry.registry_session_id {
                                                        workspace_registry.sessions.iter()
                                                            .position(|e| e.session_id == *sid)
                                                    } else { None };
                                                let promote_idx = promote_idx.or_else(|| {
                                                    workspace_registry.sessions.iter().rev()
                                                        .position(|e| e.name == entry.label)
                                                        .map(|pos_from_end| {
                                                            workspace_registry.sessions.len() - 1 - pos_from_end
                                                        })
                                                });
                                                if let Some(i) = promote_idx {
                                                    workspace_registry.sessions[i].status =
                                                        crate::registry::SessionStatus::InProgress;
                                                    if !workspace_registry.sessions[i].started.is_empty() {
                                                        workspace_registry.sessions[i].started.clear();
                                                    }
                                                }
                                                let _ = workspace_registry.save(&workspace);
                                                let sid = entry.registry_session_id.clone()
                                                    .filter(|s| !s.is_empty())
                                                    .filter(|s| {
                                                        let slug = crate::registry::project_slug(&workspace);
                                                        let path = home.join(".claude").join("projects")
                                                            .join(&slug).join(format!("{}.jsonl", s));
                                                        path.exists()
                                                    });
                                                if sid.is_none() {
                                                    let clear_idx = if let Some(ref esid) = entry.registry_session_id {
                                                        workspace_registry.sessions.iter().position(|e| e.session_id == *esid)
                                                    } else { None };
                                                    let clear_idx = clear_idx.or_else(|| {
                                                        workspace_registry.sessions.iter().rev()
                                                            .position(|e| e.name == entry.label)
                                                            .map(|pos_from_end| {
                                                                workspace_registry.sessions.len() - 1 - pos_from_end
                                                            })
                                                    });
                                                    if let Some(i) = clear_idx {
                                                        workspace_registry.sessions[i].session_id.clear();
                                                        workspace_registry.sessions[i].pids.clear();
                                                    }
                                                    let _ = workspace_registry.save(&workspace);
                                                }
                                                let reg_label = entry.label.clone();
                                                let reg_sid = sid.clone();
                                                sidebar.refresh(
                                                    session::load_all(&sessions_dir, Some(&workspace))
                                                        .unwrap_or_default(),
                                                    &workspace_registry.sessions,
                                                );
                                                let pc = pty_width(term_cols, sidebar_pct);
                                                let pr = term_rows;
                                                if let Some(ref mut p) = pty {
                                                    let _ = p.kill();
                                                }
                                                match Pty::spawn(pr, pc.max(1), &workspace, reg_sid.as_deref()) {
                                                    Ok(p) => {
                                                        if let Some(child_pid) = p.pid() {
                                                            workspace_registry.link_spawn(
                                                                &reg_label, child_pid, reg_sid.as_deref());
                                                            let _ = workspace_registry.save(&workspace);
                                                        }
                                                        screen = Some(TerminalScreen::new(pr, pc.max(1)));
                                                        last_pty_cols = pc;
                                                        last_pty_rows = pr;
                                                        pty = Some(p);
                                                    }
                                                    Err(e) => eprintln!("PTY spawn error: {e}"),
                                                }
                                                view = ViewMode::Live;
                                                focus = Focus::Pty;
                                            }
                                        }
                                    } else {
                                        focus = Focus::Sidebar;
                                    }
                                }
                            } else if mouse.column >= sw {
                                // Click in PTY area → focus PTY
                                focus = Focus::Pty;
                            }
                        }
                        MouseEventKind::Drag(MouseButton::Left) if drag_resizing => {
                            // Dragging divider: compute new percentage from mouse column.
                            let new_pct = ((mouse.column as u32 * 100) / term_cols as u32)
                                .clamp(5, 80) as u16;
                            if new_pct != sidebar_pct {
                                sidebar_pct = new_pct;
                                // Resize PTY to match new width.
                                let new_pc = pty_width(term_cols, sidebar_pct);
                                if let (Some(ref mut p), Some(ref mut sc)) =
                                    (pty.as_mut(), screen.as_mut())
                                {
                                    let _ = p.resize(term_rows, new_pc.max(1));
                                    sc.resize(term_rows, new_pc.max(1));
                                    last_pty_cols = new_pc;
                                    last_pty_rows = term_rows;
                                }
                            }
                        }
                        MouseEventKind::Up(MouseButton::Left) => {
                            drag_resizing = false;
                        }
                        MouseEventKind::ScrollDown => {
                            if mouse.column < sw as u16 {
                                sidebar.select_next();
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            if mouse.column < sw as u16 {
                                sidebar.select_prev();
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // ── Resize PTY ──────────────────────────────────────────
        let pty_cols = pty_width(term_cols, sidebar_pct);
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
                wizard.as_ref(),
                sidebar_pct,
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
    // Fill any pending session_ids (fresh spawns where Claude just
    // wrote its session file) and clean stale pids before saving.
    let _ = workspace_registry.refresh_from_live(&sessions_dir, &ws_path_str);
    let _ = workspace_registry.save(&workspace);
    // Drop the Pty — closing the PTY master fd sends SIGHUP to the
    // child. Claude traps it, saves the transcript, and exits gracefully.
    drop(pty.take());
    crossterm::execute!(
        std::io::stdout(),
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen,
    )?;
    crossterm::terminal::disable_raw_mode()?;

    Ok(())
}

// ── Setup subcommand ──────────────────────────────────────────────────

/// Run the setup script that installs session tracking globally.
/// Delegates to `scripts/setup.sh` so the logic lives in one place.
fn run_setup(bin_path: &str) -> anyhow::Result<()> {
    use std::process::Command;

    let script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("setup.sh");

    if !script.exists() {
        anyhow::bail!(
            "setup script not found at {}\n\
             (cc-tui must be run from its source tree with `cargo run setup` \
             or `cargo build && ./target/debug/cc-tui setup`)",
            script.display()
        );
    }

    println!("cc-tui setup ({})\n", bin_path);
    let status = Command::new("bash")
        .arg(&script)
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run setup script: {e}"))?;

    if !status.success() {
        anyhow::bail!("setup script exited with {status}");
    }
    Ok(())
}

// ── CLI subcommands ───────────────────────────────────────────────────

fn load_workspace_registry() -> anyhow::Result<crate::registry::WorkspaceRegistry> {
    let workspace = std::env::current_dir()?;
    crate::registry::WorkspaceRegistry::load(&workspace)
}

/// `cc-tui sessions` — all sessions, one line each, compact.
fn run_sessions() -> anyhow::Result<()> {
    let reg = load_workspace_registry()?;
    if reg.sessions.is_empty() {
        println!("(no sessions)");
        return Ok(());
    }
    for s in &reg.sessions {
        let goal = if s.goal.is_empty() { "" } else { "  " };
        let goal_text = if s.goal.len() > 80 {
            format!("{}{:.77}...", goal, &s.goal)
        } else {
            format!("{}{}", goal, &s.goal)
        };
        println!("{:12}  {:30}  {}", s.status.to_string(), s.name, goal_text.trim());
    }
    Ok(())
}

/// `cc-tui active` — only in_progress + blocked sessions.
fn run_active() -> anyhow::Result<()> {
    let reg = load_workspace_registry()?;
    let mut count = 0;
    for s in &reg.sessions {
        use crate::registry::SessionStatus;
        if !matches!(s.status, SessionStatus::InProgress | SessionStatus::Blocked) {
            continue;
        }
        let goal = if s.goal.is_empty() { "" } else { " — " };
        let goal_text = if s.goal.len() > 80 {
            format!("{}{:.77}...", goal, &s.goal)
        } else {
            format!("{}{}", goal, &s.goal)
        };
        println!("{:12}  {:30}  {}", s.status.to_string(), s.name, goal_text.trim());
        count += 1;
    }
    if count == 0 {
        println!("(no active sessions)");
    }
    Ok(())
}

/// `cc-tui summary` — counts only.
fn run_summary() -> anyhow::Result<()> {
    let reg = load_workspace_registry()?;
    use crate::registry::SessionStatus;
    let mut counts = std::collections::BTreeMap::new();
    for s in &reg.sessions {
        *counts.entry(s.status).or_insert(0) += 1;
    }
    let total: usize = counts.values().sum();
    let get = |s: SessionStatus| counts.get(&s).copied().unwrap_or(0);
    println!(
        "{} active | {} completed | {} blocked | {} abandoned | {} trashed | {} total",
        get(SessionStatus::InProgress),
        get(SessionStatus::Completed),
        get(SessionStatus::Blocked),
        get(SessionStatus::Abandoned),
        get(SessionStatus::Trashed),
        total,
    );
    Ok(())
}

// ── Mutations (shared helpers) ─────────────────────────────────────────

/// Load the registry, mutate one session by name, save, and report.
fn mutate_session<F>(name: &str, action: &str, f: F) -> anyhow::Result<()>
where
    F: FnOnce(&mut crate::registry::WorkspaceSession),
{
    let workspace = std::env::current_dir()?;
    let mut reg = crate::registry::WorkspaceRegistry::load(&workspace)?;
    {
        let entry = reg
            .sessions
            .iter_mut()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow::anyhow!("no session named '{}'", name))?;
        f(entry);
    }
    reg.updated = now_iso_ts();
    reg.save(&workspace)?;
    // Re-borrow immutably for the status line
    let entry = reg.sessions.iter().find(|s| s.name == name).unwrap();
    println!("{:12}  {}  ← {}", entry.status.to_string(), entry.name, action);
    Ok(())
}

fn now_iso_ts() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs = ts % 86400;
    let days = ts / 86400;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("day{days}T{h:02}:{m:02}:{s:02}Z")
}

// ── Mutation commands ───────────────────────────────────────────────────

/// `cc-tui new <name> [goal]` — create a new session entry.
fn run_new(name: &str, goal: &str) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let mut reg = crate::registry::WorkspaceRegistry::load(&workspace)?;
    if reg.sessions.iter().any(|s| s.name == name) {
        anyhow::bail!("session '{}' already exists", name);
    }
    reg.sessions.push(crate::registry::WorkspaceSession {
        session_id: String::new(),
        name: name.to_string(),
        goal: goal.to_string(),
        scope: String::new(),
        status: crate::registry::SessionStatus::Pending,
        pids: vec![],
        tags: vec![],
        started: String::new(),
        completed: String::new(),
    });
    reg.updated = now_iso_ts();
    reg.save(&workspace)?;
    println!("pending     {}  ← created", name);
    Ok(())
}

/// `cc-tui start|complete|block|abandon <name>` — status transitions.
fn run_status(name: &str, action: &str) -> anyhow::Result<()> {
    use crate::registry::SessionStatus;
    let new_status = match action {
        "start" => SessionStatus::InProgress,
        "complete" => SessionStatus::Completed,
        "block" => SessionStatus::Blocked,
        "abandon" => SessionStatus::Abandoned,
        _ => anyhow::bail!("unknown status action: {}", action),
    };
    mutate_session(name, action, |entry| {
        entry.status = new_status;
        if action == "complete" || action == "abandon" {
            entry.completed = now_iso_ts();
        }
    })
}

/// `cc-tui scope <name> <text>` — set the scope field.
fn run_set_field(name: &str, _field: &str, value: &str) -> anyhow::Result<()> {
    mutate_session(name, "scope updated", |entry| {
        entry.scope = value.to_string();
    })
}

/// `cc-tui tag <name> <tags...>` — replace tags.
fn run_set_tags(name: &str, tags: &[String]) -> anyhow::Result<()> {
    let tag_str = tags.join(", ");
    let _ = mutate_session(name, "tagged", |entry| {
        entry.tags = tags.to_vec();
    });
    println!("  tags: {}", tag_str);
    Ok(())
}

/// `cc-tui tag <name> <tags...>` — replace tags.

/// `cc-tui show <name>` — full session detail including progress log.
fn run_show(name: &str) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let reg = crate::registry::WorkspaceRegistry::load(&workspace)?;
    let session = reg
        .sessions
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| anyhow::anyhow!("no session named '{}'", name))?;

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  {:54} ║", session.name);
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  status: {:50} ║", session.status.to_string());
    println!("║  goal:   {:50} ║", truncate(&session.goal, 50));
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  scope:                                                      ║");
    for line in wrap_text(&session.scope, 56) {
        println!("║  {:54} ║", line);
    }
    if !session.tags.is_empty() {
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  tags: {:52} ║", session.tags.join(", "));
    }
    if !session.session_id.is_empty() {
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  session_id: {:46} ║", &session.session_id[..session.session_id.len().min(46)]);
    }
    if session.pids.is_empty() {
        println!("║  pids:    (none)                                       ║");
    } else {
        println!("║  pids:    {:46} ║", session.pids.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", "));
    }
    if !session.started.is_empty() || !session.completed.is_empty() {
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  started:   {:46} ║", session.started);
        if !session.completed.is_empty() {
            println!("║  completed: {:46} ║", session.completed);
        }
    }
    println!("╚══════════════════════════════════════════════════════════════╝");

    // Hint: detail markdown file
    let detail_path = workspace.join(".claude").join("sessions").join(format!("{}.md", name));
    if detail_path.exists() {
        println!("\n📄 detail file: .claude/sessions/{}.md", name);
    } else {
        println!("\n💡 no detail file — create one:");
        println!("   cp .claude/session-detail-template.md .claude/sessions/{}.md", name);
    }

    Ok(())
}

/// Render a single-line input field with cursor.
fn render_input_line(prompt: &str, buffer: &str, cursor: usize) -> Vec<TLine<'static>> {
    let display = format!("{}{}", prompt, buffer);
    let cursor_pos = prompt.len() + cursor;
    let mut spans: Vec<Span> = display
        .char_indices()
        .map(|(i, c)| {
            if i == cursor_pos {
                Span::styled(
                    c.to_string(),
                    Style::default().fg(Color::Black).bg(Color::White),
                )
            } else {
                Span::raw(c.to_string())
            }
        })
        .collect();
    if cursor_pos >= display.len() {
        spans.push(Span::styled(
            " ",
            Style::default().fg(Color::Black).bg(Color::White),
        ));
    }
    vec![TLine::from(spans)]
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{:.47}...", s)
    }
}

fn wrap_text(s: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut remaining = s;
    while !remaining.is_empty() {
        if remaining.len() <= width {
            lines.push(remaining.to_string());
            break;
        }
        let mut split = width;
        while split > 0 && !remaining.as_bytes()[split].is_ascii_whitespace() {
            split -= 1;
        }
        if split == 0 {
            split = width;
        }
        lines.push(remaining[..split].trim_end().to_string());
        remaining = remaining[split..].trim_start();
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

// ── Session creation wizard ───────────────────────────────────────────

/// Returns true if the key was consumed by the wizard.
fn handle_wizard_key(
    key: KeyEvent,
    wiz: &mut CreationWizard,
    workspace_registry: &mut crate::registry::WorkspaceRegistry,
    workspace: &PathBuf,
    sessions_dir: &PathBuf,
    home: &PathBuf,
    sidebar: &mut Sidebar,
    pty: &mut Option<Pty>,
    screen: &mut Option<TerminalScreen>,
    last_pty_cols: &mut u16,
    last_pty_rows: &mut u16,
    view: &mut ViewMode,
    focus: &mut Focus,
    term_cols: u16,
    term_rows: u16,
) -> bool {
    match wiz {
        CreationWizard::TopicPick { topics, selected } => match key.code {
            KeyCode::Esc => false, // caller sets wizard to None
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                *selected = selected.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                *selected = (*selected + 1).min(topics.len()); // +1 for "Other..." slot
                true
            }
            KeyCode::Enter => {
                if *selected < topics.len() {
                    let topic = &topics[*selected];
                    create_session_from_wizard(
                        topic.name.clone(), topic.goal.clone(),
                        workspace_registry, workspace, sessions_dir, home,
                        sidebar, pty, screen, last_pty_cols, last_pty_rows,
                        view, focus, term_cols, term_rows,
                    );
                } else {
                    // "Other..." — step into name input
                    // We'll return false and let the caller transition state
                    return false; // caller handles state transition
                }
                true
            }
            _ => false,
        },
        CreationWizard::NameInput { buffer, cursor } => match key.code {
            KeyCode::Esc => false, // caller sets wizard to None
            KeyCode::Enter => {
                let name = buffer.trim().to_string();
                if !name.is_empty() {
                    create_session_from_wizard(
                        name, String::new(),
                        workspace_registry, workspace, sessions_dir, home,
                        sidebar, pty, screen, last_pty_cols, last_pty_rows,
                        view, focus, term_cols, term_rows,
                    );
                }
                true
            }
            KeyCode::Left => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+Left: jump to start of previous word
                    while *cursor > 0 && buffer.as_bytes()[*cursor - 1] == b' ' {
                        *cursor -= 1;
                    }
                    while *cursor > 0 && buffer.as_bytes()[*cursor - 1] != b' ' {
                        *cursor -= 1;
                    }
                } else {
                    *cursor = cursor.saturating_sub(1);
                }
                true
            }
            KeyCode::Right => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+Right: jump to start of next word
                    while *cursor < buffer.len() && buffer.as_bytes()[*cursor] != b' ' {
                        *cursor += 1;
                    }
                    while *cursor < buffer.len() && buffer.as_bytes()[*cursor] == b' ' {
                        *cursor += 1;
                    }
                } else {
                    *cursor = (*cursor + 1).min(buffer.len());
                }
                true
            }
            KeyCode::Backspace => {
                if *cursor > 0 {
                    *cursor -= 1;
                    buffer.remove(*cursor);
                }
                true
            }
            KeyCode::Char(c) => {
                buffer.insert(*cursor, c);
                *cursor += 1;
                true
            }
            _ => false,
        },
    }
}

fn create_session_from_wizard(
    name: String,
    goal: String,
    workspace_registry: &mut crate::registry::WorkspaceRegistry,
    workspace: &PathBuf,
    sessions_dir: &PathBuf,
    home: &PathBuf,
    sidebar: &mut Sidebar,
    pty: &mut Option<Pty>,
    screen: &mut Option<TerminalScreen>,
    last_pty_cols: &mut u16,
    last_pty_rows: &mut u16,
    view: &mut ViewMode,
    focus: &mut Focus,
    term_cols: u16,
    term_rows: u16,
) {
    // Demote any other in_progress → completed (handoff)
    let now = now_iso_ts();
    for e in workspace_registry.sessions.iter_mut() {
        if e.status == crate::registry::SessionStatus::InProgress && e.name != name {
            e.status = crate::registry::SessionStatus::Completed;
            if e.completed.is_empty() { e.completed = now.clone(); }
        }
    }

    // Determine whether we can resume an existing transcript.
    // When duplicates exist (same name), check ALL matching entries, newest first.
    let slug = crate::registry::project_slug(&workspace);
    let resume_sid: Option<String> = workspace_registry
        .sessions
        .iter()
        .rev()
        .filter(|e| e.name == name && !e.session_id.is_empty())
        .find(|e| {
            home.join(".claude").join("projects")
                .join(&slug).join(format!("{}.jsonl", e.session_id))
                .exists()
        })
        .map(|e| e.session_id.clone());

    // Promote existing entry or create new. Never duplicate.
    // Match by session_id (if resuming) first, then by name (newest first).
    let promote_idx: Option<usize> = if let Some(ref sid) = resume_sid {
        workspace_registry.sessions.iter()
            .position(|e| e.session_id == *sid)
    } else {
        None
    };
    let promote_idx = promote_idx.or_else(|| {
        workspace_registry.sessions.iter().rev()
            .position(|e| e.name == name)
            .map(|pos_from_end| workspace_registry.sessions.len() - 1 - pos_from_end)
    });

    if let Some(i) = promote_idx {
        let existing = &mut workspace_registry.sessions[i];
        existing.status = crate::registry::SessionStatus::InProgress;
        if resume_sid.is_some() {
            // Keep session_id so we can --resume; only clear pids (stale process).
            existing.pids.clear();
        } else {
            existing.session_id.clear();
            existing.pids.clear();
        }
        if !goal.is_empty() { existing.goal = goal; }
    } else {
        workspace_registry.sessions.push(crate::registry::WorkspaceSession {
        session_id: String::new(),
        name: name.clone(),
        goal,
        scope: String::new(),
        status: crate::registry::SessionStatus::InProgress,
        pids: vec![],
        tags: vec![],
        started: String::new(),
        completed: String::new(),
    });
    }
    // Always save and refresh after mutation (was missing for existing entries).
    let _ = workspace_registry.save(workspace);
    sidebar.refresh(
        crate::session::load_all(sessions_dir, Some(workspace)).unwrap_or_default(),
        &workspace_registry.sessions,
    );

    if pty.is_none() {
        let pc = pty_width(term_cols, DEFAULT_SIDEBAR_PCT);
        let pr = term_rows;
        match Pty::spawn(pr, pc.max(1), workspace, resume_sid.as_deref()) {
            Ok(p) => {
                if let Some(child_pid) = p.pid() {
                    workspace_registry.link_spawn(
                        &name, child_pid, resume_sid.as_deref());
                    let _ = workspace_registry.save(workspace);
                }
                *screen = Some(TerminalScreen::new(pr, pc.max(1)));
                *last_pty_cols = pc;
                *last_pty_rows = pr;
                *pty = Some(p);
            }
            Err(e) => eprintln!("Failed to spawn cds: {e}"),
        }
    }
    *view = ViewMode::Live;
    *focus = Focus::Pty;
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
    screen: Option<&TerminalScreen>,
    sidebar: &mut Sidebar,
    focus: Focus,
    view: &ViewMode,
    wizard: Option<&CreationWizard>,
    sidebar_pct: u16,
) {
    let area = f.area();
    let sw = sidebar_width(area.width, sidebar_pct);
    let pw = pty_width(area.width, sidebar_pct);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sw),
            Constraint::Length(pw),
        ])
        .split(area);

    // Sidebar (left)
    sidebar.render(f, chunks[0]);

    // Right panel: show preview when sidebar is focused, else Landing/Live.
    let show_preview = focus == Focus::Sidebar
        && sidebar.selected_entry()
            .map(|e| !e.preview_text.is_empty() && !e.is_separator)
            .unwrap_or(false);

    if show_preview {
        let entry = sidebar.selected_entry().unwrap();
        let preview_para = Paragraph::new(entry.preview_text.as_str())
            .block(
                Block::bordered()
                    .title_top(format!(" {} ", entry.label))
                    .border_style(Style::default().fg(Color::Rgb(120, 180, 255))),
            )
            .wrap(Wrap { trim: false });
        f.render_widget(preview_para, chunks[1]);
    } else {
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
                        Focus::Pty | Focus::Wizard => Style::default(),
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
        }
    }

    // Wizard overlay (centered)
    if let Some(wiz) = wizard {
        // Dynamic sizing: wider and taller, capped to terminal
        let ow = (area.width * 3 / 4).min(90u16).max(50u16);
        // Height: 4 header/footer lines + topic count + 1 for Other
        let topic_count = match wizard {
            Some(CreationWizard::TopicPick { topics, .. }) => topics.len(),
            _ => 0,
        };
        // Each topic = 2 lines (name + goal). Header/footer = ~8 lines.
        let needed_h = (topic_count * 2 + 8).min(area.height.saturating_sub(4) as usize) as u16;
        let oh = needed_h.max(12u16);
        let ox = area.x + (area.width - ow) / 2;
        let oy = area.y + (area.height - oh) / 2;
        let overlay_area = ratatui::layout::Rect::new(ox, oy, ow, oh);

        let (title, lines) = match wiz {
            CreationWizard::TopicPick { topics, selected } => {
                let mut items: Vec<TLine> = Vec::new();
                // Header
                items.push(TLine::from(vec![
                    Span::styled(
                        "  ╭───────────────────────────╮",
                        Style::default().fg(Color::Rgb(80, 80, 80)),
                    ),
                ]));
                items.push(TLine::from(vec![
                    Span::raw("  │ "),
                    Span::styled("Pick a topic", Style::default().fg(Color::Rgb(120, 180, 255)).add_modifier(ratatui::style::Modifier::BOLD)),
                    Span::raw(" — "),
                    Span::styled(format!("{} upcoming", topics.len()), Style::default().fg(Color::Rgb(160, 160, 160))),
                    Span::raw(" │"),
                ]));
                items.push(TLine::from(vec![
                    Span::styled(
                        "  ╰───────────────────────────╯",
                        Style::default().fg(Color::Rgb(80, 80, 80)),
                    ),
                ]));
                items.push(TLine::raw(""));
                // Topic list
                let inner_w = (ow - 6) as usize;
                for (i, t) in topics.iter().enumerate() {
                    let is_sel = i == *selected;
                    let prefix = if is_sel { "▸" } else { " " };
                    let num = format!("{:2}", i + 1);
                    let name = if t.name.len() > inner_w.saturating_sub(6) {
                        format!("{:.w$}...", t.name, w = inner_w.saturating_sub(9))
                    } else {
                        t.name.clone()
                    };
                    let line = format!(" {} {} {}", prefix, num, name);
                    let style = if is_sel {
                        Style::default().fg(Color::Black).bg(Color::Rgb(180, 200, 255))
                    } else {
                        Style::default().fg(Color::Rgb(220, 220, 220))
                    };
                    let truncated_goal = if t.goal.len() > inner_w.saturating_sub(6) {
                        format!("{:.w$}...", t.goal, w = inner_w.saturating_sub(10))
                    } else {
                        t.goal.clone()
                    };
                    items.push(TLine::from(Span::styled(line, style)));
                    if !t.goal.is_empty() {
                        items.push(TLine::from(Span::styled(
                            format!("      ~ {}", truncated_goal),
                            Style::default().fg(Color::Rgb(120, 120, 120)),
                        )));
                    }
                }
                // Other... option
                let other_sel = *selected == topics.len();
                let other_prefix = if other_sel { "▸" } else { " " };
                items.push(TLine::raw(""));
                items.push(TLine::from(Span::styled(
                    "  ──────────────────────────────",
                    Style::default().fg(Color::Rgb(60, 60, 60)),
                )));
                let other_style = if other_sel {
                    Style::default().fg(Color::Black).bg(Color::Rgb(180, 200, 255))
                } else {
                    Style::default().fg(Color::Rgb(120, 200, 255)).add_modifier(ratatui::style::Modifier::BOLD)
                };
                items.push(TLine::from(Span::styled(
                    format!(" {} ✨  Other — type a custom topic...", other_prefix),
                    other_style,
                )));
                (" ▸ New Session ", items)
            }
            CreationWizard::NameInput { buffer, cursor } => {
                let prompt = "Name (the agent will ask for details): ";
                let items = render_input_line(prompt, buffer, *cursor);
                (" New Session ", items)
            }
        };

        let para = Paragraph::new(Text::from(lines))
            .block(
                Block::bordered()
                    .title(title)
                    .border_style(Style::default().fg(Color::Rgb(120, 180, 255))),
            );

        f.render_widget(ratatui::widgets::Clear, overlay_area);
        f.render_widget(para, overlay_area);
    }

    // Bottom status bar
    let status = match (wizard.is_some(), view, focus) {
        (true, _, _) => TLine::from(" NEW SESSION  │  ↑↓/jk pick  │  Enter confirm  │  Esc back/cancel "),
        (_, ViewMode::Landing, _) => {
            TLine::from(concat!(" cc-tui v", env!("CARGO_PKG_VERSION"), "  │  Ctrl+N new  │  Tab sidebar  │  Ctrl+Q quit "))
        }
        (_, ViewMode::Live, Focus::Pty) => {
            TLine::from(" cds  │  Ctrl+Q quit  │  Tab → sidebar  │  Ctrl+N new ")
        }
        (_, ViewMode::Live, Focus::Sidebar) => {
            TLine::from("◀◀ SIDEBAR ▶▶  │  ↑↓/jk nav  │  Enter resume/recover  │  d trash  │  D clean  │  C clean all")
        }
        _ => TLine::from(""),
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
