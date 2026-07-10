## Structured Error Codes via ErrorCode Enum

Symptom:
Error messages were literal `[ERR_*]` strings scattered across `anyhow::bail!` and `anyhow::anyhow!` calls. Changing the format required updating every instance. Agents parsing error output had no guarantee of format consistency.

Cause:
No abstraction layer between error semantics and their string representation. Error codes were baked into format strings as text literals.

Fix:
Define an `ErrorCode` enum with a `Display` impl that produces the `[ERR_CODE]` prefix. All error sites use `anyhow::bail!("{} message", ErrorCode::Xxx, args)` — one place to change the format string.

```rust
#[derive(Debug, Clone, Copy)]
pub(crate) enum ErrorCode {
    NoSession, Exists, BadStatus, Gate, Invalid, Section, Dep,
    Consumer, Generic,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSession => write!(f, "[ERR_NOSESSION]"),
            // ...
        }
    }
}
```

Usage: `anyhow::bail!("{} no session named '{}'", ErrorCode::NoSession, name);`

Evidence:
- 30+ error sites consolidated to one Display impl
- Format change requires editing only the enum variant match arm
- Cross-referenced in `agent-first-output` session (2026-07-05)
