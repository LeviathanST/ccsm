# CI Publish: Tag-on-Main Guard

## Symptom
CI auto-publish to crates.io triggered from a tag pushed on a feature branch, attempting to publish before the code was merged to main.

## Cause
The `on: push: tags: ["v*"]` trigger fires on any branch. Without a guard, a tag on a feature branch publishes unreviewed code.

## Fix
Added a verification step in the publish job that checks the tag commit is an ancestor of main:

```yaml
- name: verify tag is on main
  run: |
    git fetch origin main
    if ! git merge-base --is-ancestor HEAD origin/main; then
      echo "tag is not on main — refusing to publish"
      exit 1
    fi
```

Requires `fetch-depth: 0` on the checkout action to have full history.

## Evidence
- PR #19: ci.yml workflow file
- Failed run on tag `v0.16.1` from `feat/portable-sessions` was correctly blocked
