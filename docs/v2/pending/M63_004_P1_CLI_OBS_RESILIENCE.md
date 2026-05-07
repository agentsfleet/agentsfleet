<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
- See docs/TEMPLATE.md "Prohibited" section above for canonical list.
-->

# M63_004: zombiectl resilience — retry/backoff + instrumented runCommand wrapper

**Prototype:** v2.0.0
**Milestone:** M63
**Workstream:** 004
**Date:** May 07, 2026
**Status:** PENDING
**Priority:** P1 — first network blip currently surfaces as a hard error to the user; CLI resilience is on the operator hot path.
**Categories:** CLI, OBS
**Batch:** B1 — independent of M63_005 (website polish); both can ship in parallel.
**Branch:** feat/m63-004-cli-resilience (to be created at CHORE(open))
**Depends on:** None.

**Canonical architecture:** `zombiectl` is the customer/operator entry point to the control plane; resilience here is the difference between "service blipped" and "user has to retry by hand".

---

## Implementing agent — read these first

1. `zombiectl/src/lib/http.js:14-66` — current `apiRequest`. Single fetch, 15s timeout, no retry, no backoff. The retry/backoff layer wraps this without modifying the inner fetch contract.
2. `zombiectl/src/program/http-client.js:15-31` — `printApiError` formatting (JSON + human modes). The new instrumentation must NOT change what the user sees on stderr; it only adds telemetry events.
3. `zombiectl/src/cli.js:280-318` — current top-level catch distinguishing `ApiError`, `TypeError("fetch failed")` (`API_UNREACHABLE`), and unknown (`UNEXPECTED`). The `runCommand` wrapper hoists this shape into a per-command boundary so each handler stops re-implementing it.
4. `zombiectl/src/lib/cli-analytics.js` — existing PostHog client and `trackCliEvent` helper. The new per-HTTP-request span events ride this same transport; no new analytics dependency.
5. `zombiectl/test/login.unit.test.js` — pattern for mocking `request` in handler unit tests. The error-matrix tests follow the same shape (mock fetchImpl, assert exit code + analytics event sequence).
6. `docs/REST_API_DESIGN_GUIDELINES.md` §error-shape — canonical error envelope. The retry policy keys off the server-side `error_code` (UZ-* prefix) where present, else HTTP status.
7. `docs/v2/done/M63_002_P1_CLI_TELEMETRY_CONSENT.md` — telemetry consent contract. New per-request spans MUST honor the same opt-out signal; no events fire when consent is denied.

If `process.env.ZOMBIE_NO_RETRY=1` is set, the retry layer must collapse to a single attempt — escape hatch for scripted callers that handle their own retry policy.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal. Specifically: RULE NSQ (no silent quiet — every retry surfaced via instrumentation), RULE EMS (exception messages are stable surface), RULE TST-NAM (no milestone IDs in test names).
- **`docs/BUN_RULES.md`** — TS/JS file shape, const/import discipline; new helpers under `zombiectl/src/lib/` follow the existing file-as-module pattern.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — only consulted for the error envelope shape; this spec does not add or modify any HTTP route.

Standard set is the floor; no Zig, schema, or auth flow involved.

---

## Overview

**Goal (testable):** A single network blip (transient ECONNRESET, 502, 503, 504) recovers transparently with exponential backoff (capped) and the user sees no error; a per-HTTP-request span event captures every attempt with status, duration, and retry-count for the analytics pipeline; every command runs through one `runCommand({ retry, instrument, errorMap })` wrapper that owns telemetry, error formatting, and exit codes — handlers stop reimplementing the catch-block.

**Problem (operator survey table):** The current CLI surface has three reds:

| Layer | Gap |
|-------|-----|
| HTTP client | No retry, no backoff, no circuit-breaker — first blip surfaces as user error. |
| Telemetry | No per-HTTP-request span/timing/retry-attempt event; only command-level analytics. |
| Generic command context | Each command rolls its own `parseFlags → request → print` shape; no `runCommand({ retry, instrument, errorMap })` wrapper, so resilience and error-mapping drift per command. |

