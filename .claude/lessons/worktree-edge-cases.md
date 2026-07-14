## Worktree: Target branch already checked out in main workspace

Symptom:
`git worktree add` fails with `fatal: 'feat/worktree' is already used by worktree at '/path'` when the target branch is currently checked out in the main workspace.

Cause:
Git does not allow the same branch to be checked out in multiple worktrees simultaneously. If the session's target branch matches `git branch --show-current` in the main checkout, worktree creation is invalid.

Fix:
Before calling `git worktree add`, check if the target branch is the current branch of the main checkout. If it matches, skip worktree creation entirely and use the main checkout directory instead. This can be done by comparing `branch` against the output of `git branch --show-current` run from the workspace root.

Evidence:
2026-07-14 — user ran `ccsm resume worktree-create` on a session targeting `feat/worktree` while already on `feat/worktree` in the main checkout. Error: `fatal: 'feat/worktree' is already used by worktree at '/home/leviathanst/workspaces/tools/ccsm'`.

## Worktree: Filesystem detection when registry field is empty

Symptom:
`ccsm resume --worktree` errors with "worktree already exists" even though the directory is on disk. The registry's `worktree` field is empty but the canonical path exists.

Cause:
The resume code checked the registry for the worktree path but not the filesystem. When a worktree was created by an older version of ccsm (or externally), the registry entry was never populated.

Fix:
Add a filesystem fallback in the existing-worktree detection: if the registry `worktree` field is empty, check if the canonical path (via `worktree_path_for`) exists on disk. If it does, use it and store the path in the registry.

Evidence:
2026-07-14 — user ran `ccsm resume pentest-toolchain --worktree` with a worktree directory on disk but empty registry field. Error: "worktree already exists at /path".

## Worktree: Session file cwd restoration on resume

Symptom:
After `ccsm resume` with a worktree, the agent lands in the workspace root instead of the worktree, despite `cmd.current_dir(wt)` being set. Claude's `--resume` restores the working directory from the session file's `cwd` field.

Cause:
Claude reads `~/.claude/sessions/<pid>.json` on startup and restores the `cwd` from that file. If the session file was written before the worktree existed, the `cwd` points to the workspace root, overriding any `current_dir` set by ccsm.

Fix:
Two approaches that work together: (1) wrap the agent spawn in `sh -c "cd <worktree> && exec claude <args>"` so the session file records the worktree path from the start. (2) After the session file appears, rewrite its `cwd` field to the worktree path.

Evidence:
2026-07-14 — user observed `📁 worktree:` displayed but agent still worked in workspace root. Shell wrapper fix resolved it.

## Worktree: Registry shared across worktrees

Symptom:
`.ccsm/sessions.json` walk-up from a worktree directory resolves to the main checkout's `.ccsm/`, causing session metadata to be shared between the worktree and the main checkout.

Cause:
The worktree lives at `.claude/worktrees/<name>/` inside the main checkout. When ccsm walks up from PWD looking for `.ccsm/sessions.json`, it finds the main checkout's `.ccsm/` at the top, not a worktree-specific one.

Fix:
Long-term: move all ccsm data to `~/.ccsm/` (global home-directory registry), decoupling session state from workspace-isolated working trees. See `global-data-workspace` session.

Evidence:
2026-07-14 — identified during worktree feature development. Seeded as pending session `global-data-workspace`.
