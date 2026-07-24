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

# M139_003: Signal export stays below its own capacity

**Prototype:** v2.0.0
**Milestone:** M139
**Workstream:** 003
**Date:** Jul 23, 2026
**Status:** DONE
**Priority:** P1 — current runner request spans can exceed exporter drain capacity before useful work begins
**Categories:** API, Observability (OBS)
**Batch:** B2 — implementation output of the M139_002 signal-routing investigation
**Branch:** feat/m139-deadline-scheduler
**Test Baseline:** unit=2824 integration=376
**Depends on:** M139_002 — its architecture decision and capacity audit must be DONE before this opens
**Provenance:** Large Language Model (LLM)-drafted (Codex, Jul 23, 2026) from the M139_002 source audit and Indy's no-log-ingestion decision
**Canonical architecture:** `docs/architecture/observability.md` §Signal routing decision and §Capacity and loss audit

---

## Overview

**Goal (testable):** High-rate runner traffic cannot exceed a fixed request-span budget, every OpenTelemetry Protocol (OTLP) exporter drains its cycle-start backlog without one request per entry, local loss is visible on Prometheus, export attempts have monotonic deadlines, and one accepted terminal report captures one deterministic `FleetCompleted` PostHog event.

**Problem:** Every matched HTTP request currently emits a span. At the 10-second heartbeat cadence, 100 idle runners already produce the trace exporter's full steady drain budget of 10 spans per second. Logs and traces each remove only 50 entries every 5 seconds, even when their fixed rings hold far more. Ring-full discard is not exposed for logs or traces, and failed HTTP export discards an already-drained batch locally without proving whether the remote accepted it. PostHog is installed in `agentsfleetd`, but the declared `FleetCompleted` event has no production call.

**Solution summary:** Wrap admission and handler dispatch in one route-aware trace lifetime. Wall time supplies the epoch start; the boot clock supplies elapsed time and the admission window. Suppress high-rate successes, enforce fixed disjoint budgets, and count every suppression. Keep the existing fixed rings and use Zig's standard event as a coalesced wake: only accepted pushes count, and one consumer drains the entries present when a cycle begins. Race each destructive HTTP post against a boot-clock deadline, attempt it once, and publish fixed-label queue/loss counters through `/metrics`. Keep exporter warnings on stderr without feeding them back into the OTLP log ring. Capture `FleetCompleted` after the report fence wins with a deterministic `$insert_id`. Existing model and workspace metric labels remain unchanged in this correction.

The eventual Pull Request (PR) is internal reliability work. It changes no public endpoint or payload shape.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(observability): bound signal volume and expose exporter health
- **Intent (one sentence):** Useful traces and analytics survive fleet growth without making telemetry a request-path dependency.
- **Handshake:** `ASSUMPTIONS I'M MAKING: 1. Successful runner control requests suppress their generic HTTP span because accepted work retains its settled delivery span. 2. Trace lifetime starts after route match but before admission. 3. The process admits four runner 4xx responses, four server 5xx responses, and two sampled generic responses per monotonic second; excess is counted. 4. Exporters count only accepted pushes toward a coalesced wake and drain the cycle-start backlog. 5. A destructively collected OTLP batch is attempted once under a monotonic deadline. 6. Exporter warnings remain on stderr without re-entering the OTLP log queue. 7. FleetCompleted fires only after the fenced report claim succeeds and carries a fixed 64-character Secure Hash Algorithm 256-bit (SHA-256) $insert_id. 8. Existing model and workspace metric labels are preserved.`

## Implementing agent — read these first

