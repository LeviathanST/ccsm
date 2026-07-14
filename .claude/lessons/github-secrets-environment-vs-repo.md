# GitHub Secrets: Environment vs Repository Scope

## Symptom
CI workflow's `cargo publish` step received an empty `CARGO_REGISTRY_TOKEN` even though the secret was added in GitHub settings.

## Cause
The token was added as an **Environment secret** (scoped to a specific deployment environment), but the workflow job did not declare that environment. Environment secrets are only injected when the job has `environment: <name>`.

## Fix
Use **Repository secrets** (Settings → Secrets and variables → Actions → New repository secret) for secrets used across workflows without environment scoping. Environment secrets require the job to opt in via `environment:`.

## Evidence
- PR #19: first rerun showed `CARGO_REGISTRY_TOKEN: ` (empty), second rerun after switching to repo-level secret showed `CARGO_REGISTRY_TOKEN: ***`
