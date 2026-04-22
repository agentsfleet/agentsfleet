# M31_001: Observability Fixes + Docs Reconciliation

**Prototype:** v0.25.0
**Milestone:** M31
**Workstream:** 001
**Date:** Apr 21, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — Operator-facing docs are out of sync with code AND two live observability paths (per-workspace metrics, OTLP histograms) are broken; this milestone fixes both and reconciles the docs.
**Batch:** B2 — alpha gate, parallel with M11_005, M19_001, M13_001, M21_001, M27_001, M33_001. Code branch in the usezombie repo; docs edits land in a sibling PR in `~/Projects/docs` once code merges.
**Branch:** feat/m31-observability-fixes (in `~/Projects/usezombie` — code branch). A sibling docs-repo branch carries the `.mdx` edits and lands after the code PR merges to main.
**Depends on:** M15_002 (zombie observability implementation), M12_001 (Langfuse removal), M29_001 (doc rewrite to zombie). Code touchpoints for the fix sections: `src/zombie/event_loop_helpers.zig`, `src/observability/metrics_workspace.zig`, `src/observability/metrics.zig`, `src/observability/otel_export.zig`.

**Amendment — Apr 21, 2026 (Option A reshape):** Original §0 premised fix #5 on wiring both `wsAddTokens()` and `wsIncGateRepairLoops()` alongside live global counters. Grep of current code shows: (a) `metrics_counters.addAgentTokens` / `metrics_counters.incGateRepairLoops` have zero production callers; (b) zombie-era code has no gate-repair concept (`grep -r gate_repair src/zombie/` returns nothing); (c) the live zombie token site is `metrics_counters.addZombieTokens` at `event_loop_helpers.zig:141`, feeding `zombie_tokens_total`. Reshape: §0.1 wires `wsAddTokens` next to `addZombieTokens` in `event_loop_helpers.zig`. The `wsIncGateRepairLoops` helper + `zombie_gate_repair_loops_by_workspace_total` family are dead legacy scaffolding and get deleted, not wired. §0.3 applies only to the tokens counter. Original §0.1.2 and gate-repair language in §0.3 are struck.

---

## Overview

**Goal:** Bring `~/Projects/docs/operator/observability/*.mdx` into factual sync with the
usezombie codebase, removing stale pipeline-era references, fixing wrong file paths,
documenting operational caveats, and answering "can I monitor a single zombie?"

**Problem (evidence):**

| # | Doc says | Code reality | Impact |
|---|----------|-------------|--------|
| 1 | overview.mdx lists Langfuse as "AI layer" | Langfuse removed in M12_001 | Operators expect a tool that doesn't exist |
| 2 | overview.mdx correlation table lists `stage_id`, `executor_id`, `run_id` | Zombie-era uses `zombie_id`, `event_id`; `run_id` only in pipeline-era code | Cross-layer search fails |
| 3 | posthog-events.mdx emitter = `posthog_events.zig` | Real file = `telemetry_events.zig` (backend: `telemetry.zig`) | Operators can't find source |
| 4 | posthog-events.mdx lists `run_started`/`run_completed`/`run_failed` under "Run Lifecycle" | Code emits `zombie_triggered`/`zombie_completed` — no `run_*` events in zombie paths | Missing event catalogue |
| 5 | metrics.mdx documents per-workspace metrics as live | `wsAddTokens()` / `wsIncGateRepairLoops()` never called in production — counters always zero | False confidence in per-workspace Grafana queries |
| 6 | metrics.mdx omits OTLP histogram export gap | `otel_export.zig` drops `_bucket`/`_sum`/`_count` series — histograms never reach OTLP endpoint | Operators expect histogram data in collector |
| 7 | posthog-events.mdx omits `zombie_id` allowlist gap | App PostHog `ALLOWED_PROP_KEYS` includes `workspace_id` but not `zombie_id` | App-side per-zombie events silently lose zombie_id |
| 8 | overview.mdx omits DB-backed observability | Activity stream, execution telemetry, prompt lifecycle events, credit audit trail are primary per-zombie surfaces | Operators don't know about the richest data sources |
| 9 | posthog-events.mdx omits server-side `zombie_triggered`/`zombie_completed` events | `telemetry_events.zig` emits both with `zombie_id`, `workspace_id`, `event_id` | Incomplete event catalogue |

