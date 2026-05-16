# M71_001: CLI Login Resilience and UX Polish

**Prototype:** v2.0.0
**Milestone:** M71
**Workstream:** 001
**Date:** May 17, 2026
**Status:** PENDING
**Priority:** P2 — production login already works; this hardens the poll loop, error taxonomy, and UX prose for operators on flaky networks. Not blocking shipping the M68 trigger DX surface.
**Categories:** CLI
**Batch:** B1
**Branch:** feat/m71-001-cli-login-resilience (to be created at CHORE(open))
**Depends on:** M68_001 (DONE) — §13 D27 decomposition landed the named-stage skeleton this spec extends.
**Provenance:** agent-generated (deferred from `docs/v2/done/M68_001*.md` §13 during CHORE(close) on May 17, 2026).

**Canonical architecture:** `docs/ARCHITECTURE.md` §CLI — zombiectl login flow (Clerk JWT path; OAuth-poll handshake).

---

## Implementing agent — read these first

1. `zombiectl/src/commands/core.ts` — the canonical home for every dimension in this spec. M68_001 §13 D27 split `commandLogin` into named stages (`resolvePollParams`, `createLoginSession`, `announceLoginSession`, `maybeOpenBrowser`, `pollUntilComplete`, `persistAndHydrate`, `emitLoginResult`); each dimension below grafts onto one of those stages. The orchestrator's external signature `commandLogin(ctx, parsed, workspaces, deps)` is locked — do not change it.
2. `zombiectl/src/lib/error-map-presets.ts` — `AUTH_PRESET` is the existing error remap table. D28 tightens this, doesn't replace it. Mirror the per-key entry shape already in place.
3. `zombiectl/test/login.unit.test.ts` (post-D42 migration) — the pre-existing exit-code + stdout-shape contracts. The plural-flagged contracts (lines 145–172 and 174–204 in the legacy JS form) explicitly pin `exit 0` when hydration fails; D23 must preserve that.
4. `docs/v2/done/M68_001_P1_API_CLI_UI_DOCS_WEBSITE_TRIGGER_REGISTRATION_AND_FREE_TRIAL.md` §13 — the parent spec's "Deferred to follow-up" list names each dimension and what changed. Treat that prose as the contract this spec inherits.
5. `zombiectl/src/lib/http.ts` + `zombiectl/src/program/http-client.ts` — `RetryConfig` shape, attempt-event callback contract, and the existing exp-backoff helper for non-login HTTP. D29's poll-loop backoff should mirror this pattern, not reinvent it.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline (RULE NDC, RULE NSQ, RULE UFS, RULE TST-NAM).
- **`zombiectl/CLAUDE.md`** (if present) — package-local conventions; otherwise the global TS-strict migration intent in `~/.claude/CLAUDE.md` applies (no `as any`, `!`, or `@ts-expect-error` to silence strictness).
- **TS strict settings already enforced via `zombiectl/tsconfig.json`** — `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `useUnknownInCatchVariables`, `strict: true`. Every dimension below must compile under these unchanged.
- `docs/ZIG_RULES.md` and `docs/SCHEMA_CONVENTIONS.md` — N/A; this spec is TS-only.
- `docs/REST_API_DESIGN_GUIDELINES.md` — N/A; this spec is a CLI-side concern; the server-side login session endpoints stay frozen for this milestone.

---

## Overview

**Goal (testable):** `zombiectl login` survives transient network conditions (single 503/blip per poll cycle), gives operators a live countdown of session expiry, surfaces workspace-hydration failures on stderr, and maps every recoverable poll error to a distinct `AUTH_PRESET` code — without breaking the existing exit-code contract pinned in `test/login.unit.test.ts`.

**Problem:** M68_001 §13 D27 decomposed `commandLogin` into named stages but left five behaviors stubbed or absent. Operators on flaky networks see opaque "session expired" errors, silent workspace-hydration failures (post-login state never gets populated, next command 401s with no breadcrumb), and a poll loop that gives up on the first 503. The browser-handoff window has no visible countdown — operators alt-tab to the browser, miss a notification, then come back to a CLI that's already timed out.

**Solution summary:** Extend the existing `pollUntilComplete` and `persistAndHydrate` stages with five focused dimensions, each carrying its own test surface. No new top-level command; no new server-side endpoint; no schema change. The orchestrator's signature stays locked. Net new prod code is ~150 LOC across `src/commands/core.ts` + a thin extension of `src/lib/error-map-presets.ts`. Spec is sized to ship as a single PR.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/commands/core.ts` | EDIT | All five dimensions land here. The named stages from §13 D27 are the grafting points. |
| `zombiectl/src/lib/error-map-presets.ts` | EDIT | D28 tightens `AUTH_PRESET` with the new code → message mappings. No new exported preset, just additional entries on the existing one. |
| `zombiectl/test/login.unit.test.ts` | EDIT | Pre-existing exit-code/stdout-shape contracts unchanged; this spec adds rows. Specifically: countdown-tick assertions for D22, stderr-warning assertions for D23, per-code mapping for D28, backoff-delay calls for D29, single-blip survival for D30. |
| `zombiectl/test/login.acceptance.spec.ts` *(if it exists post-D42 acceptance migration)* | EDIT | One acceptance case: the full poll loop survives a single injected 503 and produces a successful login. Mirrors the §13 D31 acceptance pattern. |

