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
**Status:** IN_PROGRESS
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

**Goal (testable):** High-rate runner traffic cannot exceed a fixed generic-request span budget, rejected runner requests remain visible through bounded spans plus exact aggregate counters, every OpenTelemetry Protocol (OTLP) exporter drains queued batches without a fixed 50-entry-per-interval ceiling or one-request-per-entry regression, local discard and uncertain delivery are visible on Prometheus, every export attempt has a monotonic deadline, and one accepted terminal report has one deterministic `FleetCompleted` PostHog insertion identifier.

**Problem:** Every matched HTTP request currently emits a span. At the 10-second heartbeat cadence, 100 idle runners already produce the trace exporter's full steady drain budget of 10 spans per second. Logs and traces each remove only 50 entries every 5 seconds, even when their fixed rings hold far more. Ring-full discard is not exposed for logs or traces, and failed HTTP export discards an already-drained batch locally without proving whether the remote accepted it. PostHog is installed in `agentsfleetd`, but the declared `FleetCompleted` event has no production call.

**Solution summary:** Add total route-aware trace policy around admission and handler dispatch: suppress successful health and high-rate runner-control spans, select other traffic from server-owned entropy, enforce a hard 10-span-per-second generic-request budget split across disjoint runner 4xx, server 5xx, and sampled-success buckets, and count every budget suppression. Emit one settled delivery span per run. Refactor the shared exporter around the existing fixed ring plus Zig standard-library event/select wakeups, waking only at a reachable per-signal threshold, the maximum flush interval, or stop. Borrow the process-owned cancel-capable `std.Io` supplied by `std.process.Init`, drain bounded work while it remains, and post each destructive batch once through a monotonic transport deadline. Publish fixed-label trace-suppression and exporter-health counters through `/metrics`. Exclude exporter-internal scopes from the OTLP log sink while retaining stderr. Capture `FleetCompleted` once after the report claim wins with a fixed-length deterministic `$insert_id` and saturating numeric conversion. Keep runner logs local or direct-to-collector; add no runner exporter, signal endpoint, or trace field.

