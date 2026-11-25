---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Observability: where each signal goes, and what bounds it

> Read this when you add a log line, span or metric, or need to know where a signal lands and what can drop it. Parent: [`README.md`](./README.md) · Product events: [product analytics](./product_analytics.md).

## What it is

`agentsfleet` sends telemetry down three paths, and each serves a different reader.

| Path | Carries | Lands in | Reader |
|---|---|---|---|
| OpenTelemetry Protocol (OTLP) push | logs, traces and metrics | a collector, then Loki, Tempo and Mimir in Grafana Cloud | operators |
| PostHog | product events | PostHog | product analytics |
| Postgres | usage lines and balances | the `billing` schema | money and exact cost |

| Fact | Value | Where |
|---|---|---|
| Daemon metric families | every exported name declared once | `rustd/crates/protocol/af_observability/src/metrics/census.tsv` |
| Runner metric families | declared apart from the daemon's | `rustd/crates/runner/afr_telemetry/src/families/census.tsv` |
| Daemon collectors | `otelcol-dev` and `otelcol-prod`, one Fly app per environment | `deploy/fly/otelcol-{dev,prod}/` |
| Runner collector | its own, on the bare-metal host | `deploy/baremetal/otelcol-runner.{yml,nft,service}` |
| Pull endpoint | none; the daemon pushes and nothing scrapes it | `agentsfleetd`'s `preflight/otlp` module |
| Loss policy | a full queue drops, never blocks a request; one export attempt | `af_otlp`'s pipelines module |
| Runner span budget | 256 per run, 128 per second, the run's root always kept | `afr_telemetry`'s `budget` module |
| Per-runner series | 4,096 `runner_id` slots; counters overflow to `_other` | `af_observability` |
| Tenant identity on metrics | never | exact cost is a Postgres query |

```text
BARE-METAL HOST
  agentsfleet-runner
    ├─ logfmt on stderr ───────────────► host log store (stays on the host)
    ├─ spans + runner families ── OTLP ─► runner collector ──► Grafana Cloud
    └─ heartbeat / renew / report ─────► agentsfleetd (runner stream)

FLY
  agentsfleetd
    ├─ metric families ─┐
    ├─ spans ───────────┼─ OTLP ──► otelcol-{env} ──► Loki · Tempo · Mimir
    ├─ logs (+ stderr) ─┘           (holds the vendor credential)
    └─ product events ──────────────► PostHog
```

## Why it exists

Runners may sit behind network address translation or on a customer's host, so nothing can scrape them.
Bounded per-runner facts therefore ride the verbs a runner already sends, and the daemon exports them.
Raw runner logs stay off the daemon, because routing unbounded log bytes through it would tie its capacity to log volume.

The daemon pushes to one endpoint, named by the standard `OTEL_EXPORTER_OTLP_*` variables.
The collector holds the vendor credential and owns the fan-out, so changing a backend is collector configuration, never a daemon release.
A runner holds no telemetry credential at all.

## How it behaves

### Signal routing

| Signal | Producer | Path | Bound and loss |
|---|---|---|---|
| Daemon logs | `tracing` events in `agentsfleetd` | stderr, and OTLP to Loki when configured | the batch log queue, 2,048 records; a full queue drops |
| Daemon metrics | census families | one OTLP push every 60 s | each family's census ceiling |
| Daemon traces | HTTP requests and settled deliveries | OTLP to Tempo | the 2,048-span batch queue; drops past it are not counted |
| Per-runner facts | `agentsfleetd`, from accepted runner verbs | the daemon's OTLP push | 4,096 runner slots |
| Runner families | `afr_telemetry`, for facts no verb carries | OTLP to the runner collector | closed label sets |
| Runner traces | the runner's four span kinds | OTLP to the runner collector | 256 spans per run, 128 per second |
| Runner logs | logfmt on stderr | the host's log store only | host disk policy |
| Product events | the daemon and the dashboard | PostHog | see [product analytics](./product_analytics.md) |

### Metric families

`census.tsv` declares each daemon family once: wire name, kind, number type, unit, temporality, labels, histogram bounds, series ceiling and operator guidance.
The registry is built from that file, and `test_metric_family_registry_parity` grades the two against each other both ways.
`every_census_family_has_a_producer` fails a family nothing produces, so this page lists no families.

Producer code names a family through a `Declared<Kind>` constant and claims its instrument from `Instruments` in `af_observability`.
A claim whose kind or number type disagrees with the census is refused at boot.
Adding a family is one census row, one `Declared` constant and one producer call.

Runtime families carry the `agentsfleet_` prefix.
The cost families use dotted OpenTelemetry generative AI (GenAI) names, such as `gen_ai.invoke_agent.duration`, where the standard name is truthful.
Grafana dashboards and alerts live in `playbooks/operations/observability/assets/`, applied and checked by the operator playbook.

### Labels

Every label value comes from a closed enum in `af_observability`'s label registry, except two open ones.

