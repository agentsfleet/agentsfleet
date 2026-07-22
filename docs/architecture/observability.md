# Observability — `agentsfleetd` is the plane, the runner is bare

> One decision drives this whole file: **`agentsfleetd` owns backend-bound application
> telemetry; the host-resident `agentsfleet-runner` is deliberately bare.** A runner
> emits local logs and reports bounded liveness and result facts over the
> `/v1/runners` protocol. It holds no analytics or observability-backend credential.
> Everything below follows from that split.

This is a sibling of [`runner_fleet.md`](./runner_fleet.md) (the control-plane /
execution-plane structure) and [`data_flow.md`](./data_flow.md) (an event traced
through the runtime). This file answers a narrower question: *when something
happens, where does the signal go, and who owns it.*

---

## `agentsfleetd` — the observability plane

All of `agentsfleetd`'s telemetry lives under `src/agentsfleetd/observability/`. Four
independent signal paths, each with a different consumer:

- **Prometheus metrics (pull).** The `fleet_*` metric families — counters,
  histograms, and gauges for API and Server-Sent Events (SSE) backpressure and
  in-flight depth, process Resident Set Size (RSS), aggregate plaintext-erasure
  bytes and sensitive-write failures, the signup funnel, `fleet_triggered_total`,
  the runner fleet, and the Redis pool — render at the pull endpoint
  `GET /metrics` (`src/agentsfleetd/http/handlers/health.zig`, via `metrics_render.zig`).
  Nothing pushes; Prometheus scrapes. The plaintext-erasure families have no
  labels: tenant, workspace, fleet, route, individual secret size, and token
  material never enter telemetry.

- **OpenTelemetry (OTel) logs + traces — LIVE, exported direct.** `otel_logs.zig`
  and `otel_traces.zig` are real OpenTelemetry Protocol (OTLP) / JSON exporters: a
  ring buffer drained by a background flush thread that POSTs to Grafana Cloud
  (logs to Loki, traces to Tempo), gated on the `GRAFANA_OTLP_*` environment. Every
  structured log line fans out to the OTLP sink in addition to stderr. **There is no
  OTel collector** — the app exports straight to Grafana Cloud with no intermediary
  hop. Dashboards live in `deploy/grafana/`; the scrape/fleet setup is in
  `playbooks/operations/observability/`.

- **PostHog — product analytics.** A nullable client (see
  [`scaling.md`](./scaling.md) for where it sits in the request path). Present in
  `agentsfleetd`, absent in the runner.

- **Postgres execution telemetry.** Per-run accounting in
  `src/agentsfleetd/state/` (execution telemetry + the billing/credit-pool counters) —
  durable, queryable, the system of record for what a run cost.

### The M61 naming trap (read before you "remove" OTel)

The milestone named **`OTEL_EXPORT_REMOVAL`** did **not** remove the live OTel
export. It deleted a *different*, genuinely-dead trio (`otel_export` /
`otel_histogram` / `otel_json`) and **kept `otel_logs` and `otel_traces` wired**.
The name reads like "we stopped exporting OTel" — we did not. Before touching
anything OTel-shaped, confirm against `otel_logs.zig` / `otel_traces.zig` and the
`GRAFANA_OTLP_*` gate, not against the milestone name.

---

## `agentsfleet-runner` — deliberately bare

The host-resident runner (`src/runner/`) carries **no** metrics, OTel, PostHog, or
telemetry of its own — the lone `record_metric` hook is a no-op stub. It:

- emits **logfmt** logs locally (operator reads them on the host), and
- reports **liveness and results** to `agentsfleetd` over the `/v1/runners` protocol
  (heartbeat / `/renew` / result-report).

