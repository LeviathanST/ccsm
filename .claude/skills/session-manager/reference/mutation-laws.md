# 5 Laws of Session Mutation

Every field change has rules. Follow them.

## 1. Goal

| CAN change | CANNOT change |
|------------|---------------|
| Human redirects you | "I found a more interesting problem" |
| Initial goal was too vague | You're bored and want to pivot |
| Scope shift forces goal change | Without human approval |

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

**You cannot `ccsm complete` without an END-GATE note.**
