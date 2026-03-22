# Signal Ownership Contract

**Version:** 1.0
**Date:** Mar 19, 2026
**Status:** Active

---

## Ownership Boundaries

### Platform Observability (Grafana)

- Logs, metrics, and traces for platform health and run lifecycle.
- Delivered via: Prometheus `/metrics` endpoint, OTLP/HTTP JSON exporter (`POST /v1/metrics`).
- Backend: Grafana Cloud (or compatible).

### Product Analytics (PostHog)

- User-facing lifecycle events for product analytics and growth metrics.
- Scope: product analytics — run lifecycle, scoring, billing, trust transitions.

---

## Signal Inventory

### Logs

Structured logs use `std.log.scoped` with the following scopes:

| Scope | Source |
|---|---|
| `.zombied` | `main.zig`, `cmd/run.zig`, `cmd/serve.zig`, `cmd/migrate.zig`, `cmd/common.zig` |
| `.worker` | `pipeline/worker.zig`, `worker_claim.zig`, `worker_allocator.zig`, `worker_stage_executor.zig`, `cmd/worker.zig` |
| `.otel_export` | `observability/otel_export.zig` |
| `.event_bus` | `events/bus.zig` |
| `.reliable` | `reliability/reliable_call.zig` |
| `.scoring` | `pipeline/scoring.zig` |
| `.agents` | `pipeline/agents.zig`, `agents_runner.zig` |
| `.http` | `http/server.zig`, `handlers/runs/start.zig`, `handlers/runs/retry.zig`, `handlers/workspaces.zig` |
| `.state` | `state/machine.zig` |
| `.policy` | `state/policy.zig` |
| `.outbox_reconciler` | `state/outbox_reconciler.zig` |
| `.reconcile` | `cmd/reconcile/*.zig` |
| `.db` | `db/pool.zig` |
| `.redis_queue` | `queue/redis_client.zig` |
| `.git` | `git/pr.zig`, `command.zig`, `repo.zig` |
| `.github_auth` | `auth/github.zig` |
| `.secrets` | `secrets/crypto.zig` |
| `.memory` | `memory/workspace.zig` |

### Metrics (Prometheus)

All metrics use the `zombie_` prefix and are exposed at the `/metrics` endpoint.

#### Run Lifecycle

| Metric | Type | Description |
|---|---|---|
| `zombie_runs_created_total` | counter | Total runs accepted by API |
| `zombie_runs_completed_total` | counter | Total runs completed successfully |
| `zombie_runs_blocked_total` | counter | Total runs that ended blocked |
| `zombie_run_retries_total` | counter | Total retry attempts across runs |

#### Agent Calls

| Metric | Type | Description |
|---|---|---|
| `zombie_agent_echo_calls_total` | counter | Total Echo agent invocations |
| `zombie_agent_scout_calls_total` | counter | Total Scout agent invocations |
| `zombie_agent_warden_calls_total` | counter | Total Warden agent invocations |
| `zombie_agent_tokens_total` | counter | Total tokens consumed by agent calls |

#### External Reliability

Retries and failures by error class (`rate_limited`, `timeout`, `context_exhausted`, `auth`, `invalid_request`, `server_error`, `unknown`).

| Metric | Type | Description |
|---|---|---|
| `zombie_external_retries_total` | counter | Total retry attempts inside external side-effect wrappers |
| `zombie_external_retries_{class}_total` | counter | External retries by error class |
| `zombie_external_failures_total` | counter | External calls that exited as classified failures |
| `zombie_external_failures_{class}_total` | counter | External failures by error class |
| `zombie_retry_after_hints_total` | counter | Retry attempts that used Retry-After guidance |
| `zombie_backoff_wait_ms_total` | counter | Total backoff wait time in milliseconds |
| `zombie_rate_limit_wait_ms_total` | counter | Total wait time due to rate limiting in milliseconds |

#### Side-Effect Outbox

| Metric | Type | Description |
|---|---|---|
| `zombie_side_effect_outbox_enqueued_total` | counter | Total outbox entries enqueued |
| `zombie_side_effect_outbox_delivered_total` | counter | Total outbox entries marked delivered |
| `zombie_side_effect_outbox_dead_letter_total` | counter | Total outbox entries dead-lettered by reconciliation |

#### Infrastructure

| Metric | Type | Description |
|---|---|---|
| `zombie_worker_running` | gauge | Worker liveness (1 running, 0 stopped) |
| `zombie_worker_in_flight_runs` | gauge | Current in-flight runs across worker threads |
| `zombie_worker_errors_total` | counter | Total worker loop errors |
| `zombie_worker_allocator_leaks_total` | counter | Total worker allocator leak detections on teardown |
| `zombie_queue_depth` | gauge | Current queued runs in SPEC_QUEUED |
| `zombie_oldest_queued_age_ms` | gauge | Oldest queued run age in milliseconds |
| `zombie_api_in_flight_requests` | gauge | Current in-flight API requests (backpressure guard) |
| `zombie_api_backpressure_rejections_total` | counter | Total API requests rejected by backpressure guard |

