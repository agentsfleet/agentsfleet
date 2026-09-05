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

# M190_001: Dashboard mutations paint before the server answers, and a completion no longer re-renders the page

**Prototype:** v2.0.0
**Milestone:** M190
**Workstream:** 001
**Date:** Sep 05, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing latency and resilience on every dashboard write and on every watched chat; nothing is broken, everything is slower and more brittle than the architecture doc already prescribes.
**Categories:** DOCS, UI
**Batch:** B11 — independent of the v2 cutover sequence; touches only `ui/packages/app` and one architecture doc.
**Branch:** feat/m190-dashboard-round-trips
**Test Baseline:** unit=5448 (`make test-unit-all` on main at fa989b444: cargo 2351 · app 2410 · website 175 · other packages 512) integration=recorded before the PR (Indy, Sep 05, 2026 — see Discovery)
**Depends on:** none
**Provenance:** LLM-drafted (Claude Fable 5.1, Sep 05, 2026) from an in-session review of every `lib/api` call path, server page, mutation surface and stream registry in `ui/packages/app`; Indy chose the scope from four offered batches.
**Canonical architecture:** `docs/architecture/web_app.md` §The five statements (statements 1, 4, 5) and §Scoreboard; `docs/architecture/data_flow.md` §D. WATCH

---

## Overview

**Goal (testable):** a run completing on a watched chat updates the metrics strip through one targeted Server Action and never calls `router.refresh`; every `request()` read retries transient failures and aborts on a default timeout; the runner detail and admin models pages issue their independent reads together; secrets delete, runner state changes and approval resolves paint their outcome before the server confirms and reconcile to server truth inside the same transition.

**Problem:** an operator watching a fleet chat pays a whole-page server re-render for every completed run: the workspace list walk, the fleet, billing, the 20-turn thread with bodies and the approvals inbox are all re-fetched to move three numbers on a strip. A single transient 503 on almost any read blanks a surface, because only three of roughly sixty call paths ride the retry policy the package already ships, and no server-side read carries a timeout. Two pages serialize reads that depend only on the URL. Every write except the fleet kill switch and the chat composer waits for the round-trip before the screen changes.

**Solution summary:** the completion hook calls a summary action that reads exactly what the strip shows; `request()` and `requestWithEtag()` run through the existing retry policy by default and apply a default timeout when the caller passes no signal; the runner detail and admin models pages start their view reads beside their primary reads, the shape `view-data.ts` already uses; secrets delete, runner cordon/drain/revoke and approval resolve adopt the `KillSwitch` shape (`useOptimistic`, then the action, then reconciliation inside one transition). The architecture scoreboard is re-measured in the same diff. The operator sees the strip move without the thread re-streaming, a transient error recover on its own, two pages paint one round-trip sooner, and rows change at the click.

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(ui): targeted completion refresh, default retry+timeout, optimistic rows
- **Intent (one sentence):** dashboard writes feel instant and dashboard reads survive a transient backend blip, without adding a client cache or moving any credential to the browser.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/KillSwitch.tsx` — the one optimistic mutation in the package: `useOptimistic`, the action, then `router.refresh()` inside the same `startTransition`, with rollback on failure and on 409. §4 copies this shape verbatim.
2. `ui/packages/app/lib/api/retry.ts` — the policy §2 makes the default. `isIdempotentMethod` and `classifyRetryable` already refuse to replay a POST or PATCH on a genuine 5xx; do not re-derive that gate.
3. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/view-data.ts` — reads that depend only on route params start beside the detail read and are awaited after it. §3 applies this to two more pages.
4. `ui/packages/app/lib/api/retry.integration.test.ts` — the real-transport harness (ephemeral HTTP server, recorded sleeps). §2's timeout and default-retry proofs extend it rather than stubbing `fetch`.
5. `docs/architecture/web_app.md` — statements 1, 4 and 5 are the constraints; the scoreboard is re-measured in this diff.

