<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M191_001: The dashboard retry policy is a declared schedule with a deadline, not a hand-rolled loop

**Prototype:** v2.0.0
**Milestone:** M191
**Workstream:** 001
**Date:** Sep 05, 2026
**Status:** PENDING
**Priority:** P2 — the loop shipped in M190_001 is correct for the cases it names; the findings below are exposures it does not close, none of which is failing today.
**Categories:** UI
**Batch:** B12 — after M190_001, whose `runWithRetry` seam this spec replaces behind.
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
**Depends on:** M190_001 (the transport/policy split and the `runWithRetry(attempt, method, options)` seam every caller now rides)
**Provenance:** LLM-drafted (Claude Fable 5.1, Sep 05, 2026) from an adversarial review of `ui/packages/app/lib/api/retry.ts` requested by Indy; Indy chose "rewrite on the Effect library" over hardening in place.
**Canonical architecture:** `docs/architecture/web_app.md` §The five statements (statement 1: the server fetches)

---

## Overview

**Goal (testable):** every dashboard request runs under a declared retry schedule with a total deadline, a capped `Retry-After`, abort at every step, full-jitter backoff, and a replay rule that never re-sends a write the server may have processed; the schedule is expressed with the Effect library's `Schedule` and `Effect.retry` behind the existing `runWithRetry` seam, so no caller changes.

**Problem:** the hand-rolled loop honours any `Retry-After` unbounded, so an intermediary answering `Retry-After: 120` holds a server render for two minutes. It has no total deadline, so a default read can spend about 31 seconds before it fails. It replays a POST on a timeout or network error, and a steer that the server did process becomes two events. It classifies network failures by matching Node's error text, and its socket-code branch reads the error instead of its `cause`, so it never fires under Node fetch. Its centred jitter keeps a herd of renders synchronised, and its telemetry reports every success as status 200.

**Solution summary:** `retry.ts` keeps its public surface and re-expresses the policy as a composed `Schedule`: exponential with full jitter, capped, bounded by attempts and by a total deadline, gated by a `Retry-After` cap, interruptible by the caller's signal. Classification becomes a tagged error with the failure's provenance (pre-send, post-send, timeout, status) read from `cause` codes rather than message text. Non-idempotent methods replay only on provably pre-send failures. The CLI keeps its own loop; both runtimes prove the same behaviour from one shared fixture table, so parity is a test rather than a mirror.

## PR Intent & comprehension handshake

