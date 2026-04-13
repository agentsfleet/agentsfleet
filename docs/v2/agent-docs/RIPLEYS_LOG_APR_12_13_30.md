# Design Log — M18_001 Zombie Execution Telemetry
**Date:** Apr 12, 2026: 1:30 PM
**Author:** Design session with Oracle
**Scope:** M18_001 — per-delivery telemetry store + dual API

---

## Context: What shipped in M15 and why M18 is needed

M15 shipped two workstreams:

**M15_001 (done)** — credit metering. After each delivery, `CREDIT_DEDUCTED` writes to
`workspace_credit_audit`. The billing fields (`agent_seconds`, `debit_cents`) are there.
What is NOT there: token count (field reserved but never stored), TTFT, or wall epoch.
The audit row is billing-only by design.

**M15_002 (done)** — PostHog + Prometheus observability. `ZombieCompleted` in
`telemetry_events.zig` carries `tokens` and `wall_ms`. Prometheus gets a wall-time
histogram via `metrics_zombie.zig`. These are aggregate signals — good for Grafana and
product analytics, but not queryable per-execution or per-workspace by customers.

**The gap:** Neither workstream produces a persistent, queryable per-delivery record.
Customers cannot ask "show me the last 20 executions for zombie X with their token
counts and latencies." UseZombie cannot slice execution data across workspaces for
support or capacity planning without hitting PostHog (which has its own query latency
and no filtering by internal fields like `event_id`).

---

## What M18_001 adds

```
delivery happens
    ├─ M15_001 path: CREDIT_DEDUCTED → workspace_credit_audit (billing)
    ├─ M15_002 path: ZombieCompleted → PostHog + Prometheus (analytics / dashboards)
    └─ M18_001 path: insertTelemetry → zombie_execution_telemetry (queryable store)
```

Three parallel writes, same call site (`recordZombieDelivery`). All non-fatal.

The new table carries everything needed for both the customer API and operator queries:
`token_count`, `time_to_first_token_ms`, `epoch_wall_time_ms`, `wall_seconds`,
`plan_tier`, `credit_deducted_cents`.

---

## Field decisions

### `time_to_first_token_ms`

Time from executor stage start to first token received from the LLM. Reported by the
executor process in the `start_stage` JSON response. Currently absent from `StageResult`
in `executor/client.zig` — needs a new field with `= 0` default so older executor
versions degrade gracefully.

This field also needs to be added to `ZombieCompleted.properties()` in
`telemetry_events.zig` — M15_002 shipped without it. That's a one-line PostHog property
addition, no schema change.

**Why it matters:** TTFT is the latency the zombie user feels before the agent "starts
working". High TTFT on a fast-wall-time delivery means the model was slow to start.
Useful for LLM provider comparison and debugging stuck deliveries.

### `epoch_wall_time_ms`

Absolute Unix epoch ms captured at the start of `deliverEvent()` using
`std.time.milliTimestamp()`. Distinct from the monotonic `t_start` already in
`event_loop.zig` (which computes `wall_ms` duration for Prometheus).

Both fields coexist:
- `t_start` (monotonic) → `wall_ms` duration → Prometheus histogram
- `epoch_wall_time_ms` (epoch) → stored in `zombie_execution_telemetry`

**Why it matters:** Epoch timestamp allows correlation with external systems (GitHub
webhook timestamps, PostHog event timestamps, PagerDuty alerts). The monotonic clock
duration alone can't answer "did this delivery happen during the 2:15 AM incident?"

### `wall_seconds` vs `wall_ms`

`StageResult.wall_seconds` is what the executor reports (integer seconds). The event
loop computes `wall_ms` from the monotonic clock (higher resolution). The telemetry
store records `wall_seconds` (from executor) — consistent with the billing audit.
The Prometheus histogram uses `wall_ms` from the monotonic clock — consistent with M15_002.

These intentionally differ. The executor's `wall_seconds` covers only sandbox time;
the event loop's `wall_ms` covers gate wait + sandbox. Both are useful, they measure
different spans.

---

## API design decisions

### Customer endpoint: per-zombie scope

`GET /v1/workspaces/{ws}/zombies/{zombie_id}/telemetry`

Scoped to a single zombie. Rationale: customers typically investigate one zombie at a
time ("why was this zombie slow yesterday?"). A workspace-wide endpoint with per-zombie
filter could be added later but per-zombie scope is the 80% case and simpler to auth
(zombie_id in path forces the client to be intentional).

Cursor is opaque base64 encoding `(recorded_at, id)` — same pattern as
`activity_stream.zig`. Newest-first order. Default limit 50, max 200.

### Operator endpoint: cross-workspace, time-windowed

`GET /internal/v1/telemetry?workspace_id=&zombie_id=&after=&limit=`

All filters optional. `after` is epoch ms (not a cursor) — internal consumers run
time-window queries ("give me all deliveries in the last hour") more often than they
paginate. No cursor on the operator side keeps the implementation simple.

Auth: existing internal service token pattern (same as other `/internal/*` routes).

---

## Idempotency

`UNIQUE(event_id)` + `ON CONFLICT DO NOTHING`. Matches M15_001's idempotency contract:
if the event loop replays the same `event_id` (XACK failure scenario), the telemetry
row is a no-op. The credit audit row is also a no-op (M15_001's `hasAuditForRunId`
check). No double-recording, no error.

---

## What M18_001 does NOT do

- **TTFT histogram in Prometheus** — could add `observeZombieTTFT()` to `metrics_zombie.zig`
  in a follow-on. M18_001 stores the raw value; operators can compute percentiles from
  the DB or wait for the Prometheus histogram to be added.
- **`zombiectl telemetry` CLI** — depends on the customer API. Scoped to M18_002.
- **Retention policy** — rows accumulate indefinitely. A background sweeper or
  `recorded_at < now - 90d` partition drop is an ops milestone.
- **Token breakdown (input/output)** — requires executor-side change to split the count.
- **Real-time streaming** — polling the customer API is sufficient. SSE or WebSocket
  is a future concern once usage patterns are understood.

---

## Relationship to M15_002 `ZombieCompleted`

M15_002 emits `zombie_completed` to PostHog with `tokens` and `wall_ms`. M18_001 adds
`time_to_first_token_ms` to that same PostHog event (additive property, no breaking
change). The persistent store is separate — PostHog is not a source of truth for
customer-queryable data.

---

## Implementation order rationale (from spec §Execution Plan)

Steps 1–3 (field extensions) must land before step 5 (store) because the store
`insertTelemetry` takes an `InsertTelemetryParams` struct that references the new fields.
Step 4 (schema) can be done in parallel with steps 1–3 since it is a pure SQL/Zig
embed change with no Go/JS dependencies. Steps 7–8 (HTTP handlers + routing) come
last — they depend on the store being stable.