The server side owns the runner's observable state: `agentsfleetd` holds
`metrics_runner.zig` and derives fleet liveness itself (see `runner_fleet.md`). This
is intentional — a runner is cattle (`runner_fleet.md`, "Runners are cattle, not
pets"); it holds no datastore credentials and runs no exporter. Pushing telemetry
infrastructure onto the runner would re-couple it to the very backends the split
removed.

## Signal routing decision — logs, metrics, and traces

The runner has two exits, and they serve different jobs. Raw process output goes
to the host. Bounded execution facts go to the control plane through existing
fleet verbs.

```text
agentsfleet-runner
  ├─ structured stderr ──► journald / host supervisor
  │                         └─ optional host collector ──► Loki
  │                            (direct; never through agentsfleetd)
  └─ lease / heartbeat / renew / activity / report ──► agentsfleetd
                                                        ├─ /metrics
                                                        ├─ selected run span
                                                        └─ selected PostHog event

agentsfleetd structured logs ──► stderr + bounded OTLP exporter ──► Loki
agentsfleetd selected spans  ──► bounded OTLP exporter ──────────► Tempo
```

The optional host collector owns its backend credential and disk queue. The
runner binary still writes only stderr. A collector can read journald and push
directly to Loki, but `agentsfleetd` never accepts, stores, or relays raw runner
log lines. Activity frames remain user-visible run output; they are not a log
tunnel.

Remote collection is fail-closed on privacy. The collector may forward only
single-line logfmt records after applying an allowlist for `ts_ms`, `level`,
`scope`, `event`, registered `error_code`, and reviewed bounded metadata. It
drops prompts, response bodies, tokens, credentials, environment values,
arbitrary `msg` text, and any record it cannot parse. Sampling, when enabled, is
fixed level- or rate-based admission applied after redaction; it never examines
payload content or tenant identity. These requirements extend the repository
logging standard to the collector boundary instead of trusting a backend-side
scrubber after bytes have left the host.

No host collector is selected or deployed by this repository. The current
application-owned collector queue is therefore zero bytes. A later deployment
must name numeric memory, disk, retention, rate, sampling, and retry limits and
prove the allowlist above before enabling the direct path; defaults from a host
image are not an architecture guarantee. Until then its network rate is exactly
zero bytes per second.

| Signal | Producer and owner | Network path | Bound and loss behavior |
|---|---|---|---|
| runner logs | runner writes logfmt; host supervisor owns retention | none by default; optional host collector goes direct to Loki | host policy caps disk and queue use; collector loss never blocks a run |
| runner semantic metrics | `agentsfleetd` derives from accepted fleet verbs | existing runner API only; Prometheus pulls `agentsfleetd` | 4096 runner slots; overflow counters use `_other`; no per-event metric transport |
| runner host metrics | host collector or node exporter, if operators require them | direct to the metrics backend | outside the runner API; collector policy owns sampling and loss |
| runner traces | none in the runner at present | no trace exporter and no trace-context payload | correlate local logs with `event_id` and `lease_id`; add propagation only with a real runner span producer |
| control-plane logs | `agentsfleetd` structured logger | stderr plus OTLP to Loki | 2047 usable queued records; enqueue never blocks |
| control-plane metrics | in-memory Prometheus families plus selected OTLP run metrics | pull `/metrics`; OTLP metrics push where configured | fixed labels or explicit cardinality caps; exporter loss is best effort |
| control-plane traces | HTTP ingress and settled fleet delivery | OTLP to Tempo | route policy and sampling must keep production below the exporter budget |
| product analytics | `agentsfleetd` PostHog client | batched PostHog capture | selected business events only; no log, scheduler-arm, or trace traffic |

### Production wiring truth — Jul 23, 2026

| Surface | State | Production evidence |
|---|---|---|
| `agentsfleetd` structured stderr | installed and called | `main.zig` registers the sink; scoped loggers call it throughout the daemon |
| OTLP logs | installed and called when configured | `preflight.zig` installs; `main.zig` sends every accepted structured line |
| OTLP traces | installed and called when configured | `server.zig` emits `http.request`; `metering.zig` emits `fleet.delivery` |
| OTLP run metrics | installed and called when configured | `service_report.zig` calls `recordRunSettlement` after the fenced claim |
| PostHog product events | installed and partly called when configured | server start, workspace create, fleet trigger, and signup bootstrap have production captures |
| PostHog `FleetCompleted` | declared-only | event type and tests exist; no production capture call exists |
| runner backend exporter | absent | runner registers one local stderr sink and no OTLP or PostHog client |
| runner operational metrics | called through existing semantics | `agentsfleetd` updates fixed state after accepted heartbeat, lease, and report operations |

### Why raw runner logs bypass `agentsfleetd`

A log line is an unbounded byte stream. Heartbeat and report are bounded semantic
messages. Combining them makes request capacity, database capacity, and runner
progress depend on log volume. It also makes the control plane the failure point
for the diagnostics needed when that plane is unhealthy.

The direct collector path uses standard host supervision instead of a second
logging client inside the runner. Local logs survive a network outage according
to the host's retention policy. A full collector queue drops by collector policy;
it does not add backpressure to runner execution.

### Metrics stay semantic

The current runner metric path is the default pattern. `agentsfleetd` updates
fixed in-memory state after it accepts heartbeat, lease, and report operations.
One terminal report can also enqueue at most five OTLP samples: one credit delta,
three non-zero token directions, and one duration observation.

Do not turn scheduler arms, activity frames, log lines, lease identifiers, event
identifiers, model text, error text, or raw runner identifiers into new OTLP or
scheduler-derived metric labels. The existing Prometheus runner families retain
their explicitly capped `runner_id` drill-down. A scheduler metric is justified
only when it answers an operator question as a fixed aggregate such as queue
depth, fired total, or stale-target total. The runner currently needs no such
remote series: terminal `timeout_kill` outcomes and structured local deadline
events cover the visible failure.

### Traces stay on the process that creates spans

`agentsfleetd` accepts World Wide Web Consortium (W3C) `traceparent` at HTTP
ingress and emits `http.request` spans. It also emits one independent
`fleet.delivery` span after an accepted terminal report. The runner observer's
trace getter returns null and its setter is a no-op; no runner span exists to
join to a distributed trace.

A missing or malformed `traceparent` starts a new local root. Trace parsing is
best effort: invalid caller input never rejects the request and never crosses the
runner protocol.

Therefore the current decision adds no trace field to lease, renew, activity, or
report. `event_id` and `lease_id` provide log correlation. A future runner span
producer must first define a bounded span budget and durable context ownership;
only then should standard W3C context cross the fleet protocol.

High-rate runner API successes are not useful trace spans. The follow-on routing
work removes successful heartbeat, lease, renew, activity, and report requests
from the default HTTP span stream. The lease rule intentionally covers both empty
polls and grants; useful run work retains one settled `fleet.delivery` span. Trace
lifetime starts after route match but before API admission. Every response at
status 500 or above enters the server-error bucket first. A matched runner response
at status 400 through 499, including an admission-shed 429, enters the bounded
runner-rejection bucket. The buckets are disjoint. Other
API responses below status 500 use deterministic head sampling from the
server-generated span identifier, not caller-controlled trace input. The process
admits at most 10 generic request spans per monotonic second: four runner
rejections, four server errors, and two sampled successes. Every excess increments
`agentsfleet_http_trace_suppressed_total` under a fixed reason, so an invalid-token
storm stays visible without evicting all useful spans.

### PostHog is product analytics, not operations telemetry

The PostHog client is optional. It flushes at 20 events or 10 seconds and retries
at most three times. Initialization or capture failure disables or drops analytics
without failing a request.

The pinned PostHog library allocates two queue sides with 1000 event slots each.
One side can drain while the other accepts writes, so at most 2000 serialized
events are resident. A full write side drops the new event, increments the
library's internal drop counter, and logs a warning; its current `capture` API
does not return queue admission. Application wording must therefore say
`submitted`, never `captured` or `delivered`, for a successful call.

The Jul 23, 2026 source audit found four production event types: `ServerStarted`,
`WorkspaceCreated`, `FleetTriggered`, and `SignupBootstrapped`. `FleetCompleted`
is declared and tested but has no production capture. The follow-on work may wire
it only after durable report settlement and must set deterministic
`$insert_id` to the 64-character lowercase Secure Hash Algorithm 256-bit
(SHA-256) digest of
`fleet_id || 0x00 || event_id`, allowing PostHog to deduplicate its own batch retry
without forwarding an unbounded insertion key.
Scheduler mechanics, raw logs, spans, heartbeats, renewals, and activity frames
never become PostHog events.

---

## The shared logging module

The real logger is the named **`log`** module at `src/lib/logging/` — shared by both
binaries (it is in `src/lib/`, so both `build.zig` and `build_runner.zig` wire it as
a named module). Its shape makes conformance by construction:

- `mod.zig` — the body builder; callers write `log.scoped(.tag).level("event", .{…})`.
- `envelope.zig` — wraps every line with the required keys (`ts_ms=`, `level=`,
  `scope=`) and scrubs newlines to close log-injection.
- `sinks.zig` — fans application lines out to stderr **and** the OTLP sink, with
  a 4 KiB buffer and a `truncated=true` marker when a line overflows. The follow-on
  routing filter keeps exporter-internal scopes on stderr only, so a failed log
  exporter cannot enqueue its own warning forever.

Because the envelope and fan-out are enforced in the module, any call site that uses
`log.scoped(...).level(...)` is conformant for free — there is no per-call
discipline to remember. The field-level standard those calls must satisfy lives at
`docs/LOGGING_STANDARD.md` — a tracked symlink into the operating-model dotfiles,
so open it locally (GitHub renders only the symlink target path, not the document).
This file covers *where the signal goes*; that one covers *what a line must contain*.

---

## The OTLP exporter substrate (traces · logs · metrics)

`agentsfleetd` pushes three OTLP signals to Grafana Cloud over one shared pipeline,
gated by a single env triple (`GRAFANA_OTLP_ENDPOINT`, `GRAFANA_OTLP_INSTANCE_ID`,
`GRAFANA_OTLP_API_KEY`):

- **traces → Tempo** (`otel_traces.zig`, `/v1/traces`)
- **logs → Loki** (`otel_logs.zig`, `/v1/logs`)
- **metrics → Mimir** (`otel_metrics.zig`, `/v1/metrics`)

All three are built on `observability/otlp/`: a generic lock-free multi-producer/
single-consumer `Ring`, a shared `GrafanaOtlpConfig` + `configFromEnv`, a persistent
basic-auth HTTP `Client`, and an `Exporter(hooks)` flush driver that owns the
background flush thread. The follow-on passes the cancel-capable `std.Io` supplied
by `std.process.Init` through installation and borrows it for event waits,
monotonic timers, and HTTP. It does not use the non-canceling
`common.globalIo()` seam and does not create another input/output thread pool.
Each signal supplies only its entry type, serialization, and enqueue API. Emission
is fire-and-forget: a full ring drops the entry and never blocks the caller.
An enqueue calls the atomic producer side of `std.Io.Event` only when queue depth
reaches a reachable per-signal threshold: 50 logs, 50 traces, or 768 metrics.
The metric threshold leaves 255 usable slots while the consumer wakes. Lower
traffic waits for the maximum flush interval and therefore stays batched. Stop
always sets the event immediately. The standard event coalesces notifications
without making producers wait on an input/output mutex.
Collection removes entries before the HTTP post. A non-success HTTP response
therefore records a definite `export_rejected` local discard. A timeout or
transport outcome whose remote acceptance cannot be proven records
`export_uncertain`. A successful OTLP response with `partialSuccess` parses the
signal-specific rejected-item count and records it as `partial_rejected`; the
collector message is ignored so arbitrary backend text never enters logs. An
invalid partial-success body records the whole attempted batch as
`export_uncertain`, because remote acceptance cannot be established. These
outcomes record one stderr-only warning. The exporter
deliberately does not retry because OTLP JSON has no idempotency key and replaying
delta metrics can double-count them.

### Capacity and loss audit — Jul 23, 2026

The table uses usable ring capacity: the ring keeps one slot empty to distinguish
full from empty. Rates are capacity ceilings, not throughput benchmarks.

| Signal | Usable queue | Flush behavior | Capacity implication |
|---|---:|---|---|
| logs | 2047 records; body truncated at 512 bytes | up to 50 records every 5 seconds | 10 records per second sustained before backlog; ring drops are counted internally but not exported |
| traces | 1023 spans; 12 attributes per span | up to 50 spans every 5 seconds | 10 spans per second sustained; ring drops are counted internally but not exported |
| OTLP metrics | 1023 samples; at most 256 coalesced series | drains up to 1023 every 5 seconds, then coalesces label sets | about 204 samples per second, or about 40 worst-case five-sample settlements per second; excess series are discarded and counted |
| PostHog | 1000 events per double-buffer side; up to 2000 resident | 20 events or 10 seconds; three retries | a full write side drops the new event and increments the library drop counter; capture return alone does not prove admission |

The scenario model below makes the producer unit and the limiting resource
explicit. `R` is registered runners, `B` is committed pre-execution credit debits
per second, `C` is accepted terminal reports per second, `L` is runner log records
per second, and `D` is control-plane log records per second. Unknown workload
rates stay variables; the architecture bounds what the application owns instead
of inventing a traffic number.

| Signal | Scenario | Producer volume | Application-owned bound and outcome |
|---|---|---:|---|
| runner logs | steady | `L` records/s, each structured runner record at most 4096 bytes | local stderr only; repository network queue and remote bytes remain zero |
| runner logs | burst | arbitrary `L` until the host supervisor applies its byte/rate policy | no `agentsfleetd` load; local host retention is the sole current bound |
| runner logs | backend outage | unchanged local record rate | no application retry or queue; an enabled collector uses its declared disk ceiling then drops |
| runner logs | fleet growth | sum of host-local `L`; no central application aggregation | each host remains isolated; direct collection stays disabled until numeric host policy and redaction proof exist |
| control-plane logs | steady | `D` accepted structured records/s, each OTLP body truncated at 512 bytes | stderr remains authoritative; 2047 usable queue slots and the current 50-record batch every 5 seconds sustain 10 records/s before backlog |
| control-plane logs | burst | arbitrary `D` from bounded control-plane events | non-blocking admission fills at 2047 records, then drops and counts overflow internally; the follow-on wakes at 50 and drains cycle-start backlog |
| control-plane logs | backend outage | unchanged `D` while the endpoint is unavailable | the fixed ring fills, later entries drop, and product work continues; exporter warnings remain stderr-only after the follow-on |
| control-plane logs | fleet growth | `D` follows semantic control-plane events, never the raw runner byte stream | queue capacity remains 2047 process-wide; the logging allowlist excludes prompt, body, token, credential, environment, and arbitrary error fields |
| metrics | steady | idle heartbeats update memory but enqueue zero OTLP samples; each billed lease enqueues one credit sample and each accepted report enqueues at most five | 4096 runner slots; 1023 usable OTLP sample slots |
| metrics | burst | at most `B + 5C` OTLP samples/s before coalescing | non-blocking ring admission; overflow drops and is counted |
| metrics | backend outage | at most `B + 5C` attempted samples/s | ring holds 1023; later entries drop; request, debit, and settlement paths continue |
| metrics | fleet growth | `R` liveness keys plus debit and report samples | 4096 exact runner slots regardless of `R`; overflow counters aggregate into `_other`, while last-seen and active-lease gauges drop |
| traces | steady | `R/10` heartbeat requests/s before lease and run traffic | follow-on admits at most 10 generic request spans/s plus one settled delivery span per accepted run |
| traces | burst | arbitrary matched requests plus `C` settled runs/s | generic spans keep the fixed 4 rejection, 4 server-error, and 2 sampled-success budget; the 1023-slot ring bounds all selected spans |
| traces | backend outage | selected generic spans plus one delivery span per accepted run | ring fills to 1023, then selected spans drop without blocking product work |
| traces | fleet growth | heartbeat input grows linearly with `R`; generic output does not | generic request output remains 10 spans/s process-wide and exact suppressions aggregate in Prometheus |

Idle heartbeats alone produce one matched HTTP request every 10 seconds per
runner. The current unsampled `http.request` path therefore produces 10 spans per
second at 100 idle runners, 100 spans per second at 1000, and 1000 spans per second
at 10000. Lease polls, renewals, activity batches, and reports add to that rate.
The 100-runner case already consumes the trace exporter's steady drain budget.

Metric coalescing happens after samples enter the ring, so it reduces wire series
but not enqueue pressure. The aggregator admits at most 256 distinct label sets
per flush and discards excess samples. The existing
`agentsfleet.telemetry.samples_dropped` self-signal includes ring and aggregation
loss, but arrives only if a later metric export succeeds. Prometheus therefore
needs fixed `ring_full`, `aggregate_cap`, `serialize_failed`, `export_rejected`,
`partial_rejected`, and `export_uncertain` reasons. The follow-on work also makes log and trace draining
threshold-driven or drain-to-empty so the fixed five-second, 50-entry cadence is
not their throughput ceiling. Definite backend rejection remains distinct from
collector-declared partial rejection and uncertain remote delivery after a
transport or malformed-response failure.

### Metrics: off-Postgres dashboards, money stays in PG

Metric labels are bounded at their source and again by the 256-series flush
ceiling:

| Label | Allowed values and ceiling | Overflow action |
|---|---|---|
| `runner_id` | at most 4096 exact values | counters merge into `_other`; gauges drop |
| runner `reason` and `outcome` | compile-time execution enums plus `unknown` for reason | closed sets; no dynamic overflow value |
| `workspace` | prohibited; follow-on removes it | remove the label and its process-local guard; do not accumulate tenant series across restarts |
| `direction` | `input`, `cached`, `output` | closed set; no overflow value |
| `posture` | `platform`, `self_managed` | closed set; no overflow value |
| `model` | prohibited; follow-on removes it | remove the label and its dashboard selector; do not bucket an evolving catalog |

The metrics signal lets operators watch credit-drain, token throughput, and run
latency without any dashboard query touching the control-plane Postgres. Series:

- `agentsfleet.credit.drained_nanos` (sum) — posture only after the follow-on removes the current unbounded model and workspace labels
- `agentsfleet.tokens.processed` (sum) — by direction {input, cached, output}
- `agentsfleet.run.duration_ms` (histogram)
- `agentsfleet.telemetry.samples_dropped` (sum) — exporter self-observability

The emits live in the **service-orchestration layer** (`service_billing` at the
receive debit, `service_report` at the settle), strictly **after** the money
transaction commits — never inside `fleet_runtime/metering.zig` or
`fleet/renewal_settle.zig`. So the exporter can never block or fail a debit, and the
wallet + ledger + `metering_periods` stay transactional in Postgres. The flush
coalesces a window's samples into one **DELTA** dataPoint per (metric, labelset); a
collector with the `deltatocumulative` processor must convert them before Mimir.
No such collector is provisioned by this repository today. The exporter is called,
but the dashboard remains a prepared, not operationally proven, path until the
configured endpoint performs that conversion. Dashboard:
`deploy/grafana/agent-observability.json`.
