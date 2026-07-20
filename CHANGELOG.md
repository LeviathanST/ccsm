# Changelog

All notable changes to ccsm are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/).
ccsm uses [semantic versioning](https://semver.org/).

## [0.21.1] — 2026-07-20

- push line coverage from 28% to 82%, add 7,300 lines of unit + integration tests
- bump ccsm-swarm version to match
- add structured error codes to all untagged error output sites (#38)
- add no-op migration steps for v0.17→v0.21 chain gap (#37)
- fix: env lock poisoning recovery with Rust 2024 unsafe wrappers

### CI

- add clippy, rustfmt, and 80% coverage threshold to GitHub Actions (#46)
- coverage artifact uploads on main, checks run on all PRs

## [0.21.0] — 2026-07-19

- add `style` module with owo-colors helpers, emoji gating (`is_terminal()` + `NO_COLOR`)
- add `table` helper with ANSI-aware column layout
- add `config` subcommand (view TOML, set key-value, reset defaults)
- embed setup assets (plugin, script, skills) via `include_str!` for non-source-tree installs
- add first-run welcome banner with project slug and quick-start tips
- normalize all error output with structured `[ERR_*]` codes
- integrate `Table` helper in `action_msg`, `run_list`, and `run_group` output
- integrate `Spinner` in resume polling and archive-all progress
- remove unused `ErrorCode::Consumer` and `ErrorCode::Generic` variants
- add 20 style + 12 table unit tests, 7 DX integration tests

## [0.20.0] — 2026-07-16

- inject intent-boundary CONSTRAINTS into agent scope (#28)

## [0.19.1] — 2026-07-15

- optimize inject-scope output for DeepSeek prefix cache (#26)
- add DeepSeek prefix cache principle + bump identity version (#27)

## [0.19.0] — 2026-07-15

- structured WORKTREE BOUNDARY inject-scope with env var support (#25)

## [0.18.0] — 2026-07-15

- auto-chain migration from v0.0.0 → current (#24)
- CI: auto-create tags from Cargo.toml on merge to main

## [0.17.2] — 2026-07-15

- hard-block on identity version mismatch instead of warning
- remove all interactive prompts from ccsm
- don't auto-update identity on version mismatch
- prevent hang on identity version mismatch in non-interactive contexts
- debounce identity version mismatch warning to once per process

## [0.17.1] — 2026-07-15

- defer OpenCode harvest to after child exit (session created lazily) (#22)
- add lesson on OpenCode lazy session timing

## [0.17.0] — 2026-07-14

- ccsm-swarm: tmux MCP server for multi-agent orchestration (#17)
- refactor: remove persisted worktree path, add portability support (#19)
- ccsm init, orphaned identity detection, portability (#20)
- OpenCode consumer rename — strip worktree code, add title drift detection (#18)
- docs: audit and fix stale documentation references
- docs: note that publish tags must be on main (#21)

## [0.16.1] — 2026-07-14

- consumer parity fixes: validate_session_id, refresh harvest, doctor OpenCode fixes
- lifecycle enforcement: status transitions, pending clears, kebab-case, detail sync
- rename reliability: cross-refs, group files, serde JSON, 3-phase lock
- cleanup: remove stale .ccsm from git, fix merge artifacts (#15)
- fix: rename OpenCode session title after resume harvest (#16)

## [0.15.0] — 2026-07-14

- global workspace identity and path resolution
- refactor: migrate all callers to global workspace API
- fix: .ccsm identity version is ccsm version (0.14.0), not hardcoded
- version-gated identity migrations
- seed project skills to global on ccsm setup
- seed skills to OpenCode's native ~/.config/opencode/skills/
- fix: opencode harvest uses worktree dir, not workspace root

### OpenCode Consumer

- add opencode plugin for scope-injection and auto-attach
- add OpenCode consumer variant + rusqlite DB helpers
- add OpenCode resume spawn + SQLite harvest logic
- add OpenCode setup, attach, refresh, rename support in main
- guard clean/archive for OpenCode (no file deletion)
- update CLAUDE.md + skill reference for OpenCode consumer

## [0.14.0] — 2026-07-14

- OpenCode integration (see v0.15.0 for full list — fast-follow bumped)

## [0.13.0] — 2026-07-13

- git worktree support for ccsm sessions
- auto-rebase onto origin/main before creating worktree
- resume --worktree flag
- check filesystem for existing worktree on resume --worktree
- deny unknown fields in ccsm config
- wrap worktree resume in shell cd+exec for correct cwd
- move worktree doctor checks outside session_issues gate
- feat: improve registry parsing error messages and add ccsm branch command
- docs: worktree edge cases lesson
- docs: add docs/adding-a-consumer.md checklist for new consumers

## [0.12.0] — 2026-07-10

- ship default .ccsm/config.toml
- ccsm setup seeds .ccsm/config.toml if missing
- custom checklist templates in project config
- remove Live Session Data section, auto-sync scope/tags to detail file
- auto-sync status line, scope, and tags to detail file
- session aging in ccsm doctor
- remove outdated Live Session Data gate check

## [0.11.0] — 2026-07-05

- workspace resolution chain: CCSM_WORKSPACE env var + walk-up fallback
- add --json flags, CCSM_OUTPUT_FORMAT env var
- structured error codes
- rewrite README + CLAUDE.md agent-first, clean registry
- CodeWhale consumer variant

## [0.10.0] — 2026-06-25

- branch tracking, config-driven WIP guard, checklist templates
- update skills for branch tracking, config, checklist templates
- add structured-errors lesson

### Pi Consumer

- Pi integration: Consumer abstraction, cross-agent resume, .pi extension
- pi extension with 22 ccsm tools + auto-attach
- pi-aware resume with cross-consumer detection
- doctor uses consumer-aware project dirs
- --consumer flag threaded through all commands
- pi setup seeds 4 skills, lessons go in .claude/lessons/
- show agent field in ccsm show output

### Consumer Abstraction

- add Consumer abstraction for multi-agent support
- add consumer field to session registry + consumer-aware paths
- remove dead .ccsm/ dir/transcript code, keep migrate-ccsm command

### Wrap-up & Lessons

- add wrap-up skill + lessons/ data store, reshape learned-lesson-issue
- record lesson about lesson systems

## [0.9.0] — 2026-06-24

### Checklist System

- inject checklist status into system-reminder on every turn
- ccsm check auto-adds items
- close gate nudges when session has work but no checklist
- ccsm start nudges agent to consider checklist
- ccsm resume nudges about checklist before spawning claude
- weave checklist into agent protocol + lifecycle docs

## [0.8.0] — 2026-06-21

- ccsm scan — compact grouped output, grep-friendly, built-in --search
- ccsm check auto-adds items — no manual detail-file editing needed
- doctor auto-creates missing session-detail-template.md + essential dirs
- ccsm note auto-creates missing detail file from registry data

### Grouping & Dependencies

- session grouping — group/next commands, list filters, rank ordering
- group detail markdown files (.claude/session-group/<name>.md)
- ccsm group --list to list all groups in workspace
- depends_on field, ccsm depend, dep-aware ccsm next, ccsm group-deps
- ccsm group --roadmap — markdown table + mermaid dep graph
- fix: roadmap uses registry as canonical source, same as ccsm show
- fix: ccsm show reads goal/scope from detail file, unified with roadmap

## [0.7.1] — 2026-06-19

- extract resume/doctor to commands module, deduplicate now_iso, fix clippy
- add ccsm refresh, close, note-check subcommands + gate enforcement
- fix: inject-scope/gate-check/note-check respect CCSM_SESSION before in_progress scan
- fix: inject-scope requires CCSM_SESSION or --name, no silent fallback

### Checklist (initial)

- add checklist subcommands + opt-in gate integration

## [0.6.0] — 2026-06-17

- inject CCSM_SESSION env var on resume so agents know their session
- smart attach — auto-discover sessions by name match or recency
- ccsm rename — rename session across registry, detail file, transcript, and live files
- ccsm rename -g/-s flags for topic change + clean deletes detail files
- --force flag to ccsm new to skip fuzzy duplicate detection
- fix: ccsm resume no longer silently creates entries for unknown names
- fix: smarter session name suggestions — substring + capped edit distance
- refactor: Zig-spirit error handling in ccsm resume — no silent fallbacks

### Testing

- integration test harness + 7 lifecycle tests
- 11 attach + rename integration tests
- 8 sequence + clean integration tests
- 26 integration tests, 65 total

## [0.5.0] — 2026-06-16

- flock-based file locking to prevent write races, sequence subcommand
- rename cc-tui → ccsm, remove redundant dirs
- ccsm note command + end-gate protocol for agents
- ccsm archive + doctor + duplicate prevention
- ccsm completions + remove auto-demotion resume swap
- template residue detection in ccsm doctor + END-GATE pre-flight checklist
- scope-gate protocol to session-manager skill
- --verbose to ccsm list and --cross to ccsm note
- refactor: split SKILL.md into modular router with protocol + reference files

## [0.4.0] — 2026-06-15

- refactor: rip out TUI, fix session harvest, add trash/clean/section commands
- auto-create session detail file on cc-tui new
- feat(rules): add scope discipline — agents must stay within session scope
- feat(cli): add cc-tui pending <name> — reset to pending, clear identity

## [0.3.0] — 2026-06-14

- Merge registry entries into sidebar + switch to Ctrl+N
- Fix Enter on sidebar: work for registry entries too
- Fix session history: link live sessions to registry entries
- Fix session history persistence — capture session_id at spawn
- Fix session linking: .rev() to find last (newest) unlinked entry
- Replace broken session linking with lazy transcript lookup
- Fix session resume and sidebar flickering
- Add session trash/clean with separate trash section
- Fix trash/clean for seed entries with empty session_id
- fix(sidebar): navigation, mouse, resize, dedup, matching

## [0.2.0] — 2026-06-14

- Phase 1: PTY embedding with fixed-grid rendering
- Phase 2: Sidebar with session list and focus switching
- Fix: resize PTY to match sidebar panel area
- Add PTY-panel-size-mismatch lesson
- Add workspace awareness and session filtering
- Add session detail overlay on Enter
- Add session transcript replay — Enter shows conversation history
- Document session data architecture decision
- Add two-tier session registry (global overview + workspace detail)
- Add landing screen, lazy PTY spawn, and new session creation

## [0.1.0] — 2026-06-14

- Initial commit: cc-tui project scaffold

---

[unreleased]: https://github.com/LeviathanST/ccsm/compare/v0.21.1...HEAD
[v0.21.1]: https://github.com/LeviathanST/ccsm/releases/tag/v0.21.1
[v0.21.0]: https://github.com/LeviathanST/ccsm/releases/tag/v0.21.0
[v0.20.0]: https://github.com/LeviathanST/ccsm/releases/tag/v0.20.0
[v0.19.1]: https://github.com/LeviathanST/ccsm/releases/tag/v0.19.1
[v0.19.0]: https://github.com/LeviathanST/ccsm/releases/tag/v0.19.0
[v0.18.0]: https://github.com/LeviathanST/ccsm/releases/tag/v0.18.0
[v0.17.2]: https://github.com/LeviathanST/ccsm/releases/tag/v0.17.2
[v0.17.1]: https://github.com/LeviathanST/ccsm/releases/tag/v0.17.1
[v0.17.0]: https://github.com/LeviathanST/ccsm/releases/tag/v0.17.0
[v0.16.1]: https://github.com/LeviathanST/ccsm/releases/tag/v0.16.1
