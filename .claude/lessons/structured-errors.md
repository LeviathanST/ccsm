## Structured Error Codes via ErrorCode Enum + Macros

Symptom:
Error messages were literal `[ERR_*]` strings scattered across `anyhow::bail!` and `anyhow::anyhow!` calls. Changing the format required updating every instance. Agents parsing error output had no guarantee of format consistency.

Cause:
No abstraction layer between error semantics and their string representation. Error codes were baked into format strings as text literals.

Fix:
Define an `ErrorCode` enum with a `Display` impl that produces the `[ERR_CODE]` prefix. All error sites use `bail_err!` macro — one place to change the format string, impossible to forget the prefix.

```rust
#[derive(Debug, Clone, Copy)]
pub(crate) enum ErrorCode {
    NoSession, Exists, BadStatus, Gate, Invalid, Section, Dep,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSession => write!(f, "[ERR_NOSESSION]"),
            // ...
        }
    }
}

// At top of main.rs — auto-prepends [ERR_*] prefix
macro_rules! bail_err {
    ($code:expr, $fmt:literal $(, $arg:expr)* $(,)?) => {
        anyhow::bail!("{} {}", $code, format!($fmt $(, $arg)*))
    };
}

// Usage — no way to forget the code:
bail_err!(ErrorCode::NoSession, "session '{}' not found", name);
```

A lint test (`all_bail_calls_have_error_codes` in `tests/style_tests.rs`) scans all `bail!(` calls and fails if any lack `[ERR_` or `ErrorCode::`.

Evidence:
- 68 untagged error sites fixed across 6 files in one pass
- Format change requires editing only the enum variant match arm
- Macro prevents silent errors from missing prefixes
- Lint test prevents regression
- Cross-referenced in `general-audit-for-enhancements` session (2026-07-20)