The eventual Pull Request (PR) is internal reliability work. It changes no public endpoint or payload shape.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(observability): bound signal volume and expose exporter health
- **Intent (one sentence):** Useful traces and analytics survive fleet growth without making telemetry a request-path dependency.
- **Handshake:** Restate the source-backed findings before editing. `ASSUMPTIONS I'M MAKING: 1. Successful runner control requests, including both empty and granted lease responses, suppress their generic HTTP span because useful run work retains its settled delivery span. 2. Trace lifetime starts after route match but before admission, so an admission rejection enters the bounded error policy. 3. Generic request spans have a hard process budget of 10 per monotonic second: four runner 4xx responses, four server 5xx responses, and two sampled successes; status precedence makes the buckets disjoint and excess is counted by fixed reason. 4. Sampling uses the server-generated span identifier, not caller-controlled traceparent input. 5. Exporters use the atomic producer side of std.Io.Event and wake at 50 logs, 50 traces, or 768 metrics, the maximum interval, or stop; producers never take a wake mutex. 6. A destructively collected OTLP batch is attempted once under a monotonic deadline because an ambiguous retry can duplicate delta metrics. 7. Exporter-internal warnings remain on stderr but never re-enter the OTLP log queue. 8. FleetCompleted fires only after the fenced report claim succeeds, saturates runner u64 values at maxInt(i64), and carries a fixed 64-character Secure Hash Algorithm 256-bit (SHA-256) $insert_id so PostHog retries deduplicate.`

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
| `src/agentsfleetd/http/handlers/fleets/backpressure_integration_test.zig` | EDIT | Prove runner admission rejection retains a span on the pre-handler return path. |
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
| `src/agentsfleetd/observability/otel_metrics.zig` | EDIT | Feed ring, aggregation-cap, rejected-export, and uncertain-delivery loss into shared Prometheus self-metrics without removing the existing OTLP self-signal. |
| `src/agentsfleetd/observability/otel_metrics_test.zig` | EDIT | Preserve aggregation and prove Prometheus discard accounting. |
| `src/agentsfleetd/observability/otel_metrics_aggregate_test.zig` | EDIT | Remove model and workspace fixtures while preserving same-label-set aggregation proof. |
| `src/agentsfleetd/observability/otel_metrics_payload.zig` | EDIT | Remove the unbounded `model` and `workspace` label keys from OTLP metric payloads. |
| `src/agentsfleetd/observability/otel_metrics_cardinality.zig` | DELETE | Remove the process-local workspace guard after the workspace label is deleted. |
| `src/agentsfleetd/tests.zig` | EDIT | Remove the deleted workspace-cardinality module import. |
| `src/agentsfleetd/observability/metrics_otel.zig` | CREATE | Hold fixed atomic counters by signal and reason, with no dynamic labels. |
| `src/agentsfleetd/observability/metrics_otel_test.zig` | CREATE | Prove exact concurrent increments and rendering snapshots. |
| `src/agentsfleetd/observability/metrics_trace.zig` | CREATE | Hold fixed trace-suppression counters by policy reason. |
| `src/agentsfleetd/observability/metrics_trace_test.zig` | CREATE | Prove exact concurrent suppression accounting with fixed labels. |
| `src/agentsfleetd/observability/metrics_render.zig` | EDIT | Render OTLP queue depth, local discards, and uncertain delivery. |
| `src/agentsfleetd/observability/metrics.zig` | EDIT | Register the new self-metric tests and exports. |
| `src/agentsfleetd/observability/telemetry.zig` | EDIT | Expose a typed submission outcome and a test-only unavailable/failure injection seam while preserving best-effort callers. |
| `src/agentsfleetd/observability/telemetry_test.zig` | EDIT | Prove production, unavailable, and injected-failure outcomes remain non-throwing. |
| `src/agentsfleetd/observability/telemetry_events.zig` | EDIT | Add deterministic PostHog `$insert_id` to `FleetCompleted`. |
| `src/agentsfleetd/observability/telemetry_fleet_test.zig` | EDIT | Prove the insertion identifier property and exact event shape. |
| `src/agentsfleetd/fleet/report_telemetry.zig` | CREATE | Capture the existing typed `FleetCompleted` event from settled report facts. |
| `src/agentsfleetd/fleet/service_billing.zig` | EDIT | Stop passing model and workspace text into credit-drain metric samples. |
| `src/agentsfleetd/fleet/service_report.zig` | EDIT | Call completion capture only after the fenced report claim succeeds. |
| `src/agentsfleetd/fleet/integration_roundtrip_test.zig` | EDIT | Preserve once-only report settlement and completion semantics. |
| `deploy/grafana/agent-observability.json` | EDIT | Remove the model selector and model grouping from panels that consume the hardened metrics. |
| `audits/signal-routing.sh` | EDIT | Replace investigation-state limits and wording with shipped source assertions so the durable audit remains green. |
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

Define the route class from route and response status, then admit it through fixed monotonic-second budgets. Start the matched-request trace lifetime before the API admission gate, complete it after either an early response or handler dispatch, and make policy the single enqueue decision. Successful health, metrics, heartbeat, lease, renew, activity, and runner-report HTTP spans suppress; the lease rule intentionally covers both `lease: null` and a granted lease because the accepted run retains its settled `fleet.delivery` span. Apply status precedence before route class: every status 500 or above enters only the server-error bucket; a matched runner route at status 400 through 499 enters only the runner-rejection bucket. Other eligible responses below status 500 use deterministic 1-in-100 head sampling from the server-generated span identifier, never the caller-controlled inbound trace identifier, then enter the sampled-success bucket. Unknown future routes default to sampled; the total route switch forces review when a variant is added.

The process budget is 10 generic request spans per monotonic second, matching the audited pre-refactor trace drain ceiling: four runner rejections, four server errors, and two sampled successes. Each bucket uses atomic fixed-window admission with no allocator and no global lock. Excess requests do not enqueue a span and increment `agentsfleet_http_trace_suppressed_total` under a fixed reason. Existing structured rejection logs and request counters remain; the aggregate makes the full rejection volume visible without allowing an invalid-token flood to evict useful spans.

- **Dimension 1.1** — every current route has an explicit trace class; empty and granted lease responses plus other noisy runner successes suppress → Test `test_trace_policy_is_total_and_suppresses_runner_chatter`
- **Dimension 1.2** — deterministic sampling selects exactly the documented fraction over a fixed server-owned identifier corpus and caller trace identifiers cannot force selection → Test `test_trace_sampling_uses_server_owned_entropy`
- **Dimension 1.3** — matched runner rejections, including pre-handler admission shedding, and server errors emit only within their hard budgets; every excess is counted → Tests `test_trace_error_budgets_are_hard` and `test_runner_admission_rejection_is_traced_or_counted`

