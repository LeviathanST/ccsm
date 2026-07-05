---
name: cross-consumer-format-assumptions
description: Code paths that assume JSONL transcript format break when a consumer uses .json instead
tags: [consumer, format, rename, transcript]
---

# Cross-Consumer Format Assumptions

**Symptom:** `run_rename` appends JSONL-formatted rename events to what `find_session_file_for` returns. For CodeWhale, this is a `.json` session file — the appended JSONL corrupts it.

**Cause:** The rename code assumes all consumers store transcripts as JSONL files (Claude/Pi format). When CodeWhale was added with `.json` session files, this path wasn't updated.

**Fix:** Add a `&& !consumer.is_codewhale()` guard to skip the transcript append for CodeWhale, since CodeWhale doesn't support JSONL rename events.

**Evidence:** `src/main.rs` in `run_rename()` — the section that opens the transcript file with `.append(true)` and writes JSONL lines. This was fixed in commit 8a7d43e.

**Lesson for adding new consumers:** Every code path that reads/writes transcript files must be audited for format assumptions. Search for `.jsonl`, `jsonl`, and `append(true)` across the codebase. Each consumer's file format (extension, structure, append semantics) needs explicit handling.
