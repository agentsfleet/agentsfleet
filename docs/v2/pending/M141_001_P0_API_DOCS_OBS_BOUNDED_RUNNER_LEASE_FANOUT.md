<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M141_001: An idle runner costs the control plane one Redis read

**Prototype:** v2.0.0
**Milestone:** M141
**Workstream:** 001
**Date:** Jul 23, 2026
**Status:** PENDING
**Priority:** P0 — idle control-plane cost grows as runners × fleets, so the platform cannot enrol runners without multiplying Postgres load
**Categories:** API, DOCS, OBS
**Batch:** B1 — opens M141; nothing else in this milestone runs until the lease fan-out is bounded
**Branch:** set at CHORE(open)
**Test Baseline:** set at CHORE(open) via `make _lint_zig_test_depth`
**Depends on:** M139_004 — the pinned semantic registry must exist before §5 names a metric, and M139_004 §4 rewrites `deploy/grafana/agent-observability.json` atomically; emitting poll-cost series first would create exactly the private-name drift that spec removes and would collide in the same dashboard file
**Provenance:** Large Language Model (LLM)-drafted (Claude Opus 4.8, Jul 23, 2026) from a read-only scan of `assign.zig`, `heartbeat.zig`, and `redis_client.zig` against `docs/architecture/scaling.md`
**Canonical architecture:** `docs/architecture/scaling.md` §Per-request volume and §Where the next ceiling actually lives; `docs/architecture/runner_fleet.md` §Scaling

---

## Overview

**Goal (testable):** A lease poll that finds no work performs zero Postgres round-trips and one bounded Redis read, so idle control-plane cost scales with runner count alone and never with the number of active fleets.

**Problem:** Enrolling runners degrades the control plane even when every runner is idle. Operators see Postgres connection pressure and lease latency climb as the platform adds fleets, with no user traffic to explain it. Adding execution capacity — the one thing a runner is for — makes the API slower for everyone, so the fleet cannot grow.