**Solution summary:** Three additive layers, no behavioral change to handler code that opts out via `ZOMBIE_NO_RETRY=1`. (1) `apiRequestWithRetry()` wraps `apiRequest` with exponential backoff (250ms → 1s → 2s, jittered, max 3 attempts) — retries only on transient classes (network failure, 408, 425, 429, 5xx-without-Retry-After-zero). (2) Per-attempt span events (`cli_http_request`, `cli_http_retry`) emitted via the existing analytics client; consent-gated. (3) `runCommand({ name, retry, instrument, errorMap })` wrapper hoists the cli.js top-level catch into a per-command boundary that every handler routes through — handlers focus on shape and validation, the wrapper owns error formatting and exit codes.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/lib/http.js` | EDIT | Add `apiRequestWithRetry()` alongside existing `apiRequest`; classify error → retryable; expose retry decision via a callback hook for instrumentation. |
| `zombiectl/src/lib/run-command.js` | CREATE | New module exporting `runCommand({ name, retry, instrument, errorMap, handler })`. Owns the catch block currently inlined in `cli.js`. |
| `zombiectl/src/lib/cli-analytics.js` | EDIT | Add `trackHttpRequest({ url, method, status, duration_ms, attempt, retry_count })` and `trackHttpRetry({ ...attempt, reason })`. Both consent-gated through the existing helper. |
| `zombiectl/src/cli.js` | EDIT | Migrate the top-level catch to delegate to `runCommand`; preserve the exact stderr/JSON output shape. |
| `zombiectl/src/program/http-client.js` | EDIT | `request` (and `streamFetch` if applicable) call into `apiRequestWithRetry` by default; signature unchanged for handler callers. |
| `zombiectl/src/commands/core.js` | EDIT (light) | Use `runCommand` for `commandLogin` / `commandLogout` / hydration paths so the new boundary covers the auth-critical path first. |
| `zombiectl/src/commands/zombies.js`, `agents.js`, `workspaces.js`, `events.js` | EDIT (light) | Same migration pattern as core.js. Body unchanged; outer wrapper becomes `runCommand`. |
| `zombiectl/test/lib/http.unit.test.js` | CREATE | New unit suite for `apiRequestWithRetry` covering retry-on-503, no-retry-on-400, jitter bounds, max-attempts cap, `ZOMBIE_NO_RETRY=1` escape hatch. |
| `zombiectl/test/lib/run-command.unit.test.js` | CREATE | New unit suite for `runCommand` covering errorMap dispatch, instrumentation emit order, exit-code mapping, JSON-mode output. |
| `zombiectl/test/error-matrix.unit.test.js` | CREATE | Matrix test: every (status × content-type × error_code) cell asserts the printed stderr line + exit code + telemetry event. Drives the contract that the surface is stable across upgrades. |

---

## Sections (implementation slices)

### §1 — `apiRequestWithRetry`: classify + backoff + jitter

Wraps `apiRequest`. Classification keys off:
- Network failure (`TypeError("fetch failed")`, `ECONNRESET`, `ETIMEDOUT`) → retryable.
- HTTP 408, 425, 429, 502, 503, 504 → retryable.
- HTTP 4xx (non-408/425/429) → fatal, no retry.
- HTTP 5xx with `Retry-After: 0` → fatal (server explicitly says don't retry).
- Server `error_code` matching `UZ-*-RETRY-*` (forward-compat) → retryable regardless of status.

Backoff: 250ms → 1s → 2s, each delay jittered ±20%. Max 3 attempts. `ZOMBIE_NO_RETRY=1` collapses to 1 attempt.

**Implementation default:** plain `setTimeout` over a Promise; no third-party retry lib. The agent picks the jitter formula from existing patterns in the project (or a clean Math.random equivalent).

### §2 — Per-attempt span events

Add `cli_http_request` (one per terminal attempt — success or fatal) carrying `url`, `method`, `status`, `duration_ms`, `attempt` (1..N), `retry_count`. Add `cli_http_retry` (one per failed-and-will-retry attempt, fired before the backoff sleep) carrying `url`, `method`, `status`, `attempt`, `reason` (`network`|`timeout`|`5xx`|`429`|`server_marked_retryable`).

**`retry_count` semantics:** count of `cli_http_retry` events fired during this request. Equivalently, `attempt - 1` on the terminal event. So one fetch that succeeds first try → `attempt=1`, `retry_count=0`, zero retry events. Three fetches that all 503 → `attempt=3`, `retry_count=2`, two retry events. Pinning `retry_count = attempt - 1` lets the analytics pipeline read either field interchangeably.

Both events consent-gated through the existing telemetry helper — when consent is denied, the events do not fire and the retry behavior is unchanged.

**Implementation default:** events emit fire-and-forget through the existing analytics client; never await on the analytics call from the request hot path.

### §3 — `runCommand` wrapper

`runCommand({ name, retry, instrument, errorMap, handler })` does five things:
1. Records `cli_command_started` with the command name (already in cli.js).
2. Invokes `handler(ctx)`; on success records `cli_command_finished` and returns its exit code.
3. On `ApiError`: optionally remaps via `errorMap[err.code]` to a friendlier code/message; prints via the existing `printApiError`; emits `cli_error` with the (possibly remapped) code.
4. On `TypeError("fetch failed")`: emits `API_UNREACHABLE` exactly as cli.js does today; same stderr line.
5. On unknown: emits `UNEXPECTED`.

The wrapper centralizes what cli.js inlines today; handler code stops needing to import `ApiError`, `printApiError`, or analytics helpers.

**Implementation default:** keep the wrapper synchronous-friendly (returns a Promise) and do not change the `process.exit` semantics — exit codes flow up through `cli.js` exactly as they do now.

---

## Interfaces

```ts
// zombiectl/src/lib/http.js
apiRequestWithRetry(url: string, options: ApiRequestOptions & {
  // maxAttempts: 1..10 inclusive. Default 3. Values <1 or >10 throw
  // synchronously before any fetch is issued — out-of-range retry caps
  // are a misconfiguration, not a runtime decision.
  retry?: { maxAttempts?: number; baseDelayMs?: number; capDelayMs?: number };
  onAttempt?: (info: { attempt: number; status?: number; durationMs: number; retryReason?: RetryReason }) => void;
}): Promise<unknown>