## Files Changed (blast radius)

All paths below `ui/packages/app/` unless stated.

| File | Action | Why |
|------|--------|-----|
| `lib/api/client.ts` | EDIT | the single attempt becomes internal; `request` and `requestWithEtag` ride the retry policy and apply the default timeout when no signal is supplied |
| `lib/api/retry.ts` | EDIT | the loop wraps the single-attempt primitive, including the ETag-carrying variant, so no caller can double-retry |
| `lib/api/events.ts`, `lib/api/fleets.ts` | EDIT | import site only: `requestWithRetry` now lives in `client.ts`, so the policy module never imports the transport |
| `lib/api/retry.test.ts`, `lib/api/retry.integration.test.ts` | EDIT | policy proofs in isolation; real-transport proofs for default retry and the hung read |
| `lib/api/client.retry.test.ts`, `lib/api/client.defaults.test.ts` | CREATE | the explicit `requestWithRetry` suite beside the transport; the `request()` default-policy suite — two files so each stays under the length cap |
| `vitest.setup.ts` | EDIT | the unit suite defaults to one attempt; suites that prove retry stub the switch back |
| `components/domain/useRefreshOnCompletion.ts` | EDIT | detects new terminal events and debounces as today; invokes a caller-supplied callback instead of the router |
| `tests/use-refresh-on-completion.test.ts`, `tests/fleet-thread-dynamic.test.ts` | EDIT | the hook never touches the router, one callback per burst; the dynamic shim forwards the callback |
| `components/domain/FleetThread.tsx` | EDIT | accepts the completion callback and passes it to the hook |
| `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/ChatView.tsx` | CREATE | client leaf that owns the live run summary: renders the strip and the thread, runs the summary action on completion, refreshes the router only when the fleet status changed |
| `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/ChatView.test.tsx` | CREATE | strip updates from the action; failure keeps last good values; status change triggers one refresh |
| `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/run-summary.ts` | CREATE | the `FleetRunSummary` shape, its two limits, and the one builder both the server thread page and the action reads go through |
| `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/run-summary.test.ts` | CREATE | thread rows and event rows build the same summary; failed and empty reads stay distinct |
| `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/view-data.ts` | EDIT | imports the approvals limit from `run-summary.ts` so the constant has one declaration |
| `app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` | EDIT | the chat view renders `ChatView` with the initial summary; the header moves out so the file returns under the length cap |
| `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetHeader.tsx` | CREATE | the breadcrumb and lifecycle-control row, extracted verbatim from the 402-line page |
| `app/(dashboard)/w/[workspaceId]/fleets/actions.ts` | EDIT | `getFleetRunSummaryAction` |
| `app/(dashboard)/admin/runners/[runnerId]/page.tsx` | EDIT | the view read starts beside the runner read |
| `app/(dashboard)/admin/models/page.tsx` | EDIT | platform keys read runs with the model list |
| `tests/runner-detail-page.test.ts`, `tests/admin-models-page.test.ts` | EDIT | both reads are in flight before either resolves; failure handling unchanged |
| `app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.tsx` (+ `.test.tsx`) | EDIT | optimistic row removal on delete |
| `app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.tsx` (+ `.test.tsx`) | EDIT | optimistic admin-state badge on cordon, drain, revoke; the action tests move to a sibling suite |
| `app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.actions.test.tsx` | CREATE | the admin-action tests by concern, plus the optimistic paint and rollback |
| `app/(dashboard)/admin/runners/[runnerId]/components/RunnerIdentityLine.tsx` | CREATE | the status, badges and degraded line extracted from the header, which sat at 317 lines before the edit |
| `app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList.tsx` | EDIT | row leaves before the POST resolves; restored on failure |
| `tests/approvals-list.test.ts` | EDIT | optimistic removal, restore on failure, already-resolved notice |
| `app/(dashboard)/w/[workspaceId]/approvals/[gateId]/ResolveButtons.tsx` | EDIT | drop the redundant refresh after the push; pending state unchanged |
| `tests/approvals-resolve-buttons.test.ts`, `tests/fleet-thread.test.ts` | EDIT | push once with no refresh; the completion callback replaces the router assertion |
| `docs/architecture/web_app.md` (repo root) | EDIT | scoreboard re-measured: `useOptimistic` count and any other moved row |
| `tests/web-app-scoreboard.test.ts` | CREATE | pins the scoreboard's `useOptimistic` row to the grep it describes |
| `tests/fleet-run-summary-action.test.ts` | CREATE | the summary action: three reads together, each boundary's failure, no token |

