# Session Lifecycle Lessons

## Slug mismatch: transcript "not found" → session_id silently cleared

Symptom:
User manually sets `session_id` on a registry entry, opens ccsm, selects the entry — spawns fresh instead of resuming. After opening ccsm the session_id is empty in sessions.json.

Cause:
Claude Code's project directory slug replaces ALL non-alphanumeric chars with `-`, but `project_slug()` only replaced `/` with `-`. For `/home/user/my_project`, Claude writes transcripts to `.../projects/-home-user-my-project/` but ccsm looked in `.../projects/-home-user-my_project/`. Transcript "not found" → `resume_sid = None` → `session_id.clear()`.

Fix:
Replace all non-alphanumeric chars, not just `/`:
```rust
// src/registry.rs
pub(crate) fn project_slug(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect()
}
```
Replace all 3 inline `replace('/', "-")` calls in main.rs with `crate::registry::project_slug(&workspace)`.

Evidence:
2026-06-14, bevy_mobile_proof workspace (`bevy_mobile_proof` → slug `bevy-mobile-proof`). Session `8617c72d-...` existed on disk but `ls ~/.claude/projects/-home-...-bevy-mobile-proof/8617c72d-...` confirmed path mismatch.

---

## merge_live_sessions: polling files to rediscover spawn metadata

Symptom:
PIDs accumulating without bound. Duplicate entries with same name. Manually-set session_ids overwritten. Sidebar shows phantom "green" live entries for dead processes.

Cause:
ccsm spawned `claude` but didn't record the spawn metadata (pid, session_id). A 70-line polling loop with 3 matching strategies tried to rediscover this from session files on disk. Strategy 3 wrote the live session_id unconditionally on pid match. Strategies never removed stale pids.

Fix:
Ripped out `merge_live_sessions`. Replaced with:
- `link_spawn(name, pid, session_id)` — called at all 3 spawn sites, writes pid + session_id directly to registry entry. Never overwrites non-empty session_id.
- `refresh_from_live()` — lightweight: fills empty session_ids for fresh spawns, removes pids with no live session file. Runs every 2s + at exit.
- `Pty::pid()` — exposes child process ID.

Evidence:
2026-06-14, bevy_mobile_proof had 8 stale pids on ui-polish entry. Session_id was overwritten by pid-match strategy. User said "why do we need merge_live_sessions?" — removed entirely.

---

## Session ID is user intent: never overwrite non-empty, never clear without confirming transcript gone

Symptom:
User sets session_id manually, opens ccsm, it's empty. Reports "it back to the old session id" or "it become ''" after opening the app.

Cause:
Six independent code paths silently destroyed session_id: the wizard (`create_session_from_wizard`) always cleared it, the sidebar Enter handler cleared it when transcript "not found", Strategy 3 of merge_live_sessions overwrote it, live session entries in sidebar carried old session_id, `find(|e| e.name == name)` matched wrong entry when duplicates existed, and the sidebar showed live entries with same name despite different session_id.

Fix:
Five guards applied across all write paths:
1. `link_spawn`: only writes session_id if `entry.session_id.is_empty()`
2. `create_session_from_wizard`: only clears session_id if transcript confirmed gone
3. Sidebar Enter: same transcript-exists check before clearing
4. All `find()` calls: match by session_id first, then newest by name (`.rev()`)
5. Sidebar: hide live session when registry entry with same name has different session_id

Evidence:
2026-06-14, user manually set `8617c72d-...` on ui-polish entry 3+ times, each time ccsm cleared it. Fixed by all 5 guards + slug fix (transcript-exists check now passes).

---

## PTY exit: SIGHUP preserves state, SIGKILL destroys it

Symptom:
Non-persistent conversation: user spawns session, works, quits ccsm, re-opens — session spawns fresh (no resume). Or: stale processes accumulate after ccsm exits.

Cause:
Three exit strategies were tried, each wrong:
- `detach()`: leaked PTY, child ran forever → orphaned processes, stale pids
- `kill()`: SIGKILL, child can't trap → dies without flushing state → transcript not persisted
- Clearing pids on exit: removed the pid before the process actually died

Fix:
Exit cleanup order:
1. `refresh_from_live()` — fill pending session_ids (fresh spawns may not have hit 2s cycle)
2. `save()` — persist registry with session_id intact to disk
3. `drop(pty.take())` — close PTY master fd → kernel sends SIGHUP → Claude traps, saves transcript, exits gracefully

Evidence:
2026-06-14, user reported "non-persistent conservation problem" after kill() was added. Switched to drop(Pty) + refresh-before-save protocol. detach() method removed entirely from pty.rs.

---

## CLI `run_resume` never saves session_id — spawns fresh every time

Symptom:
`ccsm new test1 && ccsm resume test1` → types in claude, quits → `ccsm resume test1` again → spawns fresh instead of resuming. Registry shows `session_id: ""`.

Cause:
`run_resume` used `Command::status()` which blocks until exit but never captures the child pid. Three missing steps: (1) never wrote pid to registry entry, (2) never called `refresh_from_live` after exit to harvest session_id from the session file claude wrote, (3) explicitly cleared pids when `session_id.is_empty()` at line 1057, which would have prevented `refresh_from_live` from matching even if it were called. The TUI path (sidebar Enter) doesn't have this bug — it calls `link_spawn` at spawn time and `refresh_from_live` on a 2-second cycle.

Fix:
1. Use `cmd.spawn()` instead of `cmd.status()` to capture `child.id()`
2. Write pid to registry entry after spawn (so `refresh_from_live` can match)
3. Call `reg.refresh_from_live()` + `reg.save()` after `child.wait()` to harvest the session_id claude wrote on exit

Evidence:
2026-06-15, `test1` entry in sessions.json had `session_id: ""`, `pids: []`, status `in_progress` after user did `ccsm resume test1`, typed, and quit. The live session file claude wrote was cleaned up on exit, but even if it persisted, `refresh_from_live` requires `!entry.pids.is_empty()` to match — and pids were empty.

---

## Harvest session_id BEFORE child exits — Claude deletes session file on graceful exit

Symptom:
`ccsm resume <name>` spawns Claude, user chats, quits — `ccsm show <name>` shows `session_id: ""`. Second resume spawns fresh instead of `--resume`. Session file existed while Claude was running but was gone by the time `child.wait()` returned.

Cause:
Claude Code v2.1.158 writes `~/.claude/sessions/<pid>.json` at startup but **deletes it on graceful exit**. The harvest code ran after `child.wait()`, when the file was already gone. The old `refresh_from_live` had the same bug (designed for TUI polling, not post-exit reads).

Fix:
Poll for the session file immediately after spawn (up to 5s, 100ms intervals), harvest the `session_id` while Claude is running, save to registry BEFORE `child.wait()`. After `wait()`, only clear stale pids — the session_id is already persisted.

```rust
// Poll for session file — must read BEFORE wait(), Claude deletes it on exit
let session_file = home.join(".claude").join("sessions")
    .join(format!("{child_pid}.json"));
for _ in 0..50 {
    if session_file.exists() { break; }
    std::thread::sleep(std::time::Duration::from_millis(100));
}
// harvest session_id + save here...
let _ = reg.save(workspace);
let status = child.wait()?;  // Claude exits, deletes session file
// clear pids, save again
```

Evidence:
2026-06-15, user ran `ccsm resume test1`, Claude printed session ID `5d86e310-...` in its greeting, user quit, `show` had empty session_id. Session file at `~/.claude/sessions/<pid>.json` was confirmed missing after exit despite existing during the session. Polling before `wait()` fixed it — confirmed working on next test.
