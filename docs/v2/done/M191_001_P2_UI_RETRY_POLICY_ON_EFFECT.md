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
**Status:** DONE
**Priority:** P2 — the loop shipped in M190_001 is correct for the cases it names; the findings below are exposures it does not close, none of which is failing today.
**Categories:** UI
**Batch:** B12 — after M190_001, whose `runWithRetry` seam this spec replaces behind.
**Branch:** feat/m191-retry-effect-schedule
**Test Baseline:** unit=2355 (`make test-unit-all` on main at 685725d7f, cargo lane: 123 binaries, 0 failed; the vitest lanes were stopped at Indy's direction and are recorded before the PR — see Discovery) integration=recorded before the PR (Indy, Sep 06, 2026 — see Discovery)
**Test Delta:** `make test-unit-all` at the boundary — cargo 2355 passed, 0 failed (baseline 2355, +0: the Rust diff is the version bump); app vitest 2605 passed in 274 files at 100% statements, branches, functions and lines; website 146; CLI 1671 passed, 0 failed, 1684 ran (main at 685725d7f ran 1659: +25); design-system 559. `make test-integration-rustd` recorded 🟠 per Indy (see Discovery).
**Depends on:** M190_001 (the transport/policy split and the `runWithRetry(attempt, method, options)` seam every caller now rides)
**Provenance:** LLM-drafted (Claude Fable 5.1, Sep 05, 2026) from an adversarial review of `ui/packages/app/lib/api/retry.ts` requested by Indy; Indy chose "rewrite on the Effect library" over hardening in place.
**Canonical architecture:** `docs/architecture/web_app.md` §The five statements (statement 1: the server fetches)

---

## Overview

**Goal (testable):** every dashboard request runs under a declared retry schedule with a total deadline, a capped `Retry-After`, abort at every step, full-jitter backoff, and a replay rule that never re-sends a write the server may have processed; the schedule is expressed with the Effect library's `Schedule` and `Effect.retry` behind the existing `runWithRetry` seam, so no caller changes.

**Problem:** the hand-rolled loop honours any `Retry-After` unbounded, so an intermediary answering `Retry-After: 120` holds a server render for two minutes. It has no total deadline, so a default read can spend about 31 seconds before it fails — and Next serialises a client's Server Actions, so a read that long holds every later action from that tab (an approve, a steer) behind it. It replays a POST on a network error, and a steer that the server did process becomes two events; M190_001 closed the client-timeout half of that gate for the dashboard, but the CLI's loop (`cli/src/lib/http-retry.ts`) still replays a POST after its own 15 s timeout, and the network-error half is open in both. It classifies network failures by matching Node's error text, and its socket-code branch reads the error instead of its `cause`, so it never fires under Node fetch. Its centred jitter keeps a herd of renders synchronised, and its telemetry reports every success as status 200.

**Solution summary:** `retry.ts` keeps its public surface and re-expresses the policy as a composed `Schedule`: exponential with full jitter, capped, bounded by attempts and by a total deadline, gated by a `Retry-After` cap, interruptible by the caller's signal. Classification becomes a tagged error with the failure's provenance (pre-send, post-send, timeout, status) read from `cause` codes rather than message text. Non-idempotent methods replay only on provably pre-send failures. The CLI keeps its own loop; both runtimes prove the same behaviour from one shared fixture table, so parity is a test rather than a mirror.

## PR Intent & comprehension handshake

- **PR title (eventual):** refactor(ui): retry policy as an Effect schedule with deadline and safe replay
- **Intent (one sentence):** a dashboard read never hangs a render past its budget, never replays a write the server may have taken, and every retry decision is a declared schedule a reviewer can read in one place.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/lib/api/retry.ts` — the seam (`runWithRetry`, `RetryOptions`, `RETRY_DEFAULTS`, `RETRY_CODE_TIMEOUT`) that stays; everything behind it is replaced.
2. `ui/packages/app/lib/api/client.ts` — the transport that calls the seam; `attemptWithEtag` is the unit of work the schedule retries, and its timeout mapping is the `timeout` class.
3. `ui/packages/app/lib/api/client.defaults.test.ts`, `client.retry.test.ts`, `retry.integration.test.ts` — the behaviours the rewrite must keep green before it adds any.
4. `cli/src/lib/http-retry.ts` — the sibling policy; §4 turns the informal mirror into a shared fixture both suites consume.
5. `cli/node_modules/effect/dist/Schedule.d.ts` and `Effect.d.ts` at `4.0.0-rc.112`, the version this repository already ships (https://effect.website/docs/scheduling/introduction for the prose) — `Schedule.exponential`, `recurs`, `upTo`, `modifyDelay` (full jitter scales the delay by the `randomFn` seam, `Math.random` by default; `Schedule.jittered` scales by 0.8 to 1.2 and is the centred jitter this spec removes), `tap` for telemetry, and `Schedule.while` for the replay, cap, deadline and abort gates, composed before the ceiling so a refused step draws no delay and reports no retry. `Clock` is a `Context.Reference`, so the `sleepImpl` seam is a clock whose sleep is the seam, and `Effect.uninterruptibleMask` makes that sleep the run's only interruptible moment: an abort mid-attempt lets the attempt settle and surfaces what it threw, as the loop did.

## Files Changed (blast radius)

All paths below `ui/packages/app/` unless stated.

| File | Action | Why |
|------|--------|-----|
| `package.json`, `bun.lock` (repo root) | EDIT | adds `effect` at `4.0.0-rc.112`, the version `cli/package.json` already pins; the only new package |
| `lib/api/retry.ts` | EDIT | the policy becomes a composed `Schedule` run by a `RetryRun` (one request's attempts and what they threw); the public seam is unchanged |
| `lib/api/retry-config.ts` | CREATE | the options, their defaults and their one validation, split from the policy at the length cap |
| `lib/api/errors.ts` | EDIT | `HTTP_STATUS_REQUEST_TIMEOUT`, `RETRY_CODE_TIMEOUT` and `isDefiniteRefusal` move beside `ApiError`: status facts a client component reads without the policy's dependency; nothing is re-exported — every importer reads `lib/api/errors` |
| `app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.tsx`, `lib/errors.ts` | EDIT | the two client-side importers of the policy read the status facts from `lib/api/errors` instead; through `lib/errors.ts` every dashboard client bundle would otherwise have carried `effect` (Invariant 4) |
| `lib/api/retry-backoff.ts`, `lib/api/retry-backoff.test.ts` | DELETE | the schedule owns delay and sleep; `backoffDelay`, `defaultSleep` and `sleepUnlessAborted` have no caller once the loop is gone |
| `lib/api/retry-classify.ts` | CREATE | failure provenance as a tagged type, read from `cause` codes; replaces message sniffing |
| `lib/api/client.ts` | EDIT | the send and the read each classify their own failure with what only that side knows (whether headers arrived); every request runs under the policy, a write with one attempt, so a cancel lands the same way; the default per-attempt timeout is `attemptTimeoutMs(remainingMs)`, the smaller of 10 s and what remains of the deadline, so the last attempt cannot outlive it; no API change |
| `lib/api/retry.test.ts`, `lib/api/retry.integration.test.ts` | EDIT | every behavioural case stays; the two describe blocks that pinned the deleted `backoffDelay` and `classifyRetryable` leave with them, the pins of the deleted jitter arithmetic move (a `randomFn` of 0.5 or 0 becomes 1, the bare delay), and the integration suite gains the refused-at-connect and reset-after-sending cases over a real socket. `client.retry.test.ts` and `client.defaults.test.ts` pass unmodified |
| `lib/api/retry.schedule.test.ts` | CREATE | the schedule's own guarantees: deadline (including the budget each attempt is told), cap, full jitter, provenance, interruption, telemetry, configuration |
| `lib/api/retry.fixture.test.ts` | CREATE | replays the shared fixture through the dashboard transport |
| `lib/api/retry-classify.test.ts` | CREATE | provenance table: pre-send, post-send, timeout, each status |
| `samples/fixtures/retry-policy/cases.json` (repo root, beside `wire-v2` and `model-library`) | CREATE | the parity table both runtimes' tests replay: fourteen scripted cases with the attempt count each must make |
| `cli/src/lib/http-retry.ts` (repo root) | EDIT | the classifier reads provenance off the socket code (on the error under Bun, on its cause under Node) and the gate refuses a post-send replay for a non-idempotent method, so a timed-out or reset POST is not re-sent; a `Retry-After` above the same 10 s cap the dashboard has fails at once; the loop itself is not rewritten |
| `cli/test/http-retry.fixture.test.ts`, `cli/test/http-retry.provenance.unit.test.ts` (repo root) | CREATE | the CLI replays the shared fixture; the CLI's own reading of each provenance class. `http-retry.unit.test.ts` and `http-retry.integration.test.ts` pass unmodified |
| `cli/test/coverage-fill-patch.unit.test.ts` (repo root) | EDIT | its bare `{ code }` error takes the shape Node's fetch throws, a `TypeError` with the code on the cause |
| `cli/test/acceptance/retry-policy.spec.ts`, `cli/test/acceptance/run-lane.ts` (repo root) | CREATE, EDIT | the built binary against a scripted loopback server: a read recovers from a 503, a read reset after sending is read again, a write reset after sending is sent once; registered in the deterministic lane |
| `tests/e2e/acceptance/retry-policy.spec.ts` | CREATE | the Playwright acceptance lane drives the dashboard transport and the CLI binary against one scripted origin: the blip, the fail-fast `Retry-After`, the reset write |
| `.size-limit.mjs`, `scripts/server-only-modules.mjs`, `scripts/server-only-modules.d.mts`, `tests/server-only-modules.test.ts` | EDIT, CREATE, CREATE, CREATE | the Invariant 4 guard: the installed Effect runtime must carry the marker (`effect/Effect/Yield`, so a renamed marker fails loudly) and no client chunk the budgets measure may; the scan is a pure module with its own test; the budgets themselves do not move |
| `lib/api/api-keys-types.ts`, `lib/api/admin-model-library-types.ts`, `lib/api/runners-types.ts`, `lib/api/secrets-types.ts`, `lib/api/model-library-types.ts`, `lib/api/fleets-types.ts`, `lib/api/preferences-types.ts`, `lib/api/approvals-types.ts`, `lib/api/connectors-types.ts`, `lib/api/events-types.ts`, `lib/api/workspaces-types.ts` | CREATE | the constants and pure helpers client components read (`KEY_NAME_REGEX`, `API_KEY_SORTS`, the rate helpers `nanosToUsdPerMtok` and `usdPerMtokToNanos` reading `NANOS_PER_USD` from `lib/types` (its one home, with `OPENAI_COMPATIBLE_PROVIDER`; the copy `admin_model_library.ts` carried on main is gone), `LEASE_OUTCOME`, `parseLabels`, the runner sandbox, network-policy and admin-action vocabulary with `parseRegistryAllowlist` and `AssignedPolicy`, `SECRET_KIND`, `providerLabel`, `AGENTSFLEET_STATUS`, `PREFERENCE_KEY`, `APPROVAL_DECISION`, `CONNECTOR_STATUS`, `FRAME_KIND` with the four same-origin stream URL builders, the workspace-name checks, and their kin), moved out of the eleven domain modules that import the transport, so no client bundle carries `client.ts`, `retry.ts` or `effect`; the `library-types.ts` pattern. Each module is dependency-free; the trace that proved it (every `"use client"` file's value-import graph) reaches the transport from zero client files |
| `lib/api/api_keys.ts`, `lib/api/admin_model_library.ts`, `lib/api/runners.ts`, `lib/api/secrets.ts`, `lib/api/model_library.ts`, `lib/api/fleets.ts`, `lib/api/preferences.ts`, `lib/api/approvals.ts`, `lib/api/connectors.ts`, `lib/api/events.ts`, `lib/api/workspaces.ts` | EDIT | the eleven domain modules import what they still use from their `-types` sibling; nothing is re-exported |
| `app/(dashboard)/admin/models/components/AddModelDialog.tsx`, `app/(dashboard)/admin/models/components/CatalogueList.tsx`, `app/(dashboard)/admin/models/components/EditModelDialog.tsx`, `app/(dashboard)/admin/models/components/MakeDefaultDialog.tsx`, `app/(dashboard)/admin/runners/[runnerId]/components/LeaseTable.tsx`, `app/(dashboard)/admin/runners/[runnerId]/components/ReviewLease.test.tsx`, `app/(dashboard)/admin/runners/[runnerId]/components/ReviewLease.tsx`, `app/(dashboard)/admin/runners/[runnerId]/components/runner-copy.ts`, `app/(dashboard)/admin/runners/components/AddRunnerDialog.tsx`, `app/(dashboard)/admin/runners/components/RunnerTile.tsx`, `app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog.tsx`, `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetHeader.tsx`, `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetInstallGate.tsx`, `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/KillSwitch.tsx`, `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/RunMetricsStrip.tsx`, `app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.tsx`, `app/(dashboard)/w/[workspaceId]/fleets/components/FleetWall.tsx`, `app/(dashboard)/w/[workspaceId]/fleets/components/GettingStarted.tsx`, `app/(dashboard)/w/[workspaceId]/fleets/new/InstallStreamSteps.tsx`, `app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.test.tsx`, `app/(dashboard)/w/[workspaceId]/settings/models/actions.ts`, `app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog.tsx`, `app/(dashboard)/w/[workspaceId]/settings/models/components/EditModelEntryDialog.tsx`, `app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider.tsx`, `app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog.tsx`, `app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCells.tsx`, `app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderModelSelect.tsx`, `components/layout/GettingStartedWidget.refresh.test.tsx`, `components/layout/GettingStartedWidget.test.tsx`, `components/layout/GettingStartedWidget.tsx`, `lib/actions/preferences.ts`, `lib/api/admin_model_library.test.ts`, `lib/api/model_library.test.ts`, `lib/api/preferences.test.ts`, `lib/api/runners.test.ts`, `lib/streaming/fleet-stream-facts.ts`, `lib/types.ts`, `lib/wall/tile-liveness.test.ts`, `lib/wall/tile-liveness.ts`, `tests/admin-models-management.test.ts`, `tests/admin-models-ui.test.ts`, `tests/e2e/acceptance/runner-detail.spec.ts`, `tests/models-provider-loading.test.tsx`, `tests/models-registry-table.test.tsx`, `tests/provider-model-select.test.tsx`, `app/(dashboard)/admin/runners/[runnerId]/components/ActivityTable.test.tsx`, `app/(dashboard)/admin/runners/[runnerId]/components/ActivityTable.tsx`, `app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.tsx`, `app/(dashboard)/admin/runners/[runnerId]/components/RunnerIdentityLine.tsx`, `app/(dashboard)/admin/runners/[runnerId]/components/RunnerSandboxPanel.test.tsx`, `app/(dashboard)/admin/runners/[runnerId]/components/RunnerSandboxPanel.tsx`, `app/(dashboard)/admin/runners/[runnerId]/page.tsx`, `app/(dashboard)/admin/runners/actions.ts`, `app/(dashboard)/admin/runners/components/AddRunnerDialog.test.tsx`, `app/(dashboard)/admin/runners/components/EditPolicyDialog.test.tsx`, `app/(dashboard)/admin/runners/components/EditPolicyDialog.tsx`, `app/(dashboard)/admin/runners/components/policy-binds.ts`, `app/(dashboard)/admin/runners/components/PolicyFields.test.tsx`, `app/(dashboard)/admin/runners/components/PolicyFields.tsx`, `app/(dashboard)/admin/runners/components/RunnerDialogs.tsx`, `app/(dashboard)/admin/runners/components/RunnerListCells.tsx`, `app/(dashboard)/admin/runners/components/RunnerStatus.tsx`, `app/(dashboard)/settings/api-keys/actions.ts`, `app/(dashboard)/settings/api-keys/components/ApiKeyList.tsx`, `app/(dashboard)/w/[workspaceId]/approvals/[gateId]/ResolveButtons.tsx`, `app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList.tsx`, `app/(dashboard)/w/[workspaceId]/approvals/page.tsx`, `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/ChatView.test.tsx`, `app/(dashboard)/w/[workspaceId]/integrations/components/connector-rows.tsx`, `app/(dashboard)/w/[workspaceId]/integrations/components/IntegrationsConnectors.tsx`, `components/domain/useFleetRunSummary.test.tsx`, `components/domain/useWorkspaceStream.ts`, `components/layout/CreateWorkspaceDialog.tsx`, `lib/api/api_keys.test.ts`, `lib/api/events.test.ts`, `lib/api/runners.selftest.test.ts`, `lib/streaming/fleet-stream-backfill.ts`, `lib/streaming/fleet-stream-facts.test.ts`, `lib/streaming/fleet-stream-frames.figures.test.ts`, `lib/streaming/fleet-stream-frames.live.test.ts`, `lib/streaming/fleet-stream-frames.test.ts`, `lib/streaming/fleet-stream-frames.tools.test.ts`, `lib/streaming/fleet-stream-frames.ts`, `lib/streaming/fleet-stream-registry.facts.test.ts`, `lib/streaming/fleet-stream-registry.test.ts`, `lib/streaming/fleet-stream-registry.ts`, `lib/streaming/install-steps.ts`, `lib/streaming/workspace-stream.test.ts`, `lib/streaming/workspace-stream.ts`, `tests/approvals-resolve-buttons.test.ts`, `tests/connectors-api-client.test.ts`, `tests/dashboard-fleets-wall.test.tsx`, `tests/install-steps.test.ts`, `tests/integrations-connectors.test.ts`, `tests/integrations-page.test.ts`, `tests/stream-proxy-routing.test.ts`, `tests/use-fleet-event-stream.test.ts` | EDIT | importers of the moved symbols: the import path changes, nothing else |
| `tests/helpers/fetch-failed.ts` | CREATE | the one helper the three app suites share for Node's fetch failure shape |
| `tests/e2e/acceptance/fixtures/retry-probe.ts` | CREATE | drives the transport against the scripted origin in a `bun run` child process, so the origin the transport captures at module load is the spec's |
| `lib/api/client.retry.test.ts`, `lib/api/client.defaults.test.ts`, `lib/api/approvals.resolve.test.ts`, `lib/errors.test.ts` | EDIT | read the status facts from `lib/api/errors`; `client.retry.test.ts` also proves the terminal status through the transport and the bound option signal; `client.defaults.test.ts` proves the default timeout is no longer than the deadline's remainder |
| `lib/api/client.test.ts`, `tests/fleets-api-client.test.ts` | EDIT | three test doubles the old body reader masked: an unparseable body throws the `SyntaxError` a real `Response.json()` throws, not a plain `Error` the transport now rethrows as a broken stream; and the trace test hands each request its own `Response`, since a body reads once. `client.test.ts` also gains `a body that fails to read`: a failure that is neither a broken stream nor bad JSON reaches the caller as it was, unretried — the one branch of the reader nothing else exercised |
| `VERSION`, `build.zig.zon`, `cli/package.json`, `rustd/Cargo.toml`, `rustd/Cargo.lock` (repo root) | EDIT | version sync to 0.28.1: a user-visible fix |
| `docs/architecture/web_app.md` (repo root) | EDIT | one paragraph: every server fetch runs under the declared schedule, and the policy's dependency is server-only by build guard |

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

- **Reference:** Effect `Schedule` — `exponential`, `modifyDelay` (full jitter), `recurs`, `upTo`, composed with `Effect.retry`'s `while` option for the replay and `Retry-After` gates (Effect 4 has no `whileInput`). Aligned exactly; the loop's arithmetic is deleted, not ported.
- **Reference:** `cli/src/lib/http-retry.ts` — the behaviours that must survive: the transient status set, `Retry-After` as a floor, `AGENTSFLEET_NO_RETRY`, the 5xx replay gate. Divergence: the CLI is not rewritten; parity moves to a shared fixture.
- **Reference:** AWS Architecture Blog, "Exponential Backoff And Jitter" — full jitter over centred jitter for herd dispersion.

## Sections (implementation slices)

### §1 — The schedule replaces the loop

`runWithRetry` keeps its signature and runs the attempt under `Effect.retry` with a composed schedule. Defaults are the current ones where they exist and new where the loop had none. The deadline bounds the sleep the schedule chooses, not only the decision to retry: a wait that would end past it is never taken. **Implementation default:** total deadline 20 s; `Retry-After` cap 10 s, above which the request fails at once (the delay cap stays 2 s; a 429 that asks for 5 s, the case M190_001's suite proves, is still obeyed); full jitter in `[0, delay]`.

- **Dimension 1.1** — DONE — a GET answered 503 then 200 still resolves with the 200 body, with attempt count and terminal telemetry unchanged → Tests: the M190_001 suites `retry.test.ts` (loop cases), `client.retry.test.ts`, `client.defaults.test.ts`, `retry.integration.test.ts` green against the schedule
- **Dimension 1.2** — DONE — attempts stop when the total deadline is reached even though the attempt ceiling is not, and the last attempt's own timeout is bounded by what remains → Tests `a read gives up at the deadline, not at the attempt count`, `each attempt is told what remains of the deadline, so the last one cannot outrun it`, `the default timeout is no longer than what remains of the deadline`
- **Dimension 1.3** — DONE — a `Retry-After` above the cap ends the request with the 429 in hand, without sleeping → Tests `a Retry-After beyond the cap fails fast`, `a Retry-After within the cap is the sleep, exactly, and the cap is the caller's to raise`; acceptance `a Retry-After the policy will not honour fails the read at once`
- **Dimension 1.4** — DONE — the caller's aborted signal interrupts a sleep and prevents the next attempt, as today, through Effect interruption → Tests `an abort before the first attempt makes none`, `an abort during a seam sleep abandons the sleep and reports the attempt it followed`, `a cancel that lands while the attempt is answering does not lose the answer`, and M190_001's `a backoff in progress ends when the caller cancels, and no attempt follows`
- **Dimension 1.5** — DONE — jitter draws from `[0, delay]`, so two schedules seeded differently disperse → Tests `full jitter never returns the bare delay twice in a row`, `two schedules seeded differently disperse`
- **Dimension 1.6** — DONE — `effect` is not present in any client bundle → Build gate `assertServerOnlyModulesAbsent` in `.size-limit.mjs` (`bun run size`, after `next build` in CI) fails on the Effect runtime marker in any measured client chunk; the two client-side importers of the policy now read `lib/api/errors`

### §2 — Provenance decides replay, not the method alone

A failure carries where it happened. Pre-send failures (`ECONNREFUSED`, `ENOTFOUND`, `EAI_AGAIN`, a connect timeout) may replay any method. Post-send failures (`ECONNRESET`, `EPIPE`, a headers or body timeout, any 5xx) replay only idempotent methods. Codes are read from the error's `cause`, never from its message.

- **Dimension 2.1** — DONE — a POST whose connection was refused is replayed; a POST whose socket reset after sending is not → Tests `a write replays only when it provably never left` (policy), `a POST refused at connect is sent again: Node's cause code proves it never left` and `a POST whose socket reset after sending is sent once; a GET is read again` (real socket), CLI `a write reset after sending is sent once; one refused at connect never left and is sent again`, acceptance `a write whose socket reset after sending is sent exactly once`
- **Dimension 2.2** — DONE — a browser-shaped `TypeError` with no `cause` is classified network, post-send → Test `a message without a cause is treated as possibly sent`; CLI `a network failure with no cause is read as sent: a read retries, a write does not`
- **Dimension 2.3** — DONE — 408 and 425 are their own class, no longer labelled `5xx` → Tests `every status maps to its own class`, `a 408 names its own reason` (the `onRetry` reason table)

### §3 — Telemetry says what happened

- **Dimension 3.1** — DONE — the terminal `onAttempt` carries the real success status (200, 201, 204), and `onRetry` carries the delay chosen → Test `telemetry reports the status and delay that occurred`; a hook that throws surfaces its own error
- **Dimension 3.2** — DONE — invalid schedule inputs (negative or non-finite delays) are refused at configuration with `CONFIG_INVALID` → Test `a NaN delay never becomes a hot loop` (each of the four delay options)

### §4 — One fixture, two runtimes

- **Dimension 4.1** — DONE — `samples/fixtures/retry-policy/cases.json` enumerates scripted responses and the attempt count each must make; both the dashboard suite and the CLI suite replay it → Test `both runtimes agree on every fixture case` in `retry.fixture.test.ts` (vitest) and `http-retry.fixture.test.ts` (bun test), fourteen cases each

## Interfaces

```text
runWithRetry<T>(attempt: (remainingMs: number) => Promise<T>, method: string, options?: RetryOptions)   (the budget left of deadlineMs when the attempt starts; a thunk may ignore it)
RetryOptions gains: deadlineMs?, retryAfterCapMs?, statusOf?   (defaults declared once, retry-config.ts)
RetryInfo gains: delayMs                                        (the sleep the schedule chose)
RETRY_DEFAULTS gains: deadlineMs, retryAfterCapMs
classifyFailure(err, sent: boolean) → ClassifiedFailure { kind, provenance, status, retryAfterMs, cause }   (retry-classify.ts)
lib/api/errors.ts declares HTTP_STATUS_REQUEST_TIMEOUT, RETRY_CODE_TIMEOUT, isDefiniteRefusal; importers read them there (no re-export)
client.ts: attemptTimeoutMs(remainingMs) = min(DEFAULT_REQUEST_TIMEOUT_MS, remainingMs) — the default per-attempt timeout, bounded by the deadline's remainder
```

No HTTP endpoint, no caller change: `request`, `requestWithEtag`, `requestWithRetry` keep their signatures.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Intermediary asks for a long wait | `Retry-After` above the cap | request fails at once with the 429; no sleep (1.3) |
| Backend hangs repeatedly | every attempt times out | the deadline ends the schedule before the attempt ceiling (1.2) |
| Slow read blocks the tab's action queue | a Server Action's read retries for longer than a person waits | the deadline bounds the action, so an approve or a steer queued behind it is held for at most `deadlineMs` (1.2) |
| CLI replays a timed-out write | `http-retry.ts` treats its 15 s `TIMEOUT` as replayable for every method | the shared fixture (4.1) carries the case and the CLI gate is closed to match the dashboard's |
| Write timed out server-side | POST post-send failure | not replayed; error surfaces; the optimistic row rolls back (2.1) |
| Backend refused the connection | pre-send | any method replays under the schedule (2.1) |
| Caller navigated away | signal aborted mid-sleep | interruption; `RequestCancelledError` semantics as today (1.4) |
| Caller navigated away as the answer arrived | signal aborted mid-attempt | the attempt settles: an answer is returned, a failure surfaces as it threw, and no attempt follows (1.4) |
| Herd after a blip | many renders retry together | full jitter disperses them (1.5) |
| Dependency leaks to the browser | a client module imports the policy | 1.6 fails the build's size gate |
| Config mistake | NaN or negative delay | `CONFIG_INVALID` at configuration (3.2) |

## Invariants

1. A non-idempotent method is replayed only on a pre-send failure — enforced by the classifier's provenance field and test 2.1.
2. No retry, and no sleep before one, begins past `deadlineMs` — a `Schedule.while` over the delay chosen; tests 1.2 and `a sleep that would end past the deadline is not taken`.
3. No sleep exceeds `retryAfterCapMs` — the gate before the schedule; test 1.3.
4. `effect` never reaches a client bundle — test 1.6 reads the build's client manifest.
5. The public seam is unchanged — the existing M190_001 suites keep every case; the only edits are the removal of the two describe blocks that pinned the deleted helpers and the pins of the deleted jitter arithmetic (1.1).
6. A sleep is the run's only interruptible moment — `Effect.uninterruptibleMask` with the clock's sleep restored; an abort mid-attempt surfaces what the attempt threw, or returns what it answered (1.4).

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
| 1.6 | e2e (build) | `assertServerOnlyModulesAbsent` (`.size-limit.mjs`) | after `next build`, every measured client chunk is read; `effect/Effect/Yield` present → the size gate fails |
| 2.1 | integration | `a POST refused at connect is sent again`, `a POST whose socket reset after sending is sent once; a GET is read again` | POST to a bound-then-released loopback port → 2 attempts, reason `network`; POST whose socket the server destroys → 1 attempt; the same GET → 2 |
| 2.2 | unit | `a message without a cause is treated as possibly sent` | `TypeError("Failed to fetch")` → network, post-send |
| 2.3 | unit | `every status maps to its own class` | 408 → timeout class, 425 → early, 429 → rate, 502/503/504 → server |
| 3.1 | unit | `telemetry reports the status and delay that occurred` | 204 success → status 204; retry → `delayMs` equals the sleep |
| 3.2 | unit | `a NaN delay never becomes a hot loop` | `baseDelayMs: NaN` → `CONFIG_INVALID`, zero attempts |
| 4.1 | unit (both runtimes) | `both runtimes agree on every fixture case` | each fixture case → same attempt count and replay decision in vitest and in the CLI suite |
| regression | integration | M190_001's `retry.integration.test.ts` | unchanged cases green |
| 1.3, 2.1 | acceptance (CLI, deterministic lane) | `retry policy over a real socket` | the built binary: 503 then 200 → exit 0 and two GETs; socket reset then 200 → exit 0 and two GETs; POST socket reset → non-zero exit, exactly one `POST /v1/api-keys`, no stack frames |
| 1.2 | unit | `a sleep that would end past the deadline is not taken`, `the cap clamps exponential growth` | 429 with `Retry-After` equal to the deadline at t=400 ms → one attempt, no sleep; base 200, cap 300, four attempts → delays `[200, 300, 300]` |
| 2.1 | unit (CLI) | `Bun's shape, the code on the error itself, is read the same way`, `a Retry-After beyond the cap fails at once, with the answer in hand` | `ConnectionRefused` on the error → POST sent again; `ECONNRESET` on the error → once; `Retry-After: 120` → one attempt, the 429 in hand |
| 1.6 | unit | `the server-only module guard` (`tests/server-only-modules.test.ts`) | the installed runtime carries the marker; a runtime without it, or absent, is stale; a chunk carrying it is named; clean chunks pass |
| 1.3, 2.1, 4.1 | acceptance (Playwright, journeys project) | `the dashboard's transport over a real socket`, `the command line makes the same decisions` | one scripted origin: the dashboard read recovers in two round-trips; `Retry-After: 120` rejects with 429 in under 2 s after one request; a POST reset after sending is sent once even with `maxAttempts: 3`; the CLI lists after a 503 and sends a reset write once |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The loop is gone (§1) | `grep -c "for (;;)" ui/packages/app/lib/api/retry.ts` | 0 | P0 | |
| R2 | No message sniffing (§2) | `grep -c "fetch failed" ui/packages/app/lib/api/retry.ts ui/packages/app/lib/api/retry-classify.ts` | each 0 | P0 | |
| R3 | Deadline and cap declared once (§1) | `grep -c "deadlineMs\|retryAfterCapMs" ui/packages/app/lib/api/retry.ts ui/packages/app/lib/api/retry-config.ts` | 4 or more each | P0 | |
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

`lib/api/retry-backoff.ts` and `lib/api/retry-backoff.test.ts` — the schedule owns delay and sleep. Grep: `git ls-files ui/packages/app/lib/api | grep -c retry-backoff` → 0. `retry.ts` itself is rewritten in place.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `backoffDelay` (replaced by the schedule) | `grep -rn "backoffDelay" ui/packages/app/lib` | 0 matches outside a fixture replay |
| `classifyRetryable` (replaced by `classifyFailure`) | `grep -rn "classifyRetryable" ui/packages/app` | 0 matches |
| `sleepUnlessAborted`, `defaultSleep` (the schedule sleeps under `Clock`) | `grep -rn "sleepUnlessAborted\|defaultSleep" ui/packages/app/lib` | 0 matches |

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
- **Consults** — Sep 06, 2026: CHORE(open) found the Failure Modes row "CLI replays a timed-out write" closing the CLI gate while Files Changed listed only the CLI test file; the `cli/src/lib/http-retry.ts` EDIT row was added (the gate closes, the loop is not rewritten). Indy: "Yes yes go and implement chore(open)".
- **Consults** — Sep 06, 2026: the same CHORE(open) read corrected three paths against the tree (the CLI suites live in `cli/test/`, `samples/fixtures/` is at the repo root, and `retry-backoff.ts` is a separate module the sweep must delete) and the Effect names against the installed `4.0.0-rc.112` (`Effect.retry({ while })` and `modifyDelay` replace the 3.x `whileInput` and `jittered`). Indy: "keep going in parallel".
- **Consults** — Sep 06, 2026: the unit lane had reported its cargo half and the integration lane was migrating when Indy stopped both; the baseline records the measured cargo count and the remaining counts land at the boundary. Indy: "just stop all of them and continue chore(open) do the make test-unit*, lint* test-integration* prior to PR".
- **Consults** — Sep 06, 2026: Indy set the verification bar for EXECUTE: "What ever you build now ensure the coverage for rust is > 97%, ts is 100% any new tests in acceptance-e2e (cli and playwright are added) to demonstrate the working has indeed worked as you had quoted in the spec." No Rust changes here; the app and CLI coverage floors are 100% by gate; the two acceptance specs above are the demonstration. The dashboard fetches server-side, and the live acceptance backend cannot be made to answer 503 on demand, so the Playwright spec drives the transport in its worker against a scripted loopback origin and the CLI binary against the same origin.
- **Decisions at EXECUTE** — the `Retry-After` cap defaults to 10 s, not the 2 s delay cap the draft named: M190_001's `honors Retry-After header on 429` obeys a 5 s ask, and a rate limiter asking for five seconds is not an intermediary asking for two minutes. Full jitter can draw 0, so a test that lands an abort inside a backoff pins `randomFn` to 1. `SecretsList.tsx` (a client component) and `lib/errors.ts` (imported by nineteen client components) imported the policy for a status predicate and a code; both would have carried `effect` into the browser, so those facts moved beside `ApiError`. An abort mid-attempt lets the attempt settle (`Effect.uninterruptibleMask` with only the sleep restored): the loop surfaced the attempt's own outcome and so does the schedule.
- **Metrics review** — no events added; no analytics or funnel playbook update required.
- **Skill-chain outcomes** — REVIEW (gstack `/review`, Sep 06, 2026): testing, maintainability, performance and security specialists, a Claude adversarial pass and a red team over the diff; Codex passes disabled in this repository's config. Fixed from their findings: the deadline gated the decision but not the sleep it chose (a `Retry-After` could end past it) — now a `Schedule.while` over the chosen delay; the CLI read socket codes only from the cause and missed Bun's shape — now both, with `ConnectionRefused`; a mid-body network failure was swallowed as `{ detail }` — now rethrown for the policy; `requestWithRetry`'s option signal was not bound to the fetch — now one signal; a rejecting seam sleep hung the run — now a defect; the CLI had no `Retry-After` cap — now the dashboard's 10 s; the compat re-export of the status facts from `retry.ts`; two casts; duplicated test helpers and a duplicated describe; the guard's marker had no positive control and no test; the barrel `effect` import (a 1.9 s cold import measured against 0.5 s for the seven submodules). Accepted as is, with the reason: `RetryReason` carries `"fatal"` so the reason table is total (never observed on `onRetry`); a throwing telemetry hook surfaces its own error, as the loop did; every fetch `TypeError` is network, which is Dimension 2.2; PUT stays replay-safe by default, a pre-existing decision; the CLI's body opt-out (`retry_after_seconds: 0`) has no dashboard twin and DELETE differs only in `request()`'s default set, not in either policy.
- **Consults** — Sep 06, 2026 (D1): security's build found `retry.ts` and the Effect runtime in the shared client chunk (118 KB, 39 KB gzipped) through the seven domain modules that import the transport and whose constants ~18 client components read. Asked whether to split those constants out, lazy-load the policy, or defer; Indy chose the split. Sep 06, 2026 (D2): the disk reached 344 MiB free and the Rust unit build died on it; asked which caches to reclaim; Indy chose deleting this worktree's partial `rustd/target` and the reviewer's `.next`, and running the Rust lanes with `CARGO_TARGET_DIR` pointed at the main checkout's cache.
- **Consults** — Sep 06, 2026 (after D1): the split ran in three waves. The first moved the seven modules the security build named; the second, over approvals, connectors, events and workspaces, overwrote the first wave's `runners-types.ts` and `api-keys-types.ts` and was repaired by re-appending those blocks; the third moved the runner policy vocabulary (`NETWORK_POLICIES`, the worker-count bounds, `parseRegistryAllowlist`, `AssignedPolicy`, `RUNNER_ADMIN_ACTION`) that `PolicyFields.tsx` and `RunnerListCells.tsx` still read through the transport. The client-side trace then reaches the transport from zero files and the size guard's Effect scan passes over every measured chunk. The red-team pass of the gstack review was still running when the session that started it ended; its findings never arrived, and the review is recorded above without them.
- **Consults** — Sep 06, 2026 (this session's D2): `make test-integration-rustd` failed three times on three different tests, every one a datastore acquisition timeout (`the api datastore did not answer within 250ms`, a 503 `Database unavailable`) while a sibling session on the same machine ran a workspace-wide clippy and headless Chromium (load average 25–38); each earlier failure passed on the next run, and the branch's Rust diff is the version bump alone. run 1: 31 passed, 1 failed (an_installation_this_deployment_never_mapped_is_dropped_not_refused); run 2: 24 passed, 1 failed (test_pool_error_classes); run 3: 23 passed, 2 failed (test_pool_error_classes, test_a_connection_that_dies_at_begin_reports_the_transaction). Asked whether to rerun after the unit lane, record the lane 🟠 now, or hold for a quiet machine.
  > Indy (2026-09-06 19:08): "Record 🟠 now and open the PR after the unit lane" — context: the integration lane is recorded 🟠 with this evidence in the PR's Make section; CI runs the lane on the PR and is the arbiter.
- **Post-push review** — Sep 06, 2026, greptile on PR #664, two findings, both user-decided. P1 `retry.ts`: the deadline gated the sleep but an attempt begun late kept the transport's full 10 s ceiling, so the worst case was about 30 s, not 20 s; asked whether to bound each attempt to the remaining budget, refuse attempts that cannot finish, or reword to a retry-start deadline.
  > Indy (2026-09-06 19:50): "Bound each attempt to the remaining budget (recommended)" — context: `runWithRetry` now tells the attempt the remaining budget and `client.ts` caps its default timeout to it.
  P2 `admin-model-library-types.ts`: `NANOS_PER_USD` and `OPENAI_COMPATIBLE_PROVIDER` were declared there and in `lib/types.ts` (a duplicate main already carried inside `admin_model_library.ts`; the split relocated it); asked whether to fold or leave.
  > Indy (2026-09-06 19:50): "Fold into lib/types.ts and repoint importers (recommended)" — context: one declaration each, in `lib/types.ts`; four importers repointed, no re-export.
- **Deferrals** — none at authoring.
