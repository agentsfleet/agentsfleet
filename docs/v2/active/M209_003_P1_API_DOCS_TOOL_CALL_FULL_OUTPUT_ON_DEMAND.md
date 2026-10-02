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

# M209_003: "Show all" on a tool call returns its full arguments and output — posted by the runner under the lease fence, kept per call, read one call at a time

**Prototype:** v2.0.0
**Milestone:** M209
**Workstream:** 003
**Date:** Oct 02, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing: Indy's ask is that nothing a tool returned stays hidden; the thread shows the edges, this serves the rest
**Categories:** API, DOCS
**Batch:** B1 — after M209_001 in the same Pull Request; the Rust runner posts these records
**Branch:** feat/m209-tool-call-outcomes
**Baseline revision:** 0d79b0318e687b8862ee8dbd4f622069bc6ba1a4
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M209_001 — fenced call ids and the trace this record completes
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 02, 2026) from a source trace on `main`; re-scoped the same day when Indy chose a fresh Rust runner, which moved record capture into the runner
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Live activity (the SSE tail); `docs/architecture/data_flow.md` §The list read and the detail read are different reads

---

## Overview

**Goal (testable):** A runner post of a call whose output is 224 lines stores the record under the lease's fence; `GET …/events/{event_id}/tool-calls/{call_id}` returns all 224 lines and the full arguments to a member; a stale fence, a reclaimed lease's leftovers, another workspace's caller and an unknown call each get no record.
**Problem:** The thread shows a call's first and last lines (M209_001). The rest of the output exists only inside the sandbox and dies with the lease, so "show all" has nothing to open.
**Solution summary:** A fenced runner verb accepts each call's full arguments and output, up to 64 KiB per field. It is kept out of the report, because a large record set beside an uncapped reply could pass the runner routes' 2 MB request limit (`axum-core 0.5.6`; `rustd/crates/afd_api/src/router/mount.rs` sets none for runner routes). The daemon narrows each record, caps an event's total, and upserts into a new child table of `core.fleet_events` keyed by fence and call number. Settlement drops other fences' rows, and a tenant read returns one call.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(api): keep each tool call's full output for "show all"
- **Intent (one sentence):** An operator who wants more than a call's first and last lines can open everything it took and returned, minus secret values.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_fleet/src/lease/memory.rs` — the fence check the record write reuses.
2. `rustd/crates/afd_api_runner/src/handler/runner/memory.rs` — a fenced runner verb end to end.
3. `schema/831_repair_run_results.sql` — the child-table shape: UUIDv7 id, composite foreign key to `core.fleet_events` with cascade.
4. `docs/REST_API_DESIGN_GUIDELINES.md` — §1 naming, §7 the four registration steps and the silent `ALL`-entry trap.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_wire/src/tool_detail.rs`, `rustd/crates/afd_wire/src/tool_detail/tests.rs`, `rustd/crates/afd_wire/src/lib.rs` | CREATE / EDIT | Request, response and bounds, shared with the Rust runner; each record carried raw and narrowed on its own |
| `rustd/crates/afd_http/src/route/runner.rs`, `rustd/crates/afd_http/src/route/fleet.rs`, `rustd/crates/afd_http/src/openapi/path.rs`, `rustd/crates/afd_http/src/services/leasing.rs`, `rustd/crates/afd_http/src/services/event.rs` | EDIT | Variant, `ALL` entry and `meta()` for both routes; the call path's parameters; one seam method each for the write and the read |
| `rustd/crates/afd_api_runner/src/handler/runner/tool_call.rs`, `rustd/crates/afd_api_runner/src/handler/runner/mod.rs`, `rustd/crates/afd_api_runner/src/lib.rs`, `rustd/crates/afd_api_runner/src/openapi.rs` | CREATE / EDIT | Runner verb: post cap, then fence, narrow, cap, upsert |
| `rustd/crates/afd_fleet/src/lease/tool_detail.rs`, `rustd/crates/afd_fleet/src/lease/tool_detail/store.rs`, `rustd/crates/afd_fleet/src/lease/tool_detail/tests.rs`, `rustd/crates/afd_fleet/src/lease/sql/tool_detail.rs`, `rustd/crates/afd_fleet/src/lease/sql/mod.rs`, `rustd/crates/afd_fleet/src/lease/mod.rs`, `rustd/crates/afd_fleet/src/lease/commit.rs` | CREATE / EDIT | Fenced store; settlement drops other fences' rows in the settling transaction |
| `rustd/crates/afd_events/src/history/tool_call.rs`, `rustd/crates/afd_events/src/history/mod.rs`, `rustd/crates/afd_events/src/lib.rs`, `rustd/crates/afd_api_tenant/src/handler/event/tool_call.rs`, `rustd/crates/afd_api_tenant/src/handler/event/tool_call/tests.rs`, `rustd/crates/afd_api_tenant/src/handler/event/mod.rs`, `rustd/crates/afd_api_tenant/src/lib.rs`, `rustd/crates/afd_api_tenant/src/openapi.rs` | CREATE / EDIT | Tenant read of one call, scoped in its statement beside the other event reads |
| `rustd/crates/afd_core/src/error_code/fleet.rs`, `rustd/crates/afd_core/src/error_code.rs`, `rustd/crates/afd_core/src/problem/fleet.rs`, `rustd/crates/afd_core/tests/error_code.rs` | EDIT | `TOOL_CALL_NOT_FOUND` (`UZ-AGT-017`) beside `EVENT_NOT_FOUND` |
| `schema/925_fleet_tool_call_details.sql`, `rustd/crates/afd_db/src/migration.rs` | CREATE / EDIT | New table, grants, registration |
| `public/openapi.json`, `rustd/crates/afd_api/src/openapi/stability.rs` | EDIT | Regenerated; the new response fields are `x-stability: beta` |
| `rustd/crates/afd_api/tests/fleet_tool_calls.rs`, `rustd/crates/afd_api/tests/tenant_plane_suite.rs`, `rustd/crates/afd_api/tests/runner_plane/satellites.rs`, `rustd/crates/afd_api/tests/harness/stubs_runner.rs`, `rustd/crates/afd_api/tests/route_inventory.rs`, `rustd/crates/afd_api/tests/route_meta_total.rs`, `rustd/crates/afd_api/tests/router.rs` | CREATE / EDIT | Router proofs with no datastore; the route roster, count and mount matcher move with the two routes |
| `rustd/crates/agentsfleetd/tests/daemon_suite.rs`, `rustd/crates/agentsfleetd/tests/integration_tool_call_details.rs`, `rustd/crates/agentsfleetd/tests/integration_tool_call_details_lifecycle.rs`, `rustd/crates/agentsfleetd/tests/integration_tool_trace.rs` | EDIT / CREATE | Runner-shaped posts and every integration proof; the trace suite's tenant helpers become shared |
| `docs/architecture/data_flow.md`, `docs/architecture/runner_fleet.md` | EDIT | Name the table once `schema/` defines it |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS + TFX (caps are constants shared with tests and the runner), NTP (each record narrowed at the verb), OBS, SGR (the `CREATE TABLE` ends with grants), STS, NSQ, MIG, ITF, ERR (the new code is declared and referenced), TST-NAM, TCF.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md`, `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` (UUIDv7 id and its CHECK, `BIGINT` milliseconds), `dispatch/write_http.md` + `docs/REST_API_DESIGN_GUIDELINES.md` (§1, §5 registry codes, §7, §9 `x-stability`), `docs/LOGGING_STANDARD.md`.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — new table | Forward `CREATE TABLE IF NOT EXISTS`, grants in the same file, ≤100 lines |
| ERROR REGISTRY | yes | `TOOL_CALL_NOT_FOUND` declared and used by the read |
| UFS / LOGGING / MILESTONE-ID | yes | Constants once; `tool_detail_skipped` with ids and counts only |
| Architecture consult | yes | `data_flow.md` already describes the store; this spec adds its table name |
| File & Function Length (≤350/≤50/≤70) | yes — `finalize.rs` | The store lives in its own module |

