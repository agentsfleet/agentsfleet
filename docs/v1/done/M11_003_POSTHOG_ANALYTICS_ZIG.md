# M11_003: PostHog Product Analytics — Zig Event Coverage

**Prototype:** v1.0.0
**Milestone:** M11
**Workstream:** 3
**Date:** Mar 22, 2026
**Status:** DONE
**Priority:** P1 — product analytics coverage for startup, auth, workspace, and error visibility
**Batch:** B1
**Depends on:** M5_001 (PostHog Zig SDK), M11_001 (Observability Pipeline)

**Completion Update (Mar 22, 2026):**
- Added 9 new PostHog event types covering startup lifecycle, auth, workspace, and API error tracking.
- Wired events into serve.zig, worker.zig, and 6 HTTP handler files.
- Added 6 startup error codes (UZ-STARTUP-001 through UZ-STARTUP-006) to `src/errors/codes.zig`.
- Tests extracted to dedicated `posthog_events_test.zig` (100 lines).
- Created `docs/POSTHOG.md` — full event catalogue with schema reference.
- Verified: `zig build test` (332 tests, 253 passed, 79 skipped, 0 failed), `make lint` (ZLint pass), `make test-unit` (depth: unit=506, integration=101), `make memleak` (pass), `check-pg-drain.py` (168 files, pass).

---

## 1.0 Startup Lifecycle Events

**Status:** DONE

Track when server/worker processes start and when startup fails.

**Dimensions:**
- 1.1 DONE `server_started` event emitted in `serve.zig` before HTTP listen, includes `port` and `worker_concurrency`
- 1.2 DONE `worker_started` event emitted in `worker.zig` after worker threads spawn, includes `concurrency`
- 1.3 DONE `startup_failed` event emitted in `worker.zig` for DB connect and migration check failures, includes `command`, `phase`, `reason`, `error_code`
- 1.4 DONE Startup error codes added: `UZ-STARTUP-001` through `UZ-STARTUP-006` in `src/errors/codes.zig`
- 1.5 DONE PostHog client explicitly flushed (`.deinit()`) before `process.exit(1)` in failure paths

**Acceptance Criteria:**
- [x] `server_started` fires on successful serve startup
- [x] `worker_started` fires on successful worker startup
- [x] `startup_failed` fires with error code on worker DB/migration failure
- [x] No-op when `POSTHOG_API_KEY` is unset (null client pattern)

---

## 2.0 Auth Lifecycle Events

**Status:** DONE

Track auth session completion (CLI login) and auth rejections.

**Dimensions:**
- 2.1 DONE `auth_login_completed` event emitted in `auth_sessions_http.zig` after successful session completion, includes `session_id`, `request_id`
- 2.2 DONE `auth_rejected` event emitted via `writeAuthErrorWithTracking` in `common.zig`, includes `reason` (unauthorized, token_expired, auth_service_unavailable), `request_id`
- 2.3 DONE Key handlers (workspace create, auth session complete, run start) upgraded to `writeAuthErrorWithTracking` for PostHog tracking

**Acceptance Criteria:**
- [x] `auth_login_completed` fires on CLI device flow success
- [x] `auth_rejected` fires on auth failure in critical handlers
- [x] Existing `writeAuthError` remains backward-compatible (calls new function with null client)

---

## 3.0 Workspace Lifecycle Events

**Status:** DONE

Track workspace creation and GitHub App connection.

**Dimensions:**
- 3.1 DONE `workspace_created` event emitted in `workspaces_lifecycle.zig` after workspace insert + billing provisioning, includes `workspace_id`, `tenant_id`, `repo_url`, `request_id`
- 3.2 DONE `workspace_github_connected` event emitted in `github_callback.zig` after GitHub OAuth callback completes, includes `workspace_id`, `installation_id`, `request_id`

**Acceptance Criteria:**
- [x] `workspace_created` fires on `POST /v1/workspaces` success
- [x] `workspace_github_connected` fires on `GET /v1/github/callback` success
- [x] Both include request_id for correlation

---

## 4.0 API Error Code Tracking

**Status:** DONE

Track UZ-* error codes in PostHog for cross-domain error visibility.

**Dimensions:**
- 4.1 DONE `api_error` event function (`trackApiError`, `trackApiErrorWithContext`) captures `error_code`, `message`, `request_id`, optional `workspace_id`
- 4.2 DONE Billing error paths wired: invalid subscription ID, billing lifecycle event failures in `workspaces_billing.zig`
- 4.3 DONE Workspace free limit enforcement failures tracked in `workspaces_lifecycle.zig`
- 4.4 DONE Startup error codes (`UZ-STARTUP-*`) included in `startup_failed` event payload

**Acceptance Criteria:**
- [x] Billing, entitlement, and workspace enforcement errors appear in PostHog with UZ-* codes
- [x] Startup failures include structured error codes
- [x] All tracking is async/non-blocking (ring buffer → background flush)

---

## 5.0 Documentation And Test Coverage

**Status:** DONE

**Dimensions:**
- 5.1 DONE `docs/POSTHOG.md` — full event catalogue with schema, configuration, initialization sites, and contributor guide
- 5.2 DONE Tests extracted to `src/observability/posthog_events_test.zig` (main file under 500-line target)
- 5.3 DONE Integration test covers all 22 event functions with null client (no-op verification)
- 5.4 DONE Unit tests for `serverStartedProps`, `startupFailedProps`, `agentRunScoredProps`, `trustTransitionProps`, `distinctIdOrSystem`
- 5.5 DONE `check-pg-drain.py` passes (168 files scanned)

**Acceptance Criteria:**
- [x] All new events documented in `docs/POSTHOG.md`
- [x] Test depth gate: unit=506, integration=101
- [x] `make memleak` passes
- [x] `make lint` (ZLint) passes
- [x] `check-pg-drain.py` passes

---

## Files Changed

| File | Change |
|---|---|
| `src/observability/posthog_events.zig` | +9 event functions, +2 API error tracking functions, made helper fns public |
| `src/observability/posthog_events_test.zig` | NEW — extracted tests from main module |
| `src/errors/codes.zig` | +6 startup error codes (UZ-STARTUP-001..006) |
| `src/cmd/serve.zig` | +import, `trackServerStarted` before HTTP listen |
| `src/cmd/worker.zig` | +imports, `trackWorkerStarted`, `trackStartupFailed` for DB/migration |
| `src/http/handlers/workspaces_lifecycle.zig` | +import, `trackWorkspaceCreated`, `trackApiError` for billing enforcement |
| `src/http/handlers/github_callback.zig` | +import, `trackWorkspaceGithubConnected` |
| `src/http/handlers/auth_sessions_http.zig` | +import, `trackAuthLoginCompleted`, `writeAuthErrorWithTracking` |
| `src/http/handlers/common.zig` | +import, `writeAuthErrorWithTracking` (new), `writeAuthError` delegates |
| `src/http/handlers/workspaces_billing.zig` | `trackApiErrorWithContext` for billing errors |
| `src/http/handlers/runs/start.zig` | `writeAuthErrorWithTracking` for auth rejections |
| `docs/POSTHOG.md` | NEW — full event catalogue |
