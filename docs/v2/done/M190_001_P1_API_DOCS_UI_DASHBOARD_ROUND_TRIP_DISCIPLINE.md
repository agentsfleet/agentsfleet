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

# M190_001: The daemon brackets every run on the live tail, the chat strip is a view over it, and dashboard writes paint before the server answers

**Prototype:** v2.0.0
**Milestone:** M190
**Workstream:** 001
**Date:** Sep 05, 2026
**Status:** DONE
**Priority:** P1 — operator-facing latency and resilience on every dashboard write and on every watched chat; a watched chat never learned a run had finished, and everything else was slower and more brittle than the architecture doc already prescribes.
**Categories:** API, DOCS, UI
**Batch:** B11 — independent of the v2 cutover sequence; touches `ui/packages/app`, the daemon's lease, gate and inbox verbs, one additive field on the fleet detail, and three architecture docs.
**Branch:** feat/m190-dashboard-round-trips
**Test Baseline:** unit=5448 (`make test-unit-all` on main at fa989b444: cargo 2351 · app 2410 · website 175 · other packages 512) integration=recorded before the PR (Indy, Sep 05, 2026 — see Discovery)
**Test Delta:** unit=5547 (+99 against the baseline: cargo 2354 (+3) · app 2506 (+96) · website 175 · other packages 512) integration=374 (369 at the first pre-PR run, +5 — the closing-count, approve-arm and the three tail suites' cases) — `make test-unit-all` and `make test-integration-rustd`, Sep 06, 2026
**Depends on:** none
**Provenance:** LLM-drafted (Claude Fable 5.1, Sep 05, 2026) from an in-session review of every `lib/api` call path, server page, mutation surface and stream registry in `ui/packages/app`, then re-planned the same day after the daemon's live tail was found to publish no bracket frame at all; Indy chose the scope from four offered batches and then directed the rework with three instructions recorded in Discovery.
**Canonical architecture:** `docs/architecture/web_app.md` §The five statements (statements 1, 4, 5) and §Scoreboard; `docs/architecture/runner_fleet.md` §Live activity (the SSE tail); `docs/architecture/data_flow.md` §D. WATCH

---

## Overview

**Goal (testable):** the daemon publishes `event_received` when a lease opens a run's row and `event_complete` — carrying the terminal row, the fleet's status and its pending-approval count — when a report or a gate refusal closes it, and `gate_opened` / `gate_resolved` with the count when a human is asked and answers; a watched chat's summary strip is a view over that stream and issues no read to move; the chat page opens on the fleet detail and its thread alone, the pending count riding the detail; every `request()` read retries transient failures and aborts on a default timeout; the runner detail and admin models pages issue their independent reads together; secrets delete, runner state changes and approval resolves paint their outcome before the server confirms; the approvals inbox poll asks for nothing while its tab is hidden.

**Problem:** `docs/architecture/runner_fleet.md` says the daemon publishes the bracket frames, and `rustd/crates/afd_fleet/src/lease/activity.rs` publishes only the runner's four mid-run frames — so in production a watched chat never sees a run finish, every "completion" path in the dashboard is dead, and the CLI's steer tail falls back to polling on every message. On top of that, an operator watching a chat paid a whole-page server re-render per completion; the first cut of this spec replaced that with three reads per completion per viewer, which is three reads too many when the daemon already holds the row at the moment it closes it. The chat page paged fifty full approval rows to render a count. A single transient 503 on almost any read blanked a surface. Two pages serialised reads that depend only on the URL. Every write except the kill switch and the composer waited for the round-trip. The inbox polled a hidden tab every five seconds.

**Solution summary:** the closing statements return the row they closed (`RETURNING` plus the fleet's status and pending count, one round trip) and the daemon announces it on the tail; the gate plane and the inbox announce a park, a decision and a sweep the same way. The stream registry folds the row's figures and the fleet facts into its snapshot, and `useFleetRunSummary` derives the strip from that snapshot — the summary Server Action, its debounce hook and the per-render approvals read are deleted. `GET …/fleets/{id}` gains `pending_approvals`. `request()` and `requestWithEtag()` run through the retry policy by default with a default timeout; two pages start their view reads beside their primary reads; four surfaces adopt the `KillSwitch` shape; the inbox poll pauses while hidden. The operator sees the strip and the thread move the instant a run ends, with nothing on the wire but the frame that said so.

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(ui,api): the daemon brackets every run on the live tail, and the chat reads nothing to follow it
- **Intent (one sentence):** a watched chat learns a run ended from the frame that ended it — row, status and count included — and dashboard writes feel instant and dashboard reads survive a blip, without a client cache, a redundant read, or a credential in the browser.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_fleet/src/lease/activity.rs` — the runner's four frames and the best-effort publish shape §1 copies; the bracket module beside it is the daemon's own.
2. `rustd/crates/afd_events/src/sql.rs` — the two closing statements; §1 makes each return the row it closed, with the fleet facts joined in.
3. `ui/packages/app/lib/streaming/fleet-stream-registry.ts` — the one client store; §1 gives it the fleet facts and the newest figures, and the strip subscribes to it.
4. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/KillSwitch.tsx` — the one optimistic mutation in the package; §4 copies this shape verbatim.
5. `ui/packages/app/lib/api/retry.ts` — the policy §2 makes the default. `isIdempotentMethod` and `classifyRetryable` already refuse to replay a POST or PATCH on a genuine 5xx; do not re-derive that gate.
6. `docs/architecture/web_app.md` — statements 1, 4 and 5 are the constraints; the scoreboard is re-measured in this diff.

## Files Changed (blast radius)

Rust paths below `rustd/crates/`; app paths below `ui/packages/app/`; the rest as written.

| File | Action | Why |
|------|--------|-----|
| `afd_wire/src/tail.rs`, `afd_wire/src/lib.rs` | CREATE, EDIT | `TailFrame`: the four daemon-authored frames, `kind` leading; the completion flattens `TailRow` — `EventSummary` less the two scope columns the channel names, so the workspace multiplex's spliced `fleet_id` is the frame's one; `gate_resolved.event_id` is optional for a gate that held no run |
| `afd_wire/src/fleet.rs` | EDIT | `FleetDetailResponse.pending_approvals` |
| `afd_redis/src/streams.rs`, `afd_redis/src/streams/tail.rs` | EDIT, CREATE | `FleetStreams::publish_tail` formats the fleet's activity channel once for every publisher, the runner's forwarded frames included; `publish_frame` serializes, publishes and logs a dropped daemon frame once for the three publishers; the publish trio lives beside the stream verbs under the length cap |
| `afd_events/src/sql.rs`, `afd_events/src/closed.rs`, `afd_events/src/lib.rs`, `afd_events/src/history/row.rs` | EDIT, CREATE, EDIT, EDIT | both closing statements `RETURNING` the row plus `fleet_status` and `pending_approvals`; `Closed` decodes it; `EventRow::summary()` is the one row→wire mapping; `shared_columns!` takes the CTE alias so the closing select is the history read's own column list; `EventRow::COLUMNS` names where the closing's two extra columns begin; `history/statement.rs` and `history/mod.rs` expose the macro crate-wide |
| `afd_fleet/src/lease/bracket.rs`, `afd_fleet/src/lease/mod.rs`, `afd_fleet/src/lease/activity.rs` | CREATE, EDIT, EDIT | `publish_received` / `publish_completion` over `publish_frame`; the runner's forwarded frames go through `publish_tail` so the channel has one name |
| `afd_fleet/src/lease/event.rs`, `afd_fleet/src/lease/finalize.rs`, `afd_fleet/src/lease/pull.rs`, `afd_fleet/src/lease/report.rs`, `afd_fleet/src/lease/report/steps.rs`, `afd_fleet/src/lease/report/tests.rs` | EDIT, EDIT, EDIT, EDIT, CREATE, CREATE | `block` and `mark_terminal` answer the closing; the lease verb announces the opening and a refusal's closing, the report verb the settled closing; the report's five post-settle steps and its tests move beside it under the length cap |
| `afd_fleet/src/error/{mod,lift,classify,permanence,tests}.rs` | EDIT | `ErrorKind::Events` lifts the event store's decode failures with their cause |
| `afd_fleet/tests/integration_lease_block.rs`, `afd_fleet/tests/integration_activity_publish.rs` | EDIT | `Ended::Now` carries the closing; the closing's count is proven off zero (one pending gate, one answered) and its publish proven harmless with the queue down |
| `afd_gate/src/gate/park.rs`, `afd_gate/src/gate/sql.rs` | EDIT | a park announces `gate_opened` with the count; `INSERT_GATE` selects the count beside its insert on one snapshot (the raised row unioned in), so the park costs no second statement |
| `afd_gate/tests/integration_gate_lifecycle.rs`, `afd_gate/tests/integration_gate_tail.rs`, `afd_gate/tests/support/gate_fixture.rs` | EDIT, CREATE, CREATE | the fixture moves to support so the tail suite shares it; the park announcement is proven against the production subscriber |
| `afd_approval/src/inbox.rs`, `afd_approval/src/inbox/announce.rs`, `afd_approval/src/inbox/sweep.rs`, `afd_approval/src/inbox/row.rs`, `afd_approval/src/sql.rs`, `afd_approval/Cargo.toml` | EDIT, CREATE, CREATE, EDIT, EDIT, EDIT | a decision and a sweep announce `gate_resolved` with the count, which `RESOLVE_GATE` and `EXPIRE_GATES` select beside their writes on one snapshot; the announcement runs whatever the continuation did; the resolve announces the continuation row it wrote as `event_received`; `Resolved.event_id` is optional because the column is; the sweep decodes every row before it announces and never fails after its commit |
| `afd_approval/tests/integration_inbox_tail.rs`, `afd_approval/tests/approval_suite.rs`, `afd_approval/tests/support/gate_lane.rs`, `afd_approval/tests/integration_inbox.rs`, `afd_approval/tests/integration_inbox_continuation.rs` | CREATE, EDIT, EDIT, EDIT, CREATE | the decision, approval-with-continuation, re-raised-action and sweep announcements proven on the fleet's channel (`seed_gate_for` seeds a second row for one action); the inbox suite reads the optional event and splits at the continuation, which was over the cap on `main` |
| `afd_fleet_lifecycle/src/sql.rs`, `afd_fleet_lifecycle/src/read.rs`, `afd_fleet_lifecycle/Cargo.toml` | EDIT | the detail read counts pending gates in the same statement |
| `afd_api_tenant/src/handler/fleet/detail.rs`, `afd_api_tenant/src/handler/event/mod.rs`, `afd_api_tenant/src/handler/stream.rs` | EDIT | the field on the wire; the listing uses the row's own `summary()`; both stream descriptions name the frame kinds and the completion's shape |
| `agentsfleetd/tests/integration_runner_brackets.rs`, `agentsfleetd/tests/support/e2e_tail.rs`, `agentsfleetd/tests/integration_runner_activity.rs`, `agentsfleetd/tests/daemon_suite.rs` | CREATE, CREATE, EDIT, EDIT | both brackets proven over HTTP against the production subscriber: a lease opens, a report closes, a refusal closes; the tail helpers move to support; the three tail timings are named constants |
| `public/openapi.json` | EDIT | regenerated: `pending_approvals` on the fleet detail; the two stream descriptions |
| `rustd/Cargo.lock` | EDIT | the two dependency edges above |
| `lib/api/events.ts` | EDIT | `GATE_OPENED`, `GATE_RESOLVED`; the bracket frame shapes, typed partial past the identifier because the wire is untrusted; the completion omits the scope columns; `gate_resolved.event_id` is nullable |
| `lib/events/run-summary.ts` (+ `.test.ts`) | CREATE | `FleetRunSummary`, `RunFigures`, `FleetFacts`, `buildRunSummary`, `latestFigures`, `sameFigures` — one module the server page, the registry and the client leaf share; moved here from `fleets/[id]/` because three layers consume it |
| `lib/streaming/fleet-stream-row.ts` | CREATE | the row model, `rowToEvent`, and the two wire readers `figure()` and `text()`, split from the reducers so the reducers, the merge and the facts build on it without a cycle; `FleetEvent` gains its figures |
| `lib/streaming/fleet-stream-frames.ts` (+ `.figures.test.ts`, `.live.test.ts`, `.tools.test.ts`; `.test.ts` keeps the merge), `tests/helpers/fleet-stream-fixtures.ts` | EDIT, CREATE, CREATE, CREATE, EDIT, CREATE | the opening bracket stamps the row's instant and type, and re-stamps a row the browser already held; the completion folds the row's figures and instant in, opens a row the timeline never saw, and reads a malformed cause or status as absent; the row-model re-export is gone; the 476-line suite splits by concern over one shared fixture |
| `lib/streaming/fleet-stream-facts.ts` (+ `.test.ts`) | CREATE | what a frame says about the fleet; a status outside `AGENTSFLEET_STATUS` reads as unknown |
| `lib/streaming/fleet-stream-optimistic.ts` | CREATE | the optimistic reducers, moved out of the registry under the length cap |
| `lib/streaming/fleet-stream-entry.ts`, `lib/streaming/fleet-stream-registry.ts` (+ `.facts.test.ts`), `lib/streaming/fleet-stream-backfill.ts` | EDIT, EDIT, CREATE, EDIT | the snapshot carries `fleet` facts, `factsSeq` and `latest` figures, kept by identity when unchanged; a completion's facts fold into the same write as its row; a server render never moves `factsSeq`, which the hook reads to tell a render older than a frame; the entry-level backfill walk moves beside the walk it wraps |
| `components/domain/useFleetRunSummary.ts` (+ `.test.tsx`) | CREATE | the strip as three identity-stable selectors over the registry (facts, figures, facts sequence), server figures as the floor; owns the one refresh, which only a frame spoken after the server's facts landed can owe, and the guard against a render older than a frame; the test counts renders across a streaming reply |
| `components/domain/useRefreshOnCompletion.ts`, `tests/use-refresh-on-completion.test.ts` | DELETE | the debounce hook and its test: nothing is coalesced when nothing is read |
| `tests/fleet-run-summary-action.test.ts` | DELETE | the summary action is gone |
| `app/(dashboard)/w/[workspaceId]/fleets/actions.ts` | EDIT | `getFleetRunSummaryAction` and its three reads leave |
| `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/ChatView.tsx` (+ `.test.tsx`) | CREATE | the client leaf: strip over thread, one subscription, no effect of its own; the suite spies `fetch` and proves a revisit refreshes nothing |
| `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/RunMetricsStrip.tsx` (+ `.test.tsx`), `console-copy.ts` | EDIT | takes `RunFigures`; the exact count replaces the page length and its "+"; the unavailable-approvals copy leaves |
| `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/view-data.ts` (+ `.test.ts`), `page.tsx` | EDIT | the chat starts the thread read only; the count comes off the detail |
| `app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetHeader.tsx` | CREATE | the breadcrumb and lifecycle-control row, extracted from the page |
| `components/domain/FleetThread.tsx`, `tests/fleet-thread/` (`harness.ts` + seven `.test.ts`), `tests/fleet-thread-dynamic.test.ts`, `tests/fleet-thread.test.ts` | EDIT, CREATE, EDIT, DELETE | the completion callback leaves the thread; the 1751-line suite split by concern |
| `lib/types.ts`, `lib/api/fleets.test.ts`, `SkillEditor.test.tsx`, `tests/fleets-routes/harness.ts` | EDIT | `FleetDetail.pending_approvals` and every fixture that builds one |
| `tests/fleets-routes/` (`harness.ts` + six `.test.ts`), `tests/fleets-routes.test.ts` | CREATE, DELETE | the 1284-line suite split; the chat's route tests prove no inbox read and the exact count |
| `lib/api/client.ts`, `lib/api/retry.ts`, `lib/api/retry-backoff.ts`, `lib/api/errors.ts`, `lib/api/fleets.ts`, `lib/api/events.test.ts`, `lib/api/fleets.test.ts` | EDIT, EDIT, CREATE, EDIT, EDIT, EDIT, EDIT | §2: the single attempt becomes internal; the loop wraps it; the waiting half splits out; path ids are encoded once; `isDefiniteRefusal` names the one status class under which a failed write is a settled no |
| `lib/api/approvals.ts`, `lib/api/approvals.test.ts`, `lib/api/approvals.resolve.test.ts` | EDIT, EDIT, CREATE | the resolve's raw fetch gains the default timeout and the transport's classification (Invariant 2); `APPROVALS_PAGE_LIMIT` is the inbox page size at its three sites |
| `lib/errors.ts`, `lib/errors.test.ts` | EDIT | operator copy for the `TIMEOUT` class |
| `lib/api/retry.test.ts`, `lib/api/retry.integration.test.ts`, `lib/api/client.retry.test.ts`, `lib/api/client.defaults.test.ts`, `lib/api/retry-backoff.test.ts`, `lib/events/event-summary.test.ts`, `tests/approvals-pages.test.ts`, `vitest.setup.ts` | EDIT, EDIT, CREATE, CREATE, CREATE, EDIT, EDIT, EDIT | §2's proofs; the branches the coverage gate named (a cancel with a non-Error reason, the policy's own defaults, a signal already aborted, a completion with no cause, the inbox page's mock) |
| `app/(dashboard)/admin/runners/[runnerId]/page.tsx`, `app/(dashboard)/admin/models/page.tsx`, `tests/admin-models-page.test.ts`, `tests/runner-detail-page/` (`harness.ts`, `guards.test.ts`, `views.test.ts`), `tests/runner-detail-page.test.ts` | EDIT, EDIT, EDIT, CREATE, DELETE | §3 |
| `app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.tsx` (+ `.test.tsx`), `tests/secrets-list.test.ts` | EDIT | §4: optimistic row removal; the confirm holds until the write settles; a refusal the server made restores the row with no read, and only an unknown outcome re-reads |
| `app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.tsx` (+ `.test.tsx`), `RunnerHeader.actions.test.tsx`, `RunnerIdentityLine.tsx`, `app/(dashboard)/admin/runners/components/RunnerDialogs.tsx` | EDIT, CREATE, CREATE, EDIT | §4: optimistic admin-state badge; the dialog's `onConfirm` may be async |
| `app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList.tsx`, `app/(dashboard)/w/[workspaceId]/approvals/page.tsx`, `tests/approvals-list/` (`harness.ts`, `rendering`, `resolve`, `pagination`, `polling` `.test.ts`), `tests/approvals-list.test.ts` | EDIT, EDIT, CREATE, DELETE | §4: the row leaves before the POST resolves; §5: a hidden tab asks for nothing |
| `app/(dashboard)/w/[workspaceId]/approvals/[gateId]/ResolveButtons.tsx`, `tests/approvals-resolve-buttons.test.ts` | EDIT | §4: drop the redundant refresh after the push |
| `docs/architecture/runner_fleet.md`, `docs/architecture/data_flow.md`, `docs/architecture/web_app.md` (repo root) | EDIT | the bracket and gate frame payloads, the scope rule and where the counts are read; the console's two reads and its stream-driven strip; the scoreboard re-measured |
| `components/domain/useFleetEventStream.ts`, `components/domain/useWorkspaceStream.ts`, `components/domain/fleetMessageRenderers.tsx`, `components/domain/FleetToolCalls.tsx`, `components/domain/useFleetThreadEntries.ts`, `components/domain/fleetFailureCopy.tsx` (+ `.test.ts`), `components/domain/fleetMessageReaders.ts`, `lib/streaming/fleet-stream-cap.ts`, `lib/streaming/fleet-stream-optimistic.ts`, `lib/streaming/fleet-stream-frames.test.ts`, `lib/events/event-grouping.ts` (+ `.test.ts`), `lib/events/run-summary.test.ts` | EDIT | import path only: the row model is imported from `fleet-stream-row`, where it lives, rather than through a re-export the rules forbid |
| `tests/web-app-scoreboard.test.ts` | CREATE | pins the scoreboard's `useOptimistic` row to the grep it describes |
| `docs/v2/pending/M191_001_P2_UI_RETRY_POLICY_ON_EFFECT.md` (repo root) | EDIT | what this branch learned and left for the policy rewrite: the action-queue hold, the CLI's replay gate |

A changelog `<Update>` lands in `~/Projects/docs/changelog.mdx` on its own branch at CHORE(close), per `dispatch/lifecycle.md`; it is a cross-repo write and not a row here.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (the timeout constant, every statement's pending-status bind, every frame kind and every copy string are named once; `10_000` never appears bare; SQL text lives in each crate's `sql.rs`), ECL (a timeout, a 429 and a 404 are three classes; a null figure and an absent figure are both "unknown" and never zero), NDC (the summary action, its debounce hook and the unavailable-approvals copy leave; no compatibility alias for any of them), NLR (files opened here shed any dead branch they carry), TCF (every new test is made red before it is trusted), TST-NAM and TNM, HLP (no exported helper without a consumer), TFX (tests import the constants), ERR-RS (`ErrorKind::Events` composes with `#[source]`; no stringified cause).
- `dispatch/write_rust.md` — ownership across the closing's borrowed row; no `unsafe`; the best-effort publish never propagates; `Ended::Now` and the completion box their row so the enums stay the size of their other arms.
- `dispatch/write_ts_adhere_bun.md` — every app file is TypeScript; §11 timeout and cancellation applies to `client.ts` directly; no import cycle between the row model, the reducers and the registry.
- `dispatch/write_http.md` → `docs/REST_API_DESIGN_GUIDELINES.md` §3 — `pending_approvals` is an additive field on an existing resource, counted server-side, never client arithmetic; no new endpoint.
- `docs/architecture/web_app.md` — statement 1 (no client `fetch`; the strip subscribes, it does not load), statement 4 (`useOptimistic` on mutation surfaces), statement 5 (`useEffect` for the subscription and the server-render reconcile only).
- `docs/architecture/runner_fleet.md` §Live activity — the tail is best-effort and never the source of truth; the durable row is written before the frame that names it.
- `dispatch/verify.md` — every done-claim is a rubric row; a package-scoped run never satisfies one.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| LENGTH (≤350 file / ≤50 fn / ≤70 method) | yes | the inbox's announcer, the report's tests, the row model, the optimistic reducers and the tail test helpers each moved to a sibling module; every file this diff grew sits under the cap except `lib/types.ts`, which was over it on `main` (rubric S7) |
| UFS | yes | `DEFAULT_REQUEST_TIMEOUT_MS`, `COUNT_GATES_IN_STATUS`, `FRAME_KIND`, the test instants — declared once; `make harness-verify` audits the staged scope |
| UI / DESIGN TOKEN | yes | `ChatView` composes existing primitives; no raw HTML, no arbitrary values |
| MILESTONE-ID | yes | none in source or tests |
| LOGGING | yes | every dropped publish is one `tracing::debug!` with a scoped event name and the fleet; the app emits no console output |
| ERROR REGISTRY | no | no new `UZ-` code; `ErrorKind::Events` delegates its code to the event store's |
| SCHEMA GUARD / ZIG GATE | no | no schema, no Zig; the count rides an index the schema already carries |
| GREPTILE | yes | the rule IDs above, audited at CONFORM |

## Prior-Art / Reference Implementations

- **Reference:** `afd_fleet::lease::activity::Leases::publish_activity` — best-effort publish with a debug line per drop. §1's bracket module and both gate announcers align exactly.
- **Reference:** `afd_events::history::statement` — the `cost_nanos` correlated subselect. The closing statements reuse the same expression, for the same reason a join would duplicate the row.
- **Reference:** `useFleetEventStream` — `useSyncExternalStore` over the registry with the server rows reconciled from an effect. `useFleetRunSummary` is the same shape over the same store, reconciling the server's fleet facts.
- **Reference:** `KillSwitch.tsx` — optimistic paint plus reconciliation inside one transition. §4 aligns exactly; the only divergence is `ApprovalsList`, whose rows already live in client state.
- **Reference:** `view-data.ts` — start-beside-the-detail-read. §3 aligns exactly.
- **Reference:** `cli/src/lib/http-retry.ts` — the retry policy `retry.ts` mirrors; §2 changes which callers ride it, not the policy.

## Sections (implementation slices)

### §1 — The daemon brackets a run on the tail, and the strip is a view over the stream

The lease verb announces the row it opened (`event_received`: identifier, actor, event type, the row's own instant). A report's `mark_terminal` and a refusal's `block` run a closing statement that returns the row it closed joined to the fleet's status and its pending gate count, and the daemon announces that row as `event_complete`. A park announces `gate_opened`; a decision and a sweep announce `gate_resolved`; each carries the count. Every publish is best-effort and follows the durable write. On the client the registry folds a completion's figures onto the row and both fleet facts into its snapshot, a gate frame moves the count alone, a backfill's terminal rows fold in the same way, and the snapshot's `latest` is recomputed on every row change and kept by identity when unchanged. `useFleetRunSummary` subscribes to that snapshot with the server render's figures as the floor and pushes each server render's facts back into it, so the last writer — frame or render — wins on both sides. `ChatView` refreshes the server tree once when the fleet status the stream reports differs from the one the server rendered, because the header's lifecycle controls render there. The chat page reads the fleet detail and the thread; the count rides the detail. **Implementation default:** the pending count is an exact `COUNT(*)` over `(fleet_id, status)`, which the schema already indexes, so the strip shows the number rather than a page length with a "+".

- **Dimension 1.1** — DONE — a lease announces the row it opened and a report announces the row it closed, figures and fleet facts included → Rust integration `test_bracket_frames_open_and_close_a_run`
- **Dimension 1.2** — DONE — a lease refused at a gate closes the run on the tail with its label → Rust integration `test_a_refused_lease_closes_the_run_on_the_tail`
- **Dimension 1.3** — DONE — a park, a decision and a sweep announce the gate with the fleet's pending count; a queue that will not take the frame fails neither the decision nor the closing → Rust integration `a_parked_gate_is_announced_on_the_fleets_live_tail`, `a_decision_is_announced_on_the_fleets_live_tail`, `a_queue_that_will_not_take_the_frame_does_not_fail_the_decision`, `test_bracket_publish_redis_down_does_not_fail_the_closing`
- **Dimension 1.4** — DONE — every frame leads with its kind and the completion carries the whole row → Rust unit `should_lead_every_frame_with_its_kind`, `should_carry_the_whole_terminal_row_on_a_completion`
- **Dimension 1.5** — DONE — the strip shows the new run figures and count when the completion frame arrives, with nothing else on the wire → Test `the strip shows the new run figures when the completion frame arrives, with no read`
- **Dimension 1.6** — DONE — a gate frame moves the count and touches no row → Tests `a gate frame moves the pending count and touches nothing else`, `a gate frame moves the count and no row`
- **Dimension 1.7** — DONE — only a fleet status that differs from the server-rendered one refreshes the server tree, once → Test `only a status change refreshes the server-rendered controls, once`
- **Dimension 1.8** — DONE — a server render's facts win over an older frame's, and the next frame wins back → Tests `a server render resets the facts to its own — a kill from the header is not shadowed by an older frame`, `a server render's facts overwrite the tail's, and the next frame overwrites them back`
- **Dimension 1.9** — DONE — a reconnect backfill's terminal row moves the strip like a frame; a chunk of a streaming reply does not wake it → Tests `a reconnect backfill's terminal row moves the strip like a frame would`, `keep their identity across chunks of a streaming reply`
- **Dimension 1.10** — DONE — the detail read counts the fleet's pending gates and only those; the chat opens on the fleet detail and the thread, reads no inbox, and shows that count → Rust integration `the_detail_counts_the_fleets_pending_gates_and_only_those`; Tests `fleet Chat remains available when the thread read fails, and never reads the inbox`, `fleet summary links an exact pending count to the filtered Approvals inbox`
- **Dimension 1.11** — DONE — a thread read that failed leaves the strip unavailable until the stream delivers a row; an operator's message in flight is never the fleet's latest run → Tests `a thread read that failed leaves the strip unavailable until the stream delivers a row`, `skips the rows the browser made: an optimistic steer or a refused send`
- **Dimension 1.12** — DONE — an approval opens the continued run on the tail before it announces the answer, and the answer is announced whatever the continuation did; a completion for a row the timeline never opened opens it from the frame → Rust integration `an_approval_opens_the_continued_run_before_it_announces_the_answer`; Tests `opens the row from the frame, figures and all, so a late subscriber's strip still moves`, `drops a frame too thin to be a row rather than rendering a blank turn`
- **Dimension 1.13** — DONE — the chunks of a streaming reply re-render the strip zero times and its completion once; a gate frame restating the count re-renders nothing → Tests `the chunks of a reply re-render nothing; the completion re-renders once`, `a gate frame that restates the count re-renders nothing`
- **Dimension 1.14** — DONE — a revisit whose cached status the server has overtaken refreshes nothing; a render the strip asked for does not roll back a frame that landed during its round trip → Tests `a revisit whose cached status the server has overtaken does not refresh`, `a render asked for while a frame landed does not roll the frame back`
- **Dimension 1.15** — DONE — no daemon frame names its fleet or workspace, the workspace multiplex's spliced tag being the one; a runless gate's answer spells its event `null`; the closing's count is proven off zero → Rust unit `should_carry_the_whole_terminal_row_on_a_completion`, `should_spell_a_runless_gates_event_as_null`; Rust integration `test_a_closing_counts_the_fleets_pending_gates`

### §2 — Every read retries and every request times out

`request()` and `requestWithEtag()` become the retrying calls; the single attempt moves behind them. The policy is the one `requestWithRetry` already applies: transient statuses and network errors retry with the existing backoff, a genuine 5xx never replays a non-idempotent method, `AGENTSFLEET_NO_RETRY` still means one attempt. By default only GET, HEAD and PUT retry; DELETE, POST and PATCH keep one attempt unless the caller opts in through `requestWithRetry`, because a DELETE whose 204 was lost answers 404 on the replay and a POST that timed out may have been processed — the policy's replay gate refuses a non-idempotent method after a client timeout for that reason. A request whose caller passes no `signal` gets `AbortSignal.timeout(DEFAULT_REQUEST_TIMEOUT_MS)`; a caller-supplied signal is respected as is and, once aborted, ends the loop without another attempt or a held backoff. A timeout abort maps to the retryable `TIMEOUT` class, distinct from a caller cancel. The approvals resolve, which keeps its raw fetch so a 409 returns a body, carries the same timeout and classification (Invariant 2). **Implementation default:** the timeout is the same value the SSE backfill already uses, and is declared once.

- **Dimension 2.1** — DONE — a GET answered 503 then 200 resolves with the 200 body through `request()` with no options → Test `request retries a transient read and returns the recovered body`
- **Dimension 2.2** — DONE — a POST answered 503 throws without a second attempt → Test `request does not replay a non-idempotent write on a server error`
- **Dimension 2.3** — DONE — a read with no caller signal that never answers rejects with the `TIMEOUT` class after the policy's attempts → Test `a hung read times out into the retryable class and stops after the attempt ceiling`
- **Dimension 2.4** — DONE — a caller-supplied signal is passed through and the default timeout is not added → Test `a caller signal wins over the default timeout`
- **Dimension 2.5** — DONE — the three `requestWithRetry` callers make exactly the configured number of attempts, not that number squared → Test `an explicit retry caller never retries twice`
- **Dimension 2.6** — DONE — a caller abort still surfaces as `RequestCancelledError`, never as a retry → Test `a navigation abort is a cancel, not a retry`
- **Dimension 2.7** — DONE — a DELETE is not retried by default, and an aborted caller signal ends the loop without a second attempt → Tests `request does not retry a DELETE on its own`, `an already-cancelled caller gets no second attempt`
- **Dimension 2.8** — DONE — a POST that timed out client-side is never replayed; a server 408, 425 or 429 still is → Tests `a client-side timeout never replays a non-idempotent method, and still retries a read`, `does NOT replay a POST whose attempt timed out client-side (it may have been processed)`, `DOES retry a POST that returns 429 or 408 (request not processed)`
- **Dimension 2.9** — DONE — a hung approval resolve times out into the `TIMEOUT` class instead of holding the tab's action queue → Tests `a hung resolve aborts after the default timeout instead of pending forever`, `a timeout during the body read is a timeout, never an empty resolve`

### §3 — Independent reads start together

The runner detail page awaits the runner before starting the leases or activity read, though both need only the URL id; the admin models page reads the platform keys after the model list. Both pages start the second read beside the first and await it after, keeping every existing failure mapping.

- **Dimension 3.1** — DONE — the runner page issues the runner read and the view read before either resolves → Test `runner detail starts the view read beside the runner read`
- **Dimension 3.2** — DONE — the admin models page issues the model list and the platform keys read before either resolves → Test `admin models starts both reads together`
- **Dimension 3.3** — DONE — a 404 runner still renders not-found and a failed view read still renders its warning → Test `runner detail failure handling is unchanged by the parallel start`

### §4 — Writes paint before the server answers

Four surfaces adopt the `KillSwitch` shape. Secrets delete removes the row optimistically and refreshes inside the transition; the confirm dialog stays disabled until the write settles, and a failed delete refreshes to server truth because a timeout is an unknown outcome. Runner cordon, drain and revoke paint the target badge optimistically and refresh inside the transition; a 409 rolls back and the refresh shows the real state. The approvals inbox removes the row before the POST and restores it on a failed result, catching a rejected action call so a transport failure never reaches the error boundary; an already-resolved outcome keeps the row removed and shows the resolver. The approval detail page drops its second render after the push.

- **Dimension 4.1** — DONE — confirming a secret delete removes the row at once; a failed delete restores it with the error, and only a failure whose outcome is unknown re-reads the list → Tests `a secret row leaves on confirm and returns on failure`, `a refusal the server made restores the row without a read`, `delete failure surfaces errorMessage and keeps the dialog open`
- **Dimension 4.2** — DONE — a runner state action paints the target badge at once; a 409 restores the prior badge → Test `a runner badge paints the target state and rolls back on conflict`
- **Dimension 4.3** — DONE — approving from the inbox removes the row before the action resolves; a failed action restores it → Test `an inbox row leaves before the resolve settles and returns on failure`
- **Dimension 4.4** — DONE — an already-resolved outcome keeps the row removed and names the resolver → Test `an already resolved gate stays removed and shows who resolved it`
- **Dimension 4.5** — DONE — the approval detail resolve navigates once with no trailing refresh → Test `resolving from the detail page pushes once and does not refresh`
- **Dimension 4.6** — DONE — the architecture scoreboard reports the re-measured counts → Test `the scoreboard useOptimistic row equals the grep`
- **Dimension 4.7** — DONE — a confirm dialog stays disabled until the write settles, so a double confirm cannot send twice → Tests `the dialog holds its buttons disabled until the delete settles`, `should close the confirm and refresh when an admin action succeeds`

### §5 — A hidden inbox asks for nothing

The approvals inbox's poll skips its tick while the document is hidden and reads once when the tab is looked at again. One read is on the wire at a time whatever the backend's speed.

- **Dimension 5.1** — DONE — a hidden tab issues no poll; the first tick after it is visible catches the list up → Test `a hidden tab asks for nothing, and one read catches it up when it is looked at again`
- **Dimension 5.2** — DONE — a tick still in flight is not stacked by the next → Test `a tick still in flight is not stacked by the next`

## Interfaces

```text
afd_wire::tail::TailFrame (serialised with `kind` leading; published on fleet:{id}:activity;
                           no frame names its fleet or workspace — the channel does)
  event_received  { event_id, actor, event_type, created_at }
                  from the lease verb's first delivery, and from the resolve that writes a continuation
  event_complete  { ...TailRow, fleet_status, pending_approvals }
                  TailRow = EventSummary − { fleet_id, workspace_id }
  gate_opened     { gate_id, event_id, pending_approvals }
  gate_resolved   { gate_id, event_id: string | null, status, resolved_by, pending_approvals }

FleetStreams::publish_tail(fleet_id, payload)              afd_redis — every publisher, the runner's frames included
FleetStreams::publish_frame(fleet_id, &impl Serialize)     afd_redis — the daemon's frames: serialize, publish, log a drop
Leases::block(...) -> Ended::{Now(Box<Closed>), Already}   afd_fleet
Leases::mark_terminal(...) -> Option<Closed>               afd_fleet
Closed = { row: EventRow, fleet_status, pending_approvals } afd_events
EventRow::summary() -> EventSummary; EventRow::COLUMNS     afd_events
Gates::record_row(...) -> (Uuid7, pending)                 afd_gate — INSERT_GATE selects the count beside the insert
RESOLVE_GATE / EXPIRE_GATES                                afd_approval — each selects the count beside its write
Resolved.event_id: Option<String>                          afd_approval

GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}       gains `pending_approvals: i64`
GET …/fleets/{fleet_id}/events/stream, …/events/stream    descriptions name every frame kind and the completion's shape

FleetRunSummary = { status; latest: RunFigures | null; latestAvailable; pendingApprovals }
FleetStreamSnapshot gains { fleet: FleetFacts; factsSeq: number; latest: RunFigures | null }
reconcileServerFacts(fleetId, facts)                       the registry — never moves factsSeq
backfillEntry(entry, fleetId, walk)                        fleet-stream-backfill
figure(value) / text(value)                                fleet-stream-row — every wire number and string
useFleetRunSummary(workspaceId, fleetId, initial, initialSummary) → FleetRunSummary
                                                           owns the one router.refresh()
isDefiniteRefusal(status)                                  lib/api/retry
APPROVALS_PAGE_LIMIT                                       lib/api/approvals

request<T>(path, init, token)                 unchanged signature; retries per
                                              retry.ts and applies the default
                                              timeout when init.signal is absent
requestWithEtag<T>(path, init, token)         same
requestWithRetry<T>(path, init, token, opts)  unchanged; the only way to pass
                                              per-call retry options
FleetThread props lose: onRunCompleted
```

One additive field on an existing resource; no new endpoint. The four frame kinds are new wire vocabulary on an existing channel, mirrored in `ui/packages/app/lib/api/events.ts`; the CLI's steer tail (`cli/src/commands/fleet_steer_events.ts`) already switches on `event_complete`, reads `status` off the top level where the flattened row keeps it, and needs no change.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Bracket or gate frame not published | Redis would not take it, or the count could not be read | one `tracing::debug!` naming the fleet; the verb answers as if it landed; the row stands; the client's reconnect backfill carries the durable row (1.1, 1.3) |
| Closing statement matched no row | the event was already terminal (a redelivery whose acknowledgement was lost) | `Ended::Already` / `None`; nothing is announced — the watchers already hold the ending |
| Closing row unreadable | a column this build cannot decode | `ErrorKind::Events` with the decode failure as its cause; the report's finalize logs the step and continues, the lease verb reports it |
| Completion frame with unreadable fields | a malformed or partial payload on the tail | `figure()` reads each number as unknown; a missing status still marks the turn done; missing fleet facts change nothing (1.4, 1.5) |
| Run changed the fleet status | gate blocked, auto-pause | the strip shows the frame's status at once; one router refresh; lifecycle controls follow (1.7) |
| Server render lands after a frame | the kill switch's own refresh | the render's facts overwrite the frame's; the next frame overwrites back (1.8) |
| Frame lands during a refresh the strip asked for | a gate opens while the status-change render is in flight | the render's facts are dropped — the registry's `factsSeq` moved past what the hook saw when it asked — and the frame's stand; an unasked render (the kill switch's) still overwrites, and the next frame overwrites back (1.8) |
| Completion for a row the timeline never opened | a subscriber that joined after the opening; a continued run; a dropped opening | the frame carries the whole row, so the row opens from it and the strip moves; a frame too thin to be a row is dropped (1.4) |
| Two counts published out of order | two parks on one fleet in the same instant | each count rode its own statement's snapshot; the later frame's count stands until the next frame or render restates it |
| Thread read fails at first render | backend error or timeout | strip reads "unavailable" with the exact count still shown; the first row off the stream makes it available (1.11) |
| Transient status on a read | 408, 425, 429, 502, 503, 504 | retried with backoff and Retry-After floor; the final failure surfaces as today (2.1) |
| Backend hangs | no response | default timeout aborts; `TIMEOUT` class retries a read up to the ceiling, never a write (2.3, 2.8) |
| Server error on a write | 5xx on POST or PATCH | not replayed; error surfaces; optimistic state rolls back (2.2, 4.x) |
| Caller cancels | navigation, effect cleanup | `RequestCancelledError`, dropped silently, never retried (2.6) |
| Optimistic write rejected | 409, 4xx, network, timeout | row or badge restored; error shown beside the control; a secret delete whose outcome is unknown — no status, a timeout, a server error — re-reads the list, and a definite refusal does not (4.1, 4.2, 4.3) |
| Already resolved elsewhere | 409 with resolver | row stays removed; notice names the resolver (4.4) |
| Inbox tab hidden | `document.visibilityState === "hidden"` | ticks skip; one read on return (5.1) |
| Retry disabled | `AGENTSFLEET_NO_RETRY=1` | one attempt, as today (regression row) |

## Invariants

1. The durable row is written before the frame that names it — the publish follows the closing statement's return in `pull.rs`, `report.rs`, `park.rs` and `inbox.rs`; tests 1.1 to 1.3 read the frame and find the row.
2. No stream-driven effect issues a read — `useFleetRunSummary` and `ChatView` import no action and no API module; the only router refresh is on a fleet status change (rubric R1, test 1.7).
3. Every request without a caller signal carries the default timeout, the approvals resolve included — enforced in `client.ts` and `approvals.ts`; tests 2.4 and 2.9.
4. A non-idempotent method is never replayed after a server 5xx or a client timeout — `mayHaveBeenProcessed` in `retry.ts`; tests 2.2 and 2.8.
5. Optimistic state reconciles to server truth inside the transition that set it — tests 4.1 to 4.4 assert the settled DOM equals the server response.
6. The browser holds no API token and issues no `fetch` to the backend — statement 1; the strip subscribes to a cookie-authed stream; grep unchanged from `web_app.md`.
7. A bracket or gate publish never fails the verb that made it — every publish is `async fn … -> ()` with its failure logged; the integration suites' negative arms hold.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | n/a | n/a | n/a | n/a | n/a |

`approval_resolved` keeps firing from `ResolveButtons` after a successful resolve, unchanged. The daemon's dropped-frame lines (`bracket_frame_dropped`, `gate_frame_dropped`) are debug-level and carry the fleet id and the queue's reason, never a payload. The retry layer's `onAttempt` and `onRetry` hooks stay the observability seam; the app emits no console output. Metrics review: no analytics or funnel playbook update required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration (Rust) | `test_bracket_frames_open_and_close_a_run` | subscribe, beat, poll → first frame `event_received` with the seeded event id and an integer `created_at`; report → `event_complete` with status processed, `wall_ms` 1500, tokens > 0, integer `cost_nanos`, `fleet_status` active, `pending_approvals` 0 |
| 1.2 | integration (Rust) | `test_a_refused_lease_closes_the_run_on_the_tail` | wallet drained, poll → `event_received` then `event_complete` with status gate_blocked, a non-empty failure label, null tokens |
| 1.3 | integration (Rust) | `a_parked_gate_is_announced_on_the_fleets_live_tail`, `a_decision_is_announced_on_the_fleets_live_tail`, `a_queue_that_will_not_take_the_frame_does_not_fail_the_decision`, `test_bracket_publish_redis_down_does_not_fail_the_closing` | a park → `gate_opened` with the event id and count 1; two seeded gates, one denied → `gate_resolved` denied by the operator with count 1; a lapsed gate swept → `gate_resolved` timed_out by the sweeper; an unreachable Redis → the denial still lands and the row reads `denied`; the closing still answers `gate_blocked` and the publish returns |
| 1.4 | unit (Rust) | `should_lead_every_frame_with_its_kind`, `should_carry_the_whole_terminal_row_on_a_completion` | every variant's JSON starts `{"kind":"…"`; every `EventSummary` key rides the completion unchanged beside `fleet_status` and `pending_approvals` |
| 1.5 | unit | `the strip shows the new run figures when the completion frame arrives, with no read` | seed tokens 1500 → frames received + complete (tokens 1200, count 1) → strip 1,200 and "1 approval waiting"; one EventSource, no action, no refresh |
| 1.6 | unit | `a gate frame moves the pending count and touches nothing else`, `a gate frame moves the count and no row` | `gate_opened` count 2 → link "2 approvals waiting"; `gate_resolved` count 0 → link gone; events and latest keep identity |
| 1.7 | unit | `only a status change refreshes the server-rendered controls, once` | completion with fleet_status paused → strip "paused", refresh once; a second paused completion → still once |
| 1.8 | unit | `a server render resets the facts to its own…`, `a server render's facts overwrite the tail's…` | frame paused/1 then reconcile killed/0 → killed/0; a later gate frame → killed/1 |
| 1.9 | unit | `a reconnect backfill's terminal row moves the strip like a frame would`, `keep their identity across chunks of a streaming reply` | `reconcileServerRows` with a newer terminal row → strip moves; two chunk frames → `latest` is the same object |
| 1.10 | integration (Rust) + unit (route) | `the_detail_counts_the_fleets_pending_gates_and_only_those`; `fleet Chat remains available when the thread read fails, and never reads the inbox`, `fleet summary links an exact pending count…` | two pending, one denied on the fleet and one pending on a neighbour → 2, and the neighbour reads 1; thread read throws, detail carries `pending_approvals: 2` → "2 approvals waiting", no `/approvals` URL fetched; count 1 → "1 approval waiting" |
| 1.11 | unit | `a thread read that failed leaves the strip unavailable…`, `skips the rows the browser made…` | `latestAvailable: false` → "Latest data unavailable." until a completion lands; optimistic and failed rows are never the latest |
| 2.1–2.7 | unit + integration | as in the earlier revision of this table | unchanged |
| 2.8 | unit | `a client-side timeout never replays a non-idempotent method, and still retries a read`, `does NOT replay a POST whose attempt timed out client-side…`, `DOES retry a POST that returns 429 or 408…` | POST + `TIMEOUT` → one attempt; GET + `TIMEOUT` → retried; POST + 429 or 408 → retried |
| 2.9 | unit | `a hung resolve aborts after the default timeout instead of pending forever`, `a timeout during the body read is a timeout, never an empty resolve` | `fetch` never answers → `ApiError` code `TIMEOUT` after `DEFAULT_REQUEST_TIMEOUT_MS`; a body read that times out is the same class |
| 3.1–3.3 | unit | as before | unchanged |
| 4.1–4.7 | unit | as before, plus `the dialog holds its buttons disabled until the delete settles` and `should close the confirm and refresh when an admin action succeeds` | the confirm's buttons are disabled until the action settles; a settled action closes it and re-reads |
| 5.1 | unit | `a hidden tab asks for nothing, and one read catches it up when it is looked at again` | `visibilityState` hidden across three ticks → zero reads; visible + `visibilitychange` → one read |
| 5.2 | unit | `a tick still in flight is not stacked by the next` | a never-answering read across three ticks → one read |
| e2e | e2e (existing) | `chat-single-fetch.spec.ts`, `workspace-fetch-dedupe.spec.ts` | thread reads per chat render stay 1; workspace list fetches stay ≤1 |

`/orly-write-unit-test` runs once per Section over that Section's diff and again at the boundary. `/orly-write-integration-test`: §1 crosses the daemon's HTTP and Redis boundaries with real input and output (the three Rust suites above) and §2 extends `retry.integration.test.ts`; §3 to §5 are in-process and record `N/A`.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The strip's subscription issues no read and the summary action is gone (§1) | `grep -cE 'fetch\(|Action\(|import \{ [^}]*\} from "@/lib/api/' ui/packages/app/components/domain/useFleetRunSummary.ts; grep -rc "getFleetRunSummaryAction" ui/packages/app --include='*.ts' --include='*.tsx' \| grep -v ':0' \| wc -l` | 0 (a type-only import is not a read); 0 | P0 | ✅ `0`; `0` |
| R2 | The daemon announces the opening, the settled closing and the refused closing (§1) | `grep -rhoE "publish_(received\|completion)\(" rustd/crates/afd_fleet/src/lease/pull.rs rustd/crates/afd_fleet/src/lease/report.rs \| wc -l` | 3 | P0 | ✅ `3` |
| R3 | The count is on the wire (§1) | `grep -c '"pending_approvals"' public/openapi.json` | 1 or more | P0 | ✅ `2` (the required-list entry and the property) |
| R4 | The default timeout is declared once and applied (§2) | `grep -c "DEFAULT_REQUEST_TIMEOUT_MS" ui/packages/app/lib/api/client.ts` | 2 or more | P0 | ✅ `2` |
| R5 | Both pages start their reads together (§3) | `grep -c "Promise.allSettled" "ui/packages/app/app/(dashboard)/admin/models/page.tsx"; grep -n "startRunnerViewRead(\|await loadRunner(" "ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/page.tsx"` | 1; the `startRunnerViewRead(` call line precedes the `await loadRunner(` line | P0 | ✅ `1`; `startRunnerViewRead(` on line 94, `await loadRunner(` on line 95 |
| R6 | Four mutation surfaces are optimistic (§4) | `grep -rl useOptimistic ui/packages/app/app ui/packages/app/components \| wc -l` | 4 | P0 | ✅ `4` |
| R7 | Scoreboard re-measured (§4.6) | `grep -n "useOptimistic" docs/architecture/web_app.md \| grep -c "| 4 |"; grep -n 'useEffect\` files' docs/architecture/web_app.md \| grep -c "| 31 |"; grep -rl useEffect ui/packages/app/app ui/packages/app/components ui/packages/app/hooks \| wc -l` | 1; 1; 31 | P1 | ✅ `1`; `1`; `31` (`ChatView` lost its effect to the hook) |
| R8 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table (the spec itself excepted) | P0 | ✅ 170 paths; the only one absent from the table is this spec |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | ✅ `ALL GATES GREEN ── ready for VERIFY` (exit 0, staged scope) |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ exit 0; cargo 2354 · app 2506 (coverage floor 100% met) · website 175 · other packages 512 |
| S3 | Slow tier green (code-carrying branch) | `make test-integration-rustd` | exit 0 | P0 | ✅ exit 0; 374 passed, 0 failed (369 at the first pre-PR run), the tail, closing-count and approve-arm suites among them |
| S4 | Lint green | `make lint-all` | exit 0 | P0 | ✅ `✓ All lint checks passed` (exit 0) |
| S5 | Version sync | `make check-version` | exit 0 | P0 | ✅ `✓ all versions match 0.27.1` |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found`, 5197 commits scanned (Sep 06 run) |
| S7 | No source file this diff grew past the cap | `git diff --name-only origin/main...HEAD \| grep -vE '\.md$\|Cargo\.lock$\|public/openapi\.json$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | only `ui/packages/app/lib/types.ts`, which was 429 lines on `main` and gains four here | P0 | ✅ `433 ui/packages/app/lib/types.ts` and nothing else |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ plus one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

Symbols that lose their only consumer and leave in the same diff: `getFleetRunSummaryAction` and its test; `useRefreshSummariesOnCompletion`, `REFRESH_DEBOUNCE_MS` and their test; `FleetThread`'s `onRunCompleted`; `RUN_SUMMARY_APPROVALS_LIMIT` and `RUN_SUMMARY_LATEST_LIMIT`; `METRICS_APPROVALS_UNAVAILABLE`; `RunMetricsStrip`'s `pendingApprovalsHasMore` and `approvalsAvailable`; `ChatViewData.approvals`; `afd_api_tenant`'s private `summary()` (replaced by `EventRow::summary`); the router import in the retired hook; the post-push refresh in `ResolveButtons.tsx`. Covered by R1 and the tests named in §1 and §4.

## Out of Scope

- A stream-driven approvals inbox. The daemon half landed here — `gate_opened` and `gate_resolved` ride every fleet's channel — but the inbox page still polls; subscribing it to the workspace multiplex and refetching on a gate frame is a registry change on the wall's stream and a separate spec. §5's hidden-tab pause is what this spec does about the poll.
- Rendering the header's lifecycle controls from the stream's fleet status, so a status change never re-runs the server tree. The one refresh on a status change is kept; the controls are server-rendered by design.
- The retry policy's shape: the unbounded `Retry-After`, the missing total deadline, the network-class replay gate and the action-queue hold a slow read causes. All are M191_001's, with what this branch learned recorded there.
- The CLI's replay gate (`cli/src/lib/http-retry.ts` replays a POST after its own timeout) — M191_001 §4's shared fixture closes it.
- The per-trigger fan-out in `last-delivery.ts`. The fix is a per-trigger field on the fleet detail, which lives in the Rust crates.
- The wall components copying `initialFleets` into state. No live mutation path reaches them today.
- A client-side data cache. Statement 1 of `web_app.md` rules it out; the browser holds no token.
- The exhaustion badge refreshing on a completion that does not change fleet status. It refreshes on the next navigation or on the status-change refresh, which covers auto-pause.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator watching a chat sees the run finish, the strip's tokens, cost and outcome move, the "approvals waiting" link appear when a gate opens and vanish when someone answers it, and the thread does not flicker or re-scroll — with nothing on the wire but the frames that said so. They delete a secret and the row is gone as their finger lifts.
2. **Preserved user behaviour** — every read still happens on the server; every mutation still goes through the same Server Action; the kill switch, the chat composer and every list keep their current controls and copy; the CLI's steer tail reads the same `event_complete` it always switched on, and now receives it.
3. **Optimal-way check** — the unconstrained shape is a single client store fed by the stream with every mutation applied locally. The half of that which holds — the strip as a view over the stream — is what this spec builds; the half that conflicts with statement 1 (a second source of truth for writes) is not.
4. **Rebuild-vs-iterate** — iterate on the app; add one module to the daemon. Every client pattern already exists in the package once; the daemon's bracket module copies the runner-frame publisher beside it.
5. **What we build** — four tail frames and their three publishers, one field on the fleet detail, a row model and a facts module in the registry, one subscription hook, a default policy in `request()`, two parallel starts, four optimistic surfaces, one hidden-tab pause, one scoreboard update, one changelog entry.
6. **What we do NOT build** — a data-fetching library, a stream-driven inbox, a new endpoint, a retry policy that differs from the CLI's, a client-side lifecycle control.
7. **Fit with existing features** — compounds with the one-stream wall (which now also receives the brackets and gate frames, and drops the kinds it does not render) and the chat's optimistic composer. It must not destabilize the ETag `If-Match` editor, which keeps using `requestWithEtag` and gains only the retry on its GET.
8. **Surface order** — API first (the frames and the field), UI second, docs alongside. The CLI already reads the frames.
9. **Dashboard restraint** — no new control. The strip shows the same figures from the same server fields; a failed thread read shows "unavailable" rather than a spinner or a guess; the count is exact rather than "50+".
10. **Confused-user next step** — a rejected optimistic write puts the row back and shows the same error copy it shows today, beside the control that caused it.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five sections by mechanism, not by page, so each has one reference implementation to copy and one rubric row to prove it. §1 is first because it is the largest steady-state cost and the one that was silently broken. §2 is a transport change with no UI diff. §3 to §5 are page-local.
- **Alternatives considered:** keeping the summary Server Action and merely making the daemon publish a bare completion (rejected: three reads per completion per viewer when the daemon holds the row); carrying only the row on the completion and reading the count separately (rejected: the count is one indexed `COUNT(*)` in the same statement, and the gate frames need it anyway); a client cache library (rejected: statement 1); `"use cache"` on the layout reads (rejected: every page is `force-dynamic` by design); a stream-driven inbox in this spec (rejected: a wall-stream registry change; named in Out of Scope); making `requestWithRetry` the only export (rejected: same outcome, larger diff).
- **Patch-vs-refactor verdict:** a **patch** on the app and a **small addition** to the daemon: every client change reuses a pattern already shipping in the package, and the daemon's module copies the publisher beside it; no module boundary moves except the row model, split out of the reducers to keep both under the cap without a cycle.

## Discovery (consult log)

- **Consults** — Sep 05, 2026, Indy chose "Perf batch + optimistic rows" from four offered scopes; D (poll pause) and E (approvals transport) moved to a follow-up. Later the same day the poll pause came back in as §5 and the approvals transport's timeout as Invariant 3, under the directives below.
  - PLAN amendment (agent, Sep 05, 2026): Files Changed gained the import-site edits in `events.ts` and `fleets.ts`, the split test files, `vitest.setup.ts`, `view-data.ts`, `RunnerIdentityLine.tsx`, and the three existing tests under `tests/` that replace the CREATE rows. §2 narrows the default retry set to GET, HEAD, PUT and adds the abort guard after an adversarial read of the policy.
  - > Indy (2026-09-05 20:43): "Can you make the retry.ts more robust and performant, and change with effects" — asked which reading; Indy chose **rewrite on the Effect library**. Disposition: a separate spec (M191_001), because it adds a dependency and breaks CLI parity; §2 here lands the wiring and the guards, and the rewrite replaces `retry.ts` behind the same `runWithRetry` seam.
  - > Indy (2026-09-05 20:44): "adversarial review on retry.ts" — findings reported in session and carried into the Effect spec's Failure Modes; the two that touch the default path this spec introduces (DELETE replay answers 404; an aborted caller keeps retrying and sleeping) are fixed in §2.
  - Rubric S7 (Sep 05, 2026): four touched test files were over the 350-line cap on `main` already. Offered override-and-split-spec, split-the-two-grown, or split-all; Indy chose **"Split all four now"**. Each became a `tests/<suite>/` directory: one `harness.ts` plus test files by concern, `it` counts identical (29 / 21 / 42 / 77), every file under the cap.
  - > Indy (2026-09-05 20:46): "Have you upgraded all the packages to the latest vite is 5 and others" — no; dependency upgrades are outside this spec's Files Changed. `bun outdated` in the app package lists patch and minor bumps plus vitest 5.0.0; reported in session for a separate decision.
  - REVIEW amendment (agent, Sep 05, 2026, ~22:40, after eight reviewers): Indy's dispositions through AskUserQuestion D1–D5. D1 **Continue** under the replay gate — a client `TIMEOUT` never replays a non-idempotent method (2.8); network-class replay stays with M191. D2 **FIX** the body-read timeout mapping to the `TIMEOUT` class and the backoff cancel throwing the caller's class; **SKIP** the `Retry-After` cap and retry-timeout-once (M191). D3 **FIX** `ChatView`'s render sequencing and catching a rejected action in `ChatView` and `ApprovalsList`; **SKIP** surfacing a 401 on the summary read. D4 **FIX** awaiting the transition in the `SecretsList` and `RunnerHeader` confirms and gating Delete and the empty states on server truth (4.7); **SKIP** focus restore, the pending-card treatment (design-system changes) and the poll-resurrect guard (a pre-existing race). D5 **FIX** all four: settle the secrets test promise, the runner-page rejection test, move `run-summary.ts` beside the page, encode path ids in `fleets.ts` and `events.ts`. The red-team CRITICAL on the raw approvals fetch was applied as Invariant 3's one-form fix; its INFO on the summary read holding the action queue was deferred to M191 and is recorded there.
  - > Indy (2026-09-05): "ultrathink and fix the UI to be performant." · "and no redundant call" · "if the rustd agentsfleet must be updated for any changes so be it." · "ultrathink on the changes" · "and continue" — Disposition: the investigation found `rustd` publishes no bracket frame, so every completion path was dead in production; §1 was re-planned so the daemon announces the row it closes with the fleet facts, the summary action and its three reads per completion are deleted, the count rides the fleet detail so the chat reads no inbox page, and the inbox pauses while hidden. Backend changes fold into this spec and PR (Categories gain API; the file is renamed to carry it). The stream-driven inbox and the retry policy stay named in Out of Scope, with the daemon half of the inbox landed here.
  - REVIEW amendment (agent, Sep 06, 2026, after the rework's second review — six specialists and a Claude adversarial pass, `/review`): **fixed** — the completion carried the row's own `fleet_id`, which the workspace multiplex splices in as its leading key, so that stream put the key on the wire twice (`TailRow` drops the scope; a test pins it); `gate_resolved.event_id` spelled a runless gate as `""` (now `null`, and `Resolved.event_id` decodes the nullable column instead of failing after the row moved); a continued run never opened on the tail — the resolve writes the row the runner's pull then finds already there, so the resolve announces it — and the client dropped a completion for a row it never opened (it now opens one from the frame, which carries the whole row); a failed continuation suppressed the `gate_resolved` frame for a decision that had committed (announced on both arms); the park, the resolve and the sweep each paid a second statement and a second pool acquire for the count (each writing statement now selects it on its own snapshot; the sweep's N+1 is one statement, and a row it cannot decode is logged, never a failure after its commit); the strip re-rendered per chunk of a streaming reply because the hook selected the whole snapshot (two identity-stable selectors; a render-count test); a revisit within the idle window refreshed the server tree for a cached status the server had overtaken (the refresh reads the reconciled store); a render asked for by a frame rolled back a frame that landed during its round trip (`factsSeq`, which only frames advance; the hook drops a render older than the last frame it saw); a completion's facts and row were two snapshot writes (one); an untrusted `fleet_status` outside the vocabulary drove a refresh, and a non-string cause threw inside the listener (validated; `text()`); the optimistic steer kept the client clock after the daemon named its instant (both brackets re-stamp it); the secrets delete re-read the server on every refusal (`isDefiniteRefusal`: only an unknown outcome re-reads; the stale `tests/secrets-list` case that still expected the old rule is the one unit failure lane 2 found); three verbatim publish blocks and two `pending_count`s (`publish_frame`; the counts ride the statements); `publish_tail`'s doc claimed the runner's frames when `activity.rs` bypassed it (routed through); `STATUS_FAILED`, a `figure` import below its module, a row-model re-export twelve importers went through, seven fields listed three times in `run-summary.ts`, `50` at three sites, a bare `250`, a fourth copy of the db-error fixture, `finalize` and `resolve` past the function cap (all one-form fixes); the closing statements restated the history read's column list and pinned `15` by hand (`shared_columns!("c")`, `EventRow::COLUMNS`); the stream descriptions named no frame kind (both enumerate them; artifact regenerated); tests the reviewers named missing: the closing's count off zero, the approve arm's ordering on the tail, an encoded path id, a `fetch` spy on the strip, the empty-status boundary. **Accepted with reason, Indy's ack requested at CHORE(close)** — the count predicate stands in three writing statements in three crates (no crate below `afd_gate`, `afd_approval` and `afd_events` owns the table; the standalone consts are gone and each bind is `afd_wire::approval::status::PENDING`); two counts published out of order by two concurrent parks settle on the next frame (the count now rides the write's own snapshot, which is as ordered as Postgres makes it); a bracket publish on the lease path adds one Redis command to a verb that already read the stream on the same queue; a decode failure after a committed closing is a build defect the integration lane catches, and the refused arm heals on the runner's retry; a parked run's row still reads WORKING while it waits on a human (the row's status model, and the named stream-driven-inbox follow-up); the header's controls lag the strip by the one refresh (the recorded scope choice under Out of Scope); a thread read that failed leaves the thread's idle hint contradicting the strip's unavailable copy (pre-existing; a design-system notice, recorded here); the inbox poll can resurrect a row a stale page still holds (Indy's D4 **SKIP**, above). Two prior reviewers' `Frame::splice` "minor" is the first fix above.
  - Greptile on PR #658 (Sep 06, 2026), two P1s: **fixed** — `RESOLVE_GATE`'s count excluded one row of `resolved` while a re-raised action moves several in the same update, so the frame could count a row the statement had just answered (now `NOT IN (SELECT id FROM resolved)`, the sweep's own shape; `a_re_raised_actions_rows_are_counted_out_together` is red on the old text); **not a bug** — "a frame between the subscription and the mount effect" cannot interleave, both being passive effects of one commit, and a frame that pre-dates the render on a cached entry is the recorded last-writer rule.
- **Metrics review** — no events added; `approval_resolved` unchanged; no analytics or funnel playbook update required.
- **Skill-chain outcomes** — `/orly-write-unit-test`: the diff ledger (20 rows first cut, 26 after the review fixes: 23 tested, 2 `won't-test` with reason, 1 `needs-infra`) and the partial-completion matrix are in the PR's Session notes; red-green: `test_bracket_frames_open_and_close_a_run` fails without `publish_completion` and `an_approval_opens_the_continued_run_before_it_announces_the_answer` fails without the resolve's announcement, `ChatView.test.tsx` fails 3 of 8 without the facts fold, `useFleetRunSummary.test.tsx` fails without the two selectors; mutation testing not run (`cargo-mutants` and Stryker are not installed here). `/orly-write-integration-test`: the daemon-to-client boundary is crossed with real Redis and Postgres by `integration_runner_brackets.rs`, `integration_gate_tail.rs`, `integration_inbox_tail.rs` and `integration_detail_count.rs`, all in `make test-integration-rustd`. `/review`: eight reviewers on the first cut (D1–D5 above), seven on the rework (this amendment); the adversarial pass's continuation finding was the one that changed daemon behaviour.
- **Deferrals** — none at authoring. Baseline timing:
  > Indy (2026-09-05 20:23): "run the test-integration later, before PR" — context: the `verify.integration` baseline count is recorded at the pre-PR gate instead of CHORE(open); rubric S3 runs it there. The unit baseline was recorded at CHORE(open).