**Solution summary:** Every idle lease poll currently walks the entire active-fleet population to discover that nothing is queued. `assign.listCandidates` returns all active fleets with no bound, and `selectInner` iterates the whole list, spending a Postgres claim per candidate and up to three Redis commands on each candidate it wins. Ingress already funnels through one producer, so the fix is to record which fleets actually hold work at the moment they receive it, and let the lease consult that set first. An empty set answers no-work immediately; a non-empty set bounds the Postgres candidate query to a named ceiling. A low-cadence sweep re-derives readiness from the streams themselves, so a lost index entry self-heals instead of stranding an event. The `/v1/runners` wire protocol, the per-fleet claim, and the fencing semantics are untouched.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): bound the lease-poll fan-out to ready fleets
- **Intent (one sentence):** Operators can add runners to grow execution capacity without paying a control-plane cost that multiplies by the number of fleets on the platform.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/fleet/assign.zig` — the whole defect lives here: `listCandidates` has no bound and `selectInner` iterates every row. Read `tryCandidate` and `acquireFresh` to count what one idle candidate actually costs before changing either.
2. `src/agentsfleetd/queue/redis_client.zig` §`xaddFleetEvent` — the single producer for `fleet:{id}:events`, reached by all five ingress paths (messages, two webhook surfaces, Slack events, GitHub ingress). Readiness is recorded here, once, or it is recorded five times and drifts.
3. `src/agentsfleetd/fleet/reclaim_sweeper.zig` — the existing background-sweep lifecycle to mirror for the readiness sweep: boot start, tick-interruptible shutdown, join on drain.
4. `docs/architecture/scaling.md` §Per-request volume — currently models an idle poll as one bounded scan. That model is wrong and this spec corrects it; read it to understand what the deployment sizing procedure promises operators today.
5. `docs/architecture/runner_fleet.md` §Failure Recovery Model — the claim/fencing/reclaim guarantees that must survive unchanged.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/queue/fleet_ready.zig` | CREATE | Own the readiness index: mark, peek a bounded slice, and conditionally clear. |
| `src/agentsfleetd/queue/fleet_ready_test.zig` | CREATE | Prove mark/peek/clear semantics, bound enforcement, and clear-only-when-drained. |
| `src/agentsfleetd/fleet/assign_ready_test.zig` | CREATE | Prove the ready-first candidate path, the zero-Postgres idle poll, and the preserved label gate. |
| `src/agentsfleetd/fleet/assign.zig` | EDIT | Consult readiness before Postgres; bound the candidate query; clear readiness at the existing no-work release; keep sticky ordering and the label gate. |
| `src/agentsfleetd/fleet/reclaim_sweeper.zig` | EDIT | Re-mark readiness for fleets it reclaims and for fleets carrying undelivered entries; raise its own fleet-scan bound. |
| `src/agentsfleetd/queue/redis_client.zig` | EDIT | Mark a fleet ready inside the one fleet-event producer. |
| `src/agentsfleetd/queue/redis_fleet.zig` | EDIT | Memoize consumer-group creation so it stops costing a Redis command per candidate per poll. |
| `src/agentsfleetd/queue/constants.zig` | EDIT | Own the readiness key shape and the sweep cadence alongside the existing stream constants. |
| `src/lib/common/constants.zig` | EDIT | Own the per-poll candidate ceiling next to the existing lease and poll constants. |
| `src/agentsfleetd/observability/metrics_counters.zig` | EDIT | Expose poll-cost counters and the readiness depth gauge — global and unlabelled, so they belong here and not in the per-runner labelled table. |
| `src/agentsfleetd/observability/metrics_counters_test.zig` | EDIT | Prove the new families render, stay unlabelled, and need no datastore call. |
| `src/agentsfleetd/observability/metrics_render.zig` | EDIT | Render the new families on the existing Postgres-free scrape path. |
| `src/agentsfleetd/fleet/concurrency_lease_test.zig` | EDIT | Prove concurrent claim behaviour is unchanged under the ready-first path. |
| `src/agentsfleetd/fleet/event_lifecycle_reclaim_integration_test.zig` | EDIT | Prove reclaim and Pending Entries List (PEL) re-delivery still reach a runner. |
| `src/agentsfleetd/tests.zig` | EDIT | Register the new unit suites. |
| `docs/architecture/scaling.md` | EDIT | Correct the idle-cost model, the sizing procedure, and the anti-pattern list. |
| `docs/architecture/runner_fleet.md` | EDIT | Document readiness in the lease flow and its recovery bound. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (readiness key, sweep cadence, and candidate ceiling are named constants, never inline literals; the existing inline `"fleet:{s}:events"` in `xaddFleetEvent` is fixed while that function is open), **NSQ** (the bounded candidate query stays schema-qualified with named constants), **NDC** (no unreached readiness branch ships), **NLR** (the unmemoized consumer-group call is fixed, not worked around), **NLG** (no dual-path fallback kept as a compatibility shim), **ORP** (renamed or removed helpers swept across source, tests, and architecture), **ECL** (a Redis readiness failure is retryable and degrades to the sweep; it is never a fatal lease error), **CNX** (the ready-first path must not add a second concurrently-held pool connection per request), **OBS** (every new observable state gets a log or metric entry), **ITF** (integration tests use the real schema fixture), **GRD** (ground every claim about current cost in `assign.zig`, not in `scaling.md`, which this spec proves wrong).
- **`dispatch/write_zig.md`** — fixed ownership and `errdefer` placement on the new sweep thread, tagged-union results for the readiness peek, public-surface shape verdict for the two new modules, file and function length caps, both Linux target builds.
- **`dispatch/write_any.md`** — logging standard, error registry, source length, milestone-free test naming.
- **`dispatch/name_architecture.md`** — the readiness key names a new Redis namespace, so the architecture consult and the `docs/architecture/` update land with the implementation.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | format, lint, unit suites, and both Linux target builds before COMMIT |
| PUB / Struct-Shape | yes | declare a FILE SHAPE DECISION for `fleet_ready.zig` and `ready_sweep.zig` before adding either public surface |
| File & Function Length (≤350/≤50/≤70) | yes | `assign.zig` already carries the candidate scan; readiness selection lands in the new module rather than growing that file toward the cap |
| UFS (repeated/semantic literals) | yes | readiness key, sweep cadence, candidate ceiling, and metric names are named constants in the modules that own them |
| User Interface (UI) Substitution / DESIGN TOKEN | no | no TypeScript or design-system surface is touched |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | lifecycle + logging | the sweep thread follows the `reclaim_sweeper.zig` start/stop/join lifecycle; readiness failures log against the existing error registry; no Structured Query Language (SQL) schema changes |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/http/handlers/connectors/outbound/worker.zig` — the M106 answer-delivery consumer already solves the same shape: a non-blocking claim plus a client-side idle backoff, boot-started, with a bounded shutdown join. Mirror its lifecycle for the readiness sweep rather than inventing a second background-thread pattern.
- **Reference:** `src/agentsfleetd/fleet/reclaim_sweeper.zig` — the in-repo precedent for a periodic sweep that re-derives state from Redis; mirror its cadence handling and tick-interruptible shutdown.
- **Divergence:** the readiness index is a new Redis namespace with no in-repo precedent; its key shape and cardinality bound are settled in `docs/architecture/` in the same Pull Request (PR) per the architecture consult.

## Sections (implementation slices)

### §1 — Ingress records which fleets hold work

A fleet's stream receives an event through exactly one producer. Recording readiness there makes the index correct for all five ingress paths without touching any handler. The write is best-effort and never fails the ingress call: a fleet whose readiness write is lost still has its event in the stream, and §3 recovers it.

**Implementation default:** a Redis set keyed per the constant in `queue/constants.zig`, because membership is the only question the lease asks and a set gives O(1) mark with natural deduplication across repeated events for the same fleet.

- **Dimension 1.1** — an accepted fleet event marks its fleet ready before the producer returns, for every ingress path → Test `test_fleet_event_marks_fleet_ready`
- **Dimension 1.2** — a readiness write failure logs, increments a counter, and still returns the entry Identifier (ID) to the caller → Test `test_ready_mark_failure_never_fails_ingress`

### §2 — An idle poll stops touching Postgres

The lease consults readiness before it opens a Postgres connection. An empty readiness set answers no-work with the existing backoff hint and zero database work — the dominant steady state on any deployment with more fleets than concurrent events. A non-empty set yields a bounded slice of fleet IDs, and only those IDs enter the candidate query.

The label gate and sticky ordering are properties of the candidate query and stay exactly where they are: the query keeps `required_tags <@ labels` and the sticky-first ordering, and gains a membership restriction plus a ceiling. Readiness narrows the input; it never decides eligibility.

**Implementation default:** the candidate ceiling is a named constant in `src/lib/common/constants.zig` alongside `NO_WORK_RETRY_AFTER_MS`, because it trades the same axis — per-poll cost against discovery latency — and operators tuning one must see the other.

- **Dimension 2.1** — a poll against an empty readiness set acquires no pool connection and issues no query → Test `test_idle_poll_performs_zero_pg_roundtrips`
- **Dimension 2.2** — a non-empty readiness set restricts the candidate query to its members and never returns more rows than the ceiling → Test `test_candidate_scan_is_bounded_by_ceiling`
- **Dimension 2.3** — a ready fleet whose `required_tags` exceed the runner's labels is not leased to that runner → Test `test_ready_set_never_bypasses_label_gate`
- **Dimension 2.4** — the runner's own affinity still sorts ahead of other ready candidates → Test `test_sticky_ordering_survives_ready_filter`

### §3 — Readiness never strands an event

The index is a hint, not the system of record; the streams are. Two mechanisms keep a hint failure from becoming lost work.

**The clear site is where the code already proves emptiness.** Do not test whether the stream is empty — it never is. Ingress trims at `MAXLEN ~ 10000`, so delivered entries persist, and a stream-empty condition would essentially never fire, leaving every fleet that ever received an event permanently in the index and restoring the very scan this spec removes. The real proof already exists: in `acquireFresh`, a claim-won poll whose own-PEL read and whose undelivered read both return null has established there is nothing deliverable, and that site already releases the claim. Clear readiness there, at zero additional Redis cost.

**The backstop is the sweeper that already runs.** `reclaim_sweeper` already wakes on `fleet_reclaim_interval_ms`, pulls active fleets from Postgres, and walks each fleet's stream to rescue stranded entries — with a registered lifecycle in `serve_background` and an integration test that drives `sweepOnce` directly. Readiness re-marking belongs inside that existing loop, not in a second thread. This also closes a hole the clear site leaves open: the own-PEL read only sees this instance's pending entries, so a strand on another replica is invisible to it, and a reclaimed stray currently re-enters the lease flow only "on the next poll" — which never comes if readiness was cleared. Re-marking on reclaim makes those strays leasable again.

**Implementation default:** the sweeper re-marks but never clears — a false-positive entry costs one wasted candidate check, a false-negative strands an event. Worst-case recovery is therefore the shipped `fleet_xautoclaim_min_idle_ms_int` plus one `fleet_reclaim_interval_ms`, not a new cadence.

- **Dimension 3.1** — a claim-won poll clears readiness exactly when both the own-PEL read and the undelivered read return null, and never merely because the stream holds delivered entries → Test `test_ready_cleared_only_when_nothing_deliverable`
- **Dimension 3.2** — a fleet with a pending PEL entry stays ready and is re-delivered → Test `test_pending_entry_keeps_fleet_ready`
- **Dimension 3.3** — a readiness entry deleted out-of-band is restored by the sweeper, and the event is then leased → Test `test_sweeper_recovers_dropped_readiness`
- **Dimension 3.4** — a stray reclaimed by the sweeper re-marks its fleet ready, so a strand left by another replica becomes leasable without waiting for unrelated ingress → Test `test_reclaimed_stray_remarks_readiness`
- **Dimension 3.5** — the sweeper's active-fleet scan reaches every active fleet rather than a fixed head of the list, so the backstop does not silently stop covering fleets past its bound → Test `test_sweeper_scan_covers_all_active_fleets`

### §4 — Consumer-group creation stops costing a Redis command per candidate

`ensureFleetConsumerGroup` issues an `XGROUP CREATE` on every call and relies on the `BUSYGROUP` error for the steady state, so it costs one Redis round-trip per candidate per poll forever. The group is durable, so the result is memoizable in-process. Memoization is per-process and bounded; a cold process, an evicted entry, or a genuinely new fleet still takes the real path.

**Implementation default:** mirror the fixed-capacity, allocator-free table already used by `metrics_runner.zig`, because the same constraints apply — a bounded key space, no allocator on a hot path, and a defined overflow behaviour.

- **Dimension 4.1** — repeated leases against one fleet issue the group-create command once per process → Test `test_consumer_group_ensured_once_per_process`
- **Dimension 4.2** — a fleet beyond the memo capacity still gets a correctly created group → Test `test_group_memo_overflow_still_creates`

### §5 — Poll cost is visible on the existing scrape path

The defect this spec fixes was invisible: nothing on `/metrics` distinguished an idle poll that cost one Redis read from one that walked every fleet on the platform. The new families make per-poll cost and readiness depth observable so a regression is caught by a dashboard rather than by a Postgres incident. They carry no fleet, workspace, tenant, or runner label, so they live with the other global unlabelled families rather than in the per-runner table. **The scrape path stays datastore-free:** `runner_fleet.md` promises `/metrics` renders from memory so it stays healthy exactly when the database is not, so readiness depth is an in-process counter maintained by mark and clear, never read back from Redis at render time.

Names, units, and attribute keys come from the pinned semantic registry created by M139_004; this section adds no private descriptor of its own.

- **Dimension 5.1** — candidate-scan depth and per-poll database round-trips render as bounded, unlabelled families → Test `test_poll_cost_metrics_render_unlabelled`
- **Dimension 5.2** — readiness depth and readiness-write failures render and move with observed state → Test `test_ready_depth_and_failures_observable`

## Interfaces

```text
/v1/runners wire protocol — UNCHANGED.
  POST /v1/runners/me/leases keeps its empty request body, its always-200 reply,
  and the { lease | null, retry_after_ms } shape. No new field, verb, or header.

