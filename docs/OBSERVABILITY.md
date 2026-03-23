# Observability Strategy

**Date:** Mar 23, 2026
**Status:** Active

## Two Layers

| Layer | Tool | Purpose |
|---|---|---|
| Infra / ops | Grafana Cloud — Prometheus metrics, Loki logs, Tempo traces | service health, latency, sandbox signals, executor health, distributed traces |
| Product | PostHog | user-facing behavior, funnels, adoption, conversion, error attribution |

Sandbox and executor telemetry is infra-first. It must not be hidden inside product analytics.

## Grafana 3-Signal Architecture

| Signal | Exporter | Endpoint | Backend |
|---|---|---|---|
| Metrics | Prometheus `/metrics` scrape + OTLP push (`otel_export.zig`) | `/v1/metrics` | Grafana Prometheus |
| Logs | OTLP push (`otel_logs.zig`) | `/v1/logs` | Grafana Loki |
| Traces | OTLP push (`otel_traces.zig`) | `/v1/traces` | Grafana Tempo |

All OTLP exporters use shared auth: `GRAFANA_OTLP_ENDPOINT`, `GRAFANA_OTLP_INSTANCE_ID`, `GRAFANA_OTLP_API_KEY`. Export must remain fire-and-forget: failures never block request or worker paths.

## Canonical Correlation Fields

- `trace_id`
- `run_id`
- `workspace_id`
- `stage_id`
- `role_id`
- `skill_id`
- `executor_id` when available

## Execution Telemetry Model

```text
worker
  -> executor.session_created
  -> executor.stage_started
  -> executor.stage_finished | executor.stage_failed
  -> executor.session_destroyed

zombied-executor
  -> sandbox.preflight
  -> sandbox.policy_denied
  -> sandbox.timeout_kill
  -> sandbox.oom_kill
  -> sandbox.resource_usage
```

## Required Infra Signals

### Structured logs

- `executor.session_created`
- `executor.session_lost`
- `executor.stage_started`
- `executor.stage_finished`
- `executor.stage_failed`
- `sandbox.preflight`
- `sandbox.policy_denied`
- `sandbox.timeout_kill`
- `sandbox.oom_kill`

### Metrics

- `zombie_executor_sessions_total`
- `zombie_executor_session_failures_total`
- `zombie_sandbox_shell_runs_total`
- `zombie_sandbox_kill_switch_total`
- `zombie_sandbox_preflight_failures_total`
- `zombie_sandbox_oom_kills_total`
- `zombie_sandbox_memory_mb_allocated`
- `zombie_sandbox_cpu_throttled_seconds_total`

### Error codes

- `UZ-SANDBOX-001`
- `UZ-SANDBOX-002`
- `UZ-SANDBOX-003`

## Operational Interpretation

1. If worker is healthy but executor health degrades, treat that as execution-substrate failure, not application success.
2. If executor dies mid-stage, the run should show a clear infrastructure-classified failure.
3. Worker and executor upgrades should be visible as interrupted or drained runs, never silent disappearance.
4. Inbound `traceparent` should be preserved when available; otherwise generate a new root trace.

## Product Analytics Boundary

PostHog should continue to track:
- run started/completed/failed
- agent completed
- workspace and CLI product usage

PostHog should not be the primary source of truth for sandbox enforcement or executor health.