// zombiectl/src/lib/run-command.js
runCommand<T = number>({
  name: string;                                  // command name for analytics
  handler: (ctx: CliContext) => Promise<T>;     // the actual command body
  retry?: boolean | { maxAttempts: number };    // default: { maxAttempts: 3 }
  instrument?: boolean;                          // default: true (consent-gated)
  errorMap?: Record<string, { code: string; message: string }>;
}): Promise<number>                              // exit code

// zombiectl/src/lib/cli-analytics.js — additive
trackHttpRequest(client, distinctId, info: HttpRequestInfo): void
trackHttpRetry(client, distinctId, info: HttpRetryInfo): void
```

Public consumer signatures (`request`, `streamFetch`, `apiRequest`, `ApiError`, `printApiError`) are unchanged.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Server returns 503 once, 200 second time | Transient backend blip | Retry with backoff; user sees success; one `cli_http_retry` event + one `cli_http_request` (terminal) event fire with `attempt=2`, `retry_count=1`. |
| Server returns 503 three times | Sustained outage | Three fetches, two retries (one before each backoff). ApiError surfaces with `code=HTTP_503`; user sees the existing error line; two `cli_http_retry` events + one terminal `cli_http_request` fire with `attempt=3`, `retry_count=2`. |
| Server returns 400 with UZ-VALIDATION-001 | Caller bug | No retry; `printApiError` prints exactly today's line; one `cli_http_request` event fires with `attempt=1`, `retry_count=0`. |
| Server returns 429 with `Retry-After: 5` | Rate-limited | Honor the header (override the default 250ms backoff for the first retry); proceed normally. |
| Network failure (DNS, connection refused) | Local network down | Retry policy applies; if exhausted, surfaces as `API_UNREACHABLE` (cli.js's existing line, unchanged). |
| User exports `ZOMBIE_NO_RETRY=1` | Scripted caller wants single attempt | One attempt only; no retries; first failure surfaces immediately. |
| Telemetry consent denied | `cli_telemetry_consent=denied` from M63_002 | No `cli_http_request` / `cli_http_retry` events fire; retries still run. |
| `errorMap[err.code]` returns falsy | Unmapped server error | Fall through to default `printApiError` path; no behavior change. |

---

## Invariants

1. **Stderr surface is stable.** For every (status, error_code, content-type) cell, the printed line is identical to the pre-spec output (spaces, capitalization, colon placement, request_id presence). Enforced by the new error-matrix test that pins the literal output across all branches.
2. **Exit code is stable per (error_code, JSON mode) tuple.** Enforced by the same matrix test (asserts the numeric code matches the existing table per command).
3. **Consent-gated telemetry.** When `cli_telemetry_consent=denied`, no `cli_http_*` events fire — enforced by a unit test that mocks the consent helper to denied and asserts zero analytics calls on the http path.
4. **Default cap of 3 attempts.** Enforced by `apiRequestWithRetry` unit test asserting attempt count never exceeds the configured cap, and that misconfiguration to `maxAttempts > 10` is rejected.
5. **`ZOMBIE_NO_RETRY=1` honored.** Enforced by a unit test that sets the env, fakes a 503, and asserts exactly one fetch call.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `retry_on_503_recovers_silently` | First fetch 503, second 200 → wrapper resolves with the 200 body; exactly one `cli_http_retry` event; one terminal `cli_http_request` event with `attempt=2`, `retry_count=1`. |
| `retry_exhausted_surfaces_apierror` | Three 503s → throws `ApiError` with `code=HTTP_503`; three fetch calls; two `cli_http_retry` events; one terminal `cli_http_request` event with `attempt=3`, `retry_count=2`. |
| `no_retry_on_400` | One 400 with `UZ-VALIDATION-001` → throws immediately, one fetch call, zero retry events. |
| `honor_retry_after_seconds` | 429 with `Retry-After: 5` → first backoff is ≥4500ms (jitter floor for 5s). |
| `network_failure_retried` | First call throws `TypeError("fetch failed")`, second succeeds → wrapper resolves; one retry event with `reason=network`. |
| `zombie_no_retry_env_collapses_to_single_attempt` | `ZOMBIE_NO_RETRY=1` + 503 first → throws after one attempt; zero retry events. |
| `invalid_max_attempts_rejected` | `apiRequestWithRetry(url, { retry: { maxAttempts: 11 } })` throws synchronously (or rejects on the first microtask) with a config error before any fetch is issued. Same for `maxAttempts: 0` and negative values. |
| `consent_denied_emits_zero_http_events` | Consent denied + 503-then-200 retry → wrapper resolves but no `cli_http_*` events fire. |
| `error_matrix_pins_stderr_lines` | Matrix over (200, 400, 401, 403, 404, 408, 429, 500, 503, network) × (json mode on/off) → asserts exact stderr line + exit code per cell. |
| `run_command_emits_started_finished` | Successful handler → analytics receives `cli_command_started` then `cli_command_finished` with the same correlation id. |
| `run_command_errormap_remaps_code` | `errorMap = { UZ-VALIDATION-001: { code: "WORKSPACE_NAME_INVALID", message: "..." } }` → printed line uses the remapped values; analytics event uses the remapped code. |
| `run_command_unknown_throw_surfaces_unexpected` | Handler throws `Error("kaboom")` → exit 1, JSON-mode payload uses `code=UNEXPECTED`. |
| `run_command_api_unreachable_passthrough` | Handler throws `TypeError("fetch failed")` → stderr line matches the existing `API_UNREACHABLE` template character-for-character. |

Regression set: every existing zombiectl unit test continues to pass — the migration to `runCommand` must not change exit codes or printed lines for any current command.

---

## Acceptance Criteria

- [ ] `make lint` clean — verify: `make lint`
- [ ] `cd zombiectl && bun test` clean — verify: `cd zombiectl && bun test`
- [ ] Error matrix test passes against the canonical fixture — verify: `cd zombiectl && bun test test/error-matrix.unit.test.js`
- [ ] Coverage on `zombiectl/src/lib/http.js` and `zombiectl/src/lib/run-command.js` ≥ 90% lines — verify: `cd zombiectl && bun test --coverage` and inspect.
- [ ] `gitleaks detect` clean — verify: `gitleaks detect`
- [ ] No file over 350 lines added — verify: `git diff --name-only origin/main | grep -v -E '\.md$|^vendor/' | xargs wc -l 2>/dev/null | awk '$1 > 350'`
- [ ] Manual smoke against `api-dev.usezombie.com` — kill the API mid-flight; CLI recovers transparently on retry.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Lint
make lint 2>&1 | grep -E "✓|FAIL" | tail -5

# E2: Unit tests
cd zombiectl && bun test 2>&1 | tail -5

# E3: Error matrix specifically
cd zombiectl && bun test test/error-matrix.unit.test.js 2>&1 | tail -10

# E4: 350-line gate
git diff --name-only origin/main | grep -v -E '\.md$|^vendor/' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines (limit 350)" }'

# E5: Gitleaks
gitleaks detect 2>&1 | tail -3

# E6: Smoke against dev — drop the test server, run a command, assert recovery
ZOMBIE_API_URL=https://api-dev.usezombie.com bin/zombiectl doctor 2>&1 | tail -5

# E7: ZOMBIE_NO_RETRY escape hatch
ZOMBIE_NO_RETRY=1 ZOMBIE_API_URL=http://localhost:9999 bin/zombiectl doctor 2>&1 | tail -3
```