### §2 — Export drains by work, not one batch per interval

Keep one fixed ring and one consumer per signal. After a successful ring push, an enqueue calls `std.Io.Event.set` only when queue depth reaches a per-signal wake threshold. `Event.set` uses an atomic state change plus a futex wake when a waiter exists; it does not acquire the input/output mutex used by `std.Io.Semaphore.post`. The thresholds are 50 entries for logs, 50 for traces, and 768 for metrics. The metrics ring has 1023 usable slots, so the consumer has 255 slots of scheduling headroom after the wake. Entries below threshold wait for the existing maximum flush interval, preserving batching instead of turning slow traffic into one HTTP request per entry. Stop always sets the event immediately and computes one absolute monotonic shutdown deadline. The consumer resets the coalesced event before snapshotting pending work, then uses its monotonic timeout wait for the next cycle. It serializes batches within the existing 256 KiB payload buffer and continues while pending work remains. Each cycle is capped to entries present at cycle start so constant producers cannot starve stop. A cycle already active when stop begins checks the stop flag and remaining shared shutdown deadline between batches; it starts no post after that deadline. Each destructively collected body is posted exactly once. `Client` uses the same borrowed handle and `std.Io.Select` to race that post against the earlier of its normal transport deadline and the shared shutdown deadline, then cancels the loser. A non-success HTTP response records `export_rejected`; a timeout or transport outcome whose remote acceptance cannot be proven records `export_uncertain`. A successful response with `partialSuccess` parses `rejectedLogRecords`, `rejectedSpans`, or `rejectedDataPoints` according to the fixed signal and records the validated count as `partial_rejected`. The collector-provided message is discarded. A malformed partial-success response or a rejected count above the attempted batch size records the full batch as `export_uncertain`. No response outcome replays a possibly accepted delta batch. Exporters uninstall and join before the root process deinitializes the borrowed handle; exporters never own or deinitialize it.

- **Dimension 2.1** — a full log or trace ring drains all cycle-start entries across multiple HTTP posts without waiting another interval → Test `test_exporter_drains_cycle_start_backlog`
- **Dimension 2.1** — entries below threshold batch until the maximum interval, while reaching threshold wakes once and never produces one request per entry → Test `test_exporter_preserves_batching_and_coalesces_wakes`
- **Dimension 2.1** — every signal's configured wake threshold is reachable with producer headroom; metrics wakes at 768 and accepts the next 255 entries while the consumer is scheduled → Test `test_exporter_wake_thresholds_leave_headroom`
- **Dimension 2.1** — threshold and stop notifications use atomic `std.Io.Event.set`; producer threads never acquire the waiter's input/output mutex → Test `test_exporter_notify_is_nonblocking`
- **Dimension 2.2** — full rejection, validated partial rejection, and ambiguous transport or response failure attempt a collected body once, record the applicable count, and remain distinguishable → Tests `test_exporter_never_retries_destructive_batch` and `test_otlp_partial_success_counts_each_signal`
- **Dimension 2.3** — install receives the process-owned cancel-capable handle, and a server that accepts a connection and stalls is canceled at the monotonic transport deadline → Tests `test_exporter_uses_process_io` and `test_otlp_post_stall_times_out`
- **Dimension 2.4** — stop wakes the standard waiter, gives active and final drains one shared absolute monotonic deadline, starts no post after expiry, and joins cleanly → Tests `test_exporter_stop_wakes_drains_and_joins` and `test_stop_bounds_already_active_backlog`
- **Dimension 2.5** — serialization failure returns the exact number of entries already removed and records them once → Test `test_exporter_serialization_failure_counts_local_discards`

### §3 — Prometheus exposes exporter health

Add fixed atomic values for `logs`, `traces`, and `metrics`. Render queue depth gauges plus counters for `ring_full`, `aggregate_cap`, `serialize_failed`, `partial_rejected`, `export_rejected`, and `export_uncertain`. The metric aggregator reports every sample discarded after its fixed 256-series ceiling as `aggregate_cap`; this stays visible even when OTLP itself is dark. `partial_rejected` is the validated signal-specific count returned in an OTLP partial-success response. `export_rejected` means the backend returned a non-success HTTP status. Reserve `export_uncertain` for timeout, transport, malformed partial-success, or impossible rejected-count outcomes whose remote acceptance cannot be proven. No endpoint, route, workspace, runner, trace, lease, or event label is allowed.