> **Anti-pattern guard:** every other file in `zombiectl/src/` stays untouched. If a dimension demands cross-file work, it doesn't belong here — surface to spec author before reaching for new surface.

---

## Sections (implementation slices)

### §1 — D22: Session-expiry countdown (UX)

The browser-handoff window today shows a silent spinner during `pollUntilComplete`. Replace with a per-tick line: `Session expires in MM:SS — open the link in your browser…` updating on every poll. Deadline is computed client-side once at session-creation (`Date.now() + timeoutSec * 1000`); the response shape today carries no `expires_at_ms` from the server (the eventual cli-auth handshake hardening spec will add server-supplied deadlines; until then client-derived is the source). When `< 10s` remain, switch the prose to `Session expires in 0:0X — finish login soon` so operators get a visible nudge before timeout. Hidden in `--no-input` mode — that path stays mute.

**Implementation default:** mutate the in-place spinner line via the existing `stream.write("\r" + text)` pattern that `announceLoginSession` already uses. Don't reach for a new TTY library.

### §2 — D23: Fail-loud workspace hydration

`persistAndHydrate` today silences `hydrateWorkspacesAfterLogin` failures with `catch { return null; }`. That's correct for the exit code (login succeeded; hydration is best-effort), but the operator sees nothing on stderr — the next `zombiectl workspace list` 401s and looks like a broken login. Emit a single-line stderr warning: `warn: post-login workspace hydration failed (<err.code or "network">) — run "zombiectl workspace list" to retry`. Exit code remains 0; the unit-test pin in `test/login.unit.test.ts` (the "hydration fails but login still succeeds" rows) explicitly asserts exit 0 — that pin is binding.

**Implementation default:** narrow the catch to `unknown` per `useUnknownInCatchVariables`; type-guard via `instanceof ApiError` to extract `.code`; fall through to `"network"` literal otherwise.

### §3 — D28: Per-error-code AUTH_PRESET tightening

Today `AUTH_PRESET` maps a generic "auth failed" bucket. Tighten it to surface six distinct reasons during the poll loop:

| Internal trigger | Public code | User prose |
|---|---|---|
| Session row not in server's auth_sessions store | `InvalidSession` | "Login session not recognized — start over with `zombiectl login`." |
| Server returned 410 / past `expires_at` | `ExpiredSession` | "Login session expired. Start over with `zombiectl login`." |
| Fetch errored with `ECONNREFUSED`/`ENOTFOUND`/`ETIMEDOUT` | `NetworkError` | "Can't reach the server. Check connection and retry." |
| Server returned 429 | `RateLimited` | "Server rate-limited the poll loop. Backing off — this is transient." |
| Client-side `timeoutSec` exhausted | `Timeout` | "Login took too long. Start over with `zombiectl login`." |
| SIGINT during poll | `Interrupted` | "Login cancelled." |

