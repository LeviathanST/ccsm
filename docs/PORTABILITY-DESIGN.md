# Cross-Machine Portability Design

## Motivation

Copy a ccsm-managed project (with its `.ccsm` identity) to another machine and
have `ccsm list`, `ccsm show`, etc. work immediately — without manual migration,
path editing, or configuration.

---

## Audit Findings

### Persisted Absolute Paths (stored in data files)

| # | Location | Field | Type | Set By | Read By |
|---|----------|-------|------|--------|---------|
| 1 | `~/.ccsm/<id>/sessions.json` | `sessions[].worktree` | JSON string | `ccsm start`, `ccsm resume`, `ccsm rename` | `ccsm resume`, `ccsm doctor`, `ccsm show`, `inject-scope` |

**One field total.** The detail file template *had* a `{{cwd}}` placeholder historically, but the current template (`TEMPLATE_CONTENT` in doctor.rs and on-disk template) does not contain it. The `.replace("{{cwd}}", ...)` in `main.rs:3377` is a no-op — no absolute path embeds in detail files via the template.

### Machine-Dependent Pathways (runtime, not persisted)

| # | Mechanism | Dependency | Impact |
|---|-----------|-----------|--------|
| 2 | `~/.ccsm/<id>/` global data dir | `$HOME` | State invisible on another machine |
| 3 | `project_slug(root)` encodes workspace path | Workspace absolute path | Different slug on each machine → broken transcript lookup |
| 4 | `Consumer::*` paths (`~/.claude/`, etc.) | `$HOME` | Transcript files not portable |
| 5 | Worktree directories on disk at absolute paths | Machine filesystem | Physical dirs don't exist on other machine |

---

## Design

### Goal

Make `~/.ccsm/` state directory content self-contained within the project
workspace, and store all paths relative to the workspace root.

### Approach: "Move Global State Into the Project"

**Change `.ccsm` from a file to a directory** at the project root:

```
Before:                   After:
  .ccsm  (file)             .ccsm/                 (directory)
                              identity.toml        (version + id)
                              state/
                                sessions.json
                                sessions.json.lock
                                sessions/           (detail files)
                                session-group/      (group files)
                                config.toml
                                worktrees/          (if any)
                                session-detail-template.md
```

This eliminates step (2): global data moves from `$HOME`-dependent location to
project-local, making the entire `.ccsm/` directory inherently portable.

### Path Relativization

#### 1. `worktree` field in `sessions.json`

**Current**: `/home/user/project/.claude/worktrees/my-session`
**New**: `ws:.claude/worktrees/my-session`
**Resolution**: `<workspace_root> / <relative_path>`

Use a `ws:` prefix to signal workspace-relative paths. On read, if a `worktree`
value starts with `ws:`, strip the prefix and resolve against the workspace root.
If it starts with `/`, treat as legacy absolute path (backward compat).

#### 2. Detail file `cwd` field

The template already omits `{{cwd}}`, so no migration needed. If re-introduced,
use `ws:.` instead of the absolute workspace path.

#### 3. Project slug

**Current**: `project_slug("/home/user/project")` → `-home-user-project-`
**New**: `project_slug_from_uuid("0af54e00-...")` → `ccsm-0af54e00-...`

Derive the slug from the identity UUID instead of the workspace path. This
ensures the same slug on every machine, preserving consumer transcript lookup.

#### 4. Consumer transcript paths

Consumer paths (`~/.claude/projects/<slug>/`) remain `$HOME`-dependent — that's
the consumer's domain, not ccsm's. But with a stable slug (step 3), if
transcript files are also copied, they'd land in the same relative location
under `$HOME`. This is a documentation point, not a ccsm change.

### Backward Compatibility

Transition strategy:

**Phase 1 — Dual-read** (current version + 1):
- `find_project_root()` checks both:
  1. `.ccsm` is a directory → new layout (`.ccsm/state/`)
  2. `.ccsm` is a file → legacy layout (`~/.ccsm/<id>/`)
