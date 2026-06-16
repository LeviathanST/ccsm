# Proactive Ideation Dashboard

**When to read:** During work, when you notice friction, gaps, tech debt, or architecture smells worth capturing.

The session registry is a **collective brain**. During work, you will discover things worth tracking: tech debt, improvement ideas, missing features, architectural smells. Capture these as **pending sessions** before they evaporate.

## When to Offer — The 5 Triggers

You **MUST** consider offering when you encounter **any** of these during work:

| # | Trigger | Example |
|---|---------|---------|
| 1 | **Friction** — a workaround, brittle code, or repeated manual step | "I had to manually export the widget after every rebuild" |
| 2 | **Gap** — something the project clearly needs is absent | "There's no command to bulk-tag sessions" |
| 3 | **Tech debt** — a TODO comment, a hack, an unfixed edge case, dead code | "This parser panics on edge cases; see comment on line 142" |
| 4 | **Architecture smell** — a design pattern would simplify multiple areas | "Three modules implement the same matching logic" |
| 5 | **End-Gate residue** — during Q3 ("What's LEFT?"), a deferred item that warrants its own session | "The END-GATE listed OSC edge cases — that's a standalone session" |

You are **NOT required** to offer for:
- Trivial one-liner fixes you can do immediately (`ccsm note` is enough)
- Items already tracked as a pending session
- Vague feelings without a concrete, specific observation

## The 3 Quality Gates

Every candidate MUST pass all three gates. If any gate fails, do NOT offer.

**Gate 1 — Specificity.** Can you name the exact thing and what work is needed?

```
PASS: "The config parser ignores TOML tables — a session to add table support"
FAIL: "This code feels messy" (what specifically? what work?)
```

**Gate 2 — Worth-a-Session.** Would someone reading the goal/scope understand what to do and why it matters? The work should represent at least 15-30 minutes.

```
PASS: A future developer could pick up the session and start working immediately.
FAIL: A two-minute fix better handled as `ccsm note <current-session>` or done now.
```

**Gate 3 — Not-Already-Tracked.** Verify the registry before offering:

```bash
ccsm list --status pending   # is this already captured?
```

If a pending session covers this ground, **skip the offer**. Instead, enrich that session:

```bash
ccsm note <existing-pending-name> "IDEATION: also noticed <finding>"
```

## The Offer Pattern

When a candidate passes all three gates, follow this exact pattern. **Never create the session unilaterally.**

**Phase 1 — Notice** (one sentence)

> "I noticed that <specific observation>."

**Phase 2 — Diagnose** (one sentence naming the impact)

> "This means <why it matters>."

**Phase 3 — Propose** (one sentence with a clear yes/no fork)

> "Want me to create a pending session for <this>?"

If yes: create the session with goal and scope.

```bash
ccsm new <name> -g "<goal>"
ccsm scope <name> "..."
ccsm tag <name> ideation <tags...>
```

If no: no pressure. Log it so it doesn't vanish:

```bash
ccsm note <current-session> "IDEA (not tracked): <observation>"
```

You do NOT reopen the topic unless the human does.

## Bundling Rule

- **Bundle related items** into one pending session. If two observations share the same module or category, group them.
- **Max 1 new pending session per agent session** for unrelated topics.
- If you discover a genuinely independent idea after already creating one, log it instead: `ccsm note <current-session> "IDEA (deferred): <observation>"`

| If items share... | Bundle into one session named... |
|-------------------|----------------------------------|
| The same module | `<module>-improvements` |
| The same category (all debt, all gaps) | `<category>-audit` |
| A dependency chain | An umbrella session with dependencies |

## Counter-Indications

- ❌ **Do NOT offer** during the human's first interaction with a new session — they haven't seen the problem space yet
- ❌ **Do NOT offer** when mid-task with the human actively directing you — finish first, then mention at the next natural pause
- ❌ **Do NOT offer** for "this would be nice" without articulating why it matters (fails specificity gate)
- ❌ **Do NOT create the session unilaterally** — always ask first
- ❌ **Do NOT re-offer** on a topic the human declined — accept the answer
- ❌ **Do NOT offer** more than one idea per natural break point — bundle or defer