| Label | Allowed values | At the limit |
|---|---|---|
| `runner_id` | at most 4,096 exact values | counters merge into `_other`; gauges drop |
| `gen_ai.request.model` | the run's catalogue model | past the SDK's 2,000 series per stream, new values fold into `otel.metric.overflow` |
| `gen_ai.provider.name` | canonical names plus the compiled catalogue's public providers | a private name omits the attribute and counts `agentsfleet_otel_attribute_omitted_total` |
| `agentsfleet.execution.posture` | `platform`, `self_managed` | closed |
| `gen_ai.token.type` | `input`, `output` | closed |
| library `surface` | `tenant_models`, `global_models` | closed |
| library `stage` | `sql`, `serialize` | closed |

Workspace and tenant identity never reach a metric, because a per-process guard cannot cap series across replicas and restarts.
Exact per-workspace or per-model cost is a query against `billing.usage_lines`.
Correlation identifiers may ride a span, because a span is one bounded record that ends.

### Per-runner facts

The daemon keeps four per-runner families in a fixed 4,096-slot table keyed by `runner_id`.
Recording takes a read lock over atomic counters, and the export reads only that memory, never Postgres.
A daemon restart zeroes the table; counter-reset handling absorbs it, and gauges recover within one heartbeat.

| Family | Meaning | Across N daemon replicas |
|---|---|---|
| `agentsfleet_runner_runs_total` | runs by `outcome`: `acquired`, `succeeded`, `failed`, `expired` | `sum by (runner_id)`, exact |
| `agentsfleet_runner_failures_total` | failures by `reason` | `sum by (runner_id)`, exact |
| `agentsfleet_runner_last_seen_seconds` | the last time a replica saw the runner | `min by (runner_id)`, exact |
| `agentsfleet_runner_active_runs` | runs held now | `sum by (runner_id)`, approximate |

A runner's verbs spread across replicas, so each replica exports only its slice.
`active_runs` is approximate because a run's grant and release can land on different replicas.

### Traces

