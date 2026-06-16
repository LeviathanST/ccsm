# Cross-Session Teammate Awareness

**When to read:** During work, when your task touches another session's domain, or you discover a dependency on another active session.

You have teammates. Other sessions in the registry aren't just records — they represent other agents (or future-you) working on the same project. Act like it.

The Team Awareness scan at session start is the minimum. During work, you MUST maintain ongoing awareness of what other sessions are doing and coordinate when work intersects.

## When to Re-Scan

| Trigger | Why |
|----------|-----|
| Human describes a task touching another session's domain | Detect overlap before starting |
| You discover you need something another session might provide | Detect dependency |
| You complete a milestone another session might be waiting on | Unblock waiting sessions |
| Human mentions or asks about another session by name | Cross-reference context |

Token-efficient re-scan:
```bash
ccsm list --active --verbose    # full goal + tags, one line per session (~80 tokens)
```

## The 3 Coordination Patterns

**Pattern 1 — Dependency:** "We need X, and Session `<name>` is working on it."

> Flag: "Session `<name>` is working on `<goal>`. We need `<thing>` which overlaps with their work."
>
> Options:
> **(1)** Add a cross-session dependency note to both sessions.
> **(2)** Wait — block our session until theirs completes.
> **(3)** Proceed independently — we'll handle it ourselves.

If (1):
```bash
ccsm note <their-session> --cross <our-session> "DEPENDENCY: `<our-session>` needs <thing> from this session"
ccsm note <our-session> "DEPENDENCY: waiting on `<their-session>` for <thing>"
```
Also update the Dependencies section in both detail files.

If (2):
```bash
ccsm block <our-session>
ccsm note <our-session> "BLOCKED: waiting on `<their-session>` to complete <thing>"
```

**Pattern 2 — Redundancy:** "Session `<name>` already handles X, which is part of your request."

> Flag: "Session `<name>` already covers `<thing>`. Your request overlaps."
>
> Options:
> **(1)** Coordinate — merge efforts, join their session or update their scope.
> **(2)** Proceed independently — noted the overlap, continue anyway.
> **(3)** Narrow scope — strip the overlapping part, do only what's unique.

If (1):
```bash
ccsm note <their-session> --cross <our-session> "OVERLAP: `<our-session>` identified overlap in <area> — consider coordinating"
```
If (2):
```bash
ccsm note <our-session> "OVERLAP-NOTED: `<their-session>` already handles <thing>. Proceeding independently by human choice."
```

**Pattern 3 — Related Work:** "Session `<name>` is doing something adjacent but not overlapping."

> Flag: "Session `<name>` is working on `<goal>`. This is adjacent — no direct overlap, but sessions should know about each other."
>
> Options:
> **(1)** Add a cross-reference note to both sessions.
> **(2)** No action — noted, proceed.

If (1):
```bash
ccsm note <their-session> --cross <our-session> "RELATED: `<our-session>` is working on <thing> — may interest you"
ccsm note <our-session> --cross <their-session> "RELATED: `<their-session>` is working on <thing> — adjacent work"
```

## Cross-Session Note Convention

Use `ccsm note --cross <source>` which auto-prepends `CROSS-SESSION [<source>]:`. This makes cross-session coordination greppable and traceable:

```bash
grep -r "CROSS-SESSION" .claude/sessions/   # find all cross-session coordination
```

- Use `--cross` when writing to **another** session's detail file
- Use regular `ccsm note` for your own session's internal notes
- Always backtick-quote session names so they're clickable/searchable

## When NOT to Flag

- Trivial non-overlapping work
- The other session completed more than 2 weeks ago (stale — no active teammate to coordinate with)
- The human explicitly says "don't coordinate"
- You already flagged this exact overlap earlier in the session
- The overlap is in name only (similar words, different meaning)

## Decision Flow

```
During work, encounter something another session might relate to:
    │
    ├─→ ccsm list --active --verbose        (quick scan, ~80 tokens)
    │
    ├─→ We NEED their output?               → Pattern 1 (Dependency)
    ├─→ We DUPLICATE their work?            → Pattern 2 (Redundancy)
    ├─→ Our work is ADJACENT to theirs?     → Pattern 3 (Related Work)
    └─→ No meaningful connection            → Don't flag. Continue.
```

## Relationship to Other Protocols

| Protocol | Direction | Trigger |
|----------|-----------|---------|
| **Team Awareness** | Startup scan | Session begins |
| **Scope-Gate** | Reactive | Human asks for off-scope work |
| **Proactive Ideation** | Proactive (ideas) | Agent notices something worth tracking |
| **Cross-Session Teammate** | Horizontal (peer sessions) | Work intersects another session's domain |

All four are mandatory. They complement each other — Team Awareness is the startup check, Cross-Session is the ongoing version of it, Scope-Gate protects session boundaries, and Ideation captures new work.
