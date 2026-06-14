# PTY, VT100, and Ratatui Rendering Lessons

## Fixed-Grid Rendering: Never Skip Cells

Symptom:
Terminal output displayed in ratatui appears to have no whitespace between words, layout is broken, borders and dividers collapse. All text runs together.

Cause:
The renderer used `Cell::contents()` which returns `""` (empty string) for unwritten vt100 cells, then skipped or compacted those empty cells. Terminal applications position text at specific (row, col) coordinates — the whitespace between text at those positions IS the layout. Filtering cells destroys that structure.

Fix:
Use a fixed 2D grid approach (as tmux does): iterate every cell position in the screen buffer, render unwritten cells as a literal space character `" "`, and written cells by their actual contents. Never use heuristics like `has_contents()` to decide whether to render a position — every position is rendered, always. Wrap in a borderless `Paragraph` with `Wrap { trim: false }`.

Evidence:
2026-06-14 cc-tui Phase 1 — three iterations of broken layout (words without whitespace, borders collapsed) fixed by switching to fixed-grid approach. User confirmed "it worked!! All fine!" after the fix. The border on the Paragraph widget also stole 2 columns causing text wrapping — removed for PTY content.

## API-First Coding: Check Crate Signatures Before Writing

Symptom:
Code written with assumed API (e.g., `Write::write_all` on `MasterPty`) fails to compile with "method cannot be called due to unsatisfied trait bounds".

Cause:
Stub implementations were written before checking the actual crate API signatures. `portable-pty 0.9`'s `MasterPty` does NOT implement `std::io::Write` — it uses `take_writer()` for writes and `as_raw_fd()` + `libc::read` for non-blocking reads.

Fix:
Before writing PTY code against `portable-pty`, grep the trait definitions: `grep "pub trait MasterPty"` on the crate source. Same for vt100 — check `Parser` and `Screen` public methods first via `grep "pub fn"`. Build skeleton after understanding the API.

Evidence:
2026-06-14 cc-tui Phase 1 — 4 compilation errors on first build, all from mismatched API assumptions. Fixed by grep-ing crate source in `.cargo/registry/src/`.

## PTY Size Must Match Panel Area, Not Full Terminal

Symptom:
cds output breaks layout again after adding a sidebar panel. Text wraps, overflows, or is truncated even though the fixed-grid rendering is correct.

Cause:
The PTY and vt100 screen were sized to the full terminal dimensions (e.g., 120×40), but the PTY is rendered in a sub-panel (e.g., 70% width = 84 cols). The child process (cds) believes it has 120 columns and formats output accordingly, but only 84 are visible. Adding a border on the PTY panel steals 2 more columns.

Fix:
Always size the PTY and vt100 screen to match the actual rendered panel area, not the full terminal. Compute: `pty_cols = term_cols * panel_fraction / 100`, `pty_rows = term_rows`. Resize the PTY on every terminal geometry change. Never put a border on the PTY panel — borders steal columns and break the grid. Use a separate status bar or colored sidebar border for focus indication instead.

Evidence:
2026-06-14 cc-tui Phase 2 — adding a 30/70 sidebar/PTY split with a bordered PTY panel broke cds layout. Fixed by: (1) sizing PTY to `term_cols * 70 / 100` from spawn, (2) removing the PTY border, (3) tracking terminal size via Resize events and re-computing PTY dimensions, (4) moving focus indicator to a bottom status bar.
