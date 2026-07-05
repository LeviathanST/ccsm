# Agent-First Documentation Pattern

**Symptom:** Project READMEs and CLAUDE.md files are written for humans — install guide first, commands buried, no token cost awareness. Agents waste context parsing sections they don't need and miss commands they do.

**Cause:** Docs follow human conventions (README = marketing + install + reference) when the primary consumer is an AI agent reading from its context window.

**Fix:** Structure agent-facing docs as:

1. **Decision tree** — "I need X → run Y." Agents navigate by intent, not by section.
2. **Token budget table** — Estimated output tokens per command, with "start here" recommendations. Lets agents choose the cheapest variant that answers their question.
3. **Agent section first** — What agents need (commands, consumers, workspace resolution) before what humans need (install, build, tech stack).

**Evidence:** ccsm README rewrite reduced agent guesswork: `ccsm list --summary` (~30 tokens) is now the obvious first step instead of a buried flag. `ccsm scan` alternatives are compared by token cost. Decision tree covers every agent entry point.

**Application:** Apply to any project where AI agents are primary documentation consumers — CLI tools, dev platforms, SDKs. Human install/build sections go last. Token estimates need review as commands evolve.
