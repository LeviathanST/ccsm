# 5 Laws of Session Mutation

Every field change has rules. Follow them.

## 1. Goal

| CAN change | CANNOT change |
|------------|---------------|
| Human redirects you | "I found a more interesting problem" |
| Initial goal was too vague | You're bored and want to pivot |
| Scope shift forces goal change | Without human approval |
| Agent refines goal via Intent-Boundary protocol before starting work | Agent rewrites goal to match their own interpretation without human confirmation |

**Must document:** `ccsm note <name> "GOAL: <old> → <new>. Reason: <why>"`

## 2. Scope

| CAN change | CANNOT change |
|------------|---------------|
| New constraint discovered | "While I'm here, I'll also refactor X" |
| Something proved infeasible | Scope creep without human approval |
| Human adds/removes items | Gold-plating |

**Must document:** `ccsm note <name> "SCOPE: changed <what>. Deferred: <what's pushed out>"`

## 3. Status

| CAN change | CANNOT change |
|------------|---------------|
| Clear boundary crossed (started/done/blocked/abandoned) | Status ping-pong (complete→in_progress→complete) |
| Blocked by specific, named dependency | "Blocked because it's hard" |
| Abandoned with clear rationale | Abandoned because you lost interest |

**Must document:**
- Blocked: `ccsm note <name> "BLOCKED: <specific blocker>. Resolution: <what needs to happen>"`
- Abandoned: `ccsm note <name> "ABANDONED: <why>. Alternative: <what should happen instead>"`

## 4. Tags

| CAN change | CANNOT change |
|------------|---------------|
| Classification changes | Tag spam (5+ is a smell) |
| Priority shifts | Tags that duplicate the goal text |

Not mandatory to note, but do it if the tag change represents a significant reclassification.

## 5. Progress Log — MANDATORY

**You MUST `ccsm note` after ANY non-trivial work:**
- Code written or changed
- Decision made (architecture, tool choice, approach)
- Roadblock hit
- Milestone reached
- Dependency added/removed

```bash
ccsm note <name> "<what you did and why>"
```

The progress log IS the audit trail. If you did something, log it. **Never skip this.**

---

# End-Gate Protocol (BEFORE `ccsm complete`)

## 🔴 Step 1: Mechanical Gate — `ccsm close <name>`

The CLI enforces these hard checks. Fix failures before proceeding:

```
ccsm close <name>
```

**Checked mechanically (exit non-zero on violation):**
- Detail file exists
- Scope/Plan not empty or template `(fill in)`
- Tags not empty or template
- Progress Log has ≥ 2 entries
- Live Session Data filled (not `(auto — ccsm manages)`)
- Checklist: zero pending and zero blocked items (if `## Checklist` section exists)

**Self-review checklist (printed on pass):**
```
☐ Tests pass?
☐ All changes committed and pushed?
☐ Scope fulfilled? Anything left undocumented?
☐ Dependencies resolved?
☐ Detail file tags and progress log are current?
```

`ccsm complete <name>` also runs this gate internally — refuse unless `--force`.

### On `ccsm close` failure

Read the error output, fix each issue, re-run. Fixes:
```bash
ccsm scope <name> "<approach>"     # fill scope
ccsm tag <name> <tag1> <tag2>     # fill tags
ccsm note <name> "<what you did>" # add progress entry
ccsm check <name> 1 -s done       # resolve checklist items
ccsm check <name> "blocked task" -s skipped
# Edit detail file (~/.ccsm/<id>/sessions/<name>.md) for Live Session Data
```

## Step 2: The Three Questions (only after gate passes)

Before marking a session complete, you MUST answer these three questions via `ccsm note`:

```
1. WHAT was built — vs. what the scope promised?
2. What was explicitly NOT done — what's deferred, cut, or out of scope?
3. What's LEFT — technical debt, follow-up sessions, open questions?
```

Example:
```bash
ccsm note my-feature "END-GATE: built — PTY embedding with fixed-grid ANSI rendering (matches scope). deferred — F-key passthrough (needs separate session). left — vt100 parser has edge cases with OSC sequences."
```

**You cannot `ccsm complete` without a passing pre-flight AND an END-GATE note.**