`agentsfleetd` opens one `http.server.request` span per matched request (`afd_api`'s `router/trace.rs`), carrying the route template, method and status.
It reads no `traceparent` header, so every request span is a new root.
It has no sampler, so every runner heartbeat, one every 10 s per runner, is a span.

| Span | Starts | Stages or attributes |
|---|---|---|
| `fleet.delivery` | at the turn's creation | ends at the `turn_complete` publish; `queue_wait_ms`, `complete_ms`, plus the runner's `memory_ms`, `model_ms`, `tools_ms`, `sandbox_wait_ms` and `settle_ms` |
| `fleet.steer` | a message is written | `admit`, `announce` |
| `fleet.assign` | an assignment pass | `select`, `admit`, `issue`, `encode`, `send` |
| `fleet.report` | a report arrives | `load`, `prepare`, `capture`, `commit`, `announce`, `ack` |
| `fleet.load`, `fleet.capture` | a memory read or write | `fence`, then `memory_read` or `memory_write` |

Each stage is `agentsfleet.<verb>.<stage>_ms`, the milliseconds since the previous mark (`af_observability`'s `semconv::verb`).
A verb that reaches no stage records nothing, so an assignment pass that finds no work leaves no span.
Spans join each other by `agentsfleet.turn.id` and `gen_ai.agent.id`, never by trace context.

The runner exports four span kinds: `runner.run`, `invoke_agent`, `chat` and `execute_tool`.
Each run is its own root trace, and `runner.run` gains each reached segment as `agentsfleet.runner.<segment>_ms`.
A tool span names its tool from the published catalog, or `_other`, so a tool name the model invented never leaves the host.

The runner keeps at most 256 spans per run and 128 per second, and a run's root span is always kept.
A span under a dropped span is dropped too, and each one counts in `agentsfleet_runner_spans_suppressed_total`.

### The chat path's signals

| Step | Logs | Metrics | Trace |
|---|---|---|---|
| Message written | `fleet_steer_failed` on refusal | `agentsfleet_turns_total{outcome}`; `agentsfleet_steer_dispositions_total{outcome}` | `fleet.steer` |
| Wake ring | `turn_wake_ring_failed` | `agentsfleet_fleet_ready_wake_failures_total{kind}` | none |
| Run issued | `runner_run_issued` with `queue_wait_ms` | `delivery_stage{event_to_assign}` | `fleet.assign` |
| Message folded into a live run | `folds_assigned`, `folds_settled` | `agentsfleet_run_folds_total{outcome}` | none |
| Run | `run_completed` or `run_failed` with every segment | runner families | `runner.run` |
| First chunk | none | `delivery_stage{assign_to_first_chunk, event_to_first_chunk, run_to_first_chunk}` | none |
| Report settled | `report_settled`, `completion_published`, `memory_capture_failed` | `delivery_stage{assign_to_report, report_to_complete, event_to_complete}` | `fleet.report`, `fleet.delivery` |
| Runner cordoned or lifted | `runner_cordoned`, `runner_cordon_lifted` | `agentsfleet_runner_cordon_changes_total{change}` | none |
| Trigger added or removed | `trigger_changed` with `fleet_id`, `kind` and `by` | none | the request span |

`delivery_stage` is `agentsfleet_fleet_delivery_stage_seconds` (`af_observability`'s `metrics::label::fleet`).

Inside a run, each model turn logs `model_turn_started`, `model_turn_first_text`, then `model_turn_completed` or `model_turn_failed`, with no content (`af_providers/src/turn/observer/`).
A durable task logs `dt_transition` on each status change, `dt_decision_recorded` and `dt_decision_funding_refused` for decision calls, and `dt_effect_uncertain` when a write's outcome is unknown.

The steer route answers `Server-Timing: auth;dur=…, upstream;dur=…`, and the browser leaves `af:send`, `af:accepted`, `af:first-frame` and `af:complete` marks.

### Cost families

The report path records four cost samples per run, after the money transaction commits (`afd_fleet/src/run/report/observations/mod.rs`).
They are the invocation's duration, input tokens, output tokens and cache-read tokens (`af_observability`'s `producers::cost::invocation`).
An exporter fault can therefore never block or fail a debit.

| Family | Unit |
|---|---|
| `gen_ai.invoke_agent.duration` | seconds of runner wall time |
| `agentsfleet.invoke_agent.token.usage` | tokens per invocation, by `gen_ai.token.type` |
| `agentsfleet.invoke_agent.cache_read.token.usage` | cache-read tokens, a subset of input |

### The runner's own telemetry

`afr_telemetry` builds the runner's only export pipeline, through the shared `af_otlp` transport, when `OTEL_EXPORTER_OTLP_ENDPOINT` names a collector.
The runner refuses any header knob or a user in the endpoint, naming the knob, because a header is how an exporter carries a credential.
It logs `telemetry_export_started` with the knob's name, or `telemetry_export_disabled`, and never the endpoint.

Runner families cover provider turns and retries, sandbox start and release, tool calls, dropped live-tail frames, failed memory pushes and decision evaluations.
Its losses count in `agentsfleet_runner_otlp_entries_discarded_total{signal,reason}`, so a down runner collector shows as a series.
No tenant, fleet, run or event identifier rides a runner metric.

A followed tool `GET` logs `http_redirect_started`, then `http_redirect_completed` or `http_redirect_failed` (`af_egress`).
The records carry hop count, status and elapsed time, never a destination path, query or body.

### The daemon's export path

`agentsfleetd` reads four knobs at preflight, spelled as the OpenTelemetry specification spells them.

| Knob | Default | Refused |
|---|---|---|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | unset: no export, logs `startup_otel_disabled` | an endpoint `af_otlp`'s configuration refuses |
| `OTEL_EXPORTER_OTLP_HEADERS` | none | a pair with no `=` |
| `OTEL_EXPORTER_OTLP_PROTOCOL` | `http/protobuf`; `http/json` accepted | any other, `grpc` included |
| `OTEL_EXPORTER_OTLP_TIMEOUT` | 10,000 ms | zero or unreadable |

With an endpoint, boot builds pipelines on `opentelemetry-otlp` over HTTP, posting to `{endpoint}/v1/{traces,metrics,logs}`.
Each exporter makes one attempt, and a failed batch counts in `agentsfleet_otlp_entries_discarded_total{signal}` without blocking a request.
The log exporter counts its failures without a warning, because a warning would become a log record fed back to the failing exporter.

Metrics are cumulative and collected every 60 s (`COLLECT_INTERVAL` in `af_otlp`), because Grafana Cloud bills samples per minute.
A restart starts a new cumulative interval, identified by the service instance.
Shutdown flushes every provider before the process exits.

`logs.rs` installs the stderr subscriber before any knob is read, then bridges the same `tracing` events into OTLP.
Stderr stays logfmt whether the exporter is present, absent or failing.
Field rules live in [the logging standard](../../.orly/standards/domains/logging.md).

The collector's receiver requires a credential, because the Fly private network spans the whole organisation.
All three signals share one resource: `service.name`, `service.namespace=agentsfleet`, `service.version`, and `service.instance.id` when a trusted one exists.

## Limits

- **No service level objective is set.** No target, error budget or burn alert exists; the dashboards show plain rates.
- **No environment label.** No series carries `deployment.environment`, so a query cannot tell development from production.
- **Nothing watches the infrastructure.** No family measures the collector, the host or the datastore, and no `hostmetrics` receiver is configured. The only resource reading is `agentsfleet_process_resident_memory_bytes`.
- **Collector loss is invisible.** The collector's queue is in memory, and the daemon counts an export good once the collector answers 2xx.
- **Runner logs stay on the host.** The runner collector carries traces and metrics only; log forwarding waits on an allowlist that is not built.
- **Runner stream measurements are daemon-side only.** The daemon records the runner stream families; the runner side does not export them yet.
- **No route policy for spans.** Daemon span volume grows with runner count, and the batch queue drops the excess without counting.

## Related pages

- [Product analytics](./product_analytics.md): every PostHog event
- [Runner protocol](./runner_protocol.md): the stream that carries runner facts
- [Configuration reference](./config_reference.md): `[telemetry]` keys
- [Billing](./billing.md): exact cost in Postgres
- `playbooks/operations/observability/`: dashboards, alerts and their checks