## Prior-Art / Reference Implementations

- **Reference:** the memory push (`rustd/crates/afd_api_runner/src/handler/runner/memory.rs` → `rustd/crates/afd_fleet/src/lease/memory.rs`) — a fenced, best-effort runner write; this verb copies its shape.
- **Reference:** Codex `~/Projects/oss/rs/codex/codex-rs/core/src/mcp_tool_call.rs` — results over 1 MiB collapse to a preview "to avoid persisting multi-megabyte results"; our per-field and per-event caps make the same trade.

## Sections (implementation slices)

### §1 — The daemon stores records under the fence and settlement keeps only the winner's

`POST /v1/runners/me/leases/{lease_id}/tool-calls` checks the fence as the memory push does and narrows each item. An over-bound item is skipped and counted, and a post that would carry the event past 1 MiB of records stores only what fits. It upserts on `(fleet_id, event_id, fencing_token, call_number)`, so a retried post changes nothing. The settling transaction deletes the event's rows from every other fence, because a reclaimed lease restarts its call numbers at 1.

- **Dimension 1.1** — A valid post stores one row per record → Test `test_tool_call_details_stored_under_fence` — DONE (`agentsfleetd/tests/integration_tool_call_details.rs`)
- **Dimension 1.2** — A stale fence is refused and writes nothing → Test `test_stale_fence_tool_call_details_refused` — DONE (`agentsfleetd/tests/integration_tool_call_details.rs`)
- **Dimension 1.3** — Posting the same batch twice leaves one row per call → Test `test_tool_call_details_retry_is_idempotent` — DONE (`agentsfleetd/tests/integration_tool_call_details.rs`)
- **Dimension 1.4** — An over-bound item is skipped and counted while the rest store → Test `test_over_bound_tool_call_detail_skipped` — DONE (`agentsfleetd/tests/integration_tool_call_details.rs`)
- **Dimension 1.5** — Records past 1 MiB for one event are skipped and counted → Test `test_event_detail_budget_caps_records` — DONE (`agentsfleetd/tests/integration_tool_call_details.rs`; the selection is also unit-proven in `afd_fleet/src/lease/tool_detail/tests.rs`)
- **Dimension 1.6** — Settlement deletes rows written under an older fence → Test `test_settle_drops_dead_lease_details` — DONE (`agentsfleetd/tests/integration_tool_call_details.rs`)
- **Dimension 1.7** — Deleting the fleet deletes its records → Test `test_event_delete_cascades_tool_call_details` — DONE (`agentsfleetd/tests/integration_tool_call_details_lifecycle.rs`, deleting the event row the fleet cascade reaches)
- **Dimension 1.8** — `api_runtime` can insert, update, select and delete records → Test `test_tool_call_details_grants_allow_runtime` — DONE (`agentsfleetd/tests/integration_tool_call_details_lifecycle.rs`)