---

## Dead Code Sweep

N/A — no files deleted. The retry/instrument layer is additive; existing `apiRequest`, `printApiError`, and `cli.js` machinery stays in place. Any handler that doesn't migrate to `runCommand` keeps working.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits coverage of the diff against the Test Specification. Iterate until clean. |
| After tests pass, before CHORE(close) | `/review` | Adversarial review focused on (a) the retry classifier (incorrect retryable classification = bad); (b) consent-gating of the new events; (c) printed-line stability across the matrix. |
| After `gh pr create` | `/review-pr` | Address greptile feedback inline; runs via `kishore-babysit-prs`. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit | `cd zombiectl && bun test` | _filled at VERIFY_ | |
| Lint | `make lint` | _filled at VERIFY_ | |
| Coverage | `cd zombiectl && bun test --coverage` | _filled at VERIFY_ | |
| Matrix | `cd zombiectl && bun test test/error-matrix.unit.test.js` | _filled at VERIFY_ | |
| Gitleaks | `gitleaks detect` | _filled at VERIFY_ | |
| 350L gate | `wc -l` over diff | _filled at VERIFY_ | |
| Smoke | `bin/zombiectl doctor` against dev | _filled at VERIFY_ | |

---

## Out of Scope

- Bounded concurrency / parallel HTTP — no current need (CLI does not fan out). Future SSE-multiplex would warrant a separate spec.
- Circuit-breaker (open/half-open/closed states). Retry + backoff is the floor; circuit breaking is a follow-up if dev metrics show repeated cascading failures.
- Server-side observability — the spec adds CLI-side spans only. Anything beyond the existing PostHog destination belongs in a separate observability spec.
- Migrating every handler in one PR. The wrapper is opt-in per handler; auth-critical paths (login, logout, hydration) migrate first; the rest can land on subsequent PRs without reopening this spec.
- Touching `streamFetch`'s SSE path. Streaming retry semantics are different (resumable streams need server-side cursor support); separate spec when that ships.
