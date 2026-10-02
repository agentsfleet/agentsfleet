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

# M209_003: "Show all" on a tool call returns its full arguments and output, up to 64 KiB each, saved under the lease fence and read one call at a time

**Prototype:** v2.0.0
**Milestone:** M209
**Workstream:** 003
**Date:** Oct 02, 2026
**Status:** PENDING
**Priority:** P1 — operator-facing: Indy's ask is that nothing a tool returned stays hidden; the thread shows the edges, this serves the rest
**Categories:** API, DOCS
**Batch:** B1 — after M209_001 in the same backend Pull Request
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M209_001 — the observing wrapper, the scrub, fenced call ids, and the settle path this extends
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 02, 2026) from a source trace on `main` at `93e96897a`; Codex caps read at `~/Projects/oss/rs/codex` `2e5fea64e`
**Canonical architecture:** `docs/architecture/data_flow.md` §The list read and the detail read are different reads; `docs/architecture/runner_fleet.md` §Live activity

---

## Overview

**Goal (testable):** After a run whose `http_request` returned 224 lines, `GET …/events/{event_id}/tool-calls/{call_id}` returns all 224 lines and the full arguments, scrubbed, while a reclaimed lease's leftovers, another workspace's caller and an unknown call each get no body.
**Problem:** The thread can show only the head and tail of a call (M209_001). The rest of the output exists only inside the sandboxed child and dies with it, so "show all" has nothing to open.
**Solution summary:** The child keeps a full record per completed call, scrubbed exactly like the edges and capped at 64 KiB per field and 1 MiB per run. It hands each record to the parent on a new `D` pipe frame that never reaches the live tail. The parent posts the records in batches to a fenced runner verb before the report. That verb is separate from the report because the report's 2 MB body limit (`axum-core 0.5.6`, `request.rs:319`; runner routes set none, `afd_api/src/router/mount.rs:39`) must never be at risk. The daemon upserts the records into `core.fleet_tool_call_details` under the lease's fence; settlement drops other fences' rows; a tenant read returns one call.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner,api): keep each tool call's full output for "show all"
- **Intent (one sentence):** An operator who wants more than a call's first and last lines can open everything it took and returned, minus secret values.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/runner/daemon/forwarders.zig` — the memory push forwarder (`:164-183`) the detail forwarder mirrors: parse, attach lease id and fence, post best-effort.
2. `rustd/crates/afd_fleet/src/lease/memory.rs` — the fence check (`:82-136`) the detail write reuses.
3. `src/runner/child_supervisor_read.zig` — the frame dispatch (`:159-184`) and `MemorySink` (`:42`) the `D` frame joins.
4. `schema/831_repair_run_results.sql` — the child-table shape: UUIDv7 id, composite foreign key to `core.fleet_events` with cascade.
5. `docs/REST_API_DESIGN_GUIDELINES.md` — §1 naming, §7 the four registration steps and the silent `ALL`-entry trap.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/engine/tool_detail.zig`, `src/runner/engine/tool_detail_test.zig`, `src/runner/engine/observed_tool.zig` | CREATE / EDIT | Full record per call: scrubbed, UTF-8-safe, 64 KiB per field, 1 MiB per run |
| `src/runner/pipe_proto.zig`, `src/runner/child_supervisor.zig`, `src/runner/child_supervisor_read.zig` | EDIT | `tool_detail = 'D'` frame and its sink |
| `src/runner/daemon/tool_detail_forwarder.zig`, `src/runner/daemon/tool_detail_forwarder_test.zig`, `src/runner/daemon/lease_run.zig`, `src/runner/daemon/control_plane_client.zig` | CREATE / EDIT | Batch, post fenced, flush before the report |
| `src/lib/contract/protocol_tool_detail.zig`, `src/runner/tests.zig` | CREATE / EDIT | Zig wire type; register tests |
| `rustd/crates/afd_wire/src/tool_detail.rs`, `rustd/crates/afd_wire/src/lib.rs` | CREATE / EDIT | Request, response and bounds |
| `rustd/crates/afd_http/src/route/runner.rs`, `rustd/crates/afd_http/src/route/fleet.rs` | EDIT | Variant, `ALL` entry and `meta()` for both routes |
| `rustd/crates/afd_api_runner/src/handler/runner/tool_call.rs`, `rustd/crates/afd_api_runner/src/handler/runner/mod.rs` | CREATE / EDIT | Runner verb: fence, narrow, upsert |
| `rustd/crates/afd_fleet/src/lease/tool_detail.rs`, `rustd/crates/afd_fleet/src/lease/mod.rs`, `rustd/crates/afd_fleet/src/lease/finalize.rs` | CREATE / EDIT | Fenced store; settlement drops other fences' rows |
| `rustd/crates/afd_api_tenant/src/handler/event/tool_call.rs`, `rustd/crates/afd_api_tenant/src/handler/event/mod.rs` | CREATE / EDIT | Tenant read of one call |
| `rustd/crates/afd_core/src/error_code/fleet.rs`, `rustd/crates/afd_core/src/error_code.rs` | EDIT | `TOOL_CALL_NOT_FOUND` beside `EVENT_NOT_FOUND` |
| `schema/925_fleet_tool_call_details.sql`, `rustd/crates/afd_db/src/migration.rs` | CREATE / EDIT | New table, grants, registration |
| `public/openapi.json` | EDIT | Regenerated; new fields declare `x-stability` |
| `rustd/crates/agentsfleetd/tests/integration_tool_call_details.rs` | CREATE | Every §3 and §4 integration proof |
| `docs/architecture/data_flow.md` | EDIT | Landed at authoring |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS + TFX (caps are constants shared with tests), NTP (records narrowed per item at the verb), OWN (records copied out of the tool arena), ESC, VLT, OBS, SGR (the `CREATE TABLE` ends with grants), STS (no string defaults or `CHECK … IN`), NSQ, MIG, ITF, ERR (the new code is declared and referenced), TST-NAM, TCF.
- `dispatch/write_zig.md`, `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md`, `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` (UUIDv7 id and its CHECK, `BIGINT` milliseconds), `dispatch/write_http.md` + `docs/REST_API_DESIGN_GUIDELINES.md` (§1, §5 registry codes, §7, §9 `x-stability`), `docs/LOGGING_STANDARD.md`.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE / PUB GATE | yes | `FILE SHAPE DECISION` at PLAN for the record builder and forwarder |
| SCHEMA GUARD | yes — new table | Forward `CREATE TABLE IF NOT EXISTS`, grants in the same file, ≤100 lines |
| ERROR REGISTRY | yes | `TOOL_CALL_NOT_FOUND` declared and used by the read |
| UFS / LOGGING / MILESTONE-ID | yes | Constants once per stack; `tool_detail_post_failed`, `tool_detail_budget_spent` with ids and counts only |
| Architecture consult | yes | `data_flow.md` landed with the spec |
| File & Function Length (≤350/≤50/≤70) | yes — `lease_run.zig`, `forwarders.zig` | New forwarder in its own file |

## Prior-Art / Reference Implementations

- **Reference:** the memory push (`forwarders.zig:164-183` → `afd_api_runner/src/handler/runner/memory.rs:111-141` → `afd_fleet/src/lease/memory.rs:82-136`) — fenced, best-effort runner write; this verb copies its shape.
- **Reference:** Codex `core/src/exec.rs:81` (1 MiB capture cap so a runaway output cannot exhaust memory) and `core/src/mcp_tool_call.rs:124,975-1016` (results over 1 MiB collapse to a text preview "to avoid persisting multi-megabyte results"). Our caps are smaller because the record crosses a network and lands in Postgres per call.

## Sections (implementation slices)

### §1 — The child keeps a full record of each completed call

The wrapper builds one record per call it saw: call number, arguments (every leaf scrubbed, no leaf cap, at most 64 KiB serialized, cut at a key boundary with `truncated_arguments` set), output (scrubbed, invalid UTF-8 repaired, control and bidirectional characters removed except tab and newline, at most 64 KiB cut on a UTF-8 boundary with `truncated` set) and `output_line_count`. Records stop once the run has spent 1 MiB; `tool_detail_budget_spent` logs once. Records ride a `D` pipe frame to a `ToolDetailSink`, never the activity sink.

- **Dimension 1.1** — A 224-line output keeps every line, scrubbed, with `truncated` false → Test `test_tool_detail_keeps_full_scrubbed_output`
- **Dimension 1.2** — A 100 KiB output stops at 64 KiB on a UTF-8 boundary with `truncated` true → Test `test_tool_detail_truncates_at_cap`
- **Dimension 1.3** — Records past 1 MiB per run are not emitted, and the budget log fires once → Test `test_tool_detail_run_budget_stops_records`
- **Dimension 1.4** — A `D` frame reaches the detail sink and never the activity sink → Test `test_tool_detail_frame_skips_live_tail`

### §2 — The parent posts records under the lease fence before the report

The forwarder batches records at no more than 256 KiB per post to `POST /v1/runners/me/leases/{lease_id}/tool-calls` with the lease's fencing token. It flushes before the report is sent. A failed post logs `tool_detail_post_failed` and never delays or blocks the report.

- **Dimension 2.1** — Records totalling 1 MiB go out in posts of at most 256 KiB → Test `test_tool_detail_batches_stay_under_body_cap`
- **Dimension 2.2** — The final batch is posted before the report → Test `test_tool_details_flush_before_report`
- **Dimension 2.3** — A post that fails still lets the report go out → Test `test_tool_detail_post_failure_keeps_report`

### §3 — The daemon stores records under the fence and settlement keeps only the winner's

The verb checks the fence as the memory push does, narrows each item (an over-bound item is skipped and counted), and upserts on `(fleet_id, event_id, fencing_token, call_number)`, so a retried post changes nothing. The settling transaction deletes the event's rows from every other fence, because a reclaimed lease restarts its call numbers at 1.

- **Dimension 3.1** — A valid post stores one row per record → Test `test_tool_call_details_stored_under_fence`
- **Dimension 3.2** — A stale fence is refused and writes nothing → Test `test_stale_fence_tool_call_details_refused`
- **Dimension 3.3** — Posting the same batch twice leaves one row per call → Test `test_tool_call_details_retry_is_idempotent`
- **Dimension 3.4** — An over-bound item is skipped and counted while the rest store → Test `test_over_bound_tool_call_detail_skipped`
- **Dimension 3.5** — Settlement deletes rows written under an older fence → Test `test_settle_drops_dead_lease_details`
- **Dimension 3.6** — Deleting the event deletes its records → Test `test_event_delete_cascades_tool_call_details`
- **Dimension 3.7** — `api_runtime` can insert, update, select and delete records → Test `test_tool_call_details_grants_allow_runtime`

### §4 — A member reads one call

`GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}/tool-calls/{call_id}` takes the fenced id the thread already holds (`{fence}:{n}`, opaque to clients and percent-encoded in the path). It needs `fleet:read`, returns the record or `TOOL_CALL_NOT_FOUND`, and never says which part was missing.

- **Dimension 4.1** — A member reads a saved call's full arguments and output → Test `test_member_reads_tool_call_detail`
- **Dimension 4.2** — An unknown call, another workspace's fleet, or a malformed id each answer 404 → Test `test_tool_call_detail_absent_is_not_found`
- **Dimension 4.3** — Both new URLs answer through the mounted router, not a 404 from an unwalked route → Test `test_tool_call_routes_are_mounted`
- **Dimension 4.4** — A caller without `fleet:read` is refused → Test `test_tool_call_detail_requires_fleet_read`

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
  · output_line_count BIGINT · truncated BOOLEAN · truncated_arguments BOOLEAN · created_at BIGINT
  FK (fleet_id, event_id) → core.fleet_events ON DELETE CASCADE
  UNIQUE (fleet_id, event_id, fencing_token, call_number); GRANT to api_runtime

Bounds: DETAIL_FIELD_MAX_BYTES 65536 · DETAIL_RUN_MAX_BYTES 1048576 · DETAIL_POST_MAX_BYTES 262144
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Post fails | Transport error or 5xx | `tool_detail_post_failed`; report unaffected; the read answers 404 and the row says the output was not kept |
| Stale fence | Reclaimed lease posts late | Refused, nothing written (Dimension 3.2) |
| Retry | Same batch posted twice | Upsert, no duplicate (Dimension 3.3) |
| Reclaimed lease | Two leases ran the event | Settlement keeps the winner's rows (Dimension 3.5) |
| Runaway output | Tool returns megabytes | 64 KiB per field, 1 MiB per run (Dimensions 1.2, 1.3) |
| Over-bound item | Runner fault or tampered body | Skipped and counted (Dimension 3.4) |
| Wrong caller | Other workspace or missing scope | 404 or refusal with no body (Dimensions 4.2, 4.4) |

## Invariants

1. No value in the runner's redaction set appears in a stored record — the record shares the edges' scrub (Dimension 1.1).
2. A record never reaches the live tail — the `D` frame has its own sink (Dimension 1.4).
3. Only the settling fence's records survive settlement (Dimension 3.5).
4. A record never exceeds `DETAIL_FIELD_MAX_BYTES` per field — the daemon re-checks and is the authority (Dimension 3.4).
5. The report never waits on or carries a record (Dimension 2.3).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `tool_detail_post_failed` (runner log, warn) | ops | A detail post fails | lease id, record count, bytes | No record content | `test_tool_detail_post_failure_keeps_report` |
| `tool_detail_budget_spent` (runner log, info) | ops | A run passes 1 MiB of records | lease id, records kept | No record content | `test_tool_detail_run_budget_stops_records` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_tool_detail_keeps_full_scrubbed_output` | 224 lines with a known secret → 224 lines, placeholder, `truncated` false |
| 1.2 | unit | `test_tool_detail_truncates_at_cap` | 100 KiB ending mid-character → ≤ 65536 bytes, valid UTF-8, `truncated` true |
| 1.3 | unit | `test_tool_detail_run_budget_stops_records` | 20 × 64 KiB → 16 records, one budget log |
| 1.4 | unit | `test_tool_detail_frame_skips_live_tail` | `D` frame → detail sink called, activity sink not |
| 2.1 | unit | `test_tool_detail_batches_stay_under_body_cap` | 1 MiB of records → every post ≤ 262144 bytes |
| 2.2 | unit | `test_tool_details_flush_before_report` | end of run → detail post precedes report call |
| 2.3 | unit | `test_tool_detail_post_failure_keeps_report` | post returns error → report still sent, warn logged |
| 3.1 | integration | `test_tool_call_details_stored_under_fence` | live fence, 2 records → 2 rows |
| 3.2 | integration | `test_stale_fence_tool_call_details_refused` | stale token → refused, 0 rows |
| 3.3 | integration | `test_tool_call_details_retry_is_idempotent` | same batch twice → 2 rows |
| 3.4 | integration | `test_over_bound_tool_call_detail_skipped` | one 70 KiB output among 3 → 2 stored, `skipped_count` 1 |
| 3.5 | integration | `test_settle_drops_dead_lease_details` | rows at fences 6 and 7, settle at 7 → only fence 7 rows |
| 3.6 | integration | `test_event_delete_cascades_tool_call_details` | delete fleet → 0 detail rows |
| 3.7 | integration | `test_tool_call_details_grants_allow_runtime` | `api_runtime` insert/update/select/delete succeed |
| 4.1 | integration | `test_member_reads_tool_call_detail` | member GET → 200 with all 224 lines |
| 4.2 | integration | `test_tool_call_detail_absent_is_not_found` | unknown id, other workspace, `x:y:z` → 404 each |
| 4.3 | integration | `test_tool_call_routes_are_mounted` | both URLs → not the unmounted-route 404 |
| 4.4 | integration | `test_tool_call_detail_requires_fleet_read` | key without `fleet:read` → refused |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The child keeps full, scrubbed, bounded records and the parent posts them first (§1, §2) | `make test-unit-runner` | exit 0 | P0 | |
| R2 | Records store under the fence and a member reads one (§3, §4) | `make test-integration-rustd && grep -cE "fn test_(member_reads_tool_call_detail\|settle_drops_dead_lease_details)\(" rustd/crates/agentsfleetd/tests/integration_tool_call_details.rs` | 2 | P0 | |
| R3 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
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

