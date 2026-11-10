---
type: reference
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Read a chat's round trip at p95 and p99, per fleet

## Synopsis

These are the panel queries for a chat's round trip, per environment.
[Set up Grafana observability](001_playbook.md) owns the dashboards and alerts.
🦉 Aiwa runs them; 🤠 Indy grants Grafana access.

`agentsfleetd` records one `fleet.delivery` span per settled report, on the daemon's clock.
It runs from event creation to the `event_complete` publish.
`gen_ai.agent.id` is the fleet, and each segment is a millisecond attribute.

| Attribute | Measured by | From → to |
|---|---|---|
| `agentsfleet.delivery.queue_wait_ms` | daemon | event created → lease issued |
| `agentsfleet.delivery.memory_ms` | runner | memory read, only for a lease without a memory window |
| `agentsfleet.delivery.model_ms` | runner | the model's turns |
| `agentsfleet.delivery.tools_ms` | runner | the tools those turns called |
| `agentsfleet.delivery.sandbox_wait_ms` | runner | sandbox calls delayed by preparation |
| `agentsfleet.delivery.settle_ms` | runner | hold-or-destroy → start of the report post |
| `agentsfleet.delivery.complete_ms` | daemon | report received → `event_complete` published |

Segments overlap: sandbox wait runs beside model time, so never sum their percentiles.
A missing attribute has no sample; it is not zero.

## Example with output

Whole round trip per fleet, with Tempo's metrics generator and local blocks:

```traceql
{ name = "fleet.delivery" && span."gen_ai.agent.id" = "$fleet_id" } | quantile_over_time(duration, .95, .99)
```

Without the generator, run plain search and set the panel transformation to **Reduce → Calculations: 95th %, 99th %** over `Duration`:

```traceql
{ name = "fleet.delivery" && span."gen_ai.agent.id" = "$fleet_id" }
```

One segment per fleet; swap in any attribute from the table:

```traceql
{ name = "fleet.delivery" && span."gen_ai.agent.id" = "$fleet_id" } | quantile_over_time(span."agentsfleet.delivery.model_ms", .95, .99)
```

Every span one chat left, daemon and runner, by its event:

```traceql
{ span."agentsfleet.event.id" = "$event_id" }
```

That returns `fleet.steer`, `fleet.offer`, `fleet.report`, `fleet.delivery` and `runner.lease`.
A fallback `fleet.hydrate` or `fleet.capture` carries no event; find it by fleet within the window between the offer and the report:

```traceql
{ name =~ "fleet.hydrate|fleet.capture" && span."gen_ai.agent.id" = "$fleet_id" }
```

One daemon stage per fleet; each verb's stages are `agentsfleet.<verb>.<stage>_ms`:

```traceql
{ name = "fleet.report" && span."gen_ai.agent.id" = "$fleet_id" } | quantile_over_time(span."agentsfleet.report.commit_ms", .95, .99)
```

Fleet-wide per stage, against Prometheus, with no fleet identity:

```promql
histogram_quantile(0.95, sum by (le, stage) (rate(agentsfleet_fleet_delivery_stage_seconds_bucket[5m])))
histogram_quantile(0.99, sum by (le, stage) (rate(agentsfleet_fleet_delivery_stage_seconds_bucket[5m])))
```

`stage` is `event_to_lease`, `lease_to_first_chunk`, `event_to_first_chunk`, `zombie_to_first_chunk`, `event_to_complete`, `lease_to_report` or `report_to_complete`.

## Options

The Chat latency dashboard has a `tempo` datasource selector and a `fleet_id` textbox.
Its round-trip panels use plain search, `select` for segment attributes, span table output, and Reduce p95 and p99.
`duration` is in nanoseconds; each `agentsfleet.delivery.*_ms` field is in milliseconds.
Each panel reads at most 1,000 matching spans in the time range, and the percentiles describe only those.
Narrow the range when a search reaches the limit.

## Errors

To verify, send ten chats to one fleet, then run the first query in Grafana Explore against Tempo.
Record which per-fleet query answered, plus a screenshot path, in the milestone spec's Discovery.
If neither answers, check the daemon's OTLP export settings ([setup step 1](001_playbook.md#before-you-begin)) and that a report settled after the deploy.

On Nov 04, 2026, plain search returned all ten development probes while metrics queries returned no series.
Generator local-blocks state was unreadable through the Grafana proxy.
Grafana also rejected the collector's delta metrics, blocking the fleet-wide queries.
`docs/v2/done/M238_001_P1_API_DOCS_INFRA_OBS_CHAT_ROUND_TRIP_DROPS_THREE_WAITS.md` records the values and diagnosis.
Rendered-row verification remains open.
