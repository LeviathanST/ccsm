# Migration / Version Check Lessons

## run_identity_migrations should not hard-block on binary > project

Symptom:
Dev binary (v0.18.0) hard-blocks when project identity is at v0.17.2 with
"doesn't match (expected 0.18.0). Run `ccsm migrate` to update" — even
though `check_version()` in main.rs already ensures binary >= project.

Cause:
Upstream commit c126369 changed the `_` fallback in `run_identity_migrations()`
from a debounced warning to `anyhow::bail!`. This was too broad — it blocks
safe upgrades where binary > project. `run_identity_migrations` should only
handle legacy version-specific data migrations ("1"→"0.15.0", etc.), not
gate on unknown versions.

Fix:
Changed the `_` fallback from `anyhow::bail!` to `eprintln!` warning.
The safety guard (binary < project) is exclusively handled by
`check_version()` in main.rs, which correctly compares semver tuples.

Evidence:
2026-07-15, upstream c126369 introduced the hard-block. Reverted to warn
in 6f269e4. Verified with `ccsm list --active` against identity v0.17.2
using v0.18.0 dev binary — warns but proceeds.