fleet_ready (new internal module)
  mark(client, fleet_id)            best-effort; never propagates to the caller
  peek(client, alloc, max) -> [][]const u8    at most `max` fleet ids
  clear(client, fleet_id)           called ONLY from the acquireFresh site that already
                                    proved nothing is deliverable; never re-derives it (§3)
  -- depth: in-process counter (mark/clear), never read from Redis, so /metrics
     keeps its datastore-free render path per runner_fleet.md.

assign.select(ctx, alloc, runner_id) -> ?Acquired   signature UNCHANGED.
  Internally: readiness peek precedes any pool acquire; an empty peek returns
  null without touching Postgres.

Redis namespace: one key owned by queue/constants.zig alongside the existing
fleet stream prefix and suffix. No Postgres schema change.
```

No public API path, request body, response body, Command-Line Interface (CLI), or UI behaviour changes.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Readiness write fails | Redis unavailable or timing out at ingress | Log, increment the failure counter, return the entry ID normally; the sweep restores readiness within its cadence. The event is never lost. |
| Readiness peek fails | Redis unavailable at lease time | Treat as retryable, not fatal; answer no-work with the existing backoff hint. Runners retry; no partial or unbounded Postgres scan is attempted. |
| Readiness entry lost | Redis eviction, failover, or an out-of-band delete | The reclaim sweeper re-marks it on its next pass; worst-case recovery is the existing XAUTOCLAIM min-idle plus one sweep interval, stated in the architecture. |
| Fleet stuck ready | Every ready fleet requires tags no polling runner advertises | Readiness is not cleared (the fleet is genuinely non-empty); the ceiling plus rotation stops those fleets from starving the rest of the ready set. |
| Two runners peek the same fleet | Concurrent polls read overlapping slices | Unchanged: `affinity.claim` decides exactly one winner and the loser moves on having read no event. |
| Readiness cleared with work pending | A drain check races a concurrent ingress | Clearing requires a won claim plus proof both the PEL and the stream are empty; a racing ingress re-marks. The sweep is the backstop. |
| Sweep overlaps a lease | Sweep marks a fleet a lease is draining | Marking is idempotent; a redundant entry costs one wasted candidate check and is cleared on the next drained poll. |
| Memo returns a stale hit | Consumer group deleted out-of-band | The read path surfaces the missing group as a retryable error, the entry is invalidated, and the next poll recreates it. |
| Pool exhausted under load | Unrelated saturation during a busy poll | Unchanged existing behaviour: acquire failure is logged and the lease answers no-work. |

## Invariants

1. A poll whose readiness peek returns empty performs zero Postgres pool acquisitions — enforced by a test asserting the acquire count, not by review.
2. The candidate query returns at most the named ceiling of rows — enforced by the query's own bound, asserted in test.
3. Readiness is cleared only from the `acquireFresh` site where the own-PEL read and the undelivered read have both returned null — enforced by that being the single call site of `clear`, asserted by a test that leaves delivered entries in the stream and proves the fleet still clears.
4. Every fleet holding an undelivered entry or a non-empty PEL is either in the readiness index or re-marked by the reclaim sweeper — enforced by the sweeper test that deletes an entry and asserts recovery, and by the reclaim test that asserts a reclaimed stray re-marks its fleet.
5. The `/v1/runners` wire protocol is byte-identical — enforced by the frozen protocol tests in `src/lib/contract/`, which this spec does not modify.
6. Every candidate still passes `required_tags <@ labels` — enforced by keeping the gate in the query and asserting a tag-mismatched ready fleet is never leased.
7. No new metric carries a fleet, workspace, tenant, event, lease, or runner label — enforced by the unlabelled-render test.
8. A readiness failure never fails an ingress call or a lease reply — enforced by the ingress and peek failure-injection tests.
9. The `/metrics` render path issues no Redis and no Postgres call — enforced by the in-process depth counter and a render test that runs with the datastore unavailable.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| candidate-scan depth | ops | every completed lease poll | bucketed count only | no fleet, workspace, tenant, or runner identity | `test_poll_cost_metrics_render_unlabelled` |
| per-poll database round-trips | ops | every completed lease poll | bucketed count only | same | `test_poll_cost_metrics_render_unlabelled` |
| readiness depth | ops | maintained in-process by mark/clear; rendered from memory, never read back from Redis | single gauge value | same | `test_ready_depth_and_failures_observable` |
| readiness write failures | ops | a mark or clear fails against Redis | fixed reason only | no key contents or error text | `test_ready_mark_failure_never_fails_ingress` |
| sweep recoveries | ops | the sweep restores a missing entry | count only | same | `test_sweep_recovers_dropped_readiness` |

Exact names, units, and attribute keys are taken from the pinned semantic registry that M139_004 creates; this spec introduces no private descriptor. No PostHog event, product-analytics event, or funnel changes — this is operator telemetry only, so no analytics playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_fleet_event_marks_fleet_ready` | an event accepted on each ingress path leaves its fleet id in the readiness index exactly once. |
| 1.2 | unit | `test_ready_mark_failure_never_fails_ingress` | an injected Redis failure on mark still returns the stream entry id and increments the failure counter once. |
| 2.1 | integration | `test_idle_poll_performs_zero_pg_roundtrips` | with an empty readiness index, one lease poll records zero pool acquisitions and returns `lease=null` with the backoff hint. |
| 2.2 | integration | `test_candidate_scan_is_bounded_by_ceiling` | with ready fleets numbering ceiling plus ten, one poll's candidate query returns exactly the ceiling. |
| 2.3 | integration | `test_ready_set_never_bypasses_label_gate` | a ready fleet requiring a tag the runner lacks is never leased to it; a matching runner leases it. |
| 2.4 | integration | `test_sticky_ordering_survives_ready_filter` | with two ready fleets, the one carrying the runner's affinity is attempted first. |
| 3.1 | integration | `test_ready_cleared_only_when_nothing_deliverable` | a fleet whose stream still holds delivered-and-acked entries but has no PEL entry and no undelivered entry DOES clear; a fleet with an undelivered entry does not. Guards the regression where a stream-empty condition never fires. |
| 3.2 | integration | `test_pending_entry_keeps_fleet_ready` | a fleet with a PEL entry stays ready across a poll and the entry is re-delivered. |
| 3.3 | integration | `test_sweeper_recovers_dropped_readiness` | deleting a readiness entry with an undelivered event restores it on the next `sweepOnce` and the event then leases. |
| 3.4 | integration | `test_reclaimed_stray_remarks_readiness` | a stray reclaimed by `sweepOnce` from another consumer's PEL leaves its fleet ready, and the next poll leases it without any new ingress. |
| 3.5 | integration | `test_sweeper_scan_covers_all_active_fleets` | with active fleets exceeding the sweeper's batch bound, every fleet is reached across successive passes; none is permanently skipped. |
| 4.1 | integration | `test_consumer_group_ensured_once_per_process` | ten leases against one fleet issue exactly one group-create command. |
| 4.2 | unit | `test_group_memo_overflow_still_creates` | a fleet past the memo capacity still gets a created group and a successful read. |
| 5.1 | unit | `test_poll_cost_metrics_render_unlabelled` | rendered output contains the scan-depth and round-trip families with no fleet, workspace, tenant, or runner label. |
| 5.2 | unit | `test_ready_depth_and_failures_observable` | marking three fleets renders depth three; an injected failure renders a failure count of one. |
| regression | integration | `test_reclaim_and_fencing_unchanged` | the existing reclaim, fencing-token, and double-lease guarantees hold under the ready-first path. |
| regression | integration | `test_concurrent_runners_single_winner` | one hundred concurrent polls against one ready fleet yield exactly one lease. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | An idle poll costs zero Postgres round-trips (§2) | `make test-integration` | `test_idle_poll_performs_zero_pg_roundtrips` passes | P0 | |
| R2 | The candidate scan is bounded and label-correct (§2) | `make test-integration` | bound, label-gate, and sticky tests pass | P0 | |
| R3 | Readiness strands no event (§1, §3) | `make test-integration` | drain, pending-entry, and sweep-recovery tests pass | P0 | |
| R4 | Consumer-group creation is once per fleet per process (§4) | `make test-integration` | `test_consumer_group_ensured_once_per_process` passes | P0 | |
| R5 | Poll cost is observable and unlabelled (§5) | `make test-unit-agentsfleetd` | both §5 tests pass | P0 | |
| R6 | Existing lease guarantees are unchanged | `make test-integration` | both regression tests pass | P0 | |
| R7 | Architecture states the corrected idle-cost model | `git diff --stat origin/main -- docs/architecture/scaling.md docs/architecture/runner_fleet.md` | both files non-empty in the diff | P0 | |
| R8 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Repository conformance passes | `make harness-verify` | exit 0 | P0 | |
| S2 | Repository unit suites pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | No leaks | `make memleak` | exit 0 | P0 | |
| S4 | Both Linux targets build | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | both exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --no-banner` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.** N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| unbounded candidate listing | `grep -rn "listCandidates" src/ \| grep -v ready` | 0 matches outside the bounded path |
| inline fleet stream key literal | `grep -rn 'fleet:{s}:events' src/` | 0 matches |
| per-call group-create assumption | `grep -rn "ensureFleetConsumerGroup" src/ \| grep -v memo` | 0 matches on a hot poll path |

## Out of Scope

- **Heartbeat write amplification.** `POST /v1/runners/me/heartbeats` takes a row lock and a conditional durable write per runner per interval. It is real and scales with runner count, but at a tenth the cadence and with no per-fleet multiplier, and the clean fix needs a unique-index schema change that would widen this blast radius past shipping. Follow-up spec.
- **The dead `deploy/grafana/agent_run_breakdown.json` dashboard**, which queries four `agent_*` families the codebase no longer emits. It is a semantic-naming defect and belongs to M139_004's atomic cutover, not here.
- **Provisioning the OpenTelemetry Collector** that `deploy/grafana/agent-observability.json` needs before its panels resolve. Infrastructure deployment, unowned by any spec, raised separately.
- **Runner-side telemetry of any kind.** The runner stays bare per `docs/architecture/observability.md`.
- Push or notify delivery to runners, capacity-aware placement, autoscale-by-queue-depth, and the deferred Postgres metrics refresher — all remain out per the `runner_fleet.md` non-goals fence.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator triples the runner count to absorb a busy week, watches the `agentsfleetd` dashboard, and sees database load flat. The control plane did not notice.
2. **Preserved user behaviour** — every runner already enrolled keeps working with no re-registration, no version pin, and no configuration change; pickup latency for a busy fleet is unchanged; the frozen `/v1/runners` protocol means an unmodified runner binary talks to an upgraded control plane.
3. **Optimal-way check** — the direct path is to stop asking a question whose answer is already known at ingress. The unconstrained-optimal shape pushes work to runners and drops polling entirely; that touches the frozen protocol and the enrolled-runner population, so it stays a later lever. Readiness gets the same idle-cost win without either cost.
4. **Rebuild-vs-iterate** — iterate. The claim, fencing, reclaim, and label-placement semantics are correct and load-bearing; only candidate discovery is wrong. A rebuild would risk run-to-run determinism the current design already guarantees.
5. **What we build** — a readiness index written at the single ingress producer, a ready-first bounded candidate path in the lease, a self-heal sweep, a memoized consumer-group ensure, and poll-cost metrics on the existing scrape path.
6. **What we do NOT build** — no push delivery, no protocol change, no scheduler, no schema change, no runner-side telemetry, no second background-thread pattern.
7. **Fit with existing features** — compounds label placement (M85_001) and reassignment (M84_002) by making their candidate set cheap to evaluate. The one thing it must not destabilize is the per-fleet claim and fencing guarantee that keeps two runners off one fleet.
8. **Surface order** — N/A — no user surface; this is control-plane internals with an operator-metrics surface only.
9. **Dashboard restraint** — the new panels show only counters that exist; no readiness health claim ships before the sweep-recovery counter is live to back it.
10. **Confused-user next step** — an operator seeing lease latency checks the scan-depth and readiness-depth families on `/metrics`, which distinguish "many fleets hold work" from "discovery is scanning too far"; the corrected `scaling.md` sizing procedure names the knob for each.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream owns ingress marking, lease consumption, the sweep, and the metrics because they are one mechanism — shipping the index without the sweep would trade a cost defect for a correctness defect, and shipping either without metrics would leave the regression as invisible as the original.
- **Alternatives considered:** a bare `LIMIT` on the existing scan was rejected because it caps discovery throughput without removing the per-poll Postgres cost, and silently starves fleets past the bound. Sharding fleets across runners was rejected because it converts sticky preference into ownership and breaks the rebalance-free enrolment property. Push or long-poll delivery was rejected for now because it changes the frozen protocol and every enrolled runner, which is a larger upfront decision than this defect warrants.
- **Patch-vs-refactor verdict:** this is a **refactor** of candidate discovery — the code that decides which fleets a poll examines is replaced — while the claim, fencing, billing, and delivery paths it feeds are untouched.

## Discovery (consult log)

- **Consults** — Architecture: the readiness index names a new Redis namespace, so `dispatch/name_architecture.md` fires and `docs/architecture/scaling.md` plus `runner_fleet.md` land in the same PR. The sweep-cadence question raised at authoring is **closed**: readiness repair rides the existing reclaim sweeper, so the worst-case recovery bound is the already-shipped XAUTOCLAIM min-idle plus one sweep interval rather than a new invented number. Record that bound in the architecture as the operator-visible promise.
- **Eng review (`/plan-eng-review`, Jul 23, 2026)** — five findings against this spec's first draft, all folded in before any implementation:
  - **[P0]** the original clear condition ("PEL and stream both empty") can never fire — ingress trims at `MAXLEN ~ 10000`, so delivered entries persist and the stream is never empty. The index would have grown to hold every fleet and silently restored the O(fleets) scan. Now clears at the `acquireFresh` site where both reads already returned null; §3, Invariant 3, and Dimension 3.1 rewritten to guard the regression.
  - **[P1 ×2, one fix]** the original §3 specced a `ready_sweep.zig` thread duplicating `reclaim_sweeper.zig` (which already walks active fleets on an interval with a registered lifecycle and a `sweepOnce` test); separately, the own-PEL read sees only this instance's pending entries, so another replica's strand is invisible to the clear site and a reclaimed stray re-enters the lease flow only "on the next poll" — which never comes once readiness is cleared. Both close by folding re-marking into the existing sweeper: two CREATE files dropped, Dimension 3.4 added.
  - **[P1]** the original `depth(client)` read the gauge from Redis at scrape time, breaking the datastore-free `/metrics` path `runner_fleet.md` promises. Now an in-process counter; Invariant 9 added.
  - **[P2]** the poll-cost families were homed in the per-runner labelled table; they are global and unlabelled, so they belong with the other global counters.
- **Pre-existing defect found during review (not caused by this spec)** — `reclaim_sweeper.fetchActiveFleets` selects `ORDER BY updated_at ASC LIMIT SWEEP_BATCH_LIMIT`, so above that bound a pass never reaches the remaining active fleets. Tolerable while it only rescues strays; **load-bearing once it is also the readiness backstop**. Dimension 3.5 covers it.
- **Source finding** — `assign.listCandidates` carries no `LIMIT` and no workspace scope, so it returns every active fleet platform-wide; `selectInner` iterates the full list, and each candidate the runner wins costs three Postgres round-trips plus three Redis commands before it is released. Idle cost is therefore proportional to runners × active fleets, not to runners.
- **Source finding** — `docs/architecture/scaling.md` §Per-request volume models an idle poll as one bounded `XREADGROUP` scan and derives the sizing procedure and the Upstash bill from that figure. The model omits the per-candidate multiplier, so the published operator sizing understates idle cost; correcting it is in scope here.
- **Source finding** — `ensureFleetConsumerGroup` issues `XGROUP CREATE` on every call and depends on `BUSYGROUP` for the steady state, costing one Redis round-trip per candidate per poll indefinitely.
- **Source finding** — all five fleet-event ingress paths funnel through `redis_client.xaddFleetEvent`, so readiness can be recorded once rather than at each handler. That function also builds its stream key from an inline literal rather than the `queue/constants.zig` prefix and suffix — a pre-existing UFS defect fixed while the function is open.
- **Metrics review** — operator telemetry only; no PostHog event, product-analytics event, or funnel changes, so no analytics playbook update is required.
- **Skill-chain outcomes** —
- **Deferrals** —