These remap conditions zombiectl already encounters — no new error pathways, no new server contract. The taxonomy is what `--json` callers and the acceptance suite assert on.

**Implementation default:** keep the existing `AUTH_PRESET` export name and shape; this is six new keys, not a new preset. Conditions that don't match any of these continue to surface as the generic auth fallback (preserves backwards compat for unknown shapes).

### §4 — D29: Exponential-backoff polling with jitter

The poll loop today runs at a fixed `pollMs` cadence (default 2s, settable via `--poll-ms`). Switch to exp-backoff: start at the configured `pollMs` (default 1s), grow ×1.5 per attempt up to 5s cap, add ±20% jitter per tick. Honor server `Retry-After` (seconds or HTTP-date) if present in the 429 response — that beats the local backoff.

This caps polling RPS during retry storms (cli backlog of N operators all polling at 2s is the worst case; jittered exp-backoff smears it). It also doesn't change the happy-path latency meaningfully — the first attempt fires immediately, second at 1s+jitter, by which point most logins are already complete.

**Implementation default:** mirror the existing `backoffDelay()` helper in `src/lib/http.ts` rather than rolling a second one. RULE UFS — one named exp-backoff helper per package.

### §5 — D30: Transient-retry inside the poll loop

A single 503 or network blip mid-poll today kills the entire login. Treat one (1) transient failure per poll loop as recoverable: log it via the existing `attempt`-event callback (already routed to PostHog via `trackHttpRequest`/`trackHttpRetry` from `src/lib/analytics.ts`) and continue. A second transient in the same loop counts as `NetworkError` and propagates (the D28 path takes over).

The 1-blip budget is intentionally conservative — bigger budgets mask real outages. The acceptance test makes this contract visible: inject one 503, the login completes; inject two, the login surfaces `NetworkError`.

**Implementation default:** carry `transientCount: number = 0` in the `pollUntilComplete` local state. The same `RetryConfig`-style contract used by HTTP client retries is the wrong shape here — this is a much smaller, login-specific budget. Inline it.

---

## Interfaces

```
External signature — LOCKED, do not change:
  commandLogin(ctx: CommandCtx, parsed: ParsedArgs, workspaces: Workspaces, deps: CommandDeps): Promise<number>

Internal stage signatures — see §13 D27 in M68_001; this spec inherits them verbatim. New code grafts onto:
  - pollUntilComplete(session, deps, signal) — gains the transient budget + backoff
  - persistAndHydrate(token, workspaces, deps) — gains the stderr warning emit
  - emitLoginResult(result, ctx) — unchanged (already handles the success/failure branch)

JSON mode (`--json`) error envelope — UNCHANGED in shape, NEW codes:
  { error: { code: "InvalidSession" | "ExpiredSession" | "NetworkError" |
                   "RateLimited" | "Timeout" | "Interrupted",
             message: "..." } }
```