Remove the unbounded `model` and `workspace` metric labels from credit, token, and duration samples and remove the model Grafana dashboard selector/grouping. A 64-byte value ceiling, a per-flush 256-series ceiling, and a process-local 100-workspace guard do not bound backend series accumulated across flushes, replicas, or restarts. Keep `posture` because it is the fixed `platform | self_managed` enum and keep `direction` because it is the fixed three-value set. Delete the now-unused workspace cardinality guard.

- **Dimension 3.1** — all three queues render depth and the fixed `ring_full`, `aggregate_cap`, `serialize_failed`, `partial_rejected`, `export_rejected`, and `export_uncertain` reasons with valid Prometheus families → Test `test_otlp_self_metrics_render_fixed_labels`
- **Dimension 3.2** — at least 100 concurrent producers increment exact totals without allocation or a global serialization lock → Test `test_otlp_self_metrics_are_concurrent_and_exact`
- **Dimension 3.3** — an OTLP outage cannot recursively fill the log exporter with its own warnings; structured warnings still reach stderr → Test `test_exporter_failure_scope_bypasses_otlp_log_sink`
- **Dimension 3.4** — metric payloads contain no `model` or `workspace` labels, the dashboard has no model selector, and only fixed posture and direction labels remain → Test `test_otlp_metric_labels_have_durable_cardinality_bounds`

### §4 — PostHog completion closes the business funnel once

After `claimReportAndSettle` returns `claimed=true`, submit `FleetCompleted` with workspace as the stable distinct identifier, plus fleet, event, token, duration, outcome, and first-token fields already present in the typed event. Convert every runner-controlled `u64` property with `std.math.cast(i64, value) orelse std.math.maxInt(i64)` inside the typed-event property boundary, so maximum inputs cannot trap after durable settlement. Set PostHog `$insert_id` to the 64-character lowercase SHA-256 digest of `fleet_id || 0x00 || event_id`, using `std.crypto.hash.sha2.Sha256`; the fixed-length value avoids an unbounded analytics key. The report fence prevents a second local submission; the deterministic insertion identifier lets PostHog deduplicate a batch retry after remote acceptance with a lost response. A fenced or replayed report submits nothing. Add a typed `submitted | unavailable | failed` outcome below the existing non-throwing `Telemetry.capture` wrapper. `submitted` means serialization completed and the library call returned; it does not claim queue admission or delivery because PostHog v0.2.0 drops on a full 1000-event write side without returning that status. Production maps a missing client and a serialization error to the latter two outcomes; the test backend can inject either outcome without a network client. Report completion records the outcome for tests but never changes the settled response.

- **Dimension 4.1** — one accepted report makes at most one completion submission with exact settled properties and deterministic insertion identifier → Test `test_settled_report_captures_fleet_completed`
- **Dimension 4.1** — maximum `tokens`, `wall_ms`, and first-token values saturate to `maxInt(i64)` without trapping after settlement → Test `test_fleet_completed_saturates_runner_u64_properties`
- **Dimension 4.2** — fenced, replayed, malformed, and failed-settlement reports capture nothing → Test `test_unaccepted_report_never_captures_completion`
- **Dimension 4.3** — the test backend injects both `unavailable` and `failed`; neither changes report response or durable state → Tests `test_telemetry_submission_outcomes_are_non_throwing` and `test_completion_analytics_failure_never_blocks_report`

## Interfaces