- On write, always write to the layout that was found on read.
- Add `ccsm doctor` check that flags old layout and suggests migration.
- Read `worktree` with both absolute and `ws:` prefix support.

**Phase 2 — Migration tool**:
- `ccsm migrate --portable` converts legacy layout to new:
  1. Rename `.ccsm` (file) → `.ccsm.identity.bak`
  2. Create `.ccsm/` directory
  3. Write `.ccsm/identity.toml` from old identity
  4. Copy `~/.ccsm/<id>/` content → `.ccsm/state/`
  5. Rewrite all `worktree` paths from absolute to `ws:`-relative
  6. Clean up `~/.ccsm/<id>/` (optional, with `--clean` flag)
  7. Housekeeping on `~/.ccsm/` parent dir (remove if empty)

**Phase 3 — Legacy removal** (future major version):
- Remove legacy read path. Require `.ccsm/` to be a directory.

### Edge Cases

| Case | Handling |
|------|----------|
| Worktree path from another machine | `ws:` path resolves to new root; directory won't exist → doctor flags it (same as today's stale worktree check) |
| Multiple workspaces sharing same ccsm id | Each workspace has its own `.ccsm/` dir — no sharing needed post-migration |
| `.ccsm` is a file and `.ccsm-state/` dir exists | Ambiguous. Doctor flags it, prefers new layout. |
| `.ccsm/` dir exists but `identity.toml` missing | Error — malformed identity. Suggest `ccsm init` to repair. |
| Worktree in `sessions.json` is empty string | Works as today — no worktree, nothing to resolve. |

---

## Migration Plan

### Step 1: Code changes (same PR)

1. **registry.rs**: Add `workspace_local_data_dir(root)` function returning
   `root.join(".ccsm").join("state")`. Add `resolve_project_layout()` that
   returns an enum: `Layout::Directory { root, id }` | `Layout::File { root, id }`.
   Route all `global_*_path()` calls through the layout.

2. **registry.rs**: Add `ws:` prefix handling in `resolve_worktree_path()` and
   `store_worktree_path()` helpers. Backward-compatible read of legacy absolute
   paths.

3. **registry.rs**: Change `project_slug()` to derive from identity UUID:
   `format!("ccsm-{id}")`. Store in the `WorkspaceSession` or compute on the
   fly. Existing slugs would change — need migration or deprecation path.

4. **main.rs**: In `inject_scope` (line 4109), use `resolve_worktree_path()`.
   In `ccsm note` auto-create (line 3377), either remove the dead `{{cwd}}`
   replace or make it emit `ws:.`.

5. **resume.rs**: At lines 221, 242, 258, use `resolve_worktree_path()` for
   reading and `store_worktree_path()` for writing.

6. **doctor.rs**: At line 178, use `resolve_worktree_path()`.

### Step 2: Migration subcommand

Add `ccsm migrate --portable` implementing the Phase 2 steps above. Include
`--dry-run` flag that reports what would move without touching files.

### Step 3: Tests

| Test | Description |
|------|-------------|
| `test_portable_worktree_roundtrip` | Store/load `ws:` path, verify resolution |
| `test_legacy_absolute_fallback` | Load legacy absolute path, verify it still works |
| `test_migration_from_file_to_dir` | Full migration of legacy layout to new layout |
| `test_migration_dry_run` | Dry run reports correct actions |
| `test_project_slug_stability` | Same UUID always produces same slug |
| `test_dual_read_both_layouts` | Both layouts load identically |

### Risk Assessment

| Risk | Mitigation |
|------|-----------|
| `.ccsm` file → dir transition breaks git-tracked `.ccsm` files | The `.ccsm` identity file should already be gitignored (it's workspace-local). If tracked, migration renames it. |
| Users with both layouts simultaneously | Phase 1 dual-read handles both. Doctor flags ambiguity. |
| ws: prefix breaks path-based tooling (grep, scripts) | ws: paths are only in internal JSON, not on filesystem — no impact |
| Worktree `is_dir()` check on other machine | Graceful degradation: worktree path resolves but dir missing → doctor flags as stale, same as today |
