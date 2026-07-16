# Anti-Patterns

- ❌ **Touch `session_id` or `pids`** — ccsm manages these
- ❌ **Leave name/goal/scope empty** — blank labels help no one
- ❌ **Fill in vague goals/scopes with guesses from project context** — the Intent-Boundary protocol requires asking targeted clarifying questions first. See `protocols/intent-boundary.md`.
- ❌ **Skip the progress log** — `ccsm note` after every change. Never miss it.
- ❌ **Complete without END-GATE** — the three questions are mandatory.
- ❌ **Change goal/scope without documenting why** — the 5 Laws require rationale.
- ❌ **Status ping-pong** — complete↔in_progress without a real reason.
- ❌ **Read the full detail file blindly** — use `--section` to pull what you need
- ❌ **Parse JSONL transcripts** — ccsm uses `claude --resume`, not transcript parsing
- ❌ **Use `jq`/`cat` for reading** — CLI commands are token-optimized and consistent across agents
- ❌ **Work in isolation** — ignore other active sessions. You have teammates. Scan before starting related work.
- ❌ **Hand-roll cross-session note prefixes** — use `ccsm note --cross <source>`. The `CROSS-SESSION [source]:` format must be consistent.
- ❌ **Read every detail file to scan teammates** — use `ccsm list --active --verbose` (~80 tokens) instead of `ccsm show` on each (~200 tokens each)
- ❌ **Flag every adjacent session** — only flag when there's a real dependency, redundancy, or meaningful relationship. "Both touch Rust code" is not a relationship.
- ❌ **Skip checklist for multi-step sessions** — if the scope has 3+ distinct sub-tasks, use `ccsm new -c` or `ccsm checklist --init`. Checklist items block `ccsm close` — they're a mechanical gate, not optional flair.
- ❌ **Leave checklist items pending at close** — `ccsm close` will reject with pending/blocked items. Resolve them (`done`/`skipped`) before closing. Use `ccsm checklist <name>` to audit.
