---
name: seed-session
description: >
  Create a pending ccsm session stub from the user's quick description.
  The user is lazy — they give name + rough idea, you synthesize scope + tags.
  Session stays pending; another session handles the actual work later.
argument-hint: "<name> <rough description> — create a pending session for later"
trigger_patterns:
  - seed session
  - stage session
  - setup session
  - create session
  - new session stub
  - queue session
  - stash session
---

# Seed Session — Quick Session Setup

The user describes a task they want queued for later. Your job: extract the intent, synthesize a plan, create the pending entry. Do NOT start it.

## Protocol

### 1. Extract

From the user's words, extract:
- **name**: kebab-case slug (the user may already provide this)
- **goal**: one sentence — what are we doing?
- **rough scope**: what approach? any constraints? what's in/out? (infer from context if not stated)
- **tags**: 2-4 keywords

### 2. Create (pending only)

Use `ccsm sequence` for a single lock/save cycle:

```bash
ccsm sequence -q new <name> -g "<goal>" -q scope <name> "<scope>" -q tag <name> <tag1> <tag2> ...
```

Or run individually if sequence doesn't support all ops yet. Session stays **pending** — do NOT `ccsm start`.

### 3. Confirm

Print: name, goal, scope, tags. Remind the user: "Pending — `ccsm start <name>` when ready."

## Rules

- **Never start the session.** This is a stub for later.
- **Synthesize scope even if the user was vague.** Fill gaps from project context.
- **If the name is bad** (not kebab-case, too vague), fix it and note the change.
- **If the user already has a session with this name**, warn and suggest an alternative.