A changelog `<Update>` lands in `~/Projects/docs/changelog.mdx` on its own branch at CHORE(close), per `dispatch/lifecycle.md`; it is a cross-repo write and not a row here.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (the timeout constant, the summary action's limits and every new copy string are named constants; `10_000` never appears bare), ECL (a timeout, a 429 and a 404 are three classes; the retry layer already distinguishes them and `client.ts` must map the abort-on-timeout to the retryable `TIMEOUT` class, never to a cancel), NDC (the router import leaves `useRefreshOnCompletion.ts`; the redundant refresh leaves `ResolveButtons.tsx`), NLR (files opened here shed any dead branch they carry), TCF (every new test is made red before it is trusted: delete the retry branch and watch 2.1 fail), TST-NAM and TNM (test names state the behaviour, carry no milestone), HLP (no exported helper without a consumer; the single-attempt primitive stays module-private unless a test needs it), TFX (tests import `RETRY_DEFAULTS` and the timeout constant, never re-spell the numbers).
- `dispatch/write_ts_adhere_bun.md` — every file is TypeScript; TS FILE SHAPE DECISION at PLAN for the three new modules (`ChatView` is a component module, `run-summary` a functions-module, the hook test a test module); §11 timeout and cancellation applies to `client.ts` directly.
- `docs/architecture/web_app.md` — statement 1 (no client `fetch`; the summary action is a Server Action), statement 4 (`useOptimistic` on mutation surfaces), statement 5 (no fetch-on-mount; the completion hook is a stream subscription reacting to frames).
- `dispatch/verify.md` — every done-claim is a rubric row; a package-scoped vitest run never satisfies one.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| LENGTH (≤350 file / ≤50 fn / ≤70 method) | yes | `RunnerHeader.tsx` and `SecretsList.tsx` sit near the cap; the action-runner functions split to a `runner-header-actions.ts` sibling and the dialogs to `SecretDialogs.tsx` if the projected count crosses 300. `client.ts` gains a timeout helper and loses the inline attempt body |
| UFS | yes | `DEFAULT_REQUEST_TIMEOUT_MS`, `RUN_SUMMARY_APPROVALS_LIMIT`, `RUN_SUMMARY_LATEST_LIMIT` declared once; pin tests carry the carve-out comment |
| UI / DESIGN TOKEN | yes | `ChatView` composes existing primitives (`RunMetricsStrip`, `FleetThreadDynamic`); no raw HTML, no arbitrary values |
| MILESTONE-ID | yes | none in source or tests |
| LOGGING | no | the app bans console logging; the retry hooks remain the seam |
| ERROR REGISTRY | no | no new `UZ-` code; `TIMEOUT` is the retry layer's existing client-side class |
| SCHEMA GUARD / ZIG GATE | no | no schema, no Zig |
| GREPTILE | yes | the rule IDs above, audited at CONFORM |

## Prior-Art / Reference Implementations

- **Reference:** `KillSwitch.tsx` — optimistic paint plus reconciliation inside one transition. §4 aligns exactly; the only divergence is `ApprovalsList`, whose rows already live in client state, so its reconciliation is a state update rather than a refresh.
- **Reference:** `view-data.ts` — start-beside-the-detail-read. §3 aligns exactly.
- **Reference:** `ApiKeyList.tsx` `refresh()` — a targeted post-mutation re-read instead of a router refresh. §1's summary action is the same idea applied to a stream-driven trigger.
- **Reference:** `cli/src/lib/http-retry.ts` — the retry policy `retry.ts` mirrors; §2 changes which callers ride it, not the policy.

## Sections (implementation slices)

### §1 — A completion updates the strip, not the page

The strip shows fleet status, the latest run's outcome, tokens, cost, duration and the pending approval count. A completion frame carries none of the figures, so one Server Action reads them: the fleet detail, the newest event row and the fleet's pending approvals. The hook keeps its trailing-edge debounce and calls that action through a callback; `ChatView` holds the summary in state and re-renders the strip. Because the server-rendered lifecycle controls in the page header read the fleet status, a summary whose status differs from the last known one triggers one `router.refresh()` so those controls follow; an unchanged status triggers none. **Implementation default:** the newest event row comes from the events list with a limit of one, not from the thread, because the strip reads only list-row fields and the thread carries bodies.

- **Dimension 1.1** — DONE — the completion hook never calls the router; it invokes its callback once per debounced burst → Test `a burst of completions invokes the summary callback once and never the router`
- **Dimension 1.2** — DONE — the strip renders the action's latest row and pending count after a completion → Test `the strip shows the new run figures after the summary action resolves`
- **Dimension 1.3** — DONE — a failed summary read keeps the last good strip; nothing is blanked and no error surface appears → Test `a failed summary read leaves the strip unchanged`
- **Dimension 1.4** — DONE — a summary whose fleet status differs from the last known one refreshes the router exactly once; an unchanged status refreshes it zero times → Test `only a status change refreshes the server-rendered controls`
- **Dimension 1.5** — DONE — the server's thread page and the action's newest-event read go through one builder and yield the same summary → Test `the thread page and the newest-event read build the same summary`

### §2 — Every read retries and every request times out

`request()` and `requestWithEtag()` become the retrying calls; the single attempt moves behind them. The policy is the one `requestWithRetry` already applies: transient statuses and network errors retry with the existing backoff, a genuine 5xx never replays a non-idempotent method, `AGENTSFLEET_NO_RETRY` still means one attempt. By default only GET, HEAD and PUT retry; DELETE, POST and PATCH keep one attempt unless the caller opts in through `requestWithRetry`, because a DELETE whose 204 was lost answers 404 on the replay and a POST that timed out may have been processed. A request whose caller passes no `signal` gets `AbortSignal.timeout(DEFAULT_REQUEST_TIMEOUT_MS)`; a caller-supplied signal is respected as is and, once aborted, ends the loop without another attempt or a held backoff. A timeout abort maps to the retryable `TIMEOUT` class, distinct from a caller cancel. **Implementation default:** the timeout is the same value the SSE backfill already uses, and is declared once.

- **Dimension 2.1** — DONE — a GET answered 503 then 200 resolves with the 200 body through `request()` with no options → Test `request retries a transient read and returns the recovered body`
- **Dimension 2.2** — DONE — a POST answered 503 throws without a second attempt → Test `request does not replay a non-idempotent write on a server error`
- **Dimension 2.3** — DONE — a read with no caller signal that never answers rejects with the `TIMEOUT` class after the policy's attempts → Test `a hung read times out into the retryable class and stops after the attempt ceiling`
- **Dimension 2.4** — DONE — a caller-supplied signal is passed through and the default timeout is not added → Test `a caller signal wins over the default timeout`
- **Dimension 2.5** — DONE — the three `requestWithRetry` callers make exactly the configured number of attempts, not that number squared → Test `an explicit retry caller never retries twice`
- **Dimension 2.6** — DONE — a caller abort still surfaces as `RequestCancelledError`, never as a retry → Test `a navigation abort is a cancel, not a retry`
- **Dimension 2.7** — DONE — a DELETE is not retried by default, and an aborted caller signal ends the loop without a second attempt → Tests `request does not retry a DELETE on its own`, `an already-cancelled caller gets no second attempt`

### §3 — Independent reads start together

The runner detail page awaits the runner before starting the leases or activity read, though both need only the URL id; the admin models page reads the platform keys after the model list. Both pages start the second read beside the first and await it after, keeping every existing failure mapping.

- **Dimension 3.1** — DONE — the runner page issues the runner read and the view read before either resolves → Test `runner detail starts the view read beside the runner read`
- **Dimension 3.2** — DONE — the admin models page issues the model list and the platform keys read before either resolves → Test `admin models starts both reads together`
- **Dimension 3.3** — DONE — a 404 runner still renders not-found and a failed view read still renders its warning → Test `runner detail failure handling is unchanged by the parallel start`

### §4 — Writes paint before the server answers

Four surfaces adopt the `KillSwitch` shape. Secrets delete removes the row optimistically and refreshes inside the transition. Runner cordon, drain and revoke paint the target badge optimistically and refresh inside the transition; a 409 rolls back and the refresh shows the real state. The approvals inbox removes the row before the POST and restores it on a failed result; an already-resolved outcome keeps the row removed and shows the resolver. The approval detail page drops its second render after the push.

- **Dimension 4.1** — DONE — confirming a secret delete removes the row at once; a failed delete restores it with the error → Test `a secret row leaves on confirm and returns on failure`
- **Dimension 4.2** — DONE — a runner state action paints the target badge at once; a 409 restores the prior badge → Test `a runner badge paints the target state and rolls back on conflict`
- **Dimension 4.3** — DONE — approving from the inbox removes the row before the action resolves; a failed action restores it → Test `an inbox row leaves before the resolve settles and returns on failure`
- **Dimension 4.4** — DONE — an already-resolved outcome keeps the row removed and names the resolver → Test `an already resolved gate stays removed and shows who resolved it`
- **Dimension 4.5** — DONE — the approval detail resolve navigates once with no trailing refresh → Test `resolving from the detail page pushes once and does not refresh`
- **Dimension 4.6** — DONE — the architecture scoreboard reports the re-measured counts → Test `the scoreboard useOptimistic row equals the grep`

## Interfaces

```text
getFleetRunSummaryAction(workspaceId, fleetId) → ActionResult<FleetRunSummary>
FleetRunSummary = { status: FleetStatus; latest: EventRow | null;
                    pendingApprovals: number; pendingApprovalsHasMore: boolean }

request<T>(path, init, token)                 unchanged signature; now retries per
                                              retry.ts and applies the default
                                              timeout when init.signal is absent
requestWithEtag<T>(path, init, token)         same
requestWithRetry<T>(path, init, token, opts)  unchanged; the only way to pass
                                              per-call retry options
FleetThread props gain: onRunCompleted?: () => void
```

No new HTTP endpoint. Every read the summary action composes exists today.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Summary read fails | backend error or timeout after a completion | strip keeps its last values; the thread already shows the outcome; no error surface (1.3) |
| Run changed the fleet status | gate blocked, auto-pause | summary reports the new status; one router refresh; lifecycle controls follow (1.4) |
| Transient status on a read | 408, 425, 429, 502, 503, 504 | retried with backoff and Retry-After floor; the final failure surfaces as today (2.1) |
| Backend hangs | no response | default timeout aborts; `TIMEOUT` class retries up to the ceiling, then surfaces (2.3) |
| Server error on a write | 5xx on POST or PATCH | not replayed; error surfaces; optimistic state rolls back (2.2, 4.x) |
| Caller cancels | navigation, effect cleanup | `RequestCancelledError`, dropped silently, never retried (2.6) |
| Optimistic write rejected | 409, 4xx, network | row or badge restored; error shown beside the control (4.1, 4.2, 4.3) |
| Already resolved elsewhere | 409 with resolver | row stays removed; notice names the resolver (4.4) |
| Retry disabled | `AGENTSFLEET_NO_RETRY=1` | one attempt, as today (regression row) |

## Invariants

1. No stream-driven effect calls `router.refresh()` — `useRefreshOnCompletion.ts` has no router import; grep in R1 plus test 1.1.
2. Every request without a caller signal carries the default timeout — enforced in `client.ts`; test 2.4 asserts both branches.
3. A non-idempotent method is never replayed on a server 5xx — `isIdempotentMethod` gate, unchanged; test 2.2.
4. Optimistic state reconciles to server truth inside the transition that set it — the refresh or the state update runs inside the same `startTransition`; tests 4.1 to 4.4 assert the settled DOM equals the server response.
5. The browser holds no API token and issues no `fetch` to the backend — statement 1; the summary is a Server Action; grep unchanged from `web_app.md`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | n/a | n/a | n/a | n/a | n/a |

`approval_resolved` keeps firing from `ResolveButtons` after a successful resolve, unchanged. The retry layer's `onAttempt` and `onRetry` hooks stay the observability seam; the app emits no console output. Metrics review: no analytics or funnel playbook update required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `a burst of completions invokes the summary callback once and never the router` | three terminal frames inside the debounce window → callback called once; router mock never called |
| 1.2 | unit | `the strip shows the new run figures after the summary action resolves` | action resolves tokens 1200, one pending approval → strip renders both |
| 1.3 | unit | `a failed summary read leaves the strip unchanged` | action returns ok:false → previous figures still rendered, no alert |
| 1.4 | unit | `only a status change refreshes the server-rendered controls` | status active→paused → refresh once; active→active → refresh zero times |
| 1.5 | unit | `the thread page and the newest-event read build the same summary` | the same event as a thread turn and as a list row → equal summaries; null vs empty pages stay distinct |
| 2.1 | integration | `request retries a transient read and returns the recovered body` | real server scripts 503 then 200 → body of the 200; two requests logged |
| 2.2 | integration | `request does not replay a non-idempotent write on a server error` | POST scripted 503 → throws `ApiError` 503; one request logged |
| 2.3 | integration | `a hung read times out into the retryable class and stops after the attempt ceiling` | server never responds, tiny timeout → `ApiError` code `TIMEOUT`; attempts equal `RETRY_DEFAULTS.maxAttempts` |
| 2.4 | unit | `a caller signal wins over the default timeout` | init.signal supplied → fetch sees that signal; absent → fetch sees a timeout signal |
| 2.5 | unit | `an explicit retry caller never retries twice` | `requestWithRetry` with maxAttempts 3 against always-503 → exactly 3 fetches |
| 2.6 | unit | `a navigation abort is a cancel, not a retry` | fetch rejects AbortError → `RequestCancelledError`; one fetch |
| 2.7 | unit | `request does not retry a DELETE on its own`, `an already-cancelled caller gets no second attempt` | DELETE 503 → one fetch; aborted signal + TimeoutError → one fetch, code `TIMEOUT`; a backoff in progress ends on abort |
| regression | unit | `no-retry env still yields one attempt` | `AGENTSFLEET_NO_RETRY=1`, 503 → one fetch, throws |
| 3.1 | unit | `runner detail starts the view read beside the runner read` | both mocks pending → both called before either resolves |
| 3.2 | unit | `admin models starts both reads together` | same shape for model list and platform keys |
| 3.3 | unit | `runner detail failure handling is unchanged by the parallel start` | runner 404 → notFound; leases reject → warning alert |
| 4.1 | unit | `a secret row leaves on confirm and returns on failure` | confirm → row absent before action resolves; ok:false → row present, error text |
| 4.2 | unit | `a runner badge paints the target state and rolls back on conflict` | cordon → badge "cordoned" at once; 409 → prior badge, error text |
| 4.3 | unit | `an inbox row leaves before the resolve settles and returns on failure` | approve → row absent while pending; ok:false → row present |
| 4.4 | unit | `an already resolved gate stays removed and shows who resolved it` | already_resolved by "cli" → row absent; notice contains "cli" |
| 4.5 | unit | `resolving from the detail page pushes once and does not refresh` | ok resolved → push once, refresh zero |
| 4.6 | unit | `the scoreboard useOptimistic row equals the grep` | `grep -rl useOptimistic app components \| wc -l` equals the table's Today cell |
| e2e | e2e (existing) | `chat-single-fetch.spec.ts`, `workspace-fetch-dedupe.spec.ts` | thread reads per chat render stay 1; workspace list fetches stay ≤1 |

`/orly-write-unit-test` runs once per Section over that Section's diff and again at the boundary. `/orly-write-integration-test`: §2 crosses the transport boundary with real input and output and extends `retry.integration.test.ts`; the other sections are in-process and record `N/A`.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The completion hook never touches the router (§1) | `grep -cE "next/navigation\|router\." ui/packages/app/components/domain/useRefreshOnCompletion.ts` | 0 | P0 | |
| R2 | The default timeout is declared once and applied (§2) | `grep -c "DEFAULT_REQUEST_TIMEOUT_MS" ui/packages/app/lib/api/client.ts` | 2 or more | P0 | |
| R3 | Both pages start their reads together (§3) | `grep -c "Promise.allSettled" "ui/packages/app/app/(dashboard)/admin/models/page.tsx"; grep -n "startRunnerViewRead(\|await loadRunner(" "ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/page.tsx"` | 1; the `startRunnerViewRead(` call line precedes the `await loadRunner(` line | P0 | |
| R4 | Four mutation surfaces are optimistic (§4) | `grep -rl useOptimistic ui/packages/app/app ui/packages/app/components \| wc -l` | 4 | P0 | |
| R5 | Scoreboard re-measured (§4.6) | `grep -n "useOptimistic" docs/architecture/web_app.md \| grep -c "| 4 |"` | 1 | P1 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Slow tier green (code-carrying branch) | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Lint green | `make lint-all` | exit 0 | P0 | |
| S5 | Version sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ plus one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

N/A — no files deleted. Two symbols lose their only consumer and leave in the same diff: the router import in `useRefreshOnCompletion.ts`, and the post-push refresh call in `ResolveButtons.tsx`. Both are covered by R1 and test 4.5.

## Out of Scope

- The approvals inbox polling cadence and a stream-driven inbox. The workspace stream routes live frames only to per-fleet listeners; a wildcard subscriber is a registry change and a separate spec.
- `approvals.ts` resolving through a raw `fetch` instead of `request()`. It keeps its transport here; folding it in needs an `ApiError` body field and is a separate spec.
- The per-trigger fan-out in `last-delivery.ts`. The fix is a per-trigger field on the fleet detail, which lives in the Rust crates.
- The wall components copying `initialFleets` into state. No live mutation path reaches them today.
- A client-side data cache. Statement 1 of `web_app.md` rules it out; the browser holds no token.
- The exhaustion badge refreshing on a completion that does not change fleet status. It refreshes on the next navigation or on the status-change refresh, which covers auto-pause.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator watching a chat sees the run finish, the strip's tokens and cost move, and the thread does not flicker or re-scroll. They delete a secret and the row is gone as their finger lifts.
2. **Preserved user behaviour** — every read still happens on the server; every mutation still goes through the same Server Action; the kill switch, the chat composer and every list keep their current controls and copy.
3. **Optimal-way check** — the unconstrained shape is a single client store fed by the stream with every mutation applied locally. That conflicts with statement 1 and puts a second source of truth in the browser. Targeted actions plus optimistic paint deliver the moment without it.
4. **Rebuild-vs-iterate** — iterate. Every pattern already exists in the package once; this spec applies each to the surfaces that lack it.
5. **What we build** — one summary action and a client leaf for the chat, a default policy in `request()`, two parallel starts, four optimistic surfaces, one scoreboard update, one changelog entry.
6. **What we do NOT build** — a data-fetching library, a stream-driven inbox, a new endpoint, a retry policy that differs from the CLI's.
7. **Fit with existing features** — compounds with the one-stream wall and the chat's optimistic composer. It must not destabilize the ETag `If-Match` editor, which keeps using `requestWithEtag` and gains only the retry on its GET.
8. **Surface order** — UI only. The CLI already carries this retry policy; this spec brings the dashboard to parity.
9. **Dashboard restraint** — no new control. The strip shows the same figures from the same server fields; a failed summary read shows the old figures rather than a spinner or a guess.
10. **Confused-user next step** — a rejected optimistic write puts the row back and shows the same error copy it shows today, beside the control that caused it.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four sections by mechanism, not by page, so each has one reference implementation to copy and one rubric row to prove it. §1 is first because it is the largest steady-state cost. §2 is a transport change with no UI diff. §3 and §4 are page-local.
- **Alternatives considered:** a client cache library (rejected: statement 1, and it adds a dependency and a second truth); `"use cache"` with tags on the layout reads (rejected: every page is `force-dynamic` by design and reads are per-token); deriving the strip purely from stream frames (rejected: frames carry no tokens, cost or duration); making `requestWithRetry` the only export and touching all 18 modules (rejected: same outcome, larger diff, every caller re-reviewed).
- **Patch-vs-refactor verdict:** this is a **patch** because every change reuses a pattern already shipping in the package and no module boundary moves. The larger cut, a stream-driven inbox and a wildcard frame subscriber, is named in Out of Scope rather than mud-patched in.

## Discovery (consult log)

- **Consults** — Sep 05, 2026, Indy chose "Perf batch + optimistic rows" from four offered scopes; D (poll pause) and E (approvals transport) moved to a follow-up.
  - PLAN amendment (agent, Sep 05, 2026): Files Changed gained the import-site edits in `events.ts` and `fleets.ts`, the split test files, `vitest.setup.ts`, `view-data.ts`, `RunnerIdentityLine.tsx`, and the three existing tests under `tests/` that replace the CREATE rows. §2 narrows the default retry set to GET, HEAD, PUT and adds the abort guard after an adversarial read of the policy.
  - > Indy (2026-09-05 20:43): "Can you make the retry.ts more robust and performant, and change with effects" — asked which reading; Indy chose **rewrite on the Effect library**. Disposition: a separate spec, because it adds a dependency and breaks CLI parity; §2 here lands the wiring and the two guards, and the rewrite replaces `retry.ts` behind the same `runWithRetry` seam.
  - > Indy (2026-09-05 20:44): "adversarial review on retry.ts" — findings reported in session and carried into the Effect spec's Failure Modes; the two that touch the default path this spec introduces (DELETE replay answers 404; an aborted caller keeps retrying and sleeping) are fixed in §2.
  - > Indy (2026-09-05 20:46): "Have you upgraded all the packages to the latest vite is 5 and others" — no; dependency upgrades are outside this spec's Files Changed. `bun outdated` in the app package lists patch and minor bumps plus vitest 5.0.0; reported in session for a separate decision.
- **Metrics review** — no events added; `approval_resolved` unchanged; no analytics or funnel playbook update required.
- **Skill-chain outcomes** — populated during VERIFY and REVIEW.
- **Deferrals** — none at authoring. Baseline timing:
  > Indy (2026-09-05 20:23): "run the test-integration later, before PR" — context: the `verify.integration` baseline count is recorded at the pre-PR gate instead of CHORE(open); rubric S3 runs it there. The unit baseline was recorded at CHORE(open).
