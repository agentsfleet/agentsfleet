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

# M209_001: A tool call reaches the fleet thread with its arguments, its outcome and a scrubbed output preview — live, and after a reload

**Prototype:** v2.0.0
**Milestone:** M209
**Workstream:** 001
**Date:** Oct 02, 2026
**Status:** PENDING
**Priority:** P1 — operator-facing: the chat can say only that a tool ran and for how long, never what it did or whether it worked
**Categories:** API, DOCS
**Batch:** B1 — ships first and alone (runner, wire, daemon, schema); M209_002 renders what it carries
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 02, 2026) from a source trace on `main` at `93e96897a`; NullClaw claims read from the pinned package `zig-pkg/nullclaw-2026.5.29-wlZZyeYPSQ…` (`build.zig.zon:26-28`, fork tag `agentsfleet-v2026.9.25`)
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Live activity (the SSE tail); `docs/architecture/data_flow.md` §The list read and the detail read are different reads — both amended in the authoring commit

---

## Overview

**Goal (testable):** A run that calls `file_read` on `README.md` publishes `args_redacted` `{"path":"README.md"}` and a completion carrying `ok`, the output's first five lines and its line count; after the report, `GET …/messages` returns the same call in that event's `tool_calls`, with no value from the redaction set in any frame, report or row.
**Problem:** The thread shows a glyph, a tool name and a clock. The runner sends `{}` as arguments (`src/runner/engine/runner_progress_tools.zig:27,36`) because NullClaw fills them only under `log_llm_io` (pinned `src/agent/root.zig:3031-3035`). It drops the call's success and output (`runner_progress_tools.zig:60-61`). No durable record holds a call, so a reloaded thread shows none.
**Solution summary:** The runner wraps every hosted tool in an observing wrapper that sees the parsed arguments and the returned result. It emits redacted arguments and a bounded, scrubbed outcome on the live frames, and sends a bounded trace with the report. The daemon bridges the new frame fields, then checks the trace and writes it to a new nullable `core.fleet_events.tool_calls` column in the statement that settles the event. The thread and single-event reads serve it as a body column. NullClaw stays unchanged and `log_llm_io` stays off.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner,api): carry tool arguments, outcome and a scrubbed preview to the thread
- **Intent (one sentence):** An operator can see what each tool call did and whether it worked, while the fleet runs and after a reload, without any secret the runner knows reaching the browser.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/runner/engine/runtime/policy_http_request.zig` — the vtable wrapper to mirror; the observing wrapper sits OUTSIDE it, so it sees placeholders and never resolved secrets.
2. `src/runner/engine/runner_progress_tools.zig` — the call id pairing, the fail-closed redaction branch, and the started-frame re-emit the arguments now ride.
3. `src/runner/engine/runner.zig` — `bindMemoryTools` (:200) matches by vtable identity, so wrap after it; the adapter exists from `selectObserver` (:231); `Fleet.fromConfig` (:242) consumes the tools.
4. `rustd/crates/afd_fleet/src/lease/activity/published.rs` — the bridge a new frame field must pass, or the daemon drops it silently.
5. `docs/architecture/runner_fleet.md` — §Live activity: the frame shapes, bounds and rollout order this spec implements.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/engine/observed_tool.zig`, `src/runner/engine/observed_tool_test.zig` | CREATE | Observing wrapper: redacted argument summary, capture of success and scrubbed preview |
| `src/runner/engine/tool_trace.zig`, `src/runner/engine/tool_trace_test.zig` | CREATE | Bounded per-run trace (calls, bytes, omitted count) |
| `src/runner/engine/runner.zig` | EDIT | Wrap hosted tools between `selectObserver` and `Fleet.fromConfig`; hand the trace to the result |
| `src/runner/engine/runner_progress.zig`, `src/runner/engine/runner_progress_tools.zig`, `src/runner/engine/runner_progress_tools_test.zig` | EDIT | Adapter owns the open capture and the trace; completion emits the outcome or falls back to the observer's |
| `src/runner/tests.zig` | EDIT | Register the new test files (RULE TST) |
| `src/lib/contract/activity.zig`, `src/lib/contract/execution_result.zig`, `src/lib/contract/protocol_report.zig`, `src/lib/contract/report_mapping.zig` | EDIT | Zig mirrors: completion outcome, result trace, report `tool_calls` |
| `rustd/crates/afd_wire/src/activity.rs`, `rustd/crates/afd_wire/src/report.rs`, `rustd/crates/afd_wire/src/event.rs`, `rustd/crates/afd_wire/src/tool_trace.rs`, `rustd/crates/afd_wire/src/lib.rs` | EDIT / CREATE | Wire types, trace bounds as named constants, `EventDetail.tool_calls` |
| `rustd/crates/afd_fleet/src/lease/activity/published.rs`, `rustd/crates/afd_fleet/src/lease/activity/tests.rs` | EDIT | Bridge `ok`, `preview`, `output_lines` onto `fleet:{id}:activity` |
| `rustd/crates/afd_api_runner/src/handler/runner/report.rs`, `rustd/crates/afd_fleet/src/lease/finalize.rs`, `rustd/crates/afd_events/src/sql.rs` | EDIT | Parse and check the trace apart from the report; write it in the settling `UPDATE` |
| `rustd/crates/afd_events/src/history/statement.rs`, `rustd/crates/afd_events/src/history/detail.rs`, `rustd/crates/afd_api_tenant/src/handler/event/mod.rs` | EDIT | `tool_calls` as a body column on the detail and thread reads, never the list |
| `schema/924_fleet_events_tool_calls.sql`, `rustd/crates/afd_db/src/migration.rs` | CREATE / EDIT | Forward `ADD COLUMN IF NOT EXISTS tool_calls JSONB`, registered in `MIGRATIONS` |
| `public/openapi.json` | EDIT | Regenerated by `agentsfleetd openapi` |
| `rustd/crates/agentsfleetd/tests/integration_runner_activity.rs`, `rustd/crates/agentsfleetd/tests/integration_tool_trace.rs`, `rustd/crates/afd_db/tests/migrations.rs` | EDIT / CREATE | Live-datastore proofs (`#[ignore]`d, run by `make test-integration-rustd`); every §3 and §4 integration test lives in `integration_tool_trace.rs` |
| `docs/architecture/runner_fleet.md`, `docs/architecture/data_flow.md` | EDIT | Landed at authoring (name_architecture landing rule); reconcile any drift in the code commit |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (every bound and field name a named constant, shared by tests per TFX), ESC (preview and arguments escape control characters), NTP (the daemon narrows the trace at its parse boundary), OWN (the preview is copied out of the tool's arena before the wrapper returns), VLT (no resolved secret leaves the runner), OBS (every dropped trace or failed scrub logs), ORP (a field added to four stacks is swept in all four), DFS, TST-NAM, TCF, MIG, STS.
- `dispatch/write_zig.md` — init/deinit pairing and `errdefer` for the capture and trace; cross-compile both Linux targets (XCOMPILE).
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — a dropped trace is a logged outcome, not a new error variant on the report path.
- `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` — `VERSION` 0.51.0: forward file only; the table grant at `schema/800_fleet_events.sql:87` covers the new column.
- `dispatch/write_http.md` + `docs/REST_API_DESIGN_GUIDELINES.md` — an additive response field on `GET …/messages` and `GET …/events/{event_id}`.
- `docs/LOGGING_STANDARD.md` — scoped events with `error_code`; never log a preview or argument body.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE / PUB GATE | yes — new runner files and pub fns | `FILE SHAPE DECISION` at PLAN; wrapper is operations-over-value, trace is file-as-struct |
| SCHEMA GUARD | yes — `schema/924_…` | Forward `ADD COLUMN IF NOT EXISTS`, nullable, no default, no backfill; mirrors `schema/911_runner_leases_receipt.sql` |
| UFS GATE | yes | Bounds live in `tool_trace.zig` / `afd_wire::tool_trace`; tests import them |
| LOGGING | yes | `tool_capture_scrub_failed`, `report_tool_trace_dropped` with `error_code`, ids and byte counts only |
| MILESTONE-ID GATE | yes | No milestone identifiers in source or test names |
| Architecture consult | yes | Docs landed with this spec; the code commit reconciles them |
| File & Function Length (≤350/≤50/≤70) | yes — `runner.zig`, `runner_progress_tools.zig` | New logic lives in the two new files; `runner.zig` gains one wrap call |

## Prior-Art / Reference Implementations

- **Reference:** `src/runner/engine/runtime/policy_http_request.zig` — the in-repo tool wrapper; the observing wrapper copies its vtable shape and composes outside it.
- **Reference:** `rustd/crates/afd_gate/src/gate/claim.rs` — bounded, sanitized durable evidence (512 B / 1 KiB caps, control and bidirectional characters stripped); the trace applies the same discipline.
- **Reference:** Codex `codex-rs/tui/src/exec_cell/render.rs` (`~/Projects/oss/rs/codex`) — `TOOL_CALL_MAX_LINES = 5` (:43); diverges by keeping the head only, since hosted outputs are files and HTTP bodies whose first lines name what came back.

## Sections (implementation slices)

### §1 — The runner observes every hosted tool call

One wrapper per granted tool (`tool_bridge.zig:70-86` hosts 13; none returns an exit code). On execute it serializes the parsed arguments with each string leaf passed through `redactBytes` and `scrubSecretPatterns` BEFORE serialization, truncated at 256 bytes, the object capped at 2 KiB by keeping keys in order until the next would cross the cap. It emits `tool_call_started` under the adapter's open call id, then records `ok`, the first five lines of the output (failure text is `error_msg` else `output`) capped at 1 KiB, and the total line count. The preview is scrubbed by `redactBytes`, then NullClaw's public `scrubSecretPatterns`, then a control and bidirectional character strip, and copied out of the arena. `started()` clears any leftover capture; `completed()` falls back to the observer's `success` and `detail` when the wrapper did not run (cached duplicate, `memory_store` dedup, pre-dispatch refusal). **Implementation default:** wrap after `bindMemoryTools` because it matches by vtable identity.

- **Dimension 1.1** — A `file_read` call publishes its path as `args_redacted` → Test `test_observed_tool_emits_redacted_arguments`
- **Dimension 1.2** — A known secret holding `"` and `\` and a `ghp_` token inside arguments are scrubbed and the arguments stay valid JSON → Test `test_argument_leaf_redaction_keeps_json_valid`
- **Dimension 1.3** — Oversized arguments serialize to valid JSON within 2 KiB, every leaf within 256 bytes → Test `test_argument_summary_stays_bounded`
- **Dimension 1.4** — A twelve-line output yields five preview lines, `output_lines` 12, at most 1 KiB → Test `test_output_preview_keeps_first_five_lines`
- **Dimension 1.5** — A preview holding the provider key, a `ghp_` token, a `Bearer` header, a control and a bidirectional character comes out scrubbed → Test `test_output_preview_scrubs_secrets_and_controls`
- **Dimension 1.6** — A failed call reports `ok: false` with its error text as the preview → Test `test_failed_call_reports_error_preview`
- **Dimension 1.7** — A completion the wrapper never saw uses the observer's outcome and never a previous call's capture → Test `test_unwrapped_completion_uses_observer_outcome`
- **Dimension 1.8** — A wrapped `memory_store` still writes the in-run store → Test `test_wrapped_memory_tools_stay_bound`
- **Dimension 1.9** — The runner's NullClaw configuration keeps `log_llm_io` off → Test `test_runner_keeps_llm_io_logging_off`

### §2 — The live frames carry the outcome to the browser channel

`ToolCallCompleted` gains optional `ok`, `preview`, `output_lines` in `afd_wire` and its Zig mirror; `Published::ToolCallCompleted` bridges them. A frame without them still parses. Both tool frames stay far below the 64 KiB activity batch (`ActivitySender.zig:19`).

- **Dimension 2.1** — The Rust completion round-trips with the outcome fields → Test `test_tool_call_completed_outcome_roundtrip`
- **Dimension 2.2** — The Zig completion encodes the field names and order Rust parses → Test `test_tool_call_completed_outcome_encodes`
- **Dimension 2.3** — A completion without outcome fields parses with each absent → Test `test_tool_call_completed_without_outcome_parses`
- **Dimension 2.4** — The bridge publishes `ok`, `preview`, `output_lines` → Test `test_published_completed_carries_outcome`
- **Dimension 2.5** — A runner batch posted to the activity verb reaches `fleet:{id}:activity` with the outcome → Test `test_activity_tool_outcome_reaches_channel`
- **Dimension 2.6** — The largest possible started and completed frames fit one activity batch → Test `test_tool_frames_fit_activity_batch`

### §3 — The report carries a bounded trace and the event row keeps it

The adapter appends each completed call to a trace (at most 50 calls and 16 KiB serialized; later calls count into `omitted`). The trace rides `ExecutionResult` through the pipe's result frame and `report_mapping.toReport` into `ReportRequest.tool_calls`. The daemon takes `tool_calls` as raw JSON and narrows it after the report's own fields, so a bad trace can never refuse a report. A trace that fails a bound is stored `NULL` and logged. `UPDATE_FLEET_EVENT_RESULT` writes it beside `response_text`, so a fenced-out report writes neither.

- **Dimension 3.1** — Sixty calls keep fifty entries with `omitted` 10, and the byte cap holds → Test `test_tool_trace_keeps_bounds`
- **Dimension 3.2** — The result's trace maps into the report; a run with no calls sends none → Test `test_report_carries_tool_trace`
- **Dimension 3.3** — A report with a valid, absent or malformed trace parses, and only a valid trace survives → Test `test_report_tool_calls_never_refuse_report`
- **Dimension 3.4** — An over-bound trace is stored `NULL`, the report settles 2xx and `report_tool_trace_dropped` logs → Test `test_oversize_tool_trace_dropped_report_settles`
- **Dimension 3.5** — A settled report stores the trace in the row it settles → Test `test_report_writes_tool_calls_with_result`
- **Dimension 3.6** — A stale-fence report writes neither result nor trace → Test `test_fenced_report_writes_no_tool_calls`
- **Dimension 3.7** — Migrating a populated database leaves existing rows `NULL` and the migration list matches the directory → Test `test_tool_calls_column_upgrade_keeps_rows_null`

### §4 — The thread and the single-event read serve the trace

`tool_calls` joins `body_columns!`, `EventDetailRow` and `EventDetail`; the events list never selects it, and `event_complete` stays body-free (the browser re-reads the detail at settle, `ui/packages/app/lib/streaming/fleet-stream-detail-reader.ts:28`). The thread page's 512 KiB budget (`message.rs:53`) counts it.

- **Dimension 4.1** — The detail read returns the stored trace, and `null` for a row without one → Test `test_event_detail_serves_tool_calls`
- **Dimension 4.2** — Thread items carry `tool_calls` and the page budget counts their bytes → Test `test_thread_page_budget_counts_tool_calls`
- **Dimension 4.3** — The events list response has no `tool_calls` key → Test `test_event_list_omits_tool_calls`
- **Dimension 4.4** — `public/openapi.json` matches the regenerated document → Test `test_openapi_build_is_the_source`

## Interfaces

```
tool_call_completed (activity frame; new fields optional, absent from older runners)
  { "name": "file_read", "ms": 12, "call_id": "7:3",
    "ok": true, "preview": "# agentsfleet\n\nRun fleets…", "output_lines": 214 }

ReportRequest.tool_calls (optional; parsed after the report's own fields)
EventDetail.tool_calls   (null = not recorded)
  { "calls": [ { "call_id": "3", "name": "file_read", "args": {"path": "README.md"},
                 "ok": true, "preview": "…", "output_lines": 214, "ms": 12 } ],
    "omitted": 0 }

Bounds (afd_wire::tool_trace + src/runner/engine/tool_trace.zig, one value each):
  ARGS_LEAF_MAX_BYTES 256 · ARGS_MAX_BYTES 2048 · PREVIEW_MAX_LINES 5
  PREVIEW_MAX_BYTES 1024 · TRACE_MAX_CALLS 50 · TRACE_MAX_BYTES 16384
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Scrub allocation fails | OOM inside `redactBytes` or the pattern scrubber | Arguments or preview dropped, call still closes with name and ms; `tool_capture_scrub_failed` logs |
| Secret outside every scrub | A token minted mid-run with no known prefix | Residual exposure to workspace members; bounded by the preview cap; the exposure decision below owns it |
| Over-bound or malformed trace | Runner fault or tampered body | Stored `NULL`, report settles, `report_tool_trace_dropped` logs; the thread shows live rows only |
| Stale fence | Reclaimed lease reports late | Report refused as today; no trace written |
| Daemon rolled back below this release | Old daemon refuses unknown fields | Whole activity batches and whole reports refused; operational prerequisite: roll runners back first (`runner_fleet.md` §Live activity) |
| Older runner | No outcome fields, no trace | Frames parse with fields absent; `tool_calls` stays `NULL` |
| Busy thread | Fifty large calls per turn | Page holds fewer turns; budget proven by Dimension 4.2 |

## Invariants

1. No value in the runner's redaction set appears in a frame, report or stored row — string leaves redact before serialization and previews scrub before emission (Dimensions 1.2, 1.5).
2. `args_redacted` is always a valid JSON object — built by the standard serializer from the parsed map; the daemon's `serde_json::from_str` (`published.rs:93`) is the runtime check.
3. A stored trace never exceeds `TRACE_MAX_CALLS` or `TRACE_MAX_BYTES` — the daemon checks before the write and is the authority; the runner's bound is an economy.
4. A trace is written only by the statement that settles its event — one `UPDATE`, one fence check (Dimension 3.6).
5. `log_llm_io` stays off in the runner — a unit test reads the configuration the runner builds (Dimension 1.9).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `tool_capture_scrub_failed` (runner log, warn) | ops | A scrub or redaction allocation fails | `error_code`, tool name, call id | No argument or output bytes | `test_failed_call_reports_error_preview` |
| `report_tool_trace_dropped` (daemon log, warn) | ops | A trace fails parse or a bound | `fleet_id`, `event_id`, reason, byte count | No trace content | `test_oversize_tool_trace_dropped_report_settles` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_observed_tool_emits_redacted_arguments` | `file_read {path: README.md}` → started frame `{"path":"README.md"}` under the open call id |
| 1.2 | unit | `test_argument_leaf_redaction_keeps_json_valid` | secret `a"b\c` and `ghp_…` in leaves → both scrubbed; output parses as JSON |
| 1.3 | unit | `test_argument_summary_stays_bounded` | 10 KiB `content` → valid JSON ≤ 2048 bytes, leaf ≤ 256 |
| 1.4 | unit | `test_output_preview_keeps_first_five_lines` | 12-line output → 5 lines, `output_lines` 12, ≤ 1024 bytes |
| 1.5 | unit | `test_output_preview_scrubs_secrets_and_controls` | key, `ghp_…`, `Bearer …`, `\x1b`, U+202E → none survive |
| 1.6 | unit | `test_failed_call_reports_error_preview` | `success=false, error_msg="denied"` → `ok:false`, preview `denied` |
| 1.7 | unit | `test_unwrapped_completion_uses_observer_outcome` | capture from call 1, unwrapped call 2 → call 2 uses observer `success`/`detail` |
| 1.8 | unit | `test_wrapped_memory_tools_stay_bound` | wrapped `memory_store` then `memory_recall` → entry found |
| 1.9 | unit | `test_runner_keeps_llm_io_logging_off` | runner-built config → `diagnostics.log_llm_io == false` |
| 2.1 | unit | `test_tool_call_completed_outcome_roundtrip` | Rust value → JSON → equal value |
| 2.2 | unit | `test_tool_call_completed_outcome_encodes` | Zig frame bytes parse in the Rust fixture |
| 2.3 | unit | `test_tool_call_completed_without_outcome_parses` | `{name, ms}` → `ok/preview/output_lines` absent |
| 2.4 | unit | `test_published_completed_carries_outcome` | wire frame → published JSON has all three fields |
| 2.5 | integration | `test_activity_tool_outcome_reaches_channel` | POST batch → subscriber reads the outcome on the fleet channel |
| 2.6 | unit | `test_tool_frames_fit_activity_batch` | max-size started + completed < 64 KiB |
| 3.1 | unit | `test_tool_trace_keeps_bounds` | 60 calls → 50 entries, `omitted` 10, ≤ 16384 bytes |
| 3.2 | unit | `test_report_carries_tool_trace` | result with 2 calls → report `tool_calls.calls` length 2; none → field absent |
| 3.3 | unit | `test_report_tool_calls_never_refuse_report` | `tool_calls: 7` → report parses, trace dropped |
| 3.4 | integration | `test_oversize_tool_trace_dropped_report_settles` | 51 calls → 2xx, column `NULL`, warn logged |
| 3.5 | integration | `test_report_writes_tool_calls_with_result` | settled report → row `tool_calls` equals sent trace |
| 3.6 | integration | `test_fenced_report_writes_no_tool_calls` | stale token → row unchanged, `tool_calls` `NULL` |
| 3.7 | integration | `test_tool_calls_column_upgrade_keeps_rows_null` | populated db + slot 924 → old rows `NULL` |
| 4.1 | integration | `test_event_detail_serves_tool_calls` | detail read → stored trace; traceless row → `null` |
| 4.2 | integration | `test_thread_page_budget_counts_tool_calls` | heavy traces → page ≤ 512 KiB, cursor set |
| 4.3 | integration | `test_event_list_omits_tool_calls` | list item JSON has no `tool_calls` key |
| 4.4 | unit | `test_openapi_build_is_the_source` | regenerated document equals `public/openapi.json` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The runner emits real arguments and a scrubbed outcome (§1) | `make test-unit-runner` | exit 0 | P0 | |
| R2 | The frames and report carry the outcome across stacks (§2, §3) | `cargo test --manifest-path rustd/Cargo.toml -p afd_wire tool_` | exit 0 | P0 | |
| R3 | The event row keeps the trace and the thread serves it (§3, §4) | `make test-integration-rustd && grep -cE "fn test_(report_writes_tool_calls_with_result\|event_detail_serves_tool_calls)\(" rustd/crates/agentsfleetd/tests/integration_tool_trace.rs` | 2 | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
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

N/A — no files deleted. `NO_ARGS` in `runner_progress_tools.zig` keeps its caller: the opening started frame still carries `{}` until the wrapper re-emits.

## Out of Scope

- Rendering, grouping and reload hydration in the web app — M209_002.
- Inline approval rows in the thread — the gate is per event, not per call (`afd_fleet/src/lease/pull.rs:318-340`), and its rows carry no `event_id`; a follow-up spec.
- Any NullClaw fork change — the wrapper makes it unnecessary.
- Exit codes — no hosted tool returns one (`tool_bridge.zig:70-86`).

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator watches a fleet work and sees `Read README.md ✓` with its first lines; after a refresh the same rows are still there.
2. **Preserved user behaviour** — Live clocks, call pairing, reply streaming and settle timing stay exactly as they are; older runners keep working.
3. **Optimal-way check** — The direct path is capturing at the tool, which the runner owns; the remaining gap (secrets with no known shape) is bounded by the preview cap and recorded as a human decision.
4. **Rebuild-vs-iterate** — Iterate: the frames, call ids and report already exist; this fills their empty fields.
5. **What we build** — One wrapper, one trace, three optional frame fields, one report field, one column, one body column on two reads.
6. **What we do NOT build** — No full transcripts, no exit codes, no NullClaw change, no per-call table.
7. **Fit with existing features** — Compounds with call ids and the detail re-read; must not destabilise report settlement.
8. **Surface order** — API first; M209_002 renders it.
9. **Dashboard restraint** — A row with no recorded outcome shows none, never a fake ✓.
10. **Confused-user next step** — A turn without rows predates the column or its trace was dropped; `report_tool_trace_dropped` names which.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** backend in one workstream and Pull Request, because it crosses a security boundary (what tool output members can read) and owns the schema; the UI follows as M209_002.
- **Alternatives considered:** turning on `log_llm_io` (rejected: logs model requests and responses, and its ` [truncated]` suffix breaks `published.rs:93`); a NullClaw fork change (rejected: the runner can wrap); a per-call child table (rejected: no cross-call query needs it, and the thread would need a join).
- **Patch-vs-refactor verdict:** this is a **patch** because every carrier already exists and only its contents change.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "How do i make the Chat like Codex with tooling calls" and "Codex-style tool rows and others ensure we are able to show more information." **Required human decision, pending:** who may read output previews. Agent default recorded here: a scrubbed five-line preview visible to every member with `fleet:read`. Indy did not answer the in-session question; §1 Dimensions 1.4–1.6 and the `preview` field wait on Indy's confirmation before EXECUTE, and the rest of the spec proceeds. Persistence (agent default: stored on the event row) is reversible and proceeds.
- **Metrics review** — No analytics or funnel playbook update required: no new user action; two operator log events added.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