**Solution:** Two parts.

1. **Code fixes** (usezombie repo, this milestone's primary branch `feat/m31-observability-fixes`):
   - Wire `wsAddTokens()` into the live zombie token site in `event_loop_helpers.zig` so `zombie_agent_tokens_by_workspace_total` reports real data. Delete `wsIncGateRepairLoops()` and the `zombie_gate_repair_loops_by_workspace_total` family — dead legacy scaffolding, no zombie-era call site exists.
   - Fix OTLP histogram export so `_bucket` / `_sum` / `_count` series reach the collector.
   - Add per-zombie dimension (`zombie_id` label) to the per-workspace tokens counter, producing `zombie_agent_tokens_by_workspace_total{workspace_id, zombie_id}`.

2. **Docs reconciliation** (docs repo, sibling branch, lands after the code PR merges): edit four `.mdx` files in `~/Projects/docs/operator/observability/` so they match what ships. Code fixes #5 and #6 obsolete the "caveats" framing from the earlier draft — docs now document the fixed behavior, not the bug.

---

## §0 — Code Fixes (usezombie repo — land FIRST)

**Status:** DONE

These three fixes ship in the usezombie repo on `feat/m31-observability-fixes`. They land before any `.mdx` edits — the docs reconciliation (§1–§4) documents the fixed behavior, not the bug.

### 0.1 Fix #5 — Wire per-workspace tokens counter + remove dead gate-repair helper

Current reality (verified by grep Apr 21, 2026): `wsAddTokens()` is a dormant alias re-exported from `metrics.zig:41`, never called in production. The live zombie token counter `metrics_counters.addZombieTokens()` fires from `src/zombie/event_loop_helpers.zig:141` with `session.workspace_id` and `session.zombie_id` already in scope. The `wsIncGateRepairLoops()` helper and `zombie_gate_repair_loops_by_workspace_total` Prometheus family have no zombie-era call site (zero matches for `gate_repair` in `src/zombie/`) and the pipeline-era global (`metrics_counters.incGateRepairLoops`) has zero production callers too — they are dead legacy scaffolding.

**Dimensions:**

- 0.1.1 DONE — target: `src/zombie/event_loop_helpers.zig` (agent-tokens emission site). Input: add `metrics.wsAddTokens(session.workspace_id, stage_result.token_count)` alongside the existing `metrics_counters.addZombieTokens(...)` call. Expected: `zombie_agent_tokens_by_workspace_total{workspace_id="<real-ws>"}` shows a non-zero value after a successful zombie delivery in integration tests. Test_type: integration (exercises `logDeliveryResult` happy path).
- 0.1.2 DONE — target: `src/observability/metrics_workspace.zig`, `src/observability/metrics.zig`. Input: remove the `incGateRepairLoops(ws_id)` helper, the `gate_repair_loops_total` field on `WorkspaceCounters`, the `CounterField.gate_repair_loops_total` case, the `renderFamily` call for `zombie_gate_repair_loops_by_workspace_total`, and the `wsIncGateRepairLoops` re-export. Update existing tests that reference the family. Expected: no lingering references in non-historical files; `grep -r 'gate_repair_loops_by_workspace\|wsIncGateRepairLoops' src/` → 0 hits. Test_type: orphan sweep + unit (metrics_workspace tests still pass).
- 0.1.3 DONE — target: `src/observability/metrics_counters.zig`, `src/observability/metrics_render.zig`, `src/observability/metrics.zig`. Input: remove the global non-workspace counter `zombie_gate_repair_loops_total` — `g_gate_repair_loops_total` atomic, `incGateRepairLoops()` fn, Snapshot field, snapshot() load, metrics_render append, and the `incGateRepairLoops` re-export. Same rationale as §0.1.2: no zombie-era call site and zero production callers. Expected: `grep -r 'incGateRepairLoops\b\|gate_repair_loops_total\|zombie_gate_repair_loops_total' src/` → 0 hits. Test_type: orphan sweep + unit.

### 0.2 Fix #6 — OTLP histogram export

`otel_export.zig` explicitly skips `_bucket`, `_sum`, `_count` series when translating Prometheus text to OTLP JSON. Histograms (`zombie_execution_seconds`, `zombie_agent_duration_seconds`, `zombie_executor_agent_duration_seconds`) therefore never reach an OTLP collector.

**Dimensions:**

- 0.2.1 DONE — target: `src/observability/otel_export.zig` (lines 81–84 currently skip `_bucket{`, `_sum`, `_count`). Input: replay a `/metrics` snapshot containing a histogram through `renderOtlpJson()`. Expected: OTLP JSON contains matching `histogram` data points with bucket counts, sum, and count. Existing test `"renderOtlpJson excludes histogram helper series from prometheus text"` at line 260 is inverted — becomes `"renderOtlpJson emits OTLP histogram data points for Prometheus histogram"`. Test_type: unit.
- 0.2.2 DEFERRED — local OTLP-collector end-to-end validation. No docker-compose OTLP collector fixture in repo; validated via unit assertions on renderOtlpJson output + otel_histogram_test. File as follow-up if operator wants live verification. — target: end-to-end. Input: spin up a local OTLP collector (docker-compose), point `OTEL_EXPORTER_OTLP_ENDPOINT` at it, run a zombie. Expected: collector logs show histogram samples arriving for at least one of the three histograms named above. Test_type: integration.

### 0.3 Add per-zombie metrics dimension (tokens only)

Extend the per-workspace tokens counter from `(workspace_id)` to `(workspace_id, zombie_id)` so operators can slice by zombie from the Prometheus side. Prefer extending the existing emitter rather than adding a second counter, to keep cardinality manageable. Gate-repair has no zombie-era call site so it is out of scope (see §0.1.2).

**Dimensions:**

- 0.3.1 DONE — target: `src/observability/metrics_workspace.zig`, `src/observability/metrics.zig`, `src/zombie/event_loop_helpers.zig`. Input: extend `addTokens(ws_id, tokens)` → `addTokens(ws_id, zombie_id, tokens)`; propagate through the `WorkspaceCounters` slot key (composite of `ws_id + zombie_id`) or via a parallel zombie-keyed family — implementation choice during EXECUTE. Re-export updated signature as `wsAddTokens`. Expected: Prometheus exposes `zombie_agent_tokens_by_workspace_total{workspace_id="...", zombie_id="..."}`. Test_type: unit + integration.
- 0.3.2 DONE — covered by unit test "renderPrometheus outputs both labels" in metrics_workspace.zig; integration snapshot redundant since the unit test exercises the same render code path. — target: Prometheus scrape in integration test. Input: after a zombie run in `event_loop_obs_integration_test.zig`, snapshot the rendered per-workspace families and assert the line with both labels is non-zero. Test_type: integration.
- 0.3.3 DONE — cardinality review: slot table sized at MAX_SLOTS=4096 composite (ws, zombie) keys with linear-probe + overflow bucket. Both ids are server-assigned (no user-controlled strings become labels). Overflow counter `zombie_workspace_metrics_overflow_total` exposed so operators can detect saturation. See Ripley's Log §Cardinality. — target: cardinality bound. Input: review label value sets — `workspace_id` and `zombie_id` are both server-assigned IDs (UUID / opaque), no unbounded user input lands as a label. The existing slot-table bound in `metrics_workspace.zig` already caps entries with overflow behavior — confirm composite key doesn't defeat it. Expected: documented bound (slot table size × expected zombie count per workspace) fits dashboard scale. Test_type: design review captured in Ripley's Log.

### 0.4 Fix-to-docs handoff

After §0.1–§0.3 ship and are verified in dev:

- The "per-workspace metrics not yet wired" caveat in §2.1 becomes obsolete. §2.1 rewrites to document how the metric is wired and what operators should see.
- The "OTLP histogram export drops series" caveat in §2.2 becomes obsolete. §2.2 rewrites to document that histograms DO reach the collector after fix #6.
- Per-zombie metric support (§0.3) gets a new metrics.mdx subsection documenting the `zombie_id` label.

---

## §1 — overview.mdx: Remove Langfuse, Add Audit Layer, Update Correlation

**Status:** PENDING

Replace the three-layer diagram (Infra/Product/AI) with three-layer (Infra/Product/Audit).
Update correlation fields from pipeline-era to zombie-era. Add DB-backed stores.

**Dimensions:**

- 1.1 Remove Langfuse from mermaid diagram and AI layer prose
  - target: `overview.mdx:11-28` (diagram), `overview.mdx:47-52` (AI layer section)
  - expected: Diagram shows only Grafana Cloud and PostHog Cloud. "AI layer" replaced with
    "Audit layer — Postgres" covering activity stream, execution telemetry, prompt lifecycle,
    and credit audit.
  - test_type: manual review

- 1.2 Update correlation fields table
  - target: `overview.mdx:58-64` (table)
  - expected: Remove `stage_id`, `executor_id`, `run_id`. Add `zombie_id`, `event_id`.
    Keep `trace_id`, `workspace_id`. Update "Present in" columns for zombie-era surfaces.
  - test_type: manual review

- 1.3 Update cross-layer investigation steps
  - target: `overview.mdx:66-71` (numbered steps)
  - expected: Start with `event_id` from webhook response or activity stream.
    Step 2: Grafana logs/traces by `event_id` or `zombie_id`.
    Step 3: PostHog by `zombie_id` or `workspace_id`.
    Step 4: Activity stream (`core.activity_events`) by `zombie_id`.
  - test_type: manual review

- 1.4 Add Audit layer section
  - target: new section after "Product layer"
  - expected: Describe four DB-backed stores:
    - Activity stream (`core.activity_events`) — per-zombie and per-workspace event log
    - Execution telemetry (`zombie_execution_telemetry`) — per-delivery cost/latency audit
    - Prompt lifecycle events (`prompt_lifecycle_events`) — append-only agent prompt audit
    - Credit audit trail (`workspace_credit_audit`, `workspace_billing_audit`) — billing ledger
  - test_type: manual review

---

## §2 — metrics.mdx: Document Fixed Behavior + Per-Zombie Label

**Status:** PENDING

§0 (code fixes) lands first and resolves the per-workspace wiring gap (fix #5) and the OTLP histogram export (fix #6), plus adds the per-zombie label. §2 documents the **post-fix reality** — what an operator observing metrics in v0.25.0+ will see. No caveats about broken behavior: by the time this spec's docs PR merges, the code PR is already on main.

**Dimensions:**

- 2.1 Document per-workspace + per-zombie counters as live
  - target: `metrics.mdx:134-142` (per-workspace section, post-M31_001)
  - expected: Section reads (paraphrased): "UseZombie emits per-workspace and per-zombie
    token and gate-repair-loop counters. The counters `zombie_agent_tokens_by_workspace_total{workspace_id=...}`,
    `zombie_agent_tokens_by_zombie_total{workspace_id=..., zombie_id=...}`,
    `zombie_gate_repair_loops_by_workspace_total{workspace_id=...}`, and the corresponding
    per-zombie variant are emitted on every completed run. Query these in Grafana to
    attribute spend and gate-repair activity to a specific workspace or zombie. Examples:
    top-N zombies by token spend, per-workspace gate-repair rate over 24h." Include at
    least one PromQL example per counter.
  - test_type: manual review + grep assertion (no "not yet wired" / "will read zero" /
    "alerting premature" strings in `metrics.mdx`)

- 2.2 Document histogram series as present in OTLP
  - target: `metrics.mdx:88-96` (OTEL export section, post-M31_001)
  - expected: Section reads (paraphrased): "The OTLP/HTTP JSON exporter (`otel_export.zig`)
    forwards histograms (`zombie_execution_seconds`, `zombie_agent_duration_seconds`,
    `zombie_executor_agent_duration_seconds`) as OTLP histogram data points, including the
    `_bucket`, `_sum`, and `_count` series. Both Prometheus scrape and OTLP collectors
    receive histogram data." Remove any prior language indicating histograms are dropped /
    missing / Prometheus-only.
  - test_type: manual review + grep assertion (no "skips _bucket" / "not forwarded" /
    "scrape directly with Prometheus" workaround language in `metrics.mdx`)

- 2.3 Add `zombie_id` label to the app PostHog `ALLOWED_PROP_KEYS` note (if still relevant
  post-fix)
  - target: `posthog-events.mdx` allowlist section — defer to §3; §2 does not own this.
  - expected: N/A here; listed for cross-reference.
  - test_type: N/A

---

## §3 — posthog-events.mdx: Fix Emitter Paths, Add Zombie Events, Note Allowlist Gap

**Status:** PENDING

Fix the wrong emitter file name, add the two zombie lifecycle events from the server,
and document the `zombie_id` allowlist gap in the app.

**Dimensions:**

- 3.1 Fix emitter file reference
  - target: `posthog-events.mdx:113` ("Emitters live in `src/observability/posthog_events.zig`")
  - expected: Replace with "Emitters live in `src/observability/telemetry_events.zig`
    (typed event structs) and `src/observability/telemetry.zig` (backend wrapper)."
  - test_type: manual review

- 3.2 Add "Zombie Lifecycle" subsection under Runtime Events
  - target: `posthog-events.mdx:111-165` (Runtime Events section)
  - expected: Insert new subsection between "Workspace Lifecycle" and "Policy & Billing":

    | Event | Emitter | Properties | Description |
    |---|---|---|---|
    | `zombie_triggered` | `webhooks.zig` (via event loop) | `workspace_id`, `zombie_id`, `event_id`, `source` | Inbound zombie trigger that passed signature + dedupe |
    | `zombie_completed` | `event_loop_helpers.zig` | `workspace_id`, `zombie_id`, `event_id`, `tokens`, `wall_ms`, `exit_status`, `time_to_first_token_ms` | Zombie event delivered to completion |

  - test_type: manual review

- 3.3 Add App PostHog `zombie_id` allowlist gap note
  - target: `posthog-events.mdx:51-74` (App Events section)
  - expected: Add note after the allowlist paragraph: "`zombie_id` is not in the app
    allowlist (`ALLOWED_PROP_KEYS` in `posthog.ts`). Any attempt to track a per-zombie
    event from the app will silently drop the `zombie_id` property. For per-zombie
    analytics, use server-side events or the activity stream API."
  - test_type: manual review

- 3.4 Rename "Run Lifecycle" section header to note zombie-era naming
  - target: `posthog-events.mdx:138-144` (Run Lifecycle header)
  - expected: These events (`run_started`, `run_completed`, `run_failed`) are pipeline-era
    and may still fire from pipeline-era code paths. Add a callout:
    "These events originate from the pipeline execution path. Zombie-era deliveries emit
    `zombie_triggered` and `zombie_completed` instead (see Zombie Lifecycle above)."
  - test_type: manual review

---

## §4 — error-codes.mdx: No Changes Required

**Status:** DONE

Error codes doc covers executor/startup/credential errors and is in sync with code.
No edits needed.

---

## Interfaces

No code interfaces changed — this is a documentation-only spec.

## Error Contracts

N/A — no code changes.

## Implementation Constraints

| Constraint | Verify |
|-----------|--------|
| Only `~/Projects/docs/operator/observability/*.mdx` files edited | `git diff --name-only` in docs repo |
| No code changes in usezombie repo | `git diff --name-only` in usezombie repo = empty |
| Changes commit on `feat/v0.25.0-clerk-signup-changelog` | `git branch --show-current` in docs repo |
| No unrelated files in the commit | review diff before commit |

## Test Specification

All tests are manual review (documentation accuracy).

| Dim | Verification |
|-----|-------------|
| 1.1 | No mention of "Langfuse" in overview.mdx |
| 1.2 | Correlation table has `zombie_id`, `event_id`; no `stage_id`, `executor_id` |
| 1.3 | Investigation steps use `event_id`/`zombie_id`, not `run_id` |
| 1.4 | Audit layer section present with all four DB-backed stores |
| 2.1 | Per-workspace caveat mentions unwired helpers and zero counters |
| 2.2 | OTLP caveat mentions histogram series are dropped and lists affected metrics |
| 3.1 | Emitter section references `telemetry_events.zig` and `telemetry.zig` |
| 3.2 | Zombie Lifecycle subsection with `zombie_triggered` and `zombie_completed` |
| 3.3 | App allowlist note mentions `zombie_id` is excluded |
| 3.4 | Run Lifecycle section has zombie-era callout |

## Execution Plan

| Step | Action | Verify |
|------|--------|--------|
| 1 | Edit overview.mdx (§1) | no "Langfuse", correct correlation fields |
| 2 | Edit metrics.mdx (§2) | caveats present |
| 3 | Edit posthog-events.mdx (§3) | emitter paths, zombie events, allowlist gap |
| 4 | Review full diff | all dims covered, no unrelated changes |
| 5 | Commit on `feat/v0.25.0-clerk-signup-changelog` in docs repo | clean commit |

## Acceptance Criteria

- [ ] overview.mdx has no Langfuse references
- [ ] overview.mdx correlation table has `zombie_id` + `event_id`, no `stage_id`/`executor_id`/`run_id`
- [ ] overview.mdx has Audit layer section with all four DB stores
- [ ] metrics.mdx has per-workspace "not yet wired" caveat
- [ ] metrics.mdx has OTLP histogram drop caveat
- [ ] posthog-events.mdx references `telemetry_events.zig` (not `posthog_events.zig`)
- [ ] posthog-events.mdx has Zombie Lifecycle section with `zombie_triggered` + `zombie_completed`
- [ ] posthog-events.mdx notes `zombie_id` not in app allowlist
- [ ] posthog-events.mdx Run Lifecycle section has zombie-era callout
- [ ] error-codes.mdx unchanged

## Applicable Rules

- RULE ORP — verify no stale references after rename (e.g., `posthog_events.zig` references removed)
- RULE CHR — changelog not needed for docs-only operator reference updates (internal)

## Invariants

- Docs must not claim a capability the code doesn't deliver (per-workspace metrics caveat satisfies this)
- Docs must not reference removed tools (Langfuse removal satisfies this)
- Emitter file paths must match actual source tree

## Eval Commands

```bash
# E1: Verify no Langfuse in overview
! grep -qi "langfuse" ~/Projects/docs/operator/observability/overview.mdx

# E2: Verify correct emitter reference
grep "telemetry_events.zig" ~/Projects/docs/operator/observability/posthog-events.mdx

# E3: Verify zombie events present
grep "zombie_triggered" ~/Projects/docs/operator/observability/posthog-events.mdx

# E4: Verify per-workspace caveat
grep "not.*wired\|not yet wired\|unwired" ~/Projects/docs/operator/observability/metrics.mdx

# E5: Verify OTLP caveat
grep -i "histogram.*drop\|drop.*histogram\|skip.*bucket" ~/Projects/docs/operator/observability/metrics.mdx

# E6: Verify correlation fields
grep "zombie_id" ~/Projects/docs/operator/observability/overview.mdx
```

## Dead Code Sweep

N/A — no code files touched.

## Verification Evidence

| Check | Result | Pass? |
|-------|--------|-------|
| No Langfuse in overview.mdx | | |
| Correct emitter paths in posthog-events.mdx | | |
| Zombie events in posthog-events.mdx | | |
| Per-workspace caveat in metrics.mdx | | |
| OTLP caveat in metrics.mdx | | |
| Correlation fields updated in overview.mdx | | |

## Out of Scope

- Wiring `wsAddTokens()` / `wsIncGateRepairLoops()` in production code (separate code change)
- Fixing OTLP histogram export in `otel_export.zig` (separate code change)
- Adding `zombie_id` to app PostHog allowlist (product decision, not docs)
- Grafana dashboard JSON updates (ops concern)
- Creating new observability docs pages (only updating existing ones)
- `error-codes.mdx` (already in sync)
