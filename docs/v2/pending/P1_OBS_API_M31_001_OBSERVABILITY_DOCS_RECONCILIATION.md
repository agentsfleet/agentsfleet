# M31_001: Observability Docs Reconciliation

**Prototype:** v0.25.0
**Milestone:** M31
**Workstream:** 001
**Date:** Apr 21, 2026
**Status:** PENDING
**Priority:** P1 — Operator-facing docs are out of sync with code; false observability expectations
**Batch:** B4 — cleanup
**Branch:** feat/v0.25.0-clerk-signup-changelog (in `~/Projects/docs`)
**Depends on:** M15_002 (zombie observability implementation), M12_001 (Langfuse removal), M29_001 (doc rewrite to zombie)

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

**Solution:** Edit four docs files in `~/Projects/docs/operator/observability/`. No code changes.
This spec lives in the usezombie repo (milestone tracking); changes land in the docs repo on
`feat/v0.25.0-clerk-signup-changelog`.

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

## §2 — metrics.mdx: Add Operational Caveats

**Status:** PENDING

Add a "Caveats" section documenting two operational gaps that operators need to know about.

**Dimensions:**

- 2.1 Add "Per-workspace metrics not yet wired" caveat
  - target: `metrics.mdx:134-142` (per-workspace section)
  - expected: Add callout box or note: "As of v0.25.0, the per-workspace helper functions
    (`wsAddTokens`, `wsIncGateRepairLoops`) are implemented but not called from production
    code. The counters `zombie_agent_tokens_by_workspace_total` and
    `zombie_gate_repair_loops_by_workspace_total` will read zero until the event loop is
    wired to call them. Alerting on these metrics is premature."
  - test_type: manual review

- 2.2 Add "OTLP export drops histogram series" caveat
  - target: `metrics.mdx:88-96` (OTEL export section) or new "Caveats" section
  - expected: Add note: "The OTLP/HTTP JSON exporter (`otel_export.zig`) converts Prometheus
    text to OTLP JSON but explicitly skips `_bucket`, `_sum`, and `_count` series. Histograms
    (`zombie_execution_seconds`, `zombie_agent_duration_seconds`,
    `zombie_executor_agent_duration_seconds`) are not forwarded to the OTLP endpoint.
    Only counter and gauge values reach the collector. For histogram data, scrape `/metrics`
    directly with Prometheus."
  - test_type: manual review

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