```text
TracePolicy.decide(route, status, server_span_id, monotonic_now) -> suppress { reason } | emit
Exporter.install(config, process_io) -> installed | already_running | spawn_failed
Exporter.notifyAccepted() -> void
Exporter flush hook -> empty | ready { body, entry_count } | discarded { reason, entry_count }
metrics_otel.record(signal, reason, count) -> void
report_telemetry.captureCompleted(telemetry, settled facts) -> void
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
| 1.2 | unit | `test_trace_sampling_uses_server_owned_entropy` | fixed server span identifiers select exactly 1 of each 100 buckets; changing only inbound trace identifiers cannot force admission. |
| 1.3 | unit | `test_trace_error_budgets_are_hard` | 100 concurrent callers emit exactly four runner 4xx rejections, four 5xx server errors, two sampled successes, count every excess, and never charge a runner 5xx to both buckets. |
| 1.3 | integration | `test_runner_admission_rejection_is_traced_or_counted` | an admission-shed runner request enters the bounded 429 trace path before handler dispatch; exhausted budget increments suppression. |
| 2.1 | unit | `test_exporter_drains_cycle_start_backlog` | a full ring drains in several bounded posts during one cycle. |
| 2.1 | unit | `test_exporter_preserves_batching_and_coalesces_wakes` | below-threshold entries wait for interval; threshold posts one wake and one batch rather than one request per entry. |
| 2.1 | unit | `test_exporter_wake_thresholds_leave_headroom` | logs and traces wake at 50; metrics wakes at 768 and can accept 255 more samples before full. |
| 2.1 | unit | `test_exporter_notify_is_nonblocking` | a waiting consumer cannot make a threshold-crossing producer acquire an input/output mutex or wait for the consumer. |
| 2.2 | unit | `test_exporter_never_retries_destructive_batch` | rejection and ambiguous failure each make one post, count exact local discards, and use distinct reasons. |
| 2.2 | unit | `test_otlp_partial_success_counts_each_signal` | logs, traces, and metrics parse their fixed rejected-count field; malformed or oversized counts become uncertain without replay. |
| 2.3 | unit | `test_exporter_uses_process_io` | install threads the root process handle into wake, timer, and HTTP operations without a second owner. |
| 2.3 | integration | `test_otlp_post_stall_times_out` | a connected silent peer is canceled and cannot hold the exporter beyond the monotonic deadline. |
| 2.4 | integration | `test_stop_bounds_already_active_backlog` | stop during a multi-batch stalled drain shares one absolute deadline instead of multiplying it by backlog depth. |
| 2.4 | unit | `test_exporter_stop_wakes_drains_and_joins` | stopped standard waiter wakes, bounded drain ends, allocator and thread count return clean. |
| 2.5 | unit | `test_exporter_serialization_failure_counts_local_discards` | the tagged collect result reports every entry removed before encoding failed. |
| 3.1 | unit | `test_otlp_self_metrics_render_fixed_labels` | valid families include three signals and the six fixed reasons only; aggregation-cap loss remains visible without OTLP. |
| 3.2 | unit | `test_otlp_self_metrics_are_concurrent_and_exact` | 100 producers preserve exact total and no hidden global lock. |
| 3.3 | integration | `test_exporter_failure_scope_bypasses_otlp_log_sink` | exporter-only warnings reach stderr while the OTLP log queue drains to zero and stays empty. |
| 3.4 | unit | `test_otlp_metric_labels_have_durable_cardinality_bounds` | metric payloads omit model and workspace, dashboard queries omit model, and retained labels use fixed sets. |
| 4.1 | integration | `test_settled_report_captures_fleet_completed` | accepted report enqueues once with the fixed SHA-256 `$insert_id`; replay emits zero. |
| 4.1 | unit | `test_fleet_completed_saturates_runner_u64_properties` | maximum runner-controlled numeric values serialize as `maxInt(i64)` without a safety trap. |
| 4.2 | integration | `test_unaccepted_report_never_captures_completion` | replay, stale fence, malformed body, and database failure emit zero. |
| 4.3 | unit | `test_telemetry_submission_outcomes_are_non_throwing` | test injection deterministically produces unavailable and failed outcomes without a network client. |
| 4.3 | integration | `test_completion_analytics_failure_never_blocks_report` | both injected outcomes still return report success after settlement. |

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
- **Review correction:** completion analytics gains an honest typed submission outcome plus test-only unavailable/failure injection; `submitted` does not misreport PostHog queue admission or delivery.
- **Review correction:** the evolving model and workspace strings are removed from OTLP metrics because byte length, a per-flush series ceiling, and a process-local guard do not bound backend cardinality over time; Grafana model queries are removed with them.
- **Metrics review:** queue depth, fixed local discard reasons, uncertain delivery, and one selected completion event are the complete new signal set.
- **Skill-driven review chain:** empty at creation; record `/write-unit-test`, native Codex review, gstack review, and post-push review outcomes during implementation.
- **Deferrals:** none.