### §2 — A member reads one call

`GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}/tool-calls/{call_id}` takes the fenced id the thread already holds (`{fence}:{n}`, opaque to clients and percent-encoded in the path). It needs `fleet:read`, returns the record or `TOOL_CALL_NOT_FOUND`, and never says which part was missing.

- **Dimension 2.1** — A member reads a saved call's full arguments and output → Test `test_member_reads_tool_call_detail` — DONE (`agentsfleetd/tests/integration_tool_call_details.rs`, plain and percent-encoded ids)
- **Dimension 2.2** — An unknown call, another workspace's fleet, or a malformed id each answer 404 → Test `test_tool_call_detail_absent_is_not_found` — DONE (`agentsfleetd/tests/integration_tool_call_details.rs`)
- **Dimension 2.3** — Both URLs answer through the mounted router, not a 404 from an unwalked route → Test `test_tool_call_routes_are_mounted` — DONE (`afd_api/tests/fleet_tool_calls.rs` and `runner_plane/satellites.rs`, unit; `route_verbs.rs` and `router.rs` already walk every tabled route)
- **Dimension 2.4** — A caller without `fleet:read` is refused → Test `test_tool_call_detail_requires_fleet_read` — DONE (`afd_api/tests/fleet_tool_calls.rs`, unit: the rung is decided before any datastore)

