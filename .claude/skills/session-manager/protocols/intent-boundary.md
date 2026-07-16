# Intent-Boundary Protocol

**When to read:** When creating a new session, resuming an existing session, or receiving a goal/scope change — and the intent is ambiguous. Also read during execution when you sense drift from the clarified intent.

Agents default to "fill in the blanks" when goals are vague. This protocol replaces that behavior with **structured ambiguity resolution**. If you cannot articulate what the human wants across all five dimensions below, you have not understood the intent boundary — stop and clarify.

## Trigger Detection

This protocol fires when ANY of these conditions are true:

- **New session**: `ccsm new` with a goal that's generic, short, or lacks a clear outcome
- **Resume**: `ccsm resume` and the scope field is empty, template, or has no constraints — the agent can't tell what's in/out
- **Scope change**: Human asks to expand/redefine scope mid-session
- **Seed session**: Using the seed-session skill with a vague user description
- **Drift sensed**: During work, you realize you don't know whether a current task is in or out of scope

## The 5 Ambiguity Dimensions

When a goal or scope arrives, scan it across these dimensions. For each dimension, determine: **clear**, **ambiguous**, or **absent**.

| # | Dimension | Question | Clear signal | Vague signal |
|---|-----------|----------|-------------|--------------|
| 1 | **What** | What exactly is being built/changed/fixed? | Specific component, behavior, or file | "Fix things", "improve", "add feature", generic noun |
| 2 | **Why** | What problem does this solve? | Concrete pain, use case, or user story | No motivation stated, "because it needs doing" |
| 3 | **How** | What approach? Constraints? | Tech approach, architectural boundary, tool choice | No approach, no constraints, no boundaries |
| 4 | **Scope Edge** | What's explicitly in vs out? | "This includes X, excludes Y, depends on Z" | No boundaries, no listing of what won't be touched |
| 5 | **Done** | What's the success criteria? | Test passes, metric threshold, acceptance criteria | "When it works", no exit condition |

### Scoring

| Score | Meaning | Action |
|-------|---------|--------|
| **5/5 clear** | Full intent boundary understood | Proceed without questions |
| **3-4/5 clear** | Most dimensions clear, 1-2 ambiguous | Ask targeted questions on the ambiguous dimensions only |
| **0-2/5 clear** | Goal is too vague to act on | Stop. Explain which dimensions are missing. Ask for a restated goal. |

## Question Protocol (do NOT firehose all 5)

For each dimension that is **ambiguous** or **absent**, ask exactly **1 targeted question**. Never list all dimensions at once — the human will tune out.

### Template questions by dimension

**What (ambiguous component):**
> "The goal says 'fix X' — X covers several subsystems. Do you mean the Y edge case, the Z integration, or something else?"

**What (absent entirely):**
> "I don't have a clear picture of what needs to change. Can you describe the specific outcome you want — for example, 'the login flow should handle OAuth tokens'?"

**Why (absent):**
> "What's the pain this solves? Understanding the motivation will help me make better scoping decisions."

**How (absent approach):**
> "Any preferred approach or constraints I should know about? Frameworks, libraries, patterns to use or avoid?"

**Scope Edge (absent):**
> "What should I explicitly NOT touch while doing this? Any areas that are off-limits?"

**Done (absent):**
> "How will we know when this is done? What's the passing criteria — a test, a manual check, a metric?"

### Rules

- **Ask 1-2 questions per exchange**, not all at once. Let the human answer before asking more.
- **If the human is also unsure**, don't force precision. Instead: note the ambiguity, choose a reasonable starting approach, and set a checkpoint to re-evaluate after early exploration. Document the unknown.
- **If the human says "just do it"** despite ambiguity, proceed with the best interpretation. Document the assumed boundary explicitly in `ccsm note` so the next agent knows what was assumed.

## Capture Constraints in Scope

After clarifying, distill the intent into the **scope field**. This is the canonical reference — `ccsm inject-scope` pushes it into the agent's context on every turn, so every agent in the session sees the boundaries automatically.

### Format

Use the scope field's existing freeform text. Include a constraints block:

```markdown
<approach description>

CONSTRAINTS:
- In scope: <specific thing 1>, <specific thing 2>
- Out of scope: <explicitly excluded 1>, <explicitly excluded 2>
- Success: <criterion 1>, <criterion 2>
- Unknown: <anything still unresolved>
```

The `CONSTRAINTS:` prefix signals to `ccsm inject-scope` and to any agent reading the scope that these are intent boundaries, not just approach notes.

### When to write/update

- **New session**: `ccsm scope <name> "<approach> CONSTRAINTS: ..."` right after clarifying
- **Resume with empty scope**: First thing — before doing any work
- **Scope change**: `ccsm scope <name> "<updated scope>"` and note what changed
- **Seed session**: Even if partial — capture what IS clear and mark unknowns. The next agent that starts will see the gaps.

## In-Execution Boundary Check-In

During work, periodically self-check: **"Is what I'm doing right now within the stated boundaries?"**

### Check-in triggers

- **After every significant action** (file created, test written, dependency added)
- **When reaching a decision point** (architecture choice, tradeoff)
- **When exploring code outside the primary target** (touching adjacent modules)
- **When the approach changes** (planned approach didn't work, trying something new)

### The check pattern

When you sense drift:

1. **Name the gap**: "I'm about to work on `<thing>` — that's not in the scope's CONSTRAINTS block."
2. **Route to Scope-Gate**: "This looks like a scope question. Running the Scope-Gate protocol."
3. Follow the Scope-Gate protocol (`protocols/scope-gate.md`) — it handles the out-of-scope decision flow.

If boundaries are clear and work is within them: continue silently.

### Automatic drift signals

These patterns are strong signals the intent boundary has been crossed:

| Signal | Likely cause |
|--------|-------------|
| You're refactoring code unrelated to the goal | Scope creep |
| You're adding a feature the goal never mentioned | Unintended expansion |
| You spent 15+ minutes on something you can't connect to the boundary | Lost focus |
| You just thought "while I'm here, I'll also fix..." | Classic drift entry point |

## Integration with Seed-Session Skill

The seed-session skill's rule *"Synthesize scope even if the user was vague — fill gaps from project context"* is **superseded** by this protocol.

When seeding a session from a vague description:

1. **Detect ambiguity** across the 5 dimensions
2. **Ask targeted questions** (1-2 per exchange, not all 5 at once)
3. **Capture what IS clear** as CONSTRAINTS in the scope field (mark unknowns as `Unknown: [needs clarification]`)
4. **Only proceed** when you can articulate at minimum the **What** dimension

If the human gives a very rough idea and doesn't want to clarify ("just queue it"), write CONSTRAINTS with what you have, mark all gaps. The next agent that starts the session will see the gaps in the injected scope.

## Precision Threshold (the hard rule)

**Do NOT** `ccsm start` a session (transition from pending → in_progress) unless you can answer **What** and **Why** with specific, actionable language.

If the goal is still vague after clarification attempts, leave the session in `pending` with a note:
```bash
ccsm note <name> "INTENT-BOUNDARY: cannot resolve 'what' — goal is '<goal>'. Needs human refinement before start."
```

This blocks premature execution. The session stays queued until intent is sufficiently precise.