1. `docs/architecture/observability.md` — chosen routing, exact queue capacities, and prohibited runner paths.
2. `src/agentsfleetd/http/server.zig`, `router.zig`, and `route_table.zig` — current unconditional span, pre-handler admission return, and total route switches.
3. `src/agentsfleetd/observability/otlp/exporter.zig`, `ring.zig`, and `Client.zig` — one consumer, destructive collection, flush lifecycle, and HTTP failure behavior.
4. `src/agentsfleetd/observability/otel_logs.zig`, `otel_traces.zig`, and `otel_metrics.zig` — per-signal batch shape and capacity.
5. `src/agentsfleetd/observability/metrics_render.zig` and `metrics_runner.zig` — fixed-label Prometheus rendering and allocator-free counters.
6. `src/agentsfleetd/fleet/service_report.zig` and `observability/telemetry_events.zig` — fenced report claim and the existing completion event type.
7. `src/agentsfleetd/main.zig`, `cmd/serve.zig`, and `cmd/preflight.zig` — process-owned cancel-capable input/output lifetime, exporter installation, and stderr/OTLP log fan-out.
8. Ghostty `src/datastruct/blocking_queue.zig` and repository `src/lib/common/sync.zig` — standard single-consumer queue wake and stop ordering; borrow the synchronization pattern, not code.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/route_trace.zig` | CREATE | Own total route policy and deterministic trace sampling. |
| `src/agentsfleetd/http/route_trace_test.zig` | CREATE | Prove noisy-route suppression, sampling, runner rejection retention, and server-error retention. |
| `src/agentsfleetd/http/server.zig` | EDIT | Start matched-route trace lifetime before admission and emit only when policy selects the completed response. |
| `src/agentsfleetd/http/route_trace_integration_test.zig` | CREATE | Prove a real admission-shed runner request reaches trace policy. |
| `src/agentsfleetd/main.zig` | EDIT | Keep exporter-internal warnings on stderr while excluding them from the OTLP log sink. |
| `src/agentsfleetd/cmd/serve.zig` | EDIT | Pass the process-owned cancel-capable input/output handle into exporter installation. |
| `src/agentsfleetd/cmd/preflight.zig` | EDIT | Thread the borrowed input/output handle through all three exporter installs. |
| `src/agentsfleetd/observability/otlp/exporter.zig` | EDIT | Drain bounded batches while work remains, post once, and record local discard or uncertain delivery. |
| `src/agentsfleetd/observability/otlp/exporter_test.zig` | EDIT | Prove wake/drain, at-most-once posting, deadline, shutdown, and concurrency behavior. |
| `src/agentsfleetd/observability/otlp/Client.zig` | EDIT | Borrow the process input/output handle, race each HTTP post against a monotonic timeout using Zig standard `std.Io` selection, and cancel the loser. |
| `src/agentsfleetd/observability/otlp/Client_test.zig` | EDIT | Prove a stalled endpoint times out and an ambiguous failure is never retried. |
| `src/agentsfleetd/observability/otel_logs.zig` | EDIT | Report queue admission and expose pending/discard snapshots to the shared exporter. |
| `src/agentsfleetd/observability/otel_logs_test.zig` | EDIT | Prove full-ring discard and multi-batch drain. |
| `src/agentsfleetd/observability/otel_traces.zig` | EDIT | Report queue admission and expose pending/discard snapshots to the shared exporter. |
| `src/agentsfleetd/observability/otel_traces_test.zig` | EDIT | Prove full-ring discard and selected-span capacity. |
| `src/agentsfleetd/observability/otel_metrics.zig` | EDIT | Count accepted pushes, ring loss, and aggregation-cap loss while preserving existing labels. |
| `src/agentsfleetd/observability/otel_metrics_test.zig` | EDIT | Preserve model/workspace aggregation and prove accepted-push accounting. |
| `src/agentsfleetd/observability/metrics_otel.zig` | CREATE | Hold fixed atomic counters by signal and reason, with no dynamic labels. |
| `src/agentsfleetd/observability/metrics_otel_test.zig` | CREATE | Prove exact concurrent increments and rendering snapshots. |
| `src/agentsfleetd/observability/metrics_trace.zig` | CREATE | Hold fixed trace-suppression counters by policy reason and own the exported family name. |
| `src/agentsfleetd/observability/metrics_trace_test.zig` | CREATE | Prove exact concurrent suppression accounting with fixed labels. |
| `src/agentsfleetd/observability/metrics_render.zig` | EDIT | Render OTLP queue depth, local discards, and uncertain delivery. |
| `src/agentsfleetd/observability/metrics.zig` | EDIT | Register the new self-metric tests and exports. |
| `src/agentsfleetd/observability/telemetry_events.zig` | EDIT | Add deterministic PostHog `$insert_id` to `FleetCompleted`. |
| `src/agentsfleetd/observability/telemetry_fleet_test.zig` | EDIT | Prove the insertion identifier property and exact event shape. |
| `src/agentsfleetd/observability/telemetry.zig` | EDIT | Add a cross-thread capture tally so an integration test can observe a capture made on a server thread. |
| `src/agentsfleetd/observability/telemetry_test.zig` | EDIT | Keep the existing non-throwing capture proofs after the typed-outcome layer was dropped. |
| `src/agentsfleetd/fleet/service_report.zig` | EDIT | Call completion capture only after the fenced report claim succeeds. |
| `src/agentsfleetd/fleet/service_billing.zig` | EDIT | Restore the model and workspace arguments the correction preserves. |
| `src/agentsfleetd/fleet/integration_roundtrip_test.zig` | EDIT | Prove once-only completion capture and every unaccepted-report class through the real HTTP + database report path. |
| `src/agentsfleetd/observability/otel_metrics_cardinality.zig` | RESTORE | Keep the process-local workspace guard the correction preserves. |
| `src/agentsfleetd/observability/otel_metrics_payload.zig` | EDIT | Keep the `model` and `workspace` label keys in OTLP metric payloads. |
| `src/agentsfleetd/observability/otel_metrics_aggregate_test.zig` | EDIT | Keep model/workspace fixtures in the same-label-set aggregation proof. |
| `src/agentsfleetd/tests.zig` | EDIT | Register the new integration test. |
| `docs/v2/pending/M139_004_P1_OBS_TELEMETRY_SEMANTIC_CONVENTIONS.md` | EDIT | Take the Prometheus family-prefix normalization into the semantic-conventions workstream that owns metric naming. |
| ~~`audits/signal-routing.sh`~~ | RETIRED | Updated here to track the shipped limits, then removed in M139_001's review pass — re-pinning source literals on every tuning change was the maintenance cost that proved the approach wrong. The bounded-export behavior tests in this workstream are the durable assertion. |
| `docs/architecture/observability.md` | EDIT | Replace follow-on wording with the shipped policy and measured drain result. |
| `docs/architecture/runner_fleet.md` | EDIT | Mark route filtering and completion capture as implemented. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — Observability (OBS), No Dead Code (NDC), timing, concurrency, test naming, and source-grounding rules apply.
- **`dispatch/write_zig.md`** — total switches, bounded memory, explicit lifecycle, tagged outcomes, file shape, and Linux cross-build requirements.
- **`docs/LIFECYCLE_PATTERNS.md`** — exporter stop must wake, drain within its cap, join, and then free state.
- **`docs/LOGGING_STANDARD.md`** — exporter failure logs stay structured and cannot recursively amplify an outage.
- **`dispatch/name_architecture.md`** — routes and signal ownership remain those selected by M139_002.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| Zig (ZIG) / Public Surface (PUB) / Struct-Shape | yes | New route and metric modules have production callers, narrow public surfaces, and exhaustive switches. |
| File & Function Length | yes | Keep policy and self-metrics in focused files; split tests before any cap is crossed. |
| String Literals Are Always Constants (UFS) | yes | Metric names, reasons, signals, sample denominator, span budgets, batch thresholds, transport deadline, and event names are named once. |
| LOGGING | yes | One structured exporter failure record per failed batch reaches stderr and is excluded from the OTLP log sink. |
| LIFECYCLE | yes | Stop wakes the consumer; every post and final drain obey one monotonic shutdown deadline. |
| ERROR REGISTRY | no | Telemetry discard is an internal metric and existing warning, not a new user error. |
| Schema / User Interface (UI) / Design Token | no | No SQL or user-interface files change. |

## Prior-Art / Reference Implementations

- **Ghostty `blocking_queue.zig`:** standard condition-backed bounded producer/consumer behavior with explicit full/empty waits and one consumer.
- **Zig 0.16 `std.process.Init.io`, `std.Io.Event`, and `std.Io.Select`:** borrow the process-owned cancel-capable `std.Io.Threaded` handle. Producers use the event's atomic `set`; the consumer uses its monotonic timeout wait, and HTTP uses select for post versus monotonic timeout. Do not use `common.globalIo()`, which is backed by the non-canceling single-threaded input/output instance, and do not create a second thread pool or custom timer queue.
- **Repository `otlp/ring.zig`:** retain the existing fixed multi-producer/single-consumer ring and drop-on-full admission; do not replace it with a dynamic map or unbounded list.
- **Repository `metrics_runner.zig`:** fixed label and atomic counter precedent with deterministic overflow behavior.
- **Repository `call_deadline/scheduler.zig`:** stop, wake, join, and deinit order; the exporter does not reuse deadline scheduling or wall time for queue ordering.

## Sections (implementation slices)

### §1 — Route policy removes trace noise before enqueue

Define the route class from route and response status, then admit it through fixed monotonic-second budgets. Start the matched-request trace lifetime before the API admission gate, complete it after either an early response or handler dispatch, and make policy the single enqueue decision. Successful health, metrics, heartbeat, lease, renew, activity, and runner-report HTTP spans suppress; the lease rule intentionally covers both `lease: null` and a granted lease because the accepted run retains its settled `fleet.delivery` span. Apply status precedence before route class: every status 500 or above enters only the server-error bucket; a matched runner route at status 400 through 499 enters only the runner-rejection bucket. Other eligible responses below status 500 use deterministic 1-in-100 head sampling from the server-generated span identifier, never the caller-controlled inbound trace identifier, then enter the sampled-success bucket. The exhaustive route switch places every remaining route in that sampled class and forces review when a variant is added.

The process budget is 10 generic request spans per monotonic second, matching the audited pre-refactor trace drain ceiling: four runner rejections, four server errors, and two sampled successes. Each bucket uses atomic fixed-window admission with no allocator and no global lock. Excess requests do not enqueue a span and increment `agentsfleet_http_trace_suppressed_total` under a fixed reason. Existing structured rejection logs and request counters remain; the aggregate makes the full rejection volume visible without allowing an invalid-token flood to evict useful spans.

- **Dimension 1.1 — DONE** — every current route has an explicit trace class; noisy runner successes suppress → Test `test_trace_policy_is_total_and_suppresses_runner_chatter`
- **Dimension 1.2 — DONE** — admission and elapsed time use the boot clock while exported timestamps stay epoch-based → Test `test_request_span_elapsed_ignores_wall_clock_adjustment`
- **Dimension 1.3 — DONE** — runner rejections, including pre-handler admission shedding, and server errors obey hard budgets; every excess is counted → Tests `test_trace_error_budgets_are_hard` and `test_runner_admission_rejection_is_traced_or_counted`

### §2 — Export drains by work, not one batch per interval

Keep one fixed ring and one consumer per signal. Only a successful push increments the accepted-since-cycle counter. Reaching 50 logs, 50 traces, or 768 metrics sets one coalesced `std.Io.Event`; lower traffic waits for the maximum interval. On wake, the consumer snapshots pending entries and drains at most that count across bounded batches, so new producers cannot starve stop. Stop stores one absolute boot-clock deadline, sets the event, and joins before the process deinitializes the borrowed input/output handle. Each collected body is posted once. `Client` races that post against the earlier normal or shutdown boot-clock deadline with `std.Io.Select`, then cancels the loser. Rejection, validated partial rejection, timeout, transport failure, and malformed partial response update distinct fixed counters. No response outcome replays a possibly accepted delta batch.

- **Dimension 2.1 — DONE** — below-threshold entries batch; the threshold coalesces wakes; 100 producers never block → Test `test_exporter_preserves_batching_and_coalesces_wakes`
- **Dimension 2.2 — DONE** — one cycle drains its cycle-start backlog across multiple bounded collections → Test `test_exporter_drains_cycle_start_backlog`
- **Dimension 2.3 — DONE** — every collected body is attempted once and a stalled peer is canceled at the monotonic deadline → Tests `test_exporter_never_retries_destructive_batch` and `test_otlp_post_stall_times_out`
- **Dimension 2.4 — DONE** — stop wakes, drains within one shared deadline, joins, and reports exact removed entries on serialization failure → Tests `test_exporter_stop_wakes_drains_and_joins` and `test_exporter_serialization_failure_counts_local_discards`

### §3 — Prometheus exposes exporter health

Add fixed atomic values for `logs`, `traces`, and `metrics`. Render queue depth gauges plus counters for `ring_full`, `aggregate_cap`, `serialize_failed`, `partial_rejected`, `export_rejected`, and `export_uncertain`. The metric aggregator reports every sample discarded after its fixed 256-series ceiling as `aggregate_cap`; this stays visible even when OTLP itself is dark. `partial_rejected` is the validated signal-specific count returned in an OTLP partial-success response. `export_rejected` means the backend returned a non-success HTTP status. Reserve `export_uncertain` for timeout, transport, malformed partial-success, or impossible rejected-count outcomes whose remote acceptance cannot be proven. No endpoint, route, workspace, runner, trace, lease, or event label is allowed.

This correction does not change existing business metric labels or Grafana queries. Cardinality policy needs a coherent dashboard and migration decision of its own; model/workspace behavior therefore remains exactly as it was before this workstream.

- **Dimension 3.1 — DONE** — all three queues render depth and the fixed `ring_full`, `aggregate_cap`, `serialize_failed`, `partial_rejected`, `export_rejected`, and `export_uncertain` reasons with valid Prometheus families → Test `test_otlp_self_metrics_render_fixed_labels`
- **Dimension 3.2 — DONE** — at least 100 concurrent producers increment exact totals without allocation or a global serialization lock → Test `test_otlp_self_metrics_are_concurrent_and_exact`
- **Dimension 3.3 — DONE** — an OTLP outage cannot recursively fill the log exporter with its own warnings; structured warnings still reach stderr → Test `test_exporter_failure_scope_bypasses_otlp_log_sink`
- **Dimension 3.4 — DONE** — model/workspace labels and their existing guard remain unchanged while exporter-health labels stay fixed → Test `test_existing_metric_labels_are_preserved`

### §4 — PostHog completion closes the business funnel once

After `claimReportAndSettle` returns `claimed=true`, submit `FleetCompleted` with workspace as the distinct identifier and the existing typed business properties. Saturate runner-controlled integers at `maxInt(i64)`. Set PostHog `$insert_id` to the lowercase SHA-256 digest of `fleet_id || 0x00 || event_id`. The report fence prevents a second local submission; the deterministic identifier lets PostHog deduplicate its own batch retry. A fenced or failed report submits nothing. The existing nullable, non-throwing capture behavior remains unchanged.

- **Dimension 4.1 — DONE** — one accepted report makes at most one completion submission with exact settled properties and deterministic insertion identifier → Test `test_settled_report_captures_fleet_completed`
- **Dimension 4.1 — DONE** — maximum `tokens`, `wall_ms`, and first-token values saturate to `maxInt(i64)` without trapping after settlement → Test `test_fleet_completed_saturates_runner_u64_properties`
- **Dimension 4.2 — DONE** — fenced, replayed, malformed, and failed-settlement reports capture nothing → Test `test_unaccepted_report_never_captures_completion`
- **Dimension 4.3 — DONE** — a missing PostHog client remains a no-op and never changes report handling → Test `test_completion_analytics_remains_optional`

## Interfaces

```text
TracePolicy.decide(route, status, server_span_id, monotonic_now) -> suppress { reason } | emit
Exporter.install(config, process_io) -> installed | already_running | spawn_failed
Exporter.notifyAccepted() -> void
Exporter flush hook -> body plus removed-entry and exported-item counts
metrics_otel.record(signal, reason, count) -> void
FleetCompleted.init(settled facts) -> typed event with deterministic insertion identifier
```

No public HTTP path, request field, response field, environment name, or runner binary interface changes.

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Runner request storm | heartbeat, lease, renew, activity, report, malformed, or invalid-token volume | successes suppress; 400–499 responses emit within the runner-rejection bucket, 500–599 responses use only the server-error bucket, and every excess increments a fixed aggregate counter. |
| Caller forces sampling | inbound trace identifier is chosen to hit a sample bucket | sample only the server-generated span identifier and retain a hard sampled-success budget. |
| Ring full | producers outrun the consumer | drop immediately, increment `ring_full`, and leave product work untouched. |
| Metric aggregation ceiling | one flush contains more than 256 distinct label sets | discard excess samples, increment `aggregate_cap`, and preserve the existing OTLP drop signal. |
| Serialization failure | batch exceeds scratch or encoding fails | count exact local discards as `serialize_failed`; continue with later entries. |
| OTLP partial success | backend accepts the request but reports rejected signal items | parse the fixed signal-specific rejected count, validate it does not exceed the attempted batch, record `partial_rejected`, ignore collector text, and do not retry. |
| OTLP endpoint rejection | backend returns a non-success HTTP status | post once, count entries as `export_rejected`, and do not retry. |
| OTLP outcome is ambiguous | timeout, transport error, response lost after acceptance, malformed partial success, or impossible rejected count | post once, count entries as `export_uncertain`, and never replay a possibly accepted delta batch. |
| Failure warning feeds log exporter | OTLP log outage emits its own warning | exporter scopes bypass the OTLP log sink and still reach structured stderr; the self-metric remains authoritative. |
| Non-canceling input/output handle | exporter accidentally uses `common.globalIo()` | installation requires the process-owned handle; the stalled-peer test fails if cancellation is unavailable. |
| Stop during wait, post, or an active backlog drain | process shutdown begins | set the standard event, compute one absolute monotonic shutdown deadline, check it between every batch, cap an in-flight or final post by its remaining duration, start no post after expiry, join exporters, then let the root process deinitialize the borrowed handle. |
| Runner reports `u64` value above PostHog integer range | hostile or malformed terminal report | saturate at `maxInt(i64)` in typed event properties; durable report handling continues. |
| Fenced report replay | stale holder or uncertain response retries | report claim loses; no completion capture and no duplicate metrics. |
| PostHog unavailable | missing key, init error, or capture failure | nullable capture returns; report success and durable settlement remain unchanged. |
| PostHog retry after accepted response is lost | library retries the same batch | the deterministic 64-character lowercase SHA-256 digest of `fleet_id || 0x00 || event_id` in `$insert_id` lets PostHog deduplicate the logical completion event. |

## Invariants

1. No runner log, span, metric sample, or backend credential is added.
2. Trace selection occurs before enqueue, uses server-owned sampling entropy, and admits no more than the named fixed budgets per monotonic second.
3. Every status 500 or above enters only the bounded server-error bucket; matched runner status 400 through 499 enters only the bounded runner-rejection bucket, including admission rejection. Every suppressed excess is counted. Successful noisy runner routes, including empty and granted lease responses, are never selected as generic HTTP spans.
4. Queue admission is bounded and non-blocking; telemetry cannot add request backpressure.
5. A destructively collected batch has one HTTP attempt; ambiguous delivery is never replayed.
6. Exporter self-metrics use fixed signal and reason sets with no caller-provided labels.
7. `FleetCompleted` follows the same winning fenced report claim as existing settlement metrics, and its deterministic `$insert_id` is stable across PostHog delivery retries.
8. Signal timestamps remain epoch-based; queue waits, post timeouts, and shutdown bounds use monotonic time from the process-owned cancel-capable input/output handle.

## Metrics & Observability

| Metric / event | Fires when | Labels | Loss or privacy guard | Test proof |
|---|---|---|---|---|
| `agentsfleet_otlp_queue_depth` | `/metrics` renders | `signal` from fixed set | no tenant or request identity | `test_otlp_self_metrics_render_fixed_labels` |
| `agentsfleet_otlp_entries_discarded_total` | ring full, aggregation cap, serialization failure, collector-declared partial rejection, rejected export, or ambiguous one-shot export | fixed `signal`, fixed `reason` | exact local or collector-declared count; `export_uncertain` does not claim remote loss | `test_exporter_never_retries_destructive_batch` |
| `agentsfleet_http_trace_suppressed_total` | noisy route, sample miss, or fixed span budget rejects enqueue | fixed `reason` only | exact aggregate; no route, caller identifier, or tenant label | `test_trace_error_budgets_are_hard` |
| `fleet_completed` | accepted terminal report wins its fence | existing typed business properties plus deterministic `$insert_id` | no response body, prompt, credential, or error text | `test_settled_report_captures_fleet_completed` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | unit | `test_trace_policy_is_total_and_suppresses_runner_chatter` | every route compiles through one switch; empty and granted lease responses plus other noisy 2xx routes suppress. |
| 1.2 | unit | `test_request_span_elapsed_ignores_wall_clock_adjustment` | elapsed time comes from boot-clock timestamps and produces a non-negative epoch end without a second wall-clock read. |
| 1.3 | unit | `test_trace_error_budgets_are_hard` | 100 concurrent callers emit exactly four runner 4xx rejections, four 5xx server errors, two sampled successes, count every excess, and never charge a runner 5xx to both buckets. |
| 1.3 | integration | `test_runner_admission_rejection_is_traced_or_counted` | an admission-shed runner request enters the bounded 429 trace path before handler dispatch; exhausted budget increments suppression. |
| 2.1 | unit | `test_exporter_drains_cycle_start_backlog` | a full ring drains in several bounded posts during one cycle. |
| 2.1 | unit | `test_exporter_preserves_batching_and_coalesces_wakes` | accepted pushes below threshold wait; threshold wakes once; 100 producers finish without a consumer lock. |
| 2.3 | integration | `test_otlp_post_stall_times_out` | a connected silent peer is canceled and cannot hold the exporter beyond the monotonic deadline. |
| 2.4 | unit | `test_exporter_stop_wakes_drains_and_joins` | stopped standard waiter wakes, bounded drain ends, allocator and thread count return clean. |
| 2.4 | unit | `test_exporter_serialization_failure_counts_local_discards` | the collect hook reports every entry removed before encoding failed. |
| 3.1 | unit | `test_otlp_self_metrics_render_fixed_labels` | valid families include three signals and the six fixed reasons only; aggregation-cap loss remains visible without OTLP. |
| 3.2 | unit | `test_otlp_self_metrics_are_concurrent_and_exact` | 100 producers preserve exact total and no hidden global lock. |
| 3.3 | unit | `test_exporter_failure_scope_bypasses_otlp_log_sink` | exporter-only warnings reach stderr while the OTLP log queue drains to zero and stays empty. |
| 3.4 | unit | `test_existing_metric_labels_are_preserved` | model/workspace payload labels and their existing guard remain present. |
| 4.1 | integration | `test_settled_report_captures_fleet_completed` | accepted report enqueues once with the fixed SHA-256 `$insert_id`; replay emits zero. |
| 4.1 | unit | `test_fleet_completed_saturates_runner_u64_properties` | maximum runner-controlled numeric values serialize as `maxInt(i64)` without a safety trap. |
| 4.2 | integration | `test_unaccepted_report_never_captures_completion` | replay, stale fence, malformed body, and database failure emit zero. |
| 4.3 | unit | `test_completion_analytics_remains_optional` | missing PostHog client remains a no-op. |

## Acceptance Rubric (single scoring surface)

| Identifier (ID) | Weight | Criterion | Verify command | Pass condition |
|----|--------|-----------|----------------|----------------|
| R1 | 15 | Runner chatter suppressed | `make test-unit-agentsfleetd` | route policy tests pass |
| R2 | 15 | Full backlog drains in one cycle | `make test-unit-agentsfleetd` | cycle-start entries reach zero |
| R3 | 10 | Destructive batches are at-most-once | `make test-unit-agentsfleetd` | one collect and one post under every failure class |
| R4 | 10 | Discard and uncertainty visible on Prometheus | `make test-unit-agentsfleetd` | fixed families and exact local totals pass |
| R5 | 10 | Completion is once-only | `make test-integration` | report acceptance and replay tests pass |
| R6 | 10 | Telemetry never blocks product work | `make test-unit-agentsfleetd` | every injected backend failure preserves outcome |
| R7 | 10 | Concurrent queue remains bounded | `make test-unit-agentsfleetd` | 100 producers finish with exact accepted+discarded total |
| S1 | 5 | Zig checks pass | `make lint-all` | exits 0 |
| S2 | 5 | Repository units pass | `make test-unit-all` | exits 0 |
| S3 | 5 | Integration suite passes | `make test-integration` | exits 0 |
| S4 | 5 | Both Linux targets build | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | both exit 0 |

**Pass threshold:** 100/100. Any duplicate completion, unbounded queue, hidden discard, or product-path dependency returns to implementation.

## Dead Code Sweep

Remove no existing signal type. Confirm `FleetCompleted` gains a production caller, every new trace-policy branch is reachable, and no older unconditional span helper remains callable.

## Out of Scope

- Runner log ingestion, runner OTLP exporter, runner PostHog, or a new telemetry endpoint.
- Trace fields on lease, heartbeat, renew, activity, or report payloads.
- Host collector installation, Loki retention, dashboards, or alerts.
- Changing the Prometheus runner families or the activity batching protocol.
- Deploying the metric `deltatocumulative` collector; exporter health becomes visible first.
- Per-arm scheduler metrics or spans.

## Product Clarity (authoring record)

1. **Whose problem?** Operators diagnosing fleet and exporter health at growing runner counts.
2. **What can they do after this?** Trust that traces represent useful work and see when export loses data.
3. **Best direct way?** Filter before enqueue, drain bounded queues promptly, and expose local discard plus uncertain delivery through pull metrics.
4. **Larger system justified?** No. Existing fixed rings, one consumer, route enum, and Prometheus endpoint are sufficient.
5. **Smallest useful version?** Route filtering, multi-batch drain, exporter-health metrics, and once-only completion capture.
6. **What is not shipped?** Runner collectors, distributed runner tracing, dashboards, or new protocol fields.
7. **Does it compound existing strengths?** Yes — fixed memory, accepted-report derivation, nullable analytics, and total route switches.
8. **User-interface or command-line impact?** None.
9. **Dashboard required now?** No; the metric families can be scraped before visualization exists.
10. **How does an operator verify it?** Scrape queue/discard metrics and compare selected trace rate with accepted run rate.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** focused refactor of the shared exporter plus additive route policy and completion call. All three exporters already share lifecycle code, so fixing each separately would recreate drift.
- **Rejected:** increasing ring sizes alone. It delays loss but leaves the fixed drain ceiling and invisible failure.
- **Rejected:** application-layer retry of a collected batch, whether by requeueing entries or replaying immutable bytes. OTLP JSON has no idempotency key; a lost response after acceptance can duplicate logs and spans and double-count delta metrics.
- **Rejected:** exporting runner telemetry directly. It distributes credentials and retry workers to every host and violates M139_002.
- **Reference choice:** retain the repository fixed ring and one consumer; borrow Ghostty's wake shape and use Zig 0.16 `std.process.Init.io`, atomic `std.Io.Event` notification, and `std.Io.Select` for the HTTP post/timeout race. Stop follows wake→one shared absolute monotonic deadline across active and final drains→join ordering, and the root process remains the sole input/output owner.

## Discovery (consult log)

- **M139_002 finding:** logs drain 50 entries per 5 seconds from a 2047-entry usable ring; traces drain 50 from 1023; neither exports ring loss.
- **M139_002 finding:** 100 idle runners at the 10-second heartbeat cadence produce 10 matched requests per second, equal to current trace drain capacity before lease traffic.
- **M139_002 finding:** failed OTLP posts lose already-collected entries; the metric signal alone emits a later self-drop sample.
- **M139_002 finding:** PostHog production calls cover server start, workspace create, fleet trigger, and signup bootstrap; `FleetCompleted` is declared-only.
- **Indy decision:** raw runner logs do not pass through an `agentsfleetd` API; this spec adds no such path.
- **Architecture consult:** `observability.md` selects backend ownership; `runner_fleet.md` preserves outbound bounded facts and host-local raw logs.
- **Review correction:** destructive OTLP batches use at-most-once posting because replay can double-count delta metrics; every post gains a monotonic deadline.
- **Review correction:** exporters borrow `std.process.Init.io`; `common.globalIo()` cannot satisfy selection or cancellation, and an extra custom runtime is unnecessary.
- **Review correction:** successful lease HTTP spans suppress for both empty and granted outcomes; the settled delivery span carries useful run work without a response-body policy seam.
- **Review correction:** matched-request trace lifetime starts before admission so a shed runner request retains its 429 span.
- **Review correction:** exporter-internal scopes bypass the OTLP log sink and remain on stderr, preventing a failed log exporter from generating its own endless input.
- **Review correction:** exporter wake is threshold-or-interval, not per-entry, so low-rate traffic preserves batching and high-rate traffic drains promptly.
- **Review correction:** generic request spans have a hard 10-per-second process budget; runner rejections and server errors above their reserved buckets remain visible as aggregate suppression counters.
- **Review correction:** shutdown computes one absolute monotonic deadline and applies its remaining duration across every active or final batch, preventing backlog depth from multiplying the shutdown bound.
- **Review correction:** sampling uses the server-generated span identifier and a hard budget, so caller-controlled `traceparent` cannot force unbounded selection.
- **Review correction:** `FleetCompleted` carries the fixed SHA-256 digest of `fleet_id || 0x00 || event_id` as `$insert_id`, closing duplicate remote delivery under PostHog batch retry without an unbounded analytics key.
- **Review correction:** metrics wakes at 768, leaving 255 usable slots while the consumer is scheduled; waking only at full creates avoidable loss.
- **Review correction:** status precedence makes all 5xx responses server errors and only runner 4xx responses runner rejections, so the reserved buckets are disjoint.
- **Review correction:** `std.Io.Event.set` replaces semaphore notification because the event producer side is atomic and does not take the input/output mutex.
- **Review correction:** `FleetCompleted` saturates runner-controlled `u64` values before PostHog integer serialization, including maximum-value tests.
- **Review correction:** Prometheus includes the metric aggregator's 256-series `aggregate_cap` loss, so an OTLP outage cannot hide it.
- **Review correction:** non-success HTTP responses use `export_rejected`; `export_uncertain` is reserved for transport outcomes whose remote acceptance cannot be proven.
- **Review correction:** successful OTLP partial rejection parses the fixed per-signal count into `partial_rejected`; malformed or impossible responses become `export_uncertain`, collector text is discarded, and neither outcome is replayed.
- **Indy correction:** preserve PostHog completion capture, but do not add a second submission-result abstraction around the existing non-throwing telemetry boundary.
- **Indy correction:** preserve existing model/workspace metric labels and their dashboard behavior; cardinality changes require a separate coherent decision.
- **Metrics review:** queue depth, fixed local discard reasons, uncertain delivery, and one selected completion event are the complete new signal set.
- **Implementation finding:** `collectLogsBody` and `collectSpansBody` wrote the OTLP envelope prefix and then returned early when the ring drained empty. `errdefer` does not run on a successful return, so every empty collect stranded that buffer. Production masked it (the flush cycle allocates from a stack `FixedBufferAllocator` and only collects while `pending_count() > 0`), and the unit suite never ran the path because the integration tests that reach it skip without a database. `make test-integration` surfaced it. Both signals now free the envelope on that path and carry a regression test.
- **Implementation finding:** the trace-suppression counters were maintained but the rendered Prometheus family was `fleet_http_trace_suppressed_total`, while this spec and `observability.md` both name `agentsfleet_http_trace_suppressed_total`. `dispatch/name_architecture.md` gives the architecture document precedence, and the family is new on an unpushed branch with no external consumer, so the source was renamed to match and the name now lives once in `metrics_trace.zig`.
- **Implementation finding:** the durable audit's `FleetCompleted` production-call detector was a single-line pattern, but the shipped capture wraps its arguments across lines. It would have reported the event as `declared-only` forever. The detector is now `--multiline`.
- **Tier correction:** Dimension 3.3's test performs in-process sink routing with no input/output, so it is a unit test. Labeling it integration would have inflated the integration tier of the test-depth gate. The three tests that genuinely cross a boundary — the admission-shed request, the stalled OTLP peer, and the report round trip — carry the repository's `integration:` name prefix so the depth gate counts them in the right tier.
- **Skill-driven review chain:** `/write-unit-test` audit at VERIFY; runtime review route and `kishore-babysit-prs` recorded in PR Session Notes.
- **Deferrals:**
  > Indy (2026-07-23): "The prometheus naming is fixed in M139_004 ... will this suffice?" — context: the split between `fleet_*` families in `metrics_render.zig` and `agentsfleet_*` families elsewhere. It did not suffice as written, so M139_004 §4 gained the family-prefix normalization, Dimension 4.3, and its test row; the deferral now has a real owner.
