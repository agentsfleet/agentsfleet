# Read a chat's round trip at p95 and p99, per fleet

**Owner:** 🦉 Aiwa, with 🤠 Indy for Grafana access
**Applies to:** development and production independently

This page holds the queries a dashboard panel is built from. It adds no
dashboard and no alert; `001_playbook.md` owns those. A dashboard agent pastes
the queries below into panels against the environment's Tempo and Prometheus
datasources.

## What is measured

`agentsfleetd` records one `fleet.delivery` span per settled report. It runs
from the event's creation to the `event_complete` publish, on the daemon's
clock: the round trip the fleet owner waited.

`gen_ai.agent.id` is the fleet. The segments ride as attributes, in
milliseconds:

| Attribute | Measured by | From → to |
|---|---|---|
| `agentsfleet.delivery.queue_wait_ms` | daemon | event created → lease issued |
| `agentsfleet.delivery.memory_ms` | runner | its memory read, only for a lease without a window |
| `agentsfleet.delivery.model_ms` | runner | the model's turns |
| `agentsfleet.delivery.tools_ms` | runner | the tools those turns called |
| `agentsfleet.delivery.sandbox_wait_ms` | runner | sandbox calls held up by the build |
| `agentsfleet.delivery.settle_ms` | runner | hold-or-destroy → start of the report post |
| `agentsfleet.delivery.complete_ms` | daemon | report received → `event_complete` published |

The segments are not a partition: the sandbox wait runs beside the model's
time. Fleet-wide, without fleet identity, the same boundaries are on
`agentsfleet_fleet_delivery_stage_seconds{stage}` with `stage` in
`event_to_lease`, `lease_to_report`, `report_to_complete`, `event_to_complete`
and the first-chunk stages.

## Per fleet: p95 and p99 of the whole round trip

Tempo metrics query (needs Tempo's metrics generator with local blocks):

```traceql
{ name = "fleet.delivery" && span."gen_ai.agent.id" = "$fleet_id" } | quantile_over_time(duration, .95, .99)
```

Without the metrics generator, a TraceQL search answers on every Tempo; set
the panel's transformation to *Reduce → Calculations: 95th %, 99th %* over the
`Duration` field:

```traceql
{ name = "fleet.delivery" && span."gen_ai.agent.id" = "$fleet_id" }
```

## Per fleet: p95 and p99 of one segment

Replace the attribute to read any row of the table above:

```traceql
{ name = "fleet.delivery" && span."gen_ai.agent.id" = "$fleet_id" } | quantile_over_time(span."agentsfleet.delivery.model_ms", .95, .99)
```

## Fleet-wide: p95 and p99 per stage

PromQL against the environment's Prometheus datasource:

```promql
histogram_quantile(0.95, sum by (le, stage) (rate(agentsfleet_fleet_delivery_stage_seconds_bucket[5m])))
histogram_quantile(0.99, sum by (le, stage) (rate(agentsfleet_fleet_delivery_stage_seconds_bucket[5m])))
```

## Verify

1. Send ten chats to one fleet on the environment.
2. In Grafana Explore, against Tempo, run the first query with that fleet's id.
3. Record which of the two per-fleet queries answered, and a screenshot path,
   in the milestone spec's Discovery.

If neither answers, check that the daemon's OTLP export is configured
(`001_playbook.md`, step 1) and that a report settled after the deploy.
