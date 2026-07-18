# opencode2 serve API: verified vs documented

**Symptom:** v2 API docs show `/session` routes but live `opencode2 serve --service` (next-15718) only responds to `/api/` prefix. `/session` returns 404. Prompt body format is `{"text":"..."}` not `{"prompt":{"text":"..."}}`. Session creation is `{"title","directory"}` with `x-opencode-directory` header for workspace pinning.

**Cause:** Legacy `/api/` routes coexist with v2 routes during beta migration. The installed binary (v0.0.0-next-15718) still uses the legacy paths.

**Fix:** Always verify against the live server, not docs. Use HTTP Basic auth from `~/.config/opencode/service-*.json`. Route all requests through `/api/` prefix. Send prompt as `{"text":"..."}`.

**Evidence:** Verified 2026-07-18 via curl against `http://127.0.0.1:4096`:
- `GET /session` → 404
- `GET /api/session` → 200
- `POST /api/session {"title":"test"}` → 200
- `POST /api/session/{id}/prompt {"text":"hello"}` → 200
- `POST /api/session/{id}/prompt {"prompt":{"text":"hello"}}` → 400
- `POST /api/session/{id}/wait` → 204 (blocks until idle)
