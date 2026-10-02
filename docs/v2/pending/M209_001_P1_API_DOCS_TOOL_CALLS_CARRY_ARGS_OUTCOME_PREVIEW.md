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

# M209_001: Every tool call reaches the fleet thread with its arguments, a typed outcome and the head and tail of its output — live, after a crash, and after a reload

**Prototype:** v2.0.0
**Milestone:** M209
**Workstream:** 001
**Date:** Oct 02, 2026
**Status:** PENDING
**Priority:** P1 — operator-facing: the chat can say only that a tool ran and for how long, never what it did or whether it worked
**Categories:** API, DOCS
**Batch:** B1 — first; M209_003 rides the same backend Pull Request, M209_002 renders both
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 02, 2026) from a source trace on `main` at `93e96897a`; NullClaw read from the pinned `zig-pkg/nullclaw-2026.5.29-wlZZyeYPSQ…` (`build.zig.zon:26-28`); Codex patterns read at `~/Projects/oss/rs/codex` `2e5fea64e`
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Live activity (the SSE tail); `docs/architecture/data_flow.md` §The list read and the detail read are different reads

---

## Overview

**Goal (testable):** A run that reads `README.md`, fails a `POST`, and is killed during a third call publishes real arguments and a typed status for all three (`succeeded`, `failed`, `interrupted`); after the report, `GET …/messages` returns the same three calls in that event's `tool_calls`, and no value from the redaction set appears in any frame, report or row.
**Problem:** The thread shows a glyph, a tool name and a clock. The runner sends `{}` as arguments (`src/runner/engine/runner_progress_tools.zig:27,36`), because NullClaw fills them only under `log_llm_io` (pinned `src/agent/root.zig:3031-3035`). It also drops each call's success and output (`runner_progress_tools.zig:60-61`). A call open when the child dies never closes, and no durable record holds any call.
**Solution summary:** An observing wrapper around each hosted tool sees parsed arguments and the returned result. It emits scrubbed arguments, a typed status, and the head and tail of the output, cut on UTF-8 boundaries. The runner's parent tees the frames it already forwards into a bounded trace. When the child exits, it closes any open call as `interrupted`, live and in the trace, and sends the trace with the report. The daemon bridges the new frame fields, then checks the trace and writes it to `core.fleet_events.tool_calls` in the settling statement. The thread read serves it as a body column. NullClaw is unchanged and `log_llm_io` stays off.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner,api): carry tool arguments, typed outcomes and output to the thread
- **Intent (one sentence):** An operator sees what every tool call did and how it ended, live and after a reload, even when the run crashes, without any secret the runner knows reaching the browser.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/runner/engine/runtime/policy_http_request.zig` — the vtable wrapper to mirror; the observing wrapper composes OUTSIDE it, so it sees placeholders, never resolved secrets.
2. `src/runner/engine/runner_progress_tools.zig` — call-id pairing, the fail-closed redaction branch, and the started-frame re-emit that now carries arguments.
3. `src/runner/daemon/forwarders.zig` — `ActivityForwarder.forward` receives every parsed frame in the parent; the trace tees here (`lease_run.zig:126`).
4. `rustd/crates/afd_fleet/src/lease/activity/published.rs` — the bridge a new frame field must pass, or the daemon drops it silently.
5. `docs/architecture/runner_fleet.md` — §Live activity: frame shapes, bounds and rollout order.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/engine/observed_tool.zig`, `src/runner/engine/observed_tool_test.zig` | CREATE | Wrapper: scrubbed argument summary, typed status, UTF-8-safe head and tail |
| `src/runner/engine/runner.zig`, `src/runner/engine/runner_progress.zig`, `src/runner/engine/runner_progress_tools.zig`, `src/runner/engine/runner_progress_tools_test.zig` | EDIT | Wrap tools between `selectObserver` and `Fleet.fromConfig`; completion emits the capture or the observer's fallback |
| `src/runner/daemon/tool_trace.zig`, `src/runner/daemon/tool_trace_test.zig`, `src/runner/daemon/lease_run.zig` | CREATE / EDIT | Parent-side bounded trace teed from the activity sink; open calls closed as `interrupted` at child exit; trace handed to the report |
| `src/runner/tests.zig` | EDIT | Register new test files (RULE TST) |
| `src/lib/contract/activity.zig`, `src/lib/contract/protocol_report.zig`, `src/lib/contract/report_mapping.zig` | EDIT | Zig mirrors: completion outcome, report `tool_calls` |
| `rustd/crates/afd_wire/src/activity.rs`, `rustd/crates/afd_wire/src/report.rs`, `rustd/crates/afd_wire/src/event.rs`, `rustd/crates/afd_wire/src/tool_trace.rs`, `rustd/crates/afd_wire/src/lib.rs` | EDIT / CREATE | Wire types, `ToolCallStatus`, bounds as named constants, `EventDetail.tool_calls` |
| `rustd/crates/afd_fleet/src/lease/activity/published.rs`, `rustd/crates/afd_fleet/src/lease/activity/tests.rs` | EDIT | Bridge the outcome fields onto `fleet:{id}:activity` |
| `rustd/crates/afd_api_runner/src/handler/runner/report.rs`, `rustd/crates/afd_fleet/src/lease/finalize.rs`, `rustd/crates/afd_events/src/sql.rs` | EDIT | Narrow the trace apart from the report; fence its call ids; write it in the settling `UPDATE` |
| `rustd/crates/afd_events/src/history/statement.rs`, `rustd/crates/afd_events/src/history/detail.rs`, `rustd/crates/afd_api_tenant/src/handler/event/mod.rs` | EDIT | `tool_calls` as a body column on detail and thread reads, never the list |
| `schema/924_fleet_events_tool_calls.sql`, `rustd/crates/afd_db/src/migration.rs` | CREATE / EDIT | Forward `ADD COLUMN IF NOT EXISTS tool_calls JSONB`, registered in `MIGRATIONS` |
| `public/openapi.json` | EDIT | Regenerated by `agentsfleetd openapi`; new fields declare `x-stability` |
| `rustd/crates/agentsfleetd/tests/integration_runner_activity.rs`, `rustd/crates/agentsfleetd/tests/integration_tool_trace.rs`, `rustd/crates/afd_db/tests/migrations.rs` | EDIT / CREATE | Live-datastore proofs (`#[ignore]`d, run by `make test-integration-rustd`); every §3 and §4 integration test lives in `integration_tool_trace.rs` |
| `docs/architecture/runner_fleet.md`, `docs/architecture/data_flow.md` | EDIT | Landed at authoring; the code commit reconciles drift |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS + TFX (bounds and status names are constants the tests import), TGU (`ToolCallStatus` is a tagged enum, not a boolean beside optional text), ESC (control characters escaped or stripped before JSON), NTP (the daemon narrows the trace at its parse boundary), OWN (output copied out of the tool's arena), VLT, OBS, ORP, DFS, TST-NAM, TCF, MIG, STS.
- `dispatch/write_zig.md` — init/deinit and `errdefer` for capture and trace; cross-compile both Linux targets (XCOMPILE).
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — a dropped trace is a logged outcome, never a new error on the report path.
- `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` — forward file only; the table grant at `schema/800_fleet_events.sql:87` covers the column.
- `dispatch/write_http.md` + `docs/REST_API_DESIGN_GUIDELINES.md` — §1 field naming (`duration_ms`, `output_line_count`, `omitted_call_count`, no bare `result`), §9 `x-stability` on new response fields.
- `docs/LOGGING_STANDARD.md` — scoped events with `error_code`; never log an argument or output body.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE / PUB GATE | yes | `FILE SHAPE DECISION` at PLAN: the wrapper is operations-over-value, the trace is file-as-struct |
| SCHEMA GUARD | yes — `schema/924_…` | Forward `ADD COLUMN IF NOT EXISTS`, nullable, no default, no backfill; mirrors `schema/911_runner_leases_receipt.sql` |
| UFS GATE | yes | Bounds and statuses live once per stack; tests import them |
| LOGGING | yes | `tool_capture_scrub_failed`, `tool_call_interrupted`, `report_tool_trace_dropped`: ids, counts, `error_code` only |
| MILESTONE-ID GATE | yes | No milestone identifiers in source or test names |
| Architecture consult | yes | Docs landed with this spec; the code commit reconciles them |
| File & Function Length (≤350/≤50/≤70) | yes — `runner.zig`, `lease_run.zig` | New logic lives in the new files; existing files gain one call each |

## Prior-Art / Reference Implementations

- **Reference:** `src/runner/engine/runtime/policy_http_request.zig` — the in-repo tool wrapper; the observing wrapper copies its vtable shape and composes outside it.
- **Reference:** Codex `core/src/context_manager/normalize.rs:21-60` and `tui/src/exec_cell/model.rs:168-182` — every call ends exactly once; an open call is closed `aborted` at turn end. Our parent does the same at child exit.
- **Reference:** Codex `protocol/src/protocol.rs:3554` (`Completed | Failed | Declined`) and `utils/string/src/truncate.rs:85-125` (cuts on character boundaries) — typed status, UTF-8-safe cuts. Codex decodes live deltas lossily per chunk (`app-server-protocol/src/protocol/event_mapping.rs:438`); we cut whole captured outputs, so no character splits across frames.
- **Reference:** `rustd/crates/afd_gate/src/gate/claim.rs` — bounded, sanitized durable evidence.

## Sections (implementation slices)

### §1 — The runner observes every hosted tool call

One wrapper per granted tool (`tool_bridge.zig:70-86`). On execute it serializes the parsed arguments with each string leaf passed through `redactBytes` and `scrubSecretPatterns` BEFORE serialization. Each leaf is cut at 256 bytes on a UTF-8 boundary, and the object is capped at 2 KiB by keeping keys in order until the next would cross. It emits `tool_call_started` under the open call id. On return it records `status` (`succeeded` or `failed`), `output_line_count`, and the first and last five lines of the output (failure text is `error_msg` else `output`). Each edge is at most 1 KiB, cut on a UTF-8 boundary, with invalid UTF-8 replaced by U+FFFD and control and bidirectional characters removed. Both edges are scrubbed like the leaves and copied out of the arena; an output of ten lines or fewer has an empty tail. `started()` clears any leftover capture, and `completed()` falls back to the observer's `success` and `detail` when the wrapper never ran. **Implementation default:** wrap after `bindMemoryTools` (`runner.zig:200`), because it matches by vtable identity.

- **Dimension 1.1** — A `file_read` call publishes its path as `args_redacted` → Test `test_observed_tool_emits_redacted_arguments`
- **Dimension 1.2** — A known secret holding `"` and `\` and a `ghp_` token in arguments are scrubbed and the JSON stays valid → Test `test_argument_leaf_redaction_keeps_json_valid`
- **Dimension 1.3** — Oversized arguments serialize to valid JSON within 2 KiB, every leaf within 256 bytes → Test `test_argument_summary_stays_bounded`
- **Dimension 1.4** — A 220-line output yields five head lines, five tail lines and `output_line_count` 220 → Test `test_output_edges_keep_head_and_tail`
- **Dimension 1.5** — Edges carry no provider key, `ghp_` token, `Bearer` value, control or bidirectional character → Test `test_output_edges_scrub_secrets_and_controls`
- **Dimension 1.6** — A cut through a multi-byte character and invalid UTF-8 input both yield valid UTF-8 → Test `test_output_edges_stay_valid_utf8`
- **Dimension 1.7** — A failed call reports `status: failed` with its error text in the head → Test `test_failed_call_reports_error_head`
- **Dimension 1.8** — A completion the wrapper never saw uses the observer's outcome, never a previous capture → Test `test_unwrapped_completion_uses_observer_outcome`
- **Dimension 1.9** — A wrapped `memory_store` still writes the in-run store → Test `test_wrapped_memory_tools_stay_bound`
- **Dimension 1.10** — The runner's NullClaw configuration keeps `log_llm_io` off → Test `test_runner_keeps_llm_io_logging_off`

### §2 — The live frames carry the outcome to the browser channel

`ToolCallCompleted` gains optional `status`, `output_head`, `output_tail`, `output_line_count` in `afd_wire` and its Zig mirror; `Published::ToolCallCompleted` bridges them. A frame without them still parses. Each tool frame stays under 3 KiB, so a 16-frame batch fits the 64 KiB cap (`src/runner/daemon/ActivitySender.zig:19`).

- **Dimension 2.1** — The Rust completion round-trips with each status and the edges → Test `test_tool_call_completed_outcome_roundtrip`
- **Dimension 2.2** — The Zig completion encodes the field names Rust parses → Test `test_tool_call_completed_outcome_encodes`
- **Dimension 2.3** — A completion without outcome fields parses with each absent → Test `test_tool_call_completed_without_outcome_parses`
- **Dimension 2.4** — The bridge publishes `status`, `output_head`, `output_tail`, `output_line_count` → Test `test_published_completed_carries_outcome`
- **Dimension 2.5** — A runner batch posted to the activity verb reaches `fleet:{id}:activity` with the outcome → Test `test_activity_tool_outcome_reaches_channel`
- **Dimension 2.6** — Sixteen largest-possible tool frames fit one activity batch → Test `test_tool_frames_fit_activity_batch`

### §3 — Every call ends exactly once, and the event row keeps the trace

The parent tees each parsed frame into a trace keyed by call id: at most 200 calls and 64 KiB serialized. Past the byte cap a call keeps its row without output edges; past 200 calls it counts into `omitted_call_count`. When the child exits for any reason, the parent emits `tool_call_completed{status: interrupted}` for each open call through the forwarder, records it, and logs `tool_call_interrupted`. The trace rides `ReportRequest.tool_calls`. The daemon takes it as raw JSON and narrows it after the report's own fields, so a bad trace never refuses a report. It rewrites each call id to the fenced `{fence}:{n}` the live frames use, then writes the trace beside `response_text` in `UPDATE_FLEET_EVENT_RESULT`. A fenced-out report writes neither; a trace failing a bound is stored `NULL` and logged.

- **Dimension 3.1** — 250 calls keep 200 entries with `omitted_call_count` 50; past 64 KiB entries lose edges, never rows → Test `test_tool_trace_keeps_bounds`
- **Dimension 3.2** — A child killed mid-call yields a live `interrupted` completion and an `interrupted` trace entry → Test `test_child_exit_interrupts_open_calls`
- **Dimension 3.3** — A report with a valid, absent or malformed trace parses, and only a valid trace survives → Test `test_report_tool_calls_never_refuse_report`
- **Dimension 3.4** — An over-bound trace is stored `NULL`, the report settles 2xx and `report_tool_trace_dropped` logs → Test `test_oversize_tool_trace_dropped_report_settles`
- **Dimension 3.5** — A settled report stores the trace with fenced call ids → Test `test_report_writes_tool_calls_with_result`
- **Dimension 3.6** — A stale-fence report writes neither result nor trace → Test `test_fenced_report_writes_no_tool_calls`
- **Dimension 3.7** — Migrating a populated database leaves existing rows `NULL` → Test `test_tool_calls_column_upgrade_keeps_rows_null`

### §4 — The thread and the single-event read serve the trace

`tool_calls` joins `body_columns!`, `EventDetailRow` and `EventDetail`. The events list never selects it, and `event_complete` stays body-free; the browser re-reads the detail at settle (`ui/packages/app/lib/streaming/fleet-stream-detail-reader.ts:28`). The 512 KiB thread page (`message.rs:53`) counts it, so a page holds at least eight traced turns.

- **Dimension 4.1** — The detail read returns the stored trace, and `null` for a row without one → Test `test_event_detail_serves_tool_calls`
- **Dimension 4.2** — Thread items carry `tool_calls` and the page budget counts their bytes → Test `test_thread_page_budget_counts_tool_calls`
- **Dimension 4.3** — The events list response has no `tool_calls` key → Test `test_event_list_omits_tool_calls`
- **Dimension 4.4** — `public/openapi.json` matches the regenerated document → Test `test_openapi_build_is_the_source`

## Interfaces

```
tool_call_completed (activity frame; outcome fields optional, absent from older runners)
  { "name": "file_read", "ms": 12, "call_id": "7:3", "status": "succeeded",
    "output_head": "# agentsfleet\n…", "output_tail": "…\nMIT", "output_line_count": 214 }
  status ∈ succeeded | failed | interrupted

EventDetail.tool_calls  (null = not recorded)
  { "calls": [ { "call_id": "7:3", "name": "file_read", "arguments": {"path": "README.md"},
                 "status": "succeeded", "output_head": "…", "output_tail": "…",
                 "output_line_count": 214, "duration_ms": 12 } ],
    "omitted_call_count": 0 }

Bounds (afd_wire::tool_trace and src/runner/engine/observed_tool.zig, one value each):
  ARGS_LEAF_MAX_BYTES 256 · ARGS_MAX_BYTES 2048 · OUTPUT_EDGE_MAX_LINES 5
  OUTPUT_EDGE_MAX_BYTES 1024 · TRACE_MAX_CALLS 200 · TRACE_MAX_BYTES 65536
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Child dies mid-call | Crash, kill, timeout | Parent closes each open call `interrupted`, live and in the trace (Dimension 3.2) |
| Scrub allocation fails | OOM in a scrub | Arguments or edges dropped, call still closes; `tool_capture_scrub_failed` logs |
| Non-UTF-8 output | Binary HTTP body | Replaced with U+FFFD before JSON; the batch still parses (Dimension 1.6) |
| Secret with no known shape | Token minted mid-run | Visible to fleet readers, per Indy's decision; bounded by the edge caps |
| Over-bound or malformed trace | Runner fault or tampered body | Stored `NULL`, report settles, `report_tool_trace_dropped` logs |
| Stale fence | Reclaimed lease reports late | Report refused as today; no trace written (Dimension 3.6) |
| Daemon rolled back below this release | Old daemon refuses unknown fields | Batches and reports refused; roll runners back first (`runner_fleet.md` §Live activity) |
| Older runner | No outcome, no trace | Fields absent; `tool_calls` stays `NULL` |

## Invariants

1. No value in the runner's redaction set appears in a frame, report or row — leaves and edges scrub before serialization (Dimensions 1.2, 1.5).
2. Every started call ends exactly once — the parent closes open calls at child exit (Dimension 3.2).
3. Every emitted string is valid UTF-8 and every `args_redacted` is a JSON object — built by the standard serializer from repaired text; the daemon's `serde_json::from_str` (`published.rs:93`) is the runtime check.
4. A stored trace never exceeds `TRACE_MAX_CALLS` or `TRACE_MAX_BYTES` — the daemon checks before the write and is the authority.
5. A trace is written only by the statement that settles its event (Dimension 3.6).
6. `log_llm_io` stays off in the runner (Dimension 1.10).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `tool_capture_scrub_failed` (runner log, warn) | ops | A scrub allocation fails | `error_code`, tool name, call id | No argument or output bytes | `test_failed_call_reports_error_head` |
| `tool_call_interrupted` (runner log, info) | ops | The parent closes an open call | lease id, call id, tool name | No argument or output bytes | `test_child_exit_interrupts_open_calls` |
| `report_tool_trace_dropped` (daemon log, warn) | ops | A trace fails parse or a bound | `fleet_id`, `event_id`, reason, byte count | No trace content | `test_oversize_tool_trace_dropped_report_settles` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_observed_tool_emits_redacted_arguments` | `file_read {path: README.md}` → started frame `{"path":"README.md"}` |
| 1.2 | unit | `test_argument_leaf_redaction_keeps_json_valid` | secret `a"b\c` and `ghp_…` in leaves → both scrubbed; JSON parses |
| 1.3 | unit | `test_argument_summary_stays_bounded` | 10 KiB `content` → valid JSON ≤ 2048 bytes, leaf ≤ 256 |
| 1.4 | unit | `test_output_edges_keep_head_and_tail` | 220 lines → head lines 1–5, tail lines 216–220, count 220 |
| 1.5 | unit | `test_output_edges_scrub_secrets_and_controls` | key, `ghp_…`, `Bearer …`, `\x1b`, U+202E → none survive |
| 1.6 | unit | `test_output_edges_stay_valid_utf8` | cut inside `é`, bytes `0xFF 0xFE` → valid UTF-8 with U+FFFD |
| 1.7 | unit | `test_failed_call_reports_error_head` | `success=false, error_msg="denied"` → `failed`, head `denied` |
| 1.8 | unit | `test_unwrapped_completion_uses_observer_outcome` | capture from call 1, unwrapped call 2 → observer outcome |
| 1.9 | unit | `test_wrapped_memory_tools_stay_bound` | wrapped `memory_store` then `memory_recall` → entry found |
| 1.10 | unit | `test_runner_keeps_llm_io_logging_off` | runner-built config → `diagnostics.log_llm_io == false` |
| 2.1 | unit | `test_tool_call_completed_outcome_roundtrip` | each status → JSON → equal value |
| 2.2 | unit | `test_tool_call_completed_outcome_encodes` | Zig frame bytes parse in the Rust fixture |
| 2.3 | unit | `test_tool_call_completed_without_outcome_parses` | `{name, ms}` → outcome fields absent |
| 2.4 | unit | `test_published_completed_carries_outcome` | wire frame → published JSON has all four fields |
| 2.5 | integration | `test_activity_tool_outcome_reaches_channel` | POST batch → subscriber reads the outcome |
| 2.6 | unit | `test_tool_frames_fit_activity_batch` | 16 max-size frames < 64 KiB |
| 3.1 | unit | `test_tool_trace_keeps_bounds` | 250 calls → 200 rows, `omitted_call_count` 50, ≤ 65536 bytes |
| 3.2 | unit | `test_child_exit_interrupts_open_calls` | started, child exit → forwarder sees `interrupted`, trace holds it |
| 3.3 | unit | `test_report_tool_calls_never_refuse_report` | `tool_calls: 7` → report parses, trace dropped |
| 3.4 | integration | `test_oversize_tool_trace_dropped_report_settles` | 201 calls → 2xx, column `NULL`, warn logged |
| 3.5 | integration | `test_report_writes_tool_calls_with_result` | settled report → row trace equals sent, ids `{fence}:{n}` |
| 3.6 | integration | `test_fenced_report_writes_no_tool_calls` | stale token → row unchanged, `tool_calls` `NULL` |
| 3.7 | integration | `test_tool_calls_column_upgrade_keeps_rows_null` | populated db + slot 924 → old rows `NULL` |
| 4.1 | integration | `test_event_detail_serves_tool_calls` | detail read → stored trace; traceless row → `null` |
| 4.2 | integration | `test_thread_page_budget_counts_tool_calls` | heavy traces → page ≤ 512 KiB, cursor set |
| 4.3 | integration | `test_event_list_omits_tool_calls` | list item JSON has no `tool_calls` key |
| 4.4 | unit | `test_openapi_build_is_the_source` | regenerated document equals `public/openapi.json` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The runner emits scrubbed arguments, typed outcomes and closes every call (§1, §3) | `make test-unit-runner` | exit 0 | P0 | |
| R2 | Frames and report carry the outcome across stacks (§2, §3) | `cargo test --manifest-path rustd/Cargo.toml -p afd_wire tool_` | exit 0 | P0 | |
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

- Full arguments and output behind "show all" — M209_003.
- Rendering, grouping and reload hydration — M209_002.
- Live output streaming while a tool runs — hosted tools run in-process and return whole; no long-running process exists to stream.
- Per-tool timeouts and exit codes — NullClaw owns tool execution, and no hosted tool returns an exit code (`tool_bridge.zig:70-86`).
- Inline approval rows and budget notices — the next milestone.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator watches a fleet work, sees `Read README.md` turn green and `POST …` turn red with its error, and after a refresh finds the same rows, including the one the kill interrupted.
2. **Preserved user behaviour** — Live clocks, call pairing, reply streaming and settle timing are unchanged; older runners keep working.
3. **Optimal-way check** — Capturing at the tool (runner-owned) and tracing in the parent (outlives the child) is the direct path; Codex's patterns are adopted where they transfer.
4. **Rebuild-vs-iterate** — Iterate: frames, call ids and the report already exist; this fills their empty fields.
5. **What we build** — One wrapper, one parent trace, four optional frame fields, one report field, one column, one body column.
6. **What we do NOT build** — No live output streaming, no exit codes, no NullClaw change, no per-call table (M209_003 owns full outputs).
7. **Fit with existing features** — Compounds with call ids and the detail re-read; must not destabilise report settlement.
8. **Surface order** — API first; M209_002 renders it.
9. **Dashboard restraint** — A call with no recorded outcome carries none; nothing invents success.
10. **Confused-user next step** — A turn without rows predates the column or had its trace dropped; `report_tool_trace_dropped` names which.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** backend in one Pull Request with M209_003, because both cross the tool-output boundary and own schema; the UI follows as M209_002.
- **Alternatives considered:** `log_llm_io` (rejected: logs model requests and responses, and its ` [truncated]` suffix breaks `published.rs:93`); trace on the child's `ExecutionResult` (rejected: a crashed child sends none); a NullClaw fork change (rejected: the runner can wrap).
- **Patch-vs-refactor verdict:** this is a **patch** because every carrier already exists and only its contents change.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "Codex-style tool rows and others ensure we are able to show more information." On visibility: "The tool call preview like we see in codex nothing must be hidden, isnt codex displaying all". Decision recorded: every member with `fleet:read` sees arguments and output. Secret values stay masked as `«secret:NAME»` under `AGENTS.orly.md` §Hard Safety (resolving or printing credentials is forbidden without override). On full output, Indy chose "Full output on click" → M209_003. Persistence on the event row is an agent default and reversible. Codex lessons adopted: every call ends exactly once, typed status, UTF-8-safe cuts, durable means finished.
- **Metrics review** — No analytics or funnel playbook update required: no new user action; three operator log events added.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