- Rendering "show all" — M209_002.
- Full output while the call is still running — hosted tools return whole; the record exists only at completion.
- Retention shorter than the event's — records live and die with their event row.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator clicks "show all" under `GET …/deploys/9312` and reads all 224 lines, with the token shown as `«secret:FLY_API_TOKEN»`.
2. **Preserved user behaviour** — The thread, the report and settlement timing are unchanged; a run whose posts all fail settles as before.
3. **Optimal-way check** — A separate fenced verb is the direct path that keeps the report's size and timing safe; streaming output live would need long-running tools we do not host.
4. **Rebuild-vs-iterate** — Iterate on the memory push's proven shape.
5. **What we build** — One record builder, one pipe frame, one forwarder, one runner verb, one table, one read.
6. **What we do NOT build** — No live output, no download, no search inside outputs.
7. **Fit with existing features** — Compounds with M209_001's scrub and fenced ids; must not slow settlement.
8. **Surface order** — API first; M209_002 opens it.
9. **Dashboard restraint** — "Show all" offers only what was saved; an unsaved call says so instead of spinning.
10. **Confused-user next step** — "Full output wasn't kept for this call" names the reason class (budget spent or post failed) from the read's 404 and the trace's line count.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** a workstream of its own beside M209_001 in the backend Pull Request, because it adds a table and two endpoints with their own review profile.
- **Alternatives considered:** carrying records in the report (rejected: a large record set plus an uncapped reply could pass the 2 MB body limit and lose the run's result); streaming records on the live tail (rejected: the 64 KiB batch cap and best-effort drops); one JSONB column on the event (rejected: the thread read would carry megabytes).
- **Patch-vs-refactor verdict:** this is a **patch** because it copies the memory push's shape onto a new record.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "The tool call preview like we see in codex nothing must be hidden, isnt codex displaying all"; then chose "Full output on click (Recommended)" when asked whether "show all" should open a call's full output with a per-call store and one read endpoint. Secret values stay masked under `AGENTS.orly.md` §Hard Safety. The 64 KiB, 1 MiB and 256 KiB bounds are agent defaults.
- **Metrics review** — No analytics or funnel playbook update required: the "show all" click is not a funnel step; two operator log events added.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
