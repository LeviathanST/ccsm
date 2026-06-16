# Scope-Gate Protocol

**When to read:** Human makes a request that might be outside the current session's scope.

The CLAUDE.md already says: *"NEVER execute work outside the current session's scope."* This section turns that rule into an **interactive gate** — when the human asks for off-scope work, you MUST pause and offer a structured fork. Never silently drift.

## What's "in scope"?

Work is **in scope** if it directly advances the session's stated `goal` and fits within the `scope` field. The goal is your compass. When uncertain, ask: "Does this move us toward `<goal>`?" If no, the gate triggers.

## What definitely triggers the gate

| Situation | Example |
|-----------|---------|
| New feature unrelated to goal | Human: "Add session export to CSV" while session goal is "fix PTY resize bug" |
| Infrastructure change not in scope | Human: "Rewrite the lockfile module" while session goal is "add archive command" |
| Cross-cutting concern that needs its own session | Human: "Add structured logging everywhere" during a focused bugfix session |
| Work on a different project subsystem | Human: "Refactor the sidebar renderer" while session is about the `sequence` subcommand |

## What does NOT trigger the gate

- Clarifying questions about the current work
- Minor approach adjustments that stay within scope
- Things the human explicitly says are part of the current work
- Trivial one-liners that take under 2 minutes (`ccsm note` is enough)

## The Gate Pattern

When you detect an out-of-scope request, follow this exact pattern:

**Step 1 — Flag** (name the mismatch)

> "This isn't in the current session plan. `<session>`'s goal is `<goal>`, and this request is about `<different thing>`."

**Step 2 — Offer** (exactly 3 options, let the human choose)

> "Options:
> **(1)** Create a pending session for it — we'll track it and pick it up later.
> **(2)** Update the current scope to include it — expand what this session covers.
> **(3)** Handle it as a quick aside — if it's trivial enough to do in under 2 minutes."

**Step 3 — Execute** based on human choice:

*Option 1 (new pending session):*
```bash
ccsm new <name> -g "<goal>"
ccsm scope <name> "<approach>"
ccsm tag <name> scope-gate <relevant-tags>
# Then return to current session work.
```

*Option 2 (expand current scope):*
```bash
ccsm scope <current-session> "<original scope> + <what's being added>"
ccsm note <current-session> "SCOPE-GATE: expanded scope to include <thing>"
# Proceed with the now-in-scope work.
```

*Option 3 (quick aside):*
Do the thing immediately. Then:
```bash
ccsm note <current-session> "ASIDE: <one-liner about what was done>"
```

## The hard rule

**Never** start off-scope work without triggering the gate. If in doubt, trigger it. A false positive costs one exchange; silent scope drift costs the session's coherence and wastes future resume attempts.

## Scope-Gate vs. Proactive Ideation

| Protocol | Trigger | Who initiates |
|----------|---------|--------------|
| **Scope-Gate** | Human makes an out-of-scope request | Human → Agent reacts |
| **Proactive Ideation** | Agent notices something during work | Agent → Human reacts |

They are complementary. Scope-gate creates sessions reactively; ideation creates them proactively.
