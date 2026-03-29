# Metrics reference

## Overview

All UseZombie components expose Prometheus metrics on the `/metrics` endpoint (default port 9091). Metrics are organized into three categories: session lifecycle, stage execution, and sandbox enforcement.

## Session lifecycle

Counters and gauges tracking the overall run pipeline.

| Metric | Type | Description |
|--------|------|-------------|
| `sessions_created_total` | Counter | Total number of runs created. |
| `sessions_active` | Gauge | Number of runs currently in progress. |
| `failures_total` | Counter | Total number of runs that ended in a failed state. Labels: `reason`. |
| `cancellations_total` | Counter | Total number of runs cancelled by the user or system. |

## Stage execution

Metrics tracking individual stage execution within runs.

| Metric | Type | Description |
|--------|------|-------------|
| `stages_started_total` | Counter | Total number of stages that began execution. |
| `stages_completed_total` | Counter | Total number of stages that completed successfully. |
| `stages_failed_total` | Counter | Total number of stages that failed. Labels: `error_code`. |
| `agent_tokens_total` | Counter | Total LLM tokens consumed across all stages. Labels: `direction` (`input`, `output`). |
| `agent_duration_seconds` | Histogram | Wall-clock duration of agent execution per stage. Buckets: 10s, 30s, 60s, 120s, 300s, 600s. |

## Sandbox enforcement

Metrics tracking sandbox policy enforcement and resource limit events.

| Metric | Type | Description |
|--------|------|-------------|
| `oom_kills_total` | Counter | Total number of agent executions killed by OOM (cgroups memory limit exceeded). |
| `timeout_kills_total` | Counter | Total number of agent executions killed by timeout. |
| `landlock_denials_total` | Counter | Total number of filesystem access attempts denied by Landlock. |
| `resource_kills_total` | Counter | Total number of agent executions killed for any resource violation. |
| `lease_expired_total` | Counter | Total number of runs where the lease expired before completion. |
| `cpu_throttled_ms_total` | Counter | Cumulative milliseconds of CPU throttling applied by cgroups. |
| `memory_peak_bytes` | Gauge | Peak memory usage of the most recent agent execution. |

## Labels

Common labels applied across metrics:

| Label | Description | Applied to |
|-------|-------------|------------|
| `reason` | Failure reason category | `failures_total` |
| `error_code` | UZ-* error code | `stages_failed_total` |
| `direction` | Token direction (`input` or `output`) | `agent_tokens_total` |
| `workspace_id` | Workspace identifier | All metrics when cardinality allows |

## Grafana dashboard queries

Example PromQL queries for common operational views:

```promql
# Run success rate (last 1 hour)
1 - (rate(failures_total[1h]) / rate(sessions_created_total[1h]))

# P95 stage duration
histogram_quantile(0.95, rate(agent_duration_seconds_bucket[5m]))

# OOM kill rate
rate(oom_kills_total[5m])

# Active runs
sessions_active
```