#### Scoring

| Metric | Type | Description |
|---|---|---|
| `zombie_agent_score_computed_total` | counter | Total scored runs across all tiers |
| `zombie_agent_score_computed_unranked_total` | counter | Scored runs — UNRANKED tier |
| `zombie_agent_score_computed_bronze_total` | counter | Scored runs — BRONZE tier |
| `zombie_agent_score_computed_silver_total` | counter | Scored runs — SILVER tier |
| `zombie_agent_score_computed_gold_total` | counter | Scored runs — GOLD tier |
| `zombie_agent_score_computed_elite_total` | counter | Scored runs — ELITE tier |
| `zombie_agent_scoring_failed_total` | counter | Total scoring failures caught by fail-safe |
| `zombie_agent_score_latest` | gauge | Most recently computed agent score |

#### Exporter Health

| Metric | Type | Description |
|---|---|---|
| `zombie_otel_export_total` | counter | Total OTEL metric export attempts |
| `zombie_otel_export_failed_total` | counter | Total OTEL export failures |
| `zombie_otel_last_success_at_ms` | gauge | Timestamp (ms) of last successful OTEL export |

#### Histograms

Buckets: `1, 3, 5, 10, 30, 60, 120, 300`.

| Metric | Type | Description |
|---|---|---|
| `zombie_agent_duration_seconds` | histogram | Duration of individual agent calls in seconds |
| `zombie_run_total_wall_seconds` | histogram | End-to-end run wall-clock duration in seconds |
| `zombie_agent_scoring_duration_ms` | histogram | Time spent in scoreRun in milliseconds |

### Traces

W3C Trace Context (`traceparent` header) for distributed tracing.

| Field | Format |
|---|---|
| `trace_id` | 32 hex chars (16 bytes) |
| `span_id` | 16 hex chars (8 bytes) |
| `parent_span_id` | 16 hex chars (optional, null for root spans) |
| `traceparent` | `00-{trace_id}-{span_id}-01` |

Operations: `TraceContext.generate()` (root), `TraceContext.child()` (child span), `TraceContext.fromW3CHeader()` (parse inbound).

### PostHog Events

| Event | Properties |
|---|---|
| `run_started` | run_id, workspace_id, spec_id, mode, request_id |
| `run_retried` | run_id, workspace_id, attempt, request_id |
| `run_completed` | run_id, workspace_id, verdict, duration_ms |
| `run_failed` | run_id, workspace_id, reason, duration_ms |
| `agent_completed` | run_id, workspace_id, actor, tokens, duration_ms, exit_status |
| `agent.run.scored` | run_id, workspace_id, agent_id, score, tier, score_formula_version, axis_scores, weight_snapshot, scored_at, axis_completion, axis_error_rate, axis_latency, axis_resource |
| `agent.scoring.failed` | run_id, workspace_id, error |
| `agent.trust.earned` | run_id, workspace_id, agent_id, consecutive_count_at_event |
| `agent.trust.lost` | run_id, workspace_id, agent_id, consecutive_count_at_event |
| `agent.harness.changed` | agent_id, proposal_id, workspace_id, approval_mode, trigger_reason, fields_changed |
| `agent.improvement.stalled` | run_id, workspace_id, agent_id, proposal_id, consecutive_negative_deltas |
| `entitlement_rejected` | workspace_id, boundary, reason_code, request_id |
| `profile_activated` | workspace_id, agent_id, config_version_id, run_snapshot_version, request_id |
| `billing_lifecycle_event` | workspace_id, event_type, reason, plan_tier, billing_status, request_id |

### Event Bus

In-process bounded ring buffer (`capacity=1024`) with background log sink.

- Fields per event: `ts_ms`, `kind` (max 32 bytes), `run_id` (max 64 bytes), `detail` (max 256 bytes).
- Drops are logged via `.event_bus` scope: `event_drop count={n}`.
- Scope: `.event_bus`.

---

## Delivery Health Signals

### OTLP Exporter

- Fire-and-forget: export errors are logged via `.otel_export` scope, never propagated.
- Protocol: OTLP/HTTP JSON (`POST /v1/metrics`).
- Config: `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SERVICE_NAME` (default: `zombied`).

---

## No Silent-Drop Policy

All exporter failures MUST:

1. Emit a structured log line with `error_code` prefix (`UZ-OBS-*`).
2. Increment the corresponding `*_failed_total` counter.
3. Never block the worker execution path.
4. Never silently swallow errors without a metric or log.

### Error Codes

| Code | Source | Description |
|---|---|---|
| `UZ-OBS-OTEL-001` | `otel_export.zig` | OTEL export failed (generic) |
| `UZ-OBS-OTEL-002` | `otel_export.zig` | OTEL connect failed (DNS, connection refused, timeout) |
| `UZ-OBS-OTEL-003` | `otel_export.zig` | OTEL request failed (non-connect HTTP error) |
| `UZ-OBS-OTEL-004` | `otel_export.zig` | OTEL unexpected status (non-2xx response) |