- **PR title (eventual):** refactor(ui): retry policy as an Effect schedule with deadline and safe replay
- **Intent (one sentence):** a dashboard read never hangs a render past its budget, never replays a write the server may have taken, and every retry decision is a declared schedule a reviewer can read in one place.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/lib/api/retry.ts` — the seam (`runWithRetry`, `RetryOptions`, `RETRY_DEFAULTS`, `RETRY_CODE_TIMEOUT`) that stays; everything behind it is replaced.
2. `ui/packages/app/lib/api/client.ts` — the transport that calls the seam; `attemptWithEtag` is the unit of work the schedule retries, and its timeout mapping is the `timeout` class.
3. `ui/packages/app/lib/api/client.defaults.test.ts`, `client.retry.test.ts`, `retry.integration.test.ts` — the behaviours the rewrite must keep green before it adds any.
4. `cli/src/lib/http-retry.ts` — the sibling policy; §5 turns the informal mirror into a shared fixture both suites consume.
5. https://effect.website/docs/scheduling/introduction — `Schedule` composition (`exponential`, `jittered`, `upTo`, `recurs`, `whileInput`), and `Effect.retry` with `Schedule.tapInput` for telemetry.

## Files Changed (blast radius)

All paths below `ui/packages/app/` unless stated.

| File | Action | Why |
|------|--------|-----|
| `package.json` | EDIT | adds `effect` as a dependency; the only new package |
| `lib/api/retry.ts` | EDIT | the policy becomes a composed `Schedule`; the public seam is unchanged |
| `lib/api/retry-classify.ts` | CREATE | failure provenance as a tagged type, read from `cause` codes; replaces message sniffing |
| `lib/api/client.ts` | EDIT | passes the request's provenance (sent or not) to the classifier; no API change |
| `lib/api/retry.test.ts`, `lib/api/client.retry.test.ts`, `lib/api/client.defaults.test.ts`, `lib/api/retry.integration.test.ts` | EDIT | every existing case stays; new cases per Dimension |
| `lib/api/retry-classify.test.ts` | CREATE | provenance table: pre-send, post-send, timeout, each status |
| `samples/fixtures/retry-policy/cases.json` | CREATE | the shared schedule fixture both runtimes' tests replay |
| `cli/src/lib/http-retry.test.ts` (repo root) | EDIT | consumes the shared fixture; the CLI loop is not rewritten |
| `.size-limit.mjs` | EDIT only if measured | the shared-chunk budget moves only if `effect` reaches a client bundle, which §1 forbids |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — ECL (timeout, pre-send network, post-send network, 4xx, 5xx are five classes and take five paths), UFS (every schedule constant and error tag named once; the fixture file carries the same names), TFX (tests import the constants), TCF (each new guard is made red by removing it), NDC (message-sniffing and the dead socket-code branch leave), HLP (no exported helper without a consumer), TST-NAM.
- `dispatch/write_ts_adhere_bun.md` — §11 timeout and cancellation: the schedule owns the deadline; §3 no default exports; §9 one error style: the module throws `ApiError`, never returns a result type.
- `docs/architecture/web_app.md` — statement 1: the retry runs on the server; `effect` must not reach a client bundle.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| LENGTH | yes | policy and classification are two modules; each under the cap |
| UFS | yes | schedule constants, error tags, fixture keys declared once and imported |
| UI / DESIGN TOKEN | no | no component changes |
| MILESTONE-ID | yes | none in source, tests, or fixture |
| LOGGING / ERROR REGISTRY | no | no console output; no new `UZ-` code |
| Dependency | yes | one new package, server-only; Invariant 4 proves it stays off the client |

## Prior-Art / Reference Implementations

- **Reference:** Effect `Schedule` — exponential, jittered, `upTo`, `recurs`, composed with `Schedule.whileInput` for the replay gate. Aligned exactly; the loop's arithmetic is deleted, not ported.
- **Reference:** `cli/src/lib/http-retry.ts` — the behaviours that must survive: the transient status set, `Retry-After` as a floor, `AGENTSFLEET_NO_RETRY`, the 5xx replay gate. Divergence: the CLI is not rewritten; parity moves to a shared fixture.
- **Reference:** AWS Architecture Blog, "Exponential Backoff And Jitter" — full jitter over centred jitter for herd dispersion.

## Sections (implementation slices)

### §1 — The schedule replaces the loop

`runWithRetry` keeps its signature and runs the attempt under `Effect.retry` with a composed schedule. Defaults are the current ones where they exist and new where the loop had none. **Implementation default:** total deadline 20 s; `Retry-After` cap equal to the delay cap, above which the request fails at once; full jitter in `[0, delay]`.

- **Dimension 1.1** — a GET answered 503 then 200 still resolves with the 200 body, with attempt count and terminal telemetry unchanged → Test `the schedule keeps every behaviour the loop proved`
- **Dimension 1.2** — attempts stop when the total deadline is reached even though the attempt ceiling is not → Test `a read gives up at the deadline, not at the attempt count`
- **Dimension 1.3** — a `Retry-After` above the cap ends the request with the 429 in hand, without sleeping → Test `a Retry-After beyond the cap fails fast`
- **Dimension 1.4** — the caller's aborted signal interrupts a sleep and prevents the next attempt, as today, through Effect interruption → Test `an abort interrupts the schedule wherever it is`
- **Dimension 1.5** — jitter draws from `[0, delay]`, so two schedules seeded differently disperse → Test `full jitter never returns the bare delay twice in a row`
- **Dimension 1.6** — `effect` is not present in any client bundle → Test `the shared client chunk carries no effect module`

### §2 — Provenance decides replay, not the method alone

A failure carries where it happened. Pre-send failures (`ECONNREFUSED`, `ENOTFOUND`, `EAI_AGAIN`, a connect timeout) may replay any method. Post-send failures (`ECONNRESET`, `EPIPE`, a headers or body timeout, any 5xx) replay only idempotent methods. Codes are read from the error's `cause`, never from its message.

- **Dimension 2.1** — a POST whose connection was refused is replayed; a POST whose socket reset after sending is not → Test `a write replays only when it provably never left`
- **Dimension 2.2** — a browser-shaped `TypeError` with no `cause` is classified network, post-send → Test `a message without a cause is treated as possibly sent`
- **Dimension 2.3** — 408 and 425 are their own class, no longer labelled `5xx` → Test `every status maps to its own class`

### §3 — Telemetry says what happened

- **Dimension 3.1** — the terminal `onAttempt` carries the real success status (200, 201, 204), and `onRetry` carries the delay chosen → Test `telemetry reports the status and delay that occurred`
- **Dimension 3.2** — invalid schedule inputs (negative or non-finite delays) are refused at configuration with `CONFIG_INVALID` → Test `a NaN delay never becomes a hot loop`

### §4 — One fixture, two runtimes

- **Dimension 4.1** — `samples/fixtures/retry-policy/cases.json` enumerates scripted responses and expected attempt counts, delays, and replay decisions; both the dashboard suite and the CLI suite replay it → Test `both runtimes agree on every fixture case`

## Interfaces

```text
runWithRetry<T>(attempt: () => Promise<T>, method: string, options?: RetryOptions)
RetryOptions gains: deadlineMs?, retryAfterCapMs?  (defaults declared once)
RETRY_DEFAULTS gains: deadlineMs, retryAfterCapMs
classifyFailure(err, sent: boolean) → { class, provenance }   (retry-classify.ts)
```

No HTTP endpoint, no caller change: `request`, `requestWithEtag`, `requestWithRetry` keep their signatures.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Intermediary asks for a long wait | `Retry-After` above the cap | request fails at once with the 429; no sleep (1.3) |
| Backend hangs repeatedly | every attempt times out | the deadline ends the schedule before the attempt ceiling (1.2) |
| Write timed out server-side | POST post-send failure | not replayed; error surfaces; the optimistic row rolls back (2.1) |
| Backend refused the connection | pre-send | any method replays under the schedule (2.1) |
| Caller navigated away | signal aborted mid-sleep | interruption; `RequestCancelledError` semantics as today (1.4) |
| Herd after a blip | many renders retry together | full jitter disperses them (1.5) |
| Dependency leaks to the browser | a client module imports the policy | 1.6 fails the build's size gate |
| Config mistake | NaN or negative delay | `CONFIG_INVALID` at configuration (3.2) |

## Invariants

1. A non-idempotent method is replayed only on a pre-send failure — enforced by the classifier's provenance field and test 2.1.
2. No request outlives `deadlineMs` in retries — the schedule's `upTo`; test 1.2.
3. No sleep exceeds `retryAfterCapMs` — the gate before the schedule; test 1.3.
4. `effect` never reaches a client bundle — test 1.6 reads the build's client manifest.
5. The public seam is unchanged — the existing M190_001 suites pass unmodified before any new test is added (1.1).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | n/a | n/a | n/a | n/a | n/a |

The `onAttempt` and `onRetry` hooks remain the seam and become truthful (3.1). No console output. Metrics review: no analytics or funnel playbook update required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit + integration | `the schedule keeps every behaviour the loop proved` | the M190_001 suites pass unchanged against the new module |
| 1.2 | unit | `a read gives up at the deadline, not at the attempt count` | attempts 10, deadline 1 s, each attempt 400 ms → 3 attempts, error after ≈1 s |
| 1.3 | unit | `a Retry-After beyond the cap fails fast` | 429 with `Retry-After: 120` → one attempt, no sleep, `ApiError` 429 |
| 1.4 | unit | `an abort interrupts the schedule wherever it is` | abort during sleep → no further attempt; abort before attempt → none |
| 1.5 | unit | `full jitter never returns the bare delay twice in a row` | seeded random → delays within `[0, base]`, not equal to base |
| 1.6 | e2e (build) | `the shared client chunk carries no effect module` | `next build` client manifest contains no `effect` path |
| 2.1 | integration | `a write replays only when it provably never left` | POST to a closed port → 2 attempts; POST reset after write → 1 attempt |
| 2.2 | unit | `a message without a cause is treated as possibly sent` | `TypeError("Failed to fetch")` → network, post-send |
| 2.3 | unit | `every status maps to its own class` | 408 → timeout class, 425 → early, 429 → rate, 502/503/504 → server |
| 3.1 | unit | `telemetry reports the status and delay that occurred` | 204 success → status 204; retry → `delayMs` equals the sleep |
| 3.2 | unit | `a NaN delay never becomes a hot loop` | `baseDelayMs: NaN` → `CONFIG_INVALID`, zero attempts |
| 4.1 | unit (both runtimes) | `both runtimes agree on every fixture case` | each fixture case → same attempt count and replay decision in vitest and in the CLI suite |
| regression | integration | M190_001's `retry.integration.test.ts` | unchanged cases green |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The loop is gone (§1) | `grep -c "for (;;)" ui/packages/app/lib/api/retry.ts` | 0 | P0 | |
| R2 | No message sniffing (§2) | `grep -c "fetch failed" ui/packages/app/lib/api/retry.ts ui/packages/app/lib/api/retry-classify.ts` | each 0 | P0 | |
| R3 | Deadline and cap declared once (§1) | `grep -c "deadlineMs\|retryAfterCapMs" ui/packages/app/lib/api/retry.ts` | 4 or more | P0 | |
| R4 | Effect stays server-side (§1.6) | `grep -rl "from \"effect\"" ui/packages/app/app ui/packages/app/components \| wc -l` | 0 | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Slow tier green (code-carrying branch) | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Lint green | `make lint-all` | exit 0 | P0 | |
| S5 | Version sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ plus one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no file is deleted; `retry.ts` is rewritten in place.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `backoffDelay` (replaced by the schedule) | `grep -rn "backoffDelay" ui/packages/app/lib` | 0 matches outside a fixture replay |
| `classifyRetryable` (replaced by `classifyFailure`) | `grep -rn "classifyRetryable" ui/packages/app` | 0 matches |

## Out of Scope

- Rewriting the CLI's loop on Effect. The CLI stays hand-rolled; §4's fixture is the parity proof.
- An idempotency key for writes. That is a backend contract and belongs to an API spec; until it exists, §2's provenance rule is the safe ceiling for replays.
- Changing which methods `request()` retries by default (GET, HEAD, PUT since M190_001).
- Circuit breaking across requests. One request's schedule is this spec's unit.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator on a shaky network sees a page recover from a blip, and never sees a duplicated steer message or a page that hangs for half a minute.
2. **Preserved user behaviour** — every read that retries today retries; every write that is sent once today is sent once; the same error copy appears in the same places.
3. **Optimal-way check** — the direct fix is a declared schedule with a deadline. A hardened hand-rolled loop would carry the same guarantees in more code that a reviewer must simulate; Indy chose the library.
4. **Rebuild-vs-iterate** — rebuild the policy module behind an unchanged seam; the transport and every caller iterate by zero lines.
5. **What we build** — one schedule, one classifier, one fixture, one dependency.
6. **What we do NOT build** — a CLI rewrite, an idempotency key, a circuit breaker, any change to the retry defaults' method set.
7. **Fit with existing features** — compounds with M190_001's default retry and optimistic rows: a rollback now never follows a duplicated write. It must not destabilize the steer POST's optimistic reconcile, which depends on exactly one event per send.
8. **Surface order** — UI only; CLI parity through the fixture.
9. **Dashboard restraint** — no new control or copy; the strip and rows behave as they do.
10. **Confused-user next step** — the same error next to the same control, with the same retry-once affordance the surfaces already carry.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1 schedule, §2 provenance, §3 telemetry, §4 parity — each a reviewable unit with its own rubric row.
- **Alternatives considered:** hardening the loop in place (rejected by Indy: same guarantees, more hand-written control flow); adopting Effect for the CLI too (rejected for now: the CLI ships as a small binary and its loop is stable; parity moves to a fixture instead).
- **Patch-vs-refactor verdict:** this is a **refactor** because the policy's control flow is replaced wholesale behind a fixed seam; the patch alternative is named and was declined in session.

## Discovery (consult log)

- **Consults** — Sep 05, 2026: asked whether "make retry.ts more robust and performant, and change with effects" meant hardening in place or the Effect library; Indy chose the Effect library. The adversarial review that produced the Failure Modes above is recorded in M190_001's Discovery.
- **Metrics review** — no events added; no analytics or funnel playbook update required.
- **Skill-chain outcomes** — populated during VERIFY and REVIEW.
- **Deferrals** — none at authoring.
