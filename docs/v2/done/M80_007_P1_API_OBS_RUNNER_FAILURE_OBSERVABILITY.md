# M80_007: Runner failure + per-runner liveness observable on zombied /metrics

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 007
**Date:** Jun 01, 2026
**Status:** DONE — §1/§2 shipped via PR #354 (merged); §3 (multi-replica metric correctness) delivered on `feat/m80-007-multi-replica-metrics`. All sections complete.
**Priority:** P1 — operators are blind to *why* a runner-executed run failed, and to per-runner liveness, across the whole fleet (including NAT'd / untrusted hosts).
**Categories:** API, OBS
**Batch:** B1 — independent; no sibling workstream shares its files.
**Branch:** feat/m80-007-multi-replica-metrics
**Depends on:** M80_006 (M80_006_P1_API_RUNNER_FLEET_PLANE — adds the `renewal_terminate` `FailureClass` variant + the fleet lease/heartbeat semantics this reads; the wiring works for all variants regardless, but the `renewal_terminate` end-to-end claim only becomes true once M80_006 lands).
**Provenance:** LLM-drafted (Opus 4.8, Jun 01, 2026) from an eng-reviewed handoff (`HANDOFF_runner_failure_observability.md`); scope decisions are Indy's (Jun 01, 2026).

> **Provenance is load-bearing.** LLM-drafted — the implementing agent cross-checks every claim below against `main` before EXECUTE. Two handoff claims were already corrected during authoring: (1) `FailureClass` does **not** yet carry `renewal_terminate` on `main` (it is on the unmerged M80_006 branch — hence the `Depends on`); (2) the `/metrics` render path is **pure in-memory** (`mc.snapshot()`), so "derive at render time from Postgres" is not a free option — see Decomposition.

**Canonical architecture:** `docs/architecture/runner_fleet.md` — the execution-plane fence (§ scope guard) and the durable-lease model. The doc is silent on the metrics render mechanism; this spec adds that and the implementing agent reconciles the doc (Architecture Consult & Update Gate).

---

## Implementing agent — read these first

1. `src/zombied/observability/metrics_workspace.zig` — the canonical **dynamic-label** counter: fixed-capacity (4096) CAS-claimed slot table keyed on a composite id, allocator-free, lock-free `fetchAdd`, overflow→`_other`. **This is the pattern for the per-`runner_id` dimension** — mirror it; do not invent a map.
2. `src/zombied/observability/metrics_render.zig` (`appendLabeledFamily`, the signup-reason family at `:127`) — the **fixed-label-family** render pattern for the `reason`/`outcome` dimension, and proof the whole render path is in-memory `mc.snapshot()` (no DB).
3. `src/lib/contract/execution_result.zig` — `FailureClass` (9 variants) already carries `label()` = `@tagName`; `ExecutionResult.failure: ?FailureClass` already holds the granular cause. Reuse the enum on the wire.
4. `src/runner/daemon/loop.zig` (`outcomeFor`) — the exact site where the granular cause is collapsed to binary `Outcome`. Slice 1 stops the collapse here.
5. `docs/ZIG_RULES.md` — pg-drain, tagged-union, cross-compile discipline for every `.zig` edit.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Expose runner failure reasons + per-runner liveness on zombied /metrics
- **Intent (one sentence):** An operator scraping `zombied`'s `/metrics` can see, per runner, *why* runs failed and whether each runner is alive and working — for the whole fleet, including runners no scraper can reach.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`; a mismatch with the Intent above → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal. Specifically **UFS** (metric/label name strings → named consts; the `FailureClass` tag identifier is shared verbatim runner↔zombied), **NDC** (no dead code — every new `pub` has a caller), **NLG** (pre-2.0: no "legacy"/compat-shim framing; the optional `failure_reason` is forward-compat, not a legacy shim).
- **`docs/ZIG_RULES.md`** — `.zig` everywhere in scope: pg-drain (`conn.query().drain()` if any handler reads PG), tagged-union results, multi-step `errdefer`, cross-compile both linux targets.
- **`docs/LOGGING_STANDARD.md`** — `report.zig` log emits change (failure now logged with the granular reason).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — `ReportRequest` is a frozen request shape; the added field is optional + backward-compatible.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — all edits are `.zig` | read ZIG_RULES; cross-compile `x86_64-linux` + `aarch64-linux`; tagged-union over the optional field. |
| PUB / Struct-Shape | yes — new `pub fn incRunnerFailure`/`observeRunnerExecution`/render helpers | own shape verdict per new surface; counters live in a single new module file, not scattered `pub`s. |
| File & Function Length (≤350/≤50/≤70) | yes — new counter module + slot table | the slot table mirrors `metrics_workspace.zig` (already <350); keep the new module under cap or split render helper. |
| UFS | yes — metric names, label names, `reason`/`outcome` label values | named consts; the `FailureClass` `@tagName` is the shared cross-runtime identifier — no parallel string table. |
| LOGGING | yes — `report.zig` | structured field `failure_reason`, no PII, level per LOGGING_STANDARD. |
| LIFECYCLE | yes — slot table is process-global `var` (no per-request alloc); confirm no deinit needed | allocator-free static table like `metrics_workspace.zig`; document the no-free invariant. |
| ERROR REGISTRY | no — no new `UZ-XXX-NNN` codes | N/A — an absent `failure_reason` is a value, not an error. |
| SCHEMA | no — `core.zombie_events.failure_label` column already exists | N/A — write path only, no DDL. |
| UI / DESIGN TOKEN | no | N/A — no `ui/` files. |

---

## Overview

**Goal (testable):** A failed runner report writes the granular `FailureClass` label to `core.zombie_events.failure_label` **and** increments `zombie_runner_failures_total{runner_id,reason}`, and `zombied`'s in-memory `/metrics` additionally exposes `zombie_runner_executions_total{runner_id,outcome}`, `zombie_runner_last_seen_seconds{runner_id}`, and `zombie_runner_active_leases{runner_id}` — with zero Postgres access on the scrape path and zero runner or wire change beyond one optional report field.

**Problem:** (1) The runner computes a rich `FailureClass` but `loop.zig outcomeFor` collapses it to a binary `Outcome` before reporting, so the control plane records only *that* a run failed, never *why* — `failure_label` is coarse and `/metrics` carries nothing runner-scoped (the M80 cutover removed `zombie_executor_*`). (2) Operators cannot tell which runners are alive, stale, or busy, and scraping can't reach NAT'd/untrusted/customer-host runners — the most failure-prone tier.

**Solution summary:** Push the signal **outbound** on the verbs the runner already calls (report; heartbeat/lease events), never inbound scraping. Slice 1 adds one optional `failure_reason: ?FailureClass` to `ReportRequest`, stops the collapse in `loop.zig`, and has the `report.zig` handler persist the granular label and bump an in-memory per-`runner_id` failure counter. Slice 2 derives three per-runner series from data `zombied` already handles on its own write paths (reports, heartbeats, lease grant/release) — kept **in-memory** so `/metrics` stays DB-free. `zombied` remains the single Prometheus scrape target; per-runner drill-down is a `runner_id` label.

---

## Prior-Art / Reference Implementations

- **Dynamic per-`runner_id` dimension** → `src/zombied/observability/metrics_workspace.zig` (fixed-capacity CAS slot table, overflow→`_other`). Mirror wholesale; the key becomes `(runner_id)` with per-reason / per-outcome sub-counters, or `(runner_id, reason)`.
- **Fixed-label family render** → `metrics_render.zig appendLabeledFamily` + the signup-reason family (`:127`). The `reason` and `outcome` axes are fixed enums and render through this.
- **API** → `docs/REST_API_DESIGN_GUIDELINES.md` + `src/zombied/http/handlers/runner/report.zig` (the handler this extends).
- **Shared contract enum** → `src/lib/contract/execution_result.zig FailureClass` — reuse on the wire; **no parallel enum** (UFS cross-runtime identifier rule).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/lib/contract/protocol.zig` | EDIT | add optional `failure_reason: ?FailureClass = null` to `ReportRequest`. |
| `src/runner/daemon/loop.zig` | EDIT | stop collapsing in `outcomeFor`; thread `result.failure` into the report alongside `outcome`. |
| `src/runner/daemon/control_plane_client.zig` | EDIT | serialize `failure_reason` in `report()`. |
| `src/zombied/http/handlers/runner/report.zig` | EDIT | persist granular `failure_label`; call `incRunnerFailure` + `observeRunnerExecution`. |
| `src/zombied/http/handlers/runner/heartbeat.zig` | EDIT | touch in-memory `last_seen` for the runner slot (Slice 2). |
| `src/zombied/http/handlers/runner/lease.zig` (lease grant + the report/release path) | EDIT | inc/dec in-memory `active_leases` for the runner slot (Slice 2). |
| `src/zombied/observability/metrics_runner.zig` | CREATE | per-`runner_id` slot table: failures, executions, last_seen, active_leases (mirror `metrics_workspace.zig`). |
| `src/zombied/observability/metrics_render.zig` | EDIT | render the four new families (in-memory). |
| `docs/architecture/runner_fleet.md` | EDIT | add the metrics-exposition note + the §3 multi-replica aggregation contract (Architecture Consult & Update Gate). |
| `deploy/grafana/runner_fleet.json` | CREATE | §3 — runner-fleet dashboard; panels aggregate across replicas (`sum` counters, `min` last_seen, best-effort-labelled `sum` active_leases). No `zombied` code. |
| `playbooks/009_grafana_observability/03_dashboard.sh` | EDIT | §3 — generalize the import step to loop **every** `deploy/grafana/*.json` (import + verify each against its own uid + panel count) so `runner_fleet.json` is auto-deployed alongside `agent_run_breakdown.json`, not manual-import (Dimension 3.4). |
| `CHANGELOG.md` | EDIT | §3 — `[Unreleased] → Added`: the four `zombie_runner_*` families + the runner-fleet dashboard (consistent with the existing `agent_run_breakdown.json` entry). |

> Exact handler filenames for heartbeat/lease are confirmed at PLAN by grepping `src/zombied/http/handlers/runner/`; the slot-table key shape is the agent's call against the `metrics_workspace.zig` reference.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** two slices on one PR. Slice 1 is the only contract/runner change (one optional field); Slice 2 is zombied-only. They share the new per-runner counter module, so splitting them would duplicate that module across two PRs — keep together.
- **Alternatives considered:**
  - *Runner `/metrics` endpoint (scrape the runners).* Rejected: inbound scraping can't reach NAT'd/untrusted/customer-host runners (the failure-prone tier), and a listening port on a credential-less, sandbox-hosting daemon is attack surface we don't pay. Outbound push on existing verbs covers the whole fleet. Deferred to a far-future trusted-fleet deep-telemetry slice.
  - *Slice 2 derives the liveness/lease gauges via a Postgres read at render time* (the handoff's literal wording). **Rejected** — the `/metrics` render path is pure in-memory (`mc.snapshot()`); injecting a PG read couples scrape success/latency to DB health, so `/metrics` would fail exactly when the DB is sick (the worst time to lose observability) and would need pg-drain in the scrape path. **Chosen instead:** maintain the gauges in-memory on zombied's own write paths (report/heartbeat/lease), keeping `/metrics` 100% DB-free and consistent with the entire existing observability module. Cost accepted: after a `zombied` restart, `last_seen_seconds` is absent until each runner's next heartbeat and `active_leases` rebuilds as lease events flow — both self-heal within one heartbeat/lease cycle. **← the one design call to confirm with Indy before CHORE(open).**
- **Patch-vs-refactor verdict:** **patch** — additive field + a new self-contained counter module mirroring an existing one; no restructure. The larger "runner deep-telemetry / cpu·mem·disk push" is genuinely separate (Slice 2b) and named in Out of Scope, not mud-patched in here.

---

## Sections (implementation slices)

### §1 — Failure observability (the only runner/contract change)

> **DELIVERED via PR #354** (`feat/m80-006-fleet-plane`), folded in per Indy's Jun 01 2026 decision — §1 completes the `renewal_terminate` reach M80_006 introduced, so it lands in that PR rather than a separate one. M80_007's own PR carries §2 only. See Discovery.

Thread the granular `FailureClass` end-to-end so the durable record and the metric both carry *why* a run failed. **Implementation default:** the wire field is `?FailureClass` (optional) so a mixed-version fleet is safe — an old runner omits it and zombied treats absent as `unknown` (a render-time bucket, **not** a registered `FailureClass` variant — distinct sentinel, per the error-table sentinel rule).

- **Dimension 1.1** — `ReportRequest` carries optional `failure_reason`; round-trips through serialize/deserialize. → Test `test_report_request_failure_reason_roundtrip`
- **Dimension 1.2** — `loop.zig` no longer collapses: a failed execution's `result.failure` reaches the report. → Test `test_loop_threads_failure_class_into_report`
- **Dimension 1.3** — `report.zig` persists the granular value to `core.zombie_events.failure_label`. → Test `test_report_persists_granular_failure_label`
- **Dimension 1.4** — `incRunnerFailure(runner_id, reason)` buckets by reason in the per-runner slot table. → Test `test_inc_runner_failure_buckets_by_reason`
- **Dimension 1.5** — `zombie_runner_failures_total{runner_id,reason}` renders as valid Prometheus exposition. → Test `test_failures_total_render_format`
- **Dimension 1.6** — absent `failure_reason` (old runner) → `reason="unknown"`, never a crash. → Test `test_absent_failure_reason_is_unknown`

### §2 — Per-runner workload + liveness (zombied-only, in-memory)

> **DELIVERED via PR #354** (`feat/m80-006-fleet-plane`), folded in per Indy's Jun 01 2026 decision to complete the feature now (so M80_007 drops off the v2-prod critical path). Both slices ship in that PR; M80_007 has no separate PR. See Discovery.

Three series derived from zombied's own write paths; no runner or wire change. **Implementation default:** in-memory slot table keyed on `runner_id`, per the §Decomposition decision.

- **Dimension 2.1** — every report increments `zombie_runner_executions_total{runner_id,outcome}` (outcome = `processed`|`agent_error`). → Test `test_executions_total_split_by_outcome`
- **Dimension 2.2** — `zombie_runner_last_seen_seconds{runner_id}` = render-time delta from an in-memory last-seen stamp updated on report/heartbeat; a lapsed runner's value climbs. → Test `test_last_seen_seconds_climbs_when_idle`
- **Dimension 2.3** — `zombie_runner_active_leases{runner_id}` inc on lease grant, dec on release/report; a held lease reads 1. **Best-effort** (in-memory): an abandoned lease that expires without a report is not decremented — accepted v1 limitation, see Out of Scope. → Test `test_active_leases_tracks_grant_and_release`
- **Dimension 2.4** — slot-table overflow past capacity routes to `runner_id="_other"`, never drops or crashes. → Test `test_runner_slot_overflow_routes_to_other`

### §3 — Multi-replica (`zombied` N>1) metric correctness — in-memory, no shared store

> **Reopens M80_007** after §1/§2 shipped via PR #354. Forward-looking: prod runs a **single** `zombied` machine today (`deploy/fly/zombied-prod/fly.toml` — machine count 1, enforced by the release pipeline), so all four series are correct as-is. This slice makes them correct *when the control plane scales out*, keeping everything in-memory — **no shared store, no persistence.** The exact/persistent path (refresher) stays deferred (Out of Scope). Decision: Indy, Jun 01 2026 (Discovery).

**Why N>1 breaks the in-memory model:** a runner's verbs (report / heartbeat / lease grant / release) load-balance across replicas, so each replica holds only the slice of a runner's event stream it happened to serve. Prometheus scrapes each replica as a distinct target and auto-labels every series with `instance`. Correctness is therefore an **aggregation property, not a storage one** — and counters/gauges differ:

| Series | Type | Correct cross-replica query | Exact under N>1 in-memory? |
|--------|------|------------------------------|----------------------------|
| `failures_total` | counter | `sum by (runner_id,reason) (zombie_runner_failures_total)` | ✅ exact — additive |
| `executions_total` | counter | `sum by (runner_id,outcome) (…)` | ✅ exact — additive |
| `last_seen_seconds` | gauge | `min by (runner_id) (zombie_runner_last_seen_seconds)` | ✅ exact — most-recent sighting wins; a replica that never saw the runner exposes no series, so `min` ignores it |
| `active_leases` | gauge | `sum by (runner_id) (zombie_runner_active_leases)` | ⚠️ approximate — `+1` grant and `−1` release can land on different replicas, so the value is meaningful only in aggregate and a single-replica restart can transiently over-count |

**Chosen shape:** deliver multi-replica correctness as **aggregation semantics + visualization**, not new `zombied` code. The write paths and in-memory tables are unchanged from §1/§2; the Prometheus-added `instance` label already separates replicas. No code touches `src/`.

- **Dimension 3.1** — ✅ **DONE** — runner-fleet Grafana dashboard `deploy/grafana/runner_fleet.json` whose panels use the aggregations above (`sum` for counters, `min` for `last_seen`, `sum` for `active_leases`). Verified: JSON parses; every `zombie_runner_*` metric + `runner_id`/`reason`/`outcome` label in the dashboard matches `metrics_runner.zig` verbatim. Live import against a 2-replica stack is an operator acceptance check (no Zig surface to unit-test).
- **Dimension 3.2** — ✅ **DONE** — `docs/architecture/runner_fleet.md` gains an **Observability** section: the pull (Fly-managed Prometheus scrapes `:9091/metrics`) vs push (OTel logs/traces) split, the four families + in-memory DB-free render, and the N>1 aggregation contract (exact vs approximate, `instance` as the per-replica axis).
- **Dimension 3.3** — ✅ **DONE** — `active_leases` carries an explicit N>1 caveat: the dashboard panel is titled + described best-effort, the spec Failure Modes has a multi-replica torn inc/dec row, and `runner_fleet.md` names the deferred lease-table refresher as the exact fix.
- **Dimension 3.4** — ✅ **DONE** — the 009 grafana-observability playbook auto-deploys the dashboard: `03_dashboard.sh` now loops every `deploy/grafana/*.json` (import + verify each against its own `uid` + declared panel count) instead of the single hardcoded `agent_run_breakdown.json`, so `runner_fleet.json` is provisioned at priming rather than imported by hand. → Verify: `bash -n` clean; both dashboards' uids (`zombie-run-breakdown`, `zombie-runner-fleet`) covered by the glob. *(Added during CHORE(close) per Indy's "fix both now" review decision — closes the review's playbook-wiring warning.)*

**Why `active_leases` cannot be exact in-memory:** the `+1`/`−1` is a distributed inc/dec with no routing affinity (stateless LB) and no shared counter, so no per-replica view is individually meaningful and `sum` is fragile across restarts. The only exact source is the durable lease table (`fleet.runner_leases`, from M80_006), read by the deferred refresher. **Reading that table to derive a gauge is not "metrics in Postgres"** — the timeseries still lives only in Prometheus; the refresher reads current operational truth off the scrape path. Indy's "no DB for metrics" principle and the eventual exact fix are the *same design, staged* (Discovery, Jun 01 2026).

---

## Interfaces

```
# Contract — additive, backward-compatible
ReportRequest {
  …existing fields (lease_id, event_id, fencing_token, outcome, response_text, tokens, telemetry, checkpoint)…
  failure_reason: ?FailureClass = null   # NEW — granular cause; absent on old runners
}
# FailureClass is the existing shared enum (lib/contract/execution_result.zig); on the wire as its @tagName.

# Exposition (zombied GET /metrics — in-memory, no DB)
zombie_runner_failures_total{runner_id,reason}   counter   # Slice 1
zombie_runner_executions_total{runner_id,outcome} counter  # Slice 2
zombie_runner_last_seen_seconds{runner_id}        gauge     # Slice 2 (render-time delta)
zombie_runner_active_leases{runner_id}            gauge     # Slice 2
# reason ∈ {FailureClass variants} ∪ {"unknown"};  outcome ∈ {processed, agent_error};  runner_id overflow → "_other".
```

Backward-compat contract: the field is optional; old runner ⇒ omitted ⇒ `reason="unknown"`. The frozen `ReportRequest` shape changes only by addition — both binaries rebuild from the same commit.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Old runner, no `failure_reason` | mixed-version fleet | deserialize → `null` → render bucket `reason="unknown"`; report succeeds (`ReportResponse.ok=true`). |
| Unknown/garbage reason on wire | corrupt/forward-version payload | reject the field value to `unknown`, do not fail the whole report; log at warn. |
| Runner cardinality exceeds slot capacity | >4096 distinct `runner_id` | overflow→`runner_id="_other"`; counters keep advancing; no alloc, no crash. |
| Concurrent reports from same runner | parallel executions | lock-free `fetchAdd` on the claimed slot; CAS on first claim (mirror `metrics_workspace.zig`). |
| `zombied` restart | process bounce | counters reset (Prometheus-counter semantics handle it); `last_seen` absent until next heartbeat; `active_leases` rebuilds from lease events — self-heal within one cycle. |
| **Abandoned lease (runner dies/goes dark)** | lease expires by the clock (`lease_expires_at`) with no report — there is no release event | **known limitation of the in-memory approach:** in-memory `active_leases` is only decremented on an explicit report/release, so an abandoned lease leaves the gauge stuck high for that `runner_id`. `active_leases` is therefore **best-effort** until the refresher improvement lands (see Out of Scope). The other three series are unaffected. |
| Report persists label but counter inc races | ordering | persistence is the source of truth; the counter is best-effort telemetry — never block the report on a counter update. |
| **Multi-replica `zombied` (N>1)** | a runner's verbs load-balance across replicas; each replica holds only its slice of the event stream | counters + `last_seen` are exact under the §3 aggregation (`sum` / `min by (runner_id)`); `active_leases` is **approximate** because the `+1` grant and `−1` release can land on different replicas (`sum`-correct in aggregate, fragile across a single-replica restart). Exact `active_leases` awaits the deferred refresher (Out of Scope). Prometheus' per-target `instance` label is the per-replica axis. |

---

## Invariants

1. `/metrics` render performs **zero** database access — enforced by the render path consuming only `mc.snapshot()`-style in-memory snapshots (no `conn`/`pool` in `metrics_render.zig` or the new module); a pg-drain check would otherwise flag a query, so introducing one is caught by `make check-pg-drain`.
2. The wire identifier for a failure reason is exactly `FailureClass`'s `@tagName` — one source enum, no parallel string table (UFS cross-runtime rule; a duplicated literal trips the UFS gate).
3. The per-runner slot table allocates nothing at runtime — process-global fixed-capacity `var`, compile-time sized (mirrors `metrics_workspace.zig`); no deinit path to leak (LIFECYCLE gate).
4. `failure_reason` is optional on the wire — a `ReportRequest` from any runner version deserializes; enforced by the round-trip test (1.1) + absent-field test (1.6).

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_report_request_failure_reason_roundtrip` | `ReportRequest{failure_reason=.oom_kill}` → serialize → deserialize == `.oom_kill`; omitted field → `null`. |
| 1.2 | unit | `test_loop_threads_failure_class_into_report` | a failed `ExecutionResult{failure=.timeout_kill}` produces a report carrying `.timeout_kill`, not just `Outcome.agent_error`. |
| 1.3 | integration | `test_report_persists_granular_failure_label` | POST report w/ `failure_reason=.executor_crash` → `core.zombie_events.failure_label == "executor_crash"`. |
| 1.4 | unit | `test_inc_runner_failure_buckets_by_reason` | two `incRunnerFailure("r1",.oom_kill)` + one `.timeout_kill` → oom=2, timeout=1 for `r1`. |
| 1.5 | unit | `test_failures_total_render_format` | rendered output contains `zombie_runner_failures_total{runner_id="r1",reason="oom_kill"} 2` and a single `# HELP`/`# TYPE`. |
| 1.6 | unit | `test_absent_failure_reason_is_unknown` | report with no `failure_reason` → render shows `reason="unknown"`; no panic. |
| 2.1 | integration | `test_executions_total_split_by_outcome` | 3 processed + 1 agent_error reports for `r1` → `executions_total{outcome="processed"}=3`, `{outcome="agent_error"}=1`. |
| 2.2 | unit | `test_last_seen_seconds_climbs_when_idle` | stamp `last_seen`; advance the clock source by N → rendered delta ≈ N (computed at render, not stored). |
| 2.3 | integration | `test_active_leases_tracks_grant_and_release` | grant lease → `active_leases{r1}=1`; release/report → `0`. |
| 2.4 | unit | `test_runner_slot_overflow_routes_to_other` | claim >capacity distinct ids → the surplus increments `runner_id="_other"`; no crash, no alloc. |

**Regression:** existing `zombie_executor_*`-removal stays removed; the signup-reason family + existing render output unchanged (snapshot the pre-existing exposition). **Idempotency/replay:** a replayed report (same `event_id`) must not double-count `executions_total` — assert single-count on duplicate (`test_replayed_report_counts_once`).

---

## Acceptance Criteria

- [ ] A failed runner report shows the granular reason on `/metrics` and in `failure_label` — verify: `make test-integration` (the 1.3 + 2.x integration tests).
- [ ] `/metrics` issues no DB query — verify: `make check-pg-drain` clean + `grep -n "conn\|pool\|query" src/zombied/observability/metrics_render.zig src/zombied/observability/metrics_runner.zig` returns no query call.
- [ ] `make lint` clean · `make test` passes
- [ ] `make test-integration` passes (HTTP + schema write path)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no file over 350 lines added (new counter module included)

---

## Eval Commands (post-implementation)

```bash
# E1: granular failure visible end-to-end
make test-integration 2>&1 | grep -E "failure_label|runner_failures" && echo "PASS" || echo "FAIL"
# E2: Build — zig build
# E3: Tests — make test
# E4: Lint — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=x86_64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate — git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: /metrics is DB-free — grep -rn "conn\|pool\|\.query(" src/zombied/observability/ | head
```

---

## Dead Code Sweep

N/A — no files deleted. (Additive field + new module; the `zombie_executor_*` removal already happened at the M80 cutover and is not re-touched here.)

---

## Discovery (consult log)

> Empty at creation. Appended as work surfaces consults, skill outcomes, and Indy-acked deferral quotes.

- **Architecture consult (authoring, Jun 01 2026):** `docs/architecture/runner_fleet.md` is silent on the metrics render mechanism; the existing render path is pure in-memory.
- **CTO review (Jun 01 2026):** challenged in-memory-only. Finding — counters (`failures_total`, `executions_total`) are correctly in-memory (no table to read), but gauges (`last_seen_seconds`, `active_leases`) reflect Postgres truth; in particular `active_leases` in-memory **over-counts abandoned leases** because leases expire by the clock (`lease_expires_at`, per `runner_fleet.md:227`) with no release event. The fully-correct shape is in-memory counters + a background PG **refresher** thread feeding an in-memory gauge snapshot (read off the scrape path) — see Out of Scope.
- **Decision (Indy, Jun 01 2026):** ship in-memory for all four series now; refresher is a later improvement. `active_leases` documented as best-effort.
  > Indy (2026-06-01): "I think for now keep it in what ever state like inmemory can be improved later" — context: chose in-memory v1 over the refresher; refresher deferred to Out of Scope, `active_leases` over-count of abandoned leases accepted as a known v1 limitation.
- **Correction (authoring):** handoff claimed `renewal_terminate` is already on `FailureClass`; on `main` it is not → captured as `Depends on: M80_006`.
- **Scope split (Indy, Jun 01 2026):** §1 folded into PR #354 (M80_006 branch) rather than shipping as a separate M80_007 PR — §1 completes the `renewal_terminate` reach that PR introduced (it was merging a `FailureClass` variant that the report path collapsed and never persisted). M80_007's own PR is now §2 only.
  > Indy (2026-06-01): "Yes fold slice 1 into this PR" — context: #354 introduced `renewal_terminate` but dead-ended it; §1 closes that reach where it lives.
  - **§1 implemented + verified on `feat/m80-006-fleet-plane`** (commits `529142fb`, `688fd3b7`): contract field + runner threading + `failure_label` persistence + `metrics_runner` counter + tests. Green on lint-zig, all Zig unit lanes, `make test-integration` (DB+Redis), cross-compile both linux targets, gitleaks. `/review` run pre-push — one finding (failure_label/outcome trust-boundary consistency) fixed in `688fd3b7`.
- **§2 folded in too (Indy, Jun 01 2026):** completed Slice 2 now rather than as a later M80_007 PR, so M80_007 leaves the v2-prod critical path entirely.
  > Indy (2026-06-01): "I want the slice 2 to be completed so i could lower priority on m80_007 for v2 prod move" — context: both slices delivered via #354; M80_007 has no separate PR.
  - **§2 implemented + verified on `feat/m80-006-fleet-plane`** (commit `3a691be0`): `executions_total{runner_id,outcome}` + `last_seen_seconds{runner_id}` + `active_leases{runner_id}` (all in-memory), hooked on report/heartbeat/lease-grant. Tests split to `metrics_runner_test.zig` (FLL gate). Green on lint-zig, zombied unit, full `make test-integration`, cross-compile both linux targets, gitleaks. **Both slices shipped via #354 (merged).**
- **CTO review — multi-replica correctness (Indy + Orly, Jun 01 2026):** §1/§2 shipped, but the in-memory model is only correct under a single `zombied`. Verified prod is single-machine today (`zombied-prod/fly.toml` count 1), so no live bug — but the control plane will scale out. Finding: under N>1, counters (`failures_total`, `executions_total`) are exact via `sum`, `last_seen_seconds` is exact via `min by (runner_id)`, and `active_leases` is only approximate (torn `+1`/`−1` across stateless-LB'd replicas). Decision — **reopen M80_007 with §3** to deliver multi-replica correctness *in-memory* via aggregation semantics (Grafana dashboard + `runner_fleet.md`), zero `zombied` code; the exact/persistent path (lease-table refresher) stays deferred.
  > Indy (2026-06-01): "start as orly CTO M80_007 inmemory and supporting multi zombied first. I would like to figure out persistent approach later since its not wise to do the db save for metrics where its timeseries." — context: §3 added; persistence/refresher deferred; clarified that the eventual refresher *reads* the durable lease table for the gauge and does **not** write timeseries to Postgres, so it does not violate "no DB for metrics."
  - **§3 delivered on `feat/m80-007-multi-replica-metrics`** (commits `68f1dfbd` CHORE(open), `d3797469` deliverables): `deploy/grafana/runner_fleet.json` (panels bake in `sum`/`min`/best-effort-`sum` aggregations; `active_leases` panel titled + described approximate under N>1) + `runner_fleet.md` Observability section (pull-vs-push split, four families, in-memory DB-free render + ~0.7 MB fixed footprint, N>1 aggregation contract, deferred refresher). **No `src/` surface** — prod is single-replica today; correctness is delivered as PromQL aggregation + docs. Verified: dashboard JSON parses; every `zombie_runner_*` metric + `runner_id`/`reason`/`outcome` label matches `metrics_runner.zig` verbatim; gitleaks clean; HARNESS VERIFY all gates green. `/write-unit-test` N/A (no code surface); `/review` self-pass on the docs+dashboard diff.
  - **ID-collision flagged, NOT resolved here:** `runner_fleet.md` Roadmap reserves `M80_007 = SCHEDULER` (placement / trust-gated placement / autoscale), while this spec is M80_007 = runner observability. A renumber/reconcile is Indy's call — left untouched. See [[project_m80_007_id_collision_placement]].

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | audits diff coverage vs the Test Specification | clean; iteration count + coverage in Discovery |
| After tests pass, before CHORE(close) | `/review` | adversarial diff review vs this spec, `runner_fleet.md`, REST guide, ZIG_RULES, Failure Modes, Invariants | clean OR every finding dispositioned |
| After `gh pr create` | `/review-pr` | review-comments the open PR | comments addressed before human review/merge |

Indy additionally requested an **independent Orly CTO review** of the branch + PR after `/review`.

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | {paste} | |
| Integration tests | `make test-integration` | {paste} | |
| /metrics DB-free | `make check-pg-drain` | {paste} | |
| Lint | `make lint` | {paste} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |

---

## Out of Scope

- **Gauge refresher — THE DEFERRED PERSISTENT APPROACH (exact `active_leases`, restart-resilient, multi-replica truth).** Replace in-memory write-path maintenance of `last_seen_seconds` / `active_leases` with a background thread (read-only; reuse the `approval_gate_sweeper.zig` *thread scaffolding* — interval loop + `sleepInterruptible` + shutdown join — but a refresher is read-into-cache, **not** a sweeper, so name it e.g. `runner_metrics_refresher`). It polls Postgres every ~15s for `last_seen_at` and `count(active leases WHERE lease_expires_at > now())`, overwrites an in-memory snapshot, and `/metrics` renders that snapshot — keeping the scrape path DB-free while making `active_leases` exact (no abandoned-lease over-count, no torn cross-replica inc/dec) and restart-resilient. **This is not "metrics in Postgres":** it *reads* the already-durable lease table (`fleet.runner_leases`, M80_006) to derive a gauge; the timeseries still lives only in Prometheus — so it satisfies "no DB for metrics timeseries." Deferred per Indy's "figure out persistent approach later" decision (Discovery, Jun 01 2026); §3 makes counters + `last_seen` multi-replica-correct in-memory now, this slice closes `active_leases`.
- **Slice 2b** — cpu/mem/disk per-runner telemetry + the `HeartbeatRequest` body that carries it (the heartbeat-wire change lands there, not here; premature while there is no runner-only data to carry — NLG).
- **Slice 3** — cordon/drain + operator admin API → its own operator-plane spec.
- **Runner `/metrics` endpoint** — trusted-fleet deep telemetry; far future.
- **Per-agent identity health / rogue-agent detection** — needs per-zombie dimensions; analysis layer, later.
