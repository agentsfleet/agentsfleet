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

# M209_001: A runner's tool-call outcomes and run trace reach the fleet thread through the daemon — published live on the fleet channel and stored for a reload

**Prototype:** v2.0.0
**Milestone:** M209
**Workstream:** 001
**Date:** Oct 02, 2026
**Status:** DONE
**Priority:** P1 — operator-facing: the chat can say only that a tool ran and for how long, never what it did or how it ended
**Categories:** API, DOCS
**Batch:** B1 — wire and daemon; runs in parallel with the Rust runner, which emits these fields; M209_003 shares the Pull Request
**Branch:** `feat/m209-tool-call-outcomes`
**Baseline revision:** 0d79b0318e687b8862ee8dbd4f622069bc6ba1a4
**Test Baseline:** unit=3001 integration=3618 — at `0d79b0318`, whose tests are `93e96897a`'s (a `docs/`-only delta): unit 3001 passed / 0 failed / 779 ignored (`make test-unit-all`, Rust half; CI `test` run 36990590586; TypeScript CLI 1776 / app 382 / website 22 / design system 60 files passed) · integration 3618 passed / 0 failed (CI `test-integration-rustd` run 36990590709, which runs `make test-coverage-rustd`, both tiers). Final counts land in Pull Request Session Notes.
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M209_001-0d79b0318.md`
**Depends on:** none — tests drive the daemon with hand-built runner frames (`rustd/crates/agentsfleetd/tests/support/e2e_wire.rs`)
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 02, 2026) from a source trace on `main`; re-scoped the same day when Indy chose a fresh Rust runner, which moved capture into the runner
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Live activity (the SSE tail); `docs/architecture/data_flow.md` §The list read and the detail read are different reads; `docs/architecture/runner_execution.md` §Process model (who emits)

---

## Overview

**Goal (testable):** A runner batch whose `tool_call_completed` frames carry `status`, `output_head`, `output_tail` and `output_line_count` is published with them on `fleet:{id}:activity`; a report carrying `tool_calls` stores it on the event row with fenced call ids; `GET …/messages` serves it; and a malformed or over-bound trace never refuses the report.
**Problem:** The daemon's tool frames carry a name, a duration and a call id (`rustd/crates/afd_wire/src/activity.rs:82-93`), the report carries no trace (`rustd/crates/afd_wire/src/report.rs:196-231`), and no table holds a call, so the thread can show neither what a call did nor how it ended, live or after a reload.
**Solution summary:** `afd_wire` gains the outcome fields, a typed status and the trace types with their bounds as named constants — one source for the daemon and the Rust runner. The daemon bridges the outcome onto the fleet channel, narrows the report's trace after the report's own fields, rewrites call ids to their fenced form, and writes it to a new nullable `core.fleet_events.tool_calls` column in the settling statement. The thread and single-event reads serve it as a body column. Capturing calls is the runner's job (`docs/architecture/runner_execution.md`).

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(api): carry tool-call outcomes and the run trace to the fleet thread
- **Intent (one sentence):** Whatever a runner reports about each tool call reaches the operator's thread intact, live and after a reload, and a bad trace never costs a run its result.
- **Handshake** — performed at PLAN (Oct 02, 2026): "what a runner reports about each tool call reaches the thread intact, live and after a reload, and a bad trace never costs a run its result." Assumptions stated before EXECUTE: the 64 KiB bound is measured on the bytes the runner sent, fencing adding at most 21 bytes per call; `arguments` is an object whose string values hold the 256-byte bound; `x-stability` is declared by a derived pass in `afd_api/src/openapi.rs`; live completion edges carry the trace's edge bound.

## Implementing agent — read these first

1. `rustd/crates/afd_fleet/src/lease/activity/published.rs` — the bridge a new frame field must pass, or the daemon drops it silently.
2. `rustd/crates/afd_fleet/src/lease/finalize.rs` — `mark_terminal`, the settling transaction the trace joins.
3. `rustd/crates/afd_events/src/history/statement.rs` — `body_columns!`, the detail-only column set.
4. `rustd/crates/agentsfleetd/tests/support/e2e_wire.rs` — how tests speak as a runner.
5. `docs/architecture/runner_fleet.md` — §Live activity: frame shapes, bounds and rollout order.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_wire/src/activity.rs`, `rustd/crates/afd_wire/src/activity/tests.rs`, `rustd/crates/afd_wire/src/report.rs`, `rustd/crates/afd_wire/src/report/tests.rs`, `rustd/crates/afd_wire/src/event.rs`, `rustd/crates/afd_wire/src/tool_trace.rs`, `rustd/crates/afd_wire/src/tool_trace/tests.rs`, `rustd/crates/afd_wire/src/lib.rs` | EDIT / CREATE | Outcome fields, `ToolCallStatus`, trace types, bounds and their validator, the raw carrier the report holds; `EventDetail.tool_calls`. The activity tests moved to a sibling at the length cap |
| `rustd/crates/afd_fleet/src/lease/activity/published.rs`, `rustd/crates/afd_fleet/src/lease/activity/tests.rs`, `rustd/crates/afd_api_runner/src/handler/runner/activity.rs` | EDIT | Bridge the outcome onto `fleet:{id}:activity`; refuse an edge past its bound |
| `rustd/crates/afd_fleet/src/lease/tool_trace.rs`, `rustd/crates/afd_fleet/src/lease/tool_trace/tests.rs`, `rustd/crates/afd_fleet/src/lease/mod.rs`, `rustd/crates/afd_fleet/src/lease/report.rs`, `rustd/crates/afd_fleet/src/lease/verdict.rs`, `rustd/crates/afd_fleet/src/lease/finalize.rs`, `rustd/crates/afd_events/src/sql.rs` | CREATE / EDIT | Narrow the trace apart from the report; fence call ids (one function for frames and trace); write it in the settling `UPDATE` |
| `rustd/crates/afd_events/src/history/statement.rs`, `rustd/crates/afd_events/src/history/detail.rs`, `rustd/crates/afd_events/src/history/queued.rs`, `rustd/crates/afd_events/src/history/queued/tests.rs`, `rustd/crates/afd_api_tenant/src/handler/event/mod.rs`, `rustd/crates/afd_api_tenant/src/handler/fleet/message/tests.rs`, `rustd/crates/afd_api_tenant/src/handler/stream.rs` | EDIT | `tool_calls` as a body column on the detail and thread reads, never the list; the stream description names the outcome fields |
| `schema/924_fleet_events_tool_calls.sql`, `rustd/crates/afd_db/src/migration.rs` | CREATE / EDIT | Forward `ADD COLUMN IF NOT EXISTS tool_calls JSONB`, registered in `MIGRATIONS` |
| `public/openapi.json`, `rustd/crates/afd_api/src/openapi.rs`, `rustd/crates/afd_api/src/openapi/stability.rs`, `rustd/crates/afd_api/src/openapi/stability/tests.rs` | EDIT / CREATE | Regenerated; `x-stability` declared by a derived pass, because the derive takes no field extensions |
| `rustd/crates/agentsfleetd/tests/daemon_suite.rs`, `rustd/crates/agentsfleetd/tests/integration_runner_activity_call_id.rs`, `rustd/crates/agentsfleetd/tests/integration_tool_trace.rs`, `rustd/crates/agentsfleetd/tests/integration_tenant_registry.rs`, `rustd/crates/afd_db/tests/db_suite.rs`, `rustd/crates/afd_db/tests/integration_fleet_events_tool_calls.rs` | EDIT / CREATE | Runner-shaped bodies and live-datastore proofs (`#[ignore]`d, run by `make test-integration-rustd`); the tenant fixture helpers become shared |
| `rustd/crates/afd_bench/src/lane/lease/drain/runner.rs`, `rustd/crates/afd_fleet/tests/integration_report_owes_destination.rs`, `rustd/crates/afd_fleet/tests/support/fleet_report_commit.rs` | EDIT | Existing literals take the new optional field |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS + TFX (bounds and status names are constants the tests import), TGU (`ToolCallStatus` is an enum, not a boolean beside optional text), NTP (the trace is narrowed at the daemon's parse boundary), OBS, ORP, DFS, TST-NAM, TCF, MIG, STS.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — a dropped trace is a logged outcome, never a new error on the report path.
- `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` — forward file only; the table grant at `schema/800_fleet_events.sql:87` covers the column.
- `dispatch/write_http.md` + `docs/REST_API_DESIGN_GUIDELINES.md` — §1 field naming (`duration_ms`, `output_line_count`, `omitted_call_count`, no bare `result`), §9 `x-stability`.
- `docs/LOGGING_STANDARD.md` — never log a trace body.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — `schema/924_…` | Forward `ADD COLUMN IF NOT EXISTS`, nullable, no default, no backfill; mirrors `schema/911_runner_leases_receipt.sql` |
| UFS GATE | yes | Bounds live once in `afd_wire::tool_trace`; tests and the Rust runner import them |
| LOGGING | yes | `report_tool_trace_dropped` with ids, reason and byte count only |
| MILESTONE-ID GATE | yes | No milestone identifiers in source or test names |
| Architecture consult | yes | `runner_fleet.md` §Live activity already states these facts |
| File & Function Length (≤350/≤50/≤70) | yes — `finalize.rs`, `published.rs` | Trace narrowing lives in `tool_trace.rs` |

## Prior-Art / Reference Implementations

- **Reference:** the memory push (`rustd/crates/afd_fleet/src/lease/memory.rs`) — a runner-supplied body narrowed and bounded by the daemon, which is the authority.
- **Reference:** Codex `~/Projects/oss/rs/codex/codex-rs/protocol/src/protocol.rs` (`ExecCommandEndEvent`, status `Completed | Failed | Declined`) — a typed end state per call; ours adds `interrupted` for calls a run never finished.

## Sections (implementation slices)

### §1 — The wire carries outcomes and the trace

`ToolCallCompleted` gains optional `status` (`succeeded | failed | interrupted`), `output_head`, `output_tail`, `output_line_count`, and `exit_code` for calls that ran a process. A trace type carries up to 200 calls (call id, name, arguments, status, edges, line count, exit code, `duration_ms`) and `omitted_call_count`, with a validator that enforces every bound. A frame from an older runner still parses.

- **Dimension 1.1** — A completion round-trips with each status, both edges and an exit code → Test `test_tool_call_completed_outcome_roundtrip` — DONE (`afd_wire/src/activity/tests.rs`)
- **Dimension 1.2** — A completion without outcome fields parses with each absent → Test `test_tool_call_completed_without_outcome_parses` — DONE (`afd_wire/src/activity/tests.rs`)
- **Dimension 1.3** — The validator refuses 201 calls, a 65537-byte trace, a 2049-byte argument object and an edge over 1 KiB → Test `test_tool_trace_validator_enforces_bounds` — DONE (`afd_wire/src/tool_trace/tests.rs`)

### §2 — The daemon publishes the outcome live

`Published::ToolCallCompleted` bridges the five fields onto `fleet:{id}:activity`.

- **Dimension 2.1** — The bridge publishes `status`, `output_head`, `output_tail`, `output_line_count`, `exit_code` → Test `test_published_completed_carries_outcome` — DONE (`afd_fleet/src/lease/activity/tests.rs`)
- **Dimension 2.2** — A runner batch posted to the activity verb reaches the channel with the outcome → Test `test_activity_tool_outcome_reaches_channel` — DONE (`agentsfleetd/tests/integration_runner_activity_call_id.rs`; an edge past its bound refuses the batch)

### §3 — The report's trace is stored with the result it belongs to

The report takes `tool_calls` as raw JSON and narrows it after its own fields, so a bad trace never refuses a report. Each call id is rewritten to the fenced `{fence}:{n}` the live frames use. `UPDATE_FLEET_EVENT_RESULT` writes the trace beside `response_text`; a fenced-out report writes neither, and a trace failing a bound is stored `NULL` and logged.

- **Dimension 3.1** — A report with a valid, absent or malformed trace parses, and only a valid trace survives → Test `test_report_tool_calls_never_refuse_report` — DONE (`afd_wire/src/report/tests.rs`)
- **Dimension 3.2** — An over-bound trace is stored `NULL`, the report settles 2xx and `report_tool_trace_dropped` logs → Test `test_oversize_tool_trace_dropped_report_settles` — DONE (`agentsfleetd/tests/integration_tool_trace.rs`; the log line is proven in `afd_fleet/src/lease/tool_trace/tests.rs`, because the booted daemon logs to no capture)
- **Dimension 3.3** — A settled report stores the trace with fenced call ids → Test `test_report_writes_tool_calls_with_result` — DONE (`agentsfleetd/tests/integration_tool_trace.rs`)
- **Dimension 3.4** — A stale-fence report writes neither result nor trace → Test `test_fenced_report_writes_no_tool_calls` — DONE (`agentsfleetd/tests/integration_tool_trace.rs`)
- **Dimension 3.5** — Migrating a populated database leaves existing rows `NULL` → Test `test_tool_calls_column_upgrade_keeps_rows_null` — DONE (`afd_db/tests/integration_fleet_events_tool_calls.rs`)

### §4 — The thread and the single-event read serve the trace

`tool_calls` joins `body_columns!`, `EventDetailRow` and `EventDetail`. The events list never selects it, `event_complete` stays body-free (the browser re-reads the detail at settle, `ui/packages/app/lib/streaming/fleet-stream-detail-reader.ts:28`), and the 512 KiB thread page (`rustd/crates/afd_api_tenant/src/handler/fleet/message.rs:53`) counts it.

- **Dimension 4.1** — The detail read returns the stored trace, and `null` for a row without one → Test `test_event_detail_serves_tool_calls` — DONE (`agentsfleetd/tests/integration_tool_trace.rs`, detail and thread reads)
- **Dimension 4.2** — Thread items carry `tool_calls` and the page budget counts their bytes → Test `test_thread_page_budget_counts_tool_calls` — DONE (`afd_api_tenant/src/handler/fleet/message/tests.rs`, unit: the budget is decided before any datastore)
- **Dimension 4.3** — The events list response has no `tool_calls` key → Test `test_event_list_omits_tool_calls` — DONE (`agentsfleetd/tests/integration_tool_trace.rs`)
- **Dimension 4.4** — `public/openapi.json` matches the regenerated document → Test `test_openapi_build_is_the_source` — DONE (`afd_api/tests/openapi_artifact.rs`, regenerated)

## Interfaces

```
tool_call_completed (outcome fields optional, absent from older runners)
  { "name": "file_read", "ms": 12, "call_id": "7:3", "status": "succeeded",
    "output_head": "# agentsfleet\n…", "output_tail": "…\nMIT", "output_line_count": 214 }
  exit_code: optional integer, present when the call ran a process (Codex shows it as " (exit N)")

ReportRequest.tool_calls (optional; narrowed after the report's own fields)
EventDetail.tool_calls   (null = not recorded)
  { "calls": [ { "call_id": "7:3", "name": "file_read", "arguments": {"path": "README.md"},
                 "status": "succeeded", "output_head": "…", "output_tail": "…",
                 "output_line_count": 214, "duration_ms": 12 } ],
    "omitted_call_count": 0 }

afd_wire::tool_trace bounds (one value each, shared with the Rust runner):
  ARGS_MAX_BYTES 2048 · ARGS_LEAF_MAX_BYTES 256 · OUTPUT_EDGE_MAX_LINES 5
  OUTPUT_EDGE_MAX_BYTES 1024 · TRACE_MAX_CALLS 200 · TRACE_MAX_BYTES 65536
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Over-bound or malformed trace | Runner fault or tampered body | Stored `NULL`, report settles, `report_tool_trace_dropped` logs (Dimensions 3.1, 3.2) |
| Stale fence | Reclaimed lease reports late | Report refused as today; no trace written (Dimension 3.4) |
| Older runner | No outcome, no trace | Fields absent; `tool_calls` stays `NULL` (Dimension 1.2) |
| Daemon rolled back below this release | Old daemon refuses unknown fields | Batches and reports refused; roll runners back first (`runner_fleet.md` §Live activity) |
| Busy thread | Many large traces | Page holds fewer turns, never exceeds its budget (Dimension 4.2) |

## Invariants

1. A stored trace never exceeds `TRACE_MAX_CALLS` calls, and the trace the runner sent never exceeds `TRACE_MAX_BYTES` — the daemon validates before the write and is the authority (Dimension 1.3). The bytes are measured as sent, so the runner's own check agrees with the daemon's; fencing each call id adds at most 21 bytes per call after the check.
2. A trace is written only by the statement that settles its event (Dimension 3.4).
3. No trace can refuse a report — it is parsed after the report's own fields, as raw JSON (Dimension 3.1).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `report_tool_trace_dropped` (daemon log, warn) | ops | A trace fails parse or a bound | `fleet_id`, `event_id`, reason, byte count | No trace content | `test_oversize_tool_trace_dropped_report_settles` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_tool_call_completed_outcome_roundtrip` | each status, with and without `exit_code: 2` → JSON → equal value |
| 1.2 | unit | `test_tool_call_completed_without_outcome_parses` | `{name, ms}` → outcome fields absent |
| 1.3 | unit | `test_tool_trace_validator_enforces_bounds` | 201 calls, 65537 bytes, 2049-byte arguments, 1025-byte edge → each refused |
| 2.1 | unit | `test_published_completed_carries_outcome` | wire frame → published JSON has all five fields |
| 2.2 | integration | `test_activity_tool_outcome_reaches_channel` | POST batch → subscriber reads the outcome |
| 3.1 | unit | `test_report_tool_calls_never_refuse_report` | `tool_calls: 7` → report parses, trace dropped |
| 3.2 | integration | `test_oversize_tool_trace_dropped_report_settles` | 201 calls → 2xx, column `NULL`, warn logged |
| 3.3 | integration | `test_report_writes_tool_calls_with_result` | settled report → row trace equals sent, ids `{fence}:{n}` |
| 3.4 | integration | `test_fenced_report_writes_no_tool_calls` | stale token → row unchanged, `tool_calls` `NULL` |
| 3.5 | integration | `test_tool_calls_column_upgrade_keeps_rows_null` | populated db + slot 924 → old rows `NULL` |
| 4.1 | integration | `test_event_detail_serves_tool_calls` | detail read → stored trace; traceless row → `null` |
| 4.2 | unit | `test_thread_page_budget_counts_tool_calls` | heavy traces → page ≤ 512 KiB, cursor set |
| 4.3 | integration | `test_event_list_omits_tool_calls` | list item JSON has no `tool_calls` key |
| 4.4 | unit | `test_openapi_build_is_the_source` | regenerated document equals `public/openapi.json` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The wire carries outcomes and a validated trace (§1) | `cargo test --manifest-path rustd/Cargo.toml -p afd_wire tool_` | exit 0 | P0 | |
| R2 | The channel carries the outcome and the event row keeps the trace (§2–§4) | `make test-integration-rustd && grep -cE "fn test_(report_writes_tool_calls_with_result\|event_detail_serves_tool_calls)\(" rustd/crates/agentsfleetd/tests/integration_tool_trace.rs` | 2 | P0 | |
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

- Capturing calls: scrubbing, cutting output edges, closing open calls as `interrupted`, building the trace — the Rust runner (`docs/architecture/runner_execution.md` §Process model). The Zig runner gets no new code.
- Full arguments and output behind "show all" — M209_003.
- Rendering, grouping and reload hydration — M209_002.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator refreshes a settled thread and still sees `Read README.md` green and `POST …` red with its error.
2. **Preserved user behaviour** — Live clocks, call pairing, reply streaming and settle timing are unchanged; older runners keep working.
3. **Optimal-way check** — The daemon is the authority on what is stored; carrying the trace on the report keeps it inside the settling transaction.
4. **Rebuild-vs-iterate** — Iterate: frames, call ids, the report and the detail read already exist.
5. **What we build** — Four optional frame fields, one trace type with a validator, one column, one body column.
6. **What we do NOT build** — Capture (runner), full outputs (M209_003), rendering (M209_002).
7. **Fit with existing features** — Compounds with call ids and the detail re-read; must not destabilise report settlement.
8. **Surface order** — API first; the chat renders it in M209_002.
9. **Dashboard restraint** — A call with no recorded outcome carries none; nothing invents success.
10. **Confused-user next step** — A turn without rows predates the column or had its trace dropped; `report_tool_trace_dropped` names which.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** wire and daemon as one workstream that ships while the Rust runner is built, so the chat work in M209_002 proceeds in parallel. The daemon bridge, the thread read and M209_002 consume these types from the start, and tests produce them with runner-shaped bodies.
- **Alternatives considered:** capturing in the Zig runner first (rejected: Indy chose a fresh Rust runner, so that code would be thrown away); a per-call table for the trace (rejected: no query crosses calls, and the thread would need a join).
- **Patch-vs-refactor verdict:** this is a **patch** because every carrier exists and only its contents change.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "Codex-style tool rows and others ensure we are able to show more information"; on visibility, "The tool call preview like we see in codex nothing must be hidden, isnt codex displaying all"; secret values stay masked under `AGENTS.orly.md` §Hard Safety. Re-scoped the same day after "The port is a fresh port, since we always have the last binary with us and running": capture moved to the Rust runner, and this spec keeps the wire and the daemon. Codex lessons kept: typed status, every call ends once, durable means finished.
- **Metrics review** — No analytics or funnel playbook update required: no new user action; one operator log event added.
- **Implementation notes (Oct 02, 2026)** — The live `tool_call_completed` frame's edges are held to the trace's edge bound at the activity verb, so the daemon never publishes an edge it would refuse to store. `x-stability` existed nowhere in the repository and `utoipa` 5.5 takes no extensions on a derived field, so `afd_api/src/openapi/stability.rs` declares it as a derived pass; `EventDetail.tool_calls` is `beta` while the chat and the Rust runner settle its shape.
- **Skill-chain outcomes** — `/orly-write-unit-test` (boundary audit, Oct 03, 2026): every changed Rust source line covered, 692 / 692 (`make test-coverage-rustd`); one gap found and closed — no test raced posts against the budget, so 24 barrier-released posts now prove exactly 16 kept, and the same race keeps 21 with the lease-row lock removed. `/orly-write-integration-test`: done — every Dimension crossing Postgres or Dragonfly has a live test (`integration_tool_trace.rs`, `integration_tool_call_details*.rs`, `integration_tool_call_refusals.rs`, `afd_db`'s slot-924 upgrade). gstack `/review`: five readers, findings fixed or kept with reasons (M209_003 Discovery). `orly-babysit-prs`: runs after the push.
- **Deferrals** — none.