No new HTTP endpoints. No new flags. No new env vars.

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| Server returns 503 once during poll | Transient server / network blip | Log via analytics `attempt` callback, increment local transient counter, continue. |
| Server returns 503 twice in same poll | Sustained outage | Propagate as `NetworkError`; exit 1; D28 prose. |
| Server returns 429 with `Retry-After: N` | Rate limit | Sleep N seconds (capped at 30s); next attempt counts as fresh; D28 prose `RateLimited`. |
| Server returns 410 / 404 mid-poll | Session expired or deleted | Surface as `ExpiredSession` / `InvalidSession` respectively; exit 1. |
| `fetch` throws `TypeError: fetch failed` | DNS / TLS / refused | `NetworkError`; exit 1 unless first-blip budget covers it. |
| SIGINT mid-poll | Operator hit Ctrl-C | `Interrupted`; exit 130 (POSIX standard); existing handler. |
| Wall-clock `timeoutSec` exhausted | User stalled in browser past `--timeout` | `Timeout`; exit 1; clear the per-tick countdown. |
| `hydrateWorkspacesAfterLogin` rejects | Server unreachable post-login, or workspace endpoint 401 | Stderr warning per D23; exit code stays 0; credentials.json was already saved. |
| `Date.now()` clock skew during D22 countdown | Operator's clock jumps | Countdown shows the delta vs. session start; if it goes negative, prose flips to `Session expires in 0:00`. Real timeout is server-side; client display is informational. |
| `--no-input` mode | Scripted invocation | D22 countdown suppressed (no spinner); D23 stderr warning still emits (scripts want to see hydration failures); D29 backoff still applies (rate-limit safety isn't UX). |

---

## Invariants

1. `commandLogin` exit codes match `test/login.unit.test.ts` exactly — enforced by the pin tests already in the file. 0 = success (incl. hydration-failed), 1 = login failed, 130 = SIGINT.
2. `AUTH_PRESET` retains every existing key — D28 only adds keys. Enforced by the spec's own test that lists every pre-existing entry must still be in the exported preset.
3. `pollUntilComplete` makes ≤ ⌈timeoutSec / pollMs⌉ × 2 HTTP calls (the ×2 buffer absorbs jittered ticks and the transient budget). Enforced by an acceptance test that counts mock fetch invocations.
4. No prod TS file in this spec's blast-radius exceeds 350 lines after the edits. Enforced by RULE FLL pre-commit hook.
5. No `as any` / `!` / `@ts-expect-error` introduced. Enforced by `bun run lint` + `bun run typecheck`.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_d22_countdown_ticks_per_poll` | Per-poll write to stdout matches `/Session expires in \d+:\d{2}/`; transitions to single-digit second prose at `< 10s`. |
| `test_d22_countdown_suppressed_in_no_input` | `--no-input` produces zero countdown writes; only the final success line. |
| `test_d23_hydration_failure_emits_stderr_warning_exit_0` | When `hydrateWorkspacesAfterLogin` rejects, stderr matches `/warn: post-login workspace hydration failed/`; exit code is 0; credentials.json exists with 0600 mode. |
| `test_d23_hydration_success_emits_no_warning` | Happy path — no `warn:` line on stderr. |
| `test_d28_invalid_session_maps_to_InvalidSession` | 404 from server mid-poll → JSON envelope `error.code === "InvalidSession"`. |
| `test_d28_expired_session_maps_to_ExpiredSession` | 410 from server → `"ExpiredSession"`. |
| `test_d28_network_error_maps_to_NetworkError` | `fetch` throws `TypeError` → `"NetworkError"` (after blip budget). |
| `test_d28_rate_limited_maps_to_RateLimited` | 429 → `"RateLimited"`. |
| `test_d28_timeout_maps_to_Timeout` | `timeoutSec` exhausted → `"Timeout"`. |
| `test_d28_interrupted_maps_to_Interrupted` | SIGINT propagated → `"Interrupted"`, exit 130. |
| `test_d28_unknown_error_falls_through_to_generic` | Unmapped error retains the existing fallback prose; backwards compat. |
| `test_d29_first_poll_immediate` | First mock-fetch call fires at t≈0 (no backoff). |
| `test_d29_subsequent_polls_use_exp_backoff_with_jitter` | Inter-poll delays grow geometrically up to 5s cap; each delay is within ±20% of the nominal. |
| `test_d29_retry_after_honored` | 429 with `Retry-After: 3` → next poll fires at t+3s (overrides local backoff). |
| `test_d30_single_503_survives_login_completes` | Inject one 503 mid-poll; login completes; analytics `attempt` callback fires with `attempt: 2`. |
| `test_d30_double_503_surfaces_NetworkError` | Inject two 503s back-to-back; login fails with `NetworkError`. |
| `test_invariant_existing_pin_tests_still_pass` | All `test/login.unit.test.ts` rows that predate this spec still pass byte-for-byte. |
| `test_invariant_auth_preset_keys_superset` | Exported `AUTH_PRESET` contains every pre-existing key plus the six new codes from D28. |

Per-dimension acceptance:
- `acceptance_d30_full_poll_survives_one_injected_503` — stub backend injects one 503 on the second poll tick; assert exit 0, stdout contains `login complete`, credentials.json exists.

---

## Acceptance Criteria

- [ ] `bun run typecheck` clean — verify: `(cd zombiectl && bun run typecheck)`
- [ ] `bun run lint` 0/0 — verify: `(cd zombiectl && bun run lint)`
- [ ] `bun test` baseline + new rows all pass — verify: `(cd zombiectl && bun test)`
- [ ] `make harness-verify` 7/7 green — verify: `make harness-verify`
- [ ] `AUTH_PRESET` contains all six new codes — verify: `grep -E "InvalidSession|ExpiredSession|NetworkError|RateLimited|Timeout|Interrupted" zombiectl/src/lib/error-map-presets.ts | wc -l` ≥ 6
- [ ] `src/commands/core.ts` stays ≤ 350 lines — verify: `wc -l zombiectl/src/commands/core.ts`
- [ ] No `as any` / `!` / `@ts-expect-error` added — verify: `git diff origin/main..HEAD -- 'zombiectl/**/*.ts' | grep -E "as any|@ts-expect-error|: !" | wc -l` == 0
- [ ] PR #326 (parent M68) merged into main — verify: `gh pr view 326 --json state -q .state` == `MERGED`

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Typecheck + lint clean
(cd zombiectl && bun run typecheck && bun run lint) && echo "PASS" || echo "FAIL"

# E2: Test baseline preserved + new rows green
(cd zombiectl && bun test) | tail -5

# E3: AUTH_PRESET completeness
grep -cE "InvalidSession|ExpiredSession|NetworkError|RateLimited|Timeout|Interrupted" \
  zombiectl/src/lib/error-map-presets.ts

# E4: File-length cap on the primary touched file
wc -l zombiectl/src/commands/core.ts

# E5: No silenced strictness in the diff
git diff origin/main..HEAD -- 'zombiectl/**/*.ts' \
  | grep -E "^\\+.*\\b(as any|@ts-expect-error)\\b|^\\+.*: !\\s*[A-Z]" \
  | grep -v "^+++ "

# E6: Harness gates
make harness-verify
```

---

## Dead Code Sweep

N/A — no files deleted. D27's stage decomposition is the architecture; this spec extends those stages, doesn't replace any.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does |
|------|-------|--------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage against the Test Specification above. The 18 listed test rows are the floor. |
| After tests pass, still before CHORE(close) | `/review` | Adversarial diff review against this spec + the locked external signature + the pinned exit-code contracts. |
| After `gh pr create` | `/review-pr` | Post-merge-diff review on the open PR; comment-resolve before requesting human review. |

If `/review` flags D29's exp-backoff helper as duplicating the HTTP-side `backoffDelay`: that's the RULE UFS-relevant judgment call — pick one named helper and route both call sites through it. Captain decision per `feedback_gate_flag_triage`.

---

## Verification Evidence

> Filled in during VERIFY; this section is empty at PENDING.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit + acceptance tests | `(cd zombiectl && bun test)` | _pending_ | |
| Lint | `(cd zombiectl && bun run lint)` | _pending_ | |
| Harness | `make harness-verify` | _pending_ | |
| File-length | `wc -l zombiectl/src/commands/core.ts` | _pending_ | |
| Strictness compliance | `git diff ... \| grep ...` | _pending_ | |

---

## Out of Scope

- **D20 — Idempotency check** (already-logged-in detection). Deferred to the **cli-auth handshake hardening** sibling spec; overlaps with the new handshake UX.
- **D21 — Token name flag** (device label). Same sibling spec; needs schema work this milestone doesn't touch.
- **D24 — Token validation before save** (`/me` ping). Same sibling spec; the introspection endpoint is its responsibility.
- **D25 — Argv-leak warning for `--token`**. Adds a new auth pathway that the sibling spec owns.
- **D26 — TTY-priority env resolution** (`ZMB_TOKEN`/`ZOMBIE_TOKEN`). Same sibling spec.
- **D32 — `zombiectl logout --all`**. Needs server-side revocation design from the sibling spec (Clerk JWTs are stateless).
- **Server-side handshake redesign** — the `auth_sessions` endpoint shape, token introspection, expiry semantics, revocation. Out-of-scope on both axes (CLI-only milestone; M68_001 closed without touching them).
- **PostHog event-schema changes** — D30 emits the existing `cli_http_retry` event shape; no new event types or properties.