## Interfaces

```
POST /v1/runners/me/leases/{lease_id}/tool-calls          (runner plane, fenced)
  { "fencing_token": 7,
    "calls": [ { "call_number": 3, "arguments": {"url": "https://…", "method": "GET"},
                 "truncated_arguments": false, "output": "…224 lines…",
                 "output_line_count": 224, "truncated": false } ] }
  200 { "stored_count": 1, "skipped_count": 0 }     refusal as the memory push refuses

GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}/tool-calls/{call_id}
  200 { "call_id": "7:3", "arguments": {…}, "truncated_arguments": false,
        "output": "…", "output_line_count": 224, "truncated": false }
  404 TOOL_CALL_NOT_FOUND

core.fleet_tool_call_details: id UUIDv7 PK + CHECK · workspace_id · fleet_id · event_id
  · fencing_token BIGINT · call_number BIGINT · arguments JSONB · output TEXT
  · output_line_count BIGINT · truncated BOOLEAN · truncated_arguments BOOLEAN · byte_count BIGINT
  · created_at BIGINT · updated_at BIGINT
  FK (fleet_id, event_id) → core.fleet_events ON DELETE CASCADE
  UNIQUE (fleet_id, event_id, fencing_token, call_number); GRANT to api_runtime

afd_wire::tool_detail bounds: DETAIL_FIELD_MAX_BYTES 65536 · DETAIL_EVENT_MAX_BYTES 1048576
  · DETAIL_POST_MAX_BYTES 262144
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Stale fence | Reclaimed lease posts late | Refused, nothing written (Dimension 1.2) |
| Retry | Same batch posted twice | Upsert, no duplicate (Dimension 1.3) |
| Reclaimed lease | Two leases ran the event | Settlement keeps the winner's rows (Dimension 1.6) |
| Runaway output | A run returns megabytes | 64 KiB per field, 1 MiB per event (Dimensions 1.4, 1.5) |
| Record never posted | Runner post failed | The read answers 404; the thread says the output was not kept |
| Wrong caller | Other workspace or missing scope | 404 or refusal with no body (Dimensions 2.2, 2.4) |

## Invariants

1. A record never exceeds `DETAIL_FIELD_MAX_BYTES` per field, nor an event `DETAIL_EVENT_MAX_BYTES` — the daemon re-checks and is the authority (Dimensions 1.4, 1.5).
2. Only the settling fence's records survive settlement (Dimension 1.6).
3. No record outlives its event (Dimension 1.7).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `tool_detail_skipped` (daemon log, info) | ops | An item fails a bound or the event budget | `fleet_id`, `event_id`, position in the post, reason, bytes | No record content | `test_over_bound_tool_call_detail_skipped` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_tool_call_details_stored_under_fence` | live fence, 2 records → 2 rows |
| 1.2 | integration | `test_stale_fence_tool_call_details_refused` | stale token → refused, 0 rows |
| 1.3 | integration | `test_tool_call_details_retry_is_idempotent` | same batch twice → 2 rows |
| 1.4 | integration | `test_over_bound_tool_call_detail_skipped` | one 70 KiB output among 3 → 2 stored, `skipped_count` 1 |
| 1.5 | integration | `test_event_detail_budget_caps_records` | 20 × 64 KiB for one event → 16 stored, 4 skipped |
| 1.6 | integration | `test_settle_drops_dead_lease_details` | rows at fences 6 and 7, settle at 7 → only fence 7 rows |
| 1.7 | integration | `test_event_delete_cascades_tool_call_details` | delete fleet → 0 detail rows |
| 1.8 | integration | `test_tool_call_details_grants_allow_runtime` | `api_runtime` insert/update/select/delete succeed |
| 2.1 | integration | `test_member_reads_tool_call_detail` | member GET → 200 with all 224 lines |
| 2.2 | integration | `test_tool_call_detail_absent_is_not_found` | unknown id, other workspace, `x:y:z` → 404 each |
| 2.3 | unit | `test_tool_call_routes_are_mounted` | both URLs → not the unmounted-route 404 |
| 2.4 | unit | `test_tool_call_detail_requires_fleet_read` | key without `fleet:read` → refused |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Records store under the fence and a member reads one (§1, §2) | `make test-integration-rustd && grep -cE "fn test_(member_reads_tool_call_detail\|settle_drops_dead_lease_details)\(" rustd/crates/agentsfleetd/tests/integration_tool_call_details.rs` | 2 | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md` (successor carries the row, both specs record it, owner's verbatim quote in Discovery); a MOVED row is never ✅.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Building and posting records: scrub, UTF-8-safe cuts, the 1 MiB per-run budget, posting before the report — the Rust runner (`docs/architecture/runner_execution.md`). The Zig runner gets no new code.
- Rendering "show all" — M209_002.
- Full output while a call is still running — a record exists only at completion.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator clicks "show all" under `GET …/deploys/9312` and reads all 224 lines, with the token shown as `«secret:FLY_API_TOKEN»`.
2. **Preserved user behaviour** — The thread, the report and settlement timing are unchanged; a run whose posts all fail settles as before.
3. **Optimal-way check** — A separate fenced verb is the direct path that keeps the report's size and timing safe.
4. **Rebuild-vs-iterate** — Iterate on the memory push's proven shape.
5. **What we build** — One runner verb, one table, one read, one error code.
6. **What we do NOT build** — No live output, no download, no search inside outputs.
7. **Fit with existing features** — Compounds with M209_001's fenced ids; must not slow settlement.
8. **Surface order** — API first; M209_002 opens it.
9. **Dashboard restraint** — "Show all" offers only what was saved; an unsaved call says so instead of spinning.
10. **Confused-user next step** — "Full output wasn't kept for this call" names the gap from the read's 404.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** its own workstream beside M209_001 in the backend Pull Request, because it adds a table and two endpoints with their own review profile.
- **Alternatives considered:** carrying records in the report (rejected: a large record set beside an uncapped reply could pass the 2 MB limit and lose the run's result); one JSONB column on the event (rejected: the thread read would carry megabytes).
- **Patch-vs-refactor verdict:** this is a **patch** because it copies the memory push's shape onto a new record.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "The tool call preview like we see in codex nothing must be hidden, isnt codex displaying all"; then chose "Full output on click (Recommended)". Secret values stay masked under `AGENTS.orly.md` §Hard Safety. Re-scoped the same day after Indy chose a fresh Rust runner: the record builder, the pipe frame and the forwarder moved to the runner, and this spec keeps the daemon. The 64 KiB, 1 MiB and 256 KiB bounds are agent defaults.
- **Metrics review** — No analytics or funnel playbook update required: the "show all" click is not a funnel step; one operator log event added.
- **Implementation notes (Oct 03, 2026)** — The table carries `byte_count`, the bytes the runner measured (arguments encoded compactly, plus output), because Postgres renders `arguments` as JSONB text with spaces of its own and a budget summed from that text would disagree with the runner's. The budget is per event and per fence: settlement keeps one fence's records, so a reclaimed lease starts with the whole 1 MiB. A post over 256 KiB is refused whole with `413 UZ-REQ-002`; a call named twice in one post keeps its last record and counts the earlier one as skipped. Posts for one event serialize on the event row (`FOR NO KEY UPDATE`), so two cannot both pass the budget.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
