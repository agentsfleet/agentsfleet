# Observability — `agentsfleetd` is the plane, the runner is bare

> One decision drives this file: **`agentsfleetd` owns backend-bound telemetry;
> `agentsfleet-runner` is deliberately bare.** A runner emits local logs and
> reports bounded liveness and result facts over `/v1/runners`. It holds no
> analytics or observability-backend credential.

Siblings: [`runner_fleet.md`](./runner_fleet.md) (plane structure),
[`data_flow.md`](./data_flow.md) (an event traced through the runtime). This
file answers: when something happens, where does the signal go, and who owns it.

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Signal paths | 3 | OTLP push (no collector hop) · <img src="https://cdn.simpleicons.org/posthog" width="14" alt="" /> PostHog · 🐘 Postgres (money) | §The three signal paths |
| Metric namespace | `agentsfleet_` runtime families; dotted semconv cost families | the metric-family registry declares every exported name; the namespace guard reads it | §The three signal paths |
| Runner telemetry | deliberately bare | `record_metric` is a no-op stub; local logfmt to the host, liveness over `/v1/runners` | §`agentsfleet-runner` — deliberately bare |
| Library read series | 102 total, build-asserted | closed enums; a new member fails the build, never grows the export | §Library read stages are metrics, not spans |
| Trace budget | 10 generic spans per monotonic second | 4 runner rejections + 4 server errors + 2 sampled successes; successful runner verbs never enqueue | §Traces |
| OTLP queues | logs 2047 · traces 1023 · metrics 1023 (derived series ceiling: 256 cost + runtime worst case) | fire-and-forget; a full ring drops, never blocks; no retry, deliberately | §The OTLP exporter substrate, §Capacity and loss audit |
| PostHog events | 9 captured, 2 declared-uncaptured | `FleetCompleted` fires only after the fenced claim; `$insert_id` = SHA-256 of `fleet_id \|\| 0x00 \|\| event_id` | §PostHog is product analytics |
| Per-runner label ceiling | 4096 exact `runner_id` slots | counters overflow to `_other`, gauges drop | §Label registry |
| Tenant identity on metrics | never | exact per-workspace cost is a Postgres ledger query, which is exact rather than bounded | §Label registry |
| Log envelope | 4 KiB buffer, `truncated=true` on overflow | exporter-internal scopes stay stderr-only so a failing exporter cannot feed itself | §The shared logging module |
| Performance gating | nothing gates on a percentile | the exported series are the evidence; a threshold that cannot fail reports success forever | §Library read stages are metrics, not spans |
| The M61 naming trap | the live OTel export survived `OTEL_EXPORT_REMOVAL` | check the OTLP log and trace exporters + the `GRAFANA_OTLP_*` gate, never the milestone name | §The M61 naming trap |
| Production wiring truth | per-surface state, each row with its code evidence | re-read the evidence column rather than trusting the row | §Signal routing |

## The three signal paths

The control plane's telemetry surfaces live in `afd_observability`.

| Path | What | Consumer |
|---|---|---|
| OTLP (push) | logs → Loki, traces → Tempo, metrics (runtime + cost families) → Mimir. Direct to <img src="https://cdn.simpleicons.org/grafana" width="14" alt="" /> Grafana Cloud; **no collector hop**. Gated on the `GRAFANA_OTLP_*` env triple. The daemon's **only** metrics egress: there is no pull endpoint. | Grafana Cloud, operator dashboards |
| <img src="https://cdn.simpleicons.org/posthog" width="14" alt="" /> PostHog | nullable client, product events only | product analytics |
| Postgres | per-run execution telemetry + billing counters in `afd_billing` | the money system of record |

**One process, one registry.** Every runtime family carries the `agentsfleet_`
prefix; the evented cost families use dotted OpenTelemetry
semantic-convention names (each divergence is explained in §Metrics stay semantic;
`docs/metrics.census.tsv` carries the full contract).
The metric-family registry declares every exported name, and the namespace
guard fails on any family outside it. `fleet_id`, log event names,
`EventKind` tags, and the Redis consumer group keep their old spelling; the
namespace rule covers only exported metric families.

**One registry row is the whole family.** Beside its wire identity, each
family declares its label dimensions — the closed enum per label key, plus an
at-most-one dynamic dimension (request model, runner identifier) — in
the dimension table, the registry's sibling. The instrument layer
generates everything downstream from that one table at build time: the flat atomic storage cells (one per label combination), a
typed writer whose label struct makes a wrong or missing dimension a compile
error, snapshot reads, and the flush-time collect loop that emits every cell —
zero values included — into the aggregator. Sources that cannot be storage
cells (the Redis pool snapshot, the resident-set probe, flush-thread liveness)
are `live_read` hooks the collect loop runs after the cells; their absence
keeps the family out of the window rather than faking a zero. Labels are
interned to build-time indices, so a sample is a fixed ≤128-byte value and the
aggregator locates a series by open-addressed hash instead of a linear scan.
Adding a family is one registry row plus one writer call; everything else —
storage, collection, series ceiling, census membership — derives from the
declaration, so there is no second copy to drift.

Runtime deployment carries no dashboard files. Grafana dashboard and alert
definitions live under
`playbooks/operations/observability/providers/grafana/assets/`, where the
operator playbook checks, applies, and verifies them against source-owned
metrics.

### The M61 naming trap

The milestone named `OTEL_EXPORT_REMOVAL` did **not** remove the live OTel
export. It deleted a dead trio (`otel_export`/`otel_histogram`/`otel_json`) and
kept `otel_logs` and `otel_traces` wired. Before touching anything OTel-shaped,
check the OTLP log and trace exporters and the `GRAFANA_OTLP_*` gate, not
the milestone name.

## Metric family census — what to watch, and what it means

The complete export lives in ONE file: [`docs/metrics.census.tsv`](../metrics.census.tsv)
— every family the daemon pushes over OTLP, exactly once, with its kind,
number type, unit, temporality, label keys, histogram bounds, series policy,
and the operator guidance (`category`, `watch_for`) that used to be a table
here. The registry test grades the declared registry against that file in
both directions, so a family cannot exist on the wire without a census row
or a census row without a family. This section deliberately repeats none of
it; a second copy is a second thing to keep true.

Category legend: **latency** (how slow), **traffic** (how much), **errors**
(what failed), **saturation** (how full), **health** (is the plumbing itself
working). Improve latency by finding the slow stage; errors by rate per cause;
saturation by capacity or shedding; health by fixing the exporter or pool, not
the workload.

## `agentsfleet-runner` — deliberately bare

The runner (`src/runner/`) carries no metrics, OTel, or PostHog. Its lone
`record_metric` hook is a no-op stub. It emits logfmt locally for the host
operator and reports liveness and results over `/v1/runners` (heartbeat,
`/renew`, result-report). `agentsfleetd` owns the runner's observable state in
`afd_observability`'s per-runner table and derives fleet liveness itself. Runners are cattle
(`runner_fleet.md`); an exporter on the runner would re-couple it to the
backends the split removed.

## Signal routing

```text
agentsfleet-runner
  ├─ structured stderr ──► journald / host supervisor
  │                         └─ optional host collector ──► Loki
  │                            (direct; never through agentsfleetd)
  └─ lease / heartbeat / renew / activity / report ──► agentsfleetd
                                                        ├─ runtime metric families (OTLP push)
                                                        ├─ selected run span
                                                        └─ selected PostHog event

agentsfleetd structured logs ──► stderr + bounded OTLP exporter ──► Loki
agentsfleetd selected spans  ──► bounded OTLP exporter ──────────► Tempo
```

**Why raw runner logs bypass `agentsfleetd`.** A log line is an unbounded byte
stream; heartbeat and report are bounded semantic messages. Routing logs
through the control plane would tie request and database capacity to log
volume, and would make the control plane the failure point for the diagnostics
you need when that plane is unhealthy.

**Collector rules (fail-closed on privacy).** A host collector may forward
only single-line logfmt records after an allowlist: `ts_ms`, `level`, `scope`,
`event`, registered `error_code`, reviewed bounded metadata. It drops prompts,
response bodies, tokens, credentials, environment values, arbitrary `msg`
text, and anything it cannot parse. Sampling is level- or rate-based after
redaction; it never reads payload content or tenant identity. No collector is
deployed by this repository today, so its network rate is zero bytes per
second. Enabling one requires numeric memory, disk, retention, rate, sampling,
and retry limits plus the allowlist proof.

| Signal | Producer / owner | Path | Bound and loss |
|---|---|---|---|
| runner logs | runner logfmt; host owns retention | none by default; optional collector direct to Loki | host policy caps disk; loss never blocks a run |
| runner semantic metrics | `agentsfleetd`, from accepted fleet verbs | OTLP push (streamed per-runner families) | 4096 runner slots; overflow → `_other` |
| runner host metrics | node exporter, if operators want it | direct to metrics backend | outside the runner API |
| runner traces | none | none | correlate logs via `event_id` + `lease_id` |
| control-plane logs | structured logger | stderr + OTLP to Loki | 2047 queued records; enqueue never blocks |
| control-plane metrics | runtime + cost families | one OTLP push; no pull endpoint | fixed labels or explicit caps |
| control-plane traces | HTTP ingress + settled delivery | OTLP to Tempo | route policy keeps output under the budget |
| product analytics | PostHog client | batched capture | selected business events only |

### Production wiring truth

| Surface | State | Evidence |
|---|---|---|
| structured stderr | installed, called | `rustd/crates/agentsfleetd/src/logs.rs` installs the subscriber on stderr at boot; `AGENTSFLEET_LOG_LEVEL` sets the level |
| server spans | emitted | `rustd/crates/afd_api/src/router/trace.rs` opens one span per matched request, carrying the route template, the method and the status — never the raw path |
| OTLP logs | absent | no transport: `rustd/crates/afd_observability/src/lib.rs` states the OTLP transport is not in the crate |
| OTLP traces | absent | no transport: `otlp_export` is a reserved name in `rustd/crates/agentsfleetd/src/inventory.rs` with no spawn site |
| OTLP run metrics | absent | no transport, and no exported metric family is declared under `rustd/crates/` |
| PostHog events | installed, called when configured | `rustd/crates/afd_observability/src/product.rs`; boot opens the client, the supervised `analytics_flush` task drains it before exit |
| runner exporter | absent | one local stderr sink, nothing else |

## Metrics stay semantic

`agentsfleetd` updates fixed in-memory state after it accepts heartbeat, lease,
and report verbs. One terminal report enqueues at most five OTLP samples: one
credit delta, three non-zero token directions, one duration.

Do not turn scheduler arms, activity frames, log lines, lease or event
identifiers, model text, error text, or raw runner identifiers into metric
labels. A scheduler metric is justified only as a fixed aggregate (queue depth,
fired total, stale-target total). The runner needs no remote series today:
terminal `timeout_kill` outcomes and local deadline events cover the visible
failure.

## Traces

`agentsfleetd` accepts W3C `traceparent` at ingress and emits `http.request`
spans, plus one `fleet.delivery` span after an accepted terminal report. A
missing or malformed `traceparent` starts a new local root; invalid input never
rejects a request. The runner has no span producer; its verbs carry no trace
field. `event_id` and `lease_id` correlate logs instead. A future runner span
producer must first define a bounded span budget and durable context ownership.

**Route policy.** Successful heartbeat, lease, renew,
activity, and report requests never enqueue spans. Responses ≥ 500 enter the
server-error bucket. Matched runner 4xx (including admission-shed 429) enter
the runner-rejection bucket. Other sub-500 responses use deterministic head
sampling from the server-generated span id, never caller input. Budget: **10
generic spans per monotonic second** (4 runner rejections + 4 server errors +
2 sampled successes). Excess increments
`agentsfleet_http_trace_suppressed_total{reason}`.

Why it matters: idle heartbeats alone are one matched request per runner per
10 s. Unfiltered, 100 idle runners consumed the exporter's whole steady drain
budget. The ceiling is now fixed at any fleet size.

## Library read stages are metrics, not spans

The authenticated library reads (tenant model registry, global catalogue,
Fleet gallery) record stage timing as fixed-cardinality families in
the library stage registry. A six-stage read emitting spans would spend most of a
second's span admission on its own timing and evict the server-error spans the
budget protects. Stage timing is high-frequency, closed-label data; that is
what a metric is for. The trace half is unchanged: `traceparent` in, one
`http.request` span out. The browser client mints a fresh root per request
(`ui/packages/app/lib/api/client.ts`); it holds no parent span.

Label members live in the label registry below. Series are fixed at build
time: 102 total, asserted by the build, so a new enum member fails the build
instead of growing the export.

| Family | Labels | Series |
|---|---|---|
| `agentsfleet_library_stage_duration_seconds_total` | `surface`,`stage` | 30 |
| `agentsfleet_library_stage_observations_total` | `surface`,`stage` | 30 |
| `agentsfleet_library_read_outcome_total` | `surface`,`outcome` | 27 |
| `agentsfleet_library_pool_result_total` | `pool_result` | 4 |
| `agentsfleet_library_cache_outcome_total` | `cache` | 5 |
| `agentsfleet_library_payload_bytes_total` | `surface` | 3 |
| `agentsfleet_library_results_total` | `surface` | 3 |

Design points, each load-bearing:

- One observation carries five dimensions; no metric does. The cross-product
  is 5400 series, nearly all permanently zero.
- Pool and cache families carry no `surface` label: a starving pool is
  process-wide, and neither may carry tenant or request identity.
- Duration and observations are two counters, not a summary.
  `rate(duration)/rate(observations)` is the mean cost of a *span*, and `sql`
  fires twice per registry read (both sides of `secret_project`). Dividing by
  requests would halve its apparent cost.
- `secret_project` survives the read-path decryption removal with a narrowed
  meaning: it times the batch presence query, and its decryption counter is
  pinned at zero. A regression that reintroduces per-row decryption shows up
  as a stage that suddenly decrypts, not one that silently reappears.
- One outcome per request on every exit path. The read-scope layer owns
  the lifecycle; the default outcome is `internal_error`, so an unclassified
  path surfaces as something to investigate, never as `ok`.

**The exported series are the evidence, and nothing gates on a percentile.** A latency
threshold in a universal check fails on a noisy runner, gets widened until it
cannot fail, then reports success forever. Percentile comparison needs a
provisioned environment with pinned pool size, warm state, and concurrency.
That capture harness is deferred to its own spec. An earlier draft shipped a
report validator and a `capture-library-performance` target; the target could
not capture, so both were removed rather than left looking like a capability.

## PostHog is product analytics, not operations telemetry

The client is optional. Flush: 20 events or 10 s; at most 3 retries. Failure
disables or drops analytics without failing a request. The pinned library
holds two 1000-slot buffer sides (≤ 2000 resident events); a full write side
drops the new event and counts it. `capture` does not return admission, so
application wording says `submitted`, never `captured` or `delivered`.

Nine events reach production code: `ServerStarted`, `WorkerStarted`,
`StartupFailed`, `WorkspaceCreated`, `FleetCompleted`, `ApiError`,
`AuthLoginCompleted`, `AuthRejected`, `EntitlementRejected`. Two more —
`FleetTriggered` and `SignupBootstrapped` — are declared and mapped but emitted
by nothing: they appear in the telemetry enum, its property mapper and its own
tests, and in no call site. A declared event nobody captures is a dashboard
panel that stays empty for a reason no operator can see from the outside, so
either the capture lands or the variant goes. `FleetCompleted` fires only after
the fenced report claim returns `claimed=true`, so replays emit nothing. Its
`$insert_id` is the SHA-256 hex digest of `fleet_id || 0x00 || event_id`.
Runner-controlled `u64` properties saturate at `maxInt(i64)`. Scheduler
mechanics, raw logs, spans, heartbeats, renewals, and activity frames never
become PostHog events.

## The shared logging module

One logging discipline serves both binaries, in three parts:

- **The scoped emit API** — one call shape per record, so every site carries its
  scope and its event name rather than composing a line.
- **The log envelope** — enforces `ts_ms=`, `level=`, `scope=`; scrubs newlines.
- **The sink fan-out** — stderr **and** OTLP; 4 KiB buffer with
  `truncated=true` on overflow; exporter-internal scopes stay stderr-only so a
  failing exporter cannot enqueue its own warnings forever.

A call site that goes through the scoped API is conformant by construction. The
control plane's records leave through a stderr subscriber installed at boot
(`rustd/crates/agentsfleetd/src/logs.rs`, level from `AGENTSFLEET_LOG_LEVEL`);
the runner's go to the host supervisor. Field rules:
`docs/LOGGING_STANDARD.md`, committed in this repository.

## The export path — one endpoint, and the collector owns the fan-out

**Decision (M181_001).** The Rust daemon is a pure OTLP pusher to ONE configured
endpoint, addressed by the OpenTelemetry specification's own environment names.
It does not know which backend its signal reaches, and that is the point.

The Zig daemon posts direct to a vendor, gated on a `GRAFANA_OTLP_*` triple —
vendor identity spelled into the daemon's own configuration. That worked while
there was one backend, and it makes moving to a second one a daemon change:
new credentials, new configuration, a redeploy, and a window in which the old
and new paths are both half-configured. Fan-out to two backends at once is not
expressible at all without teaching the daemon about both.

Pointing the daemon at a collector moves that decision out of the binary.
Adding a backend, splitting one signal to two destinations, or moving a vendor
becomes collector configuration, applied without redeploying the thing that
serves requests. The daemon keeps one endpoint, one credential and one failure
mode no matter how many backends exist downstream, and an infrastructure change
stays separately attributable from a binary change — which is exactly the
property the cutover needs on swap day.

The cost is honest: a collector is one more thing to run, and a collector that
is down is a signal gap the daemon cannot route around. The bounded exporter
already answers for that — a full ring drops and counts rather than blocking a
request, which is the same behaviour it has when a vendor endpoint is
unreachable. The gap was never zero; the collector does not widen it.

**There is no pull endpoint, and there will not be one.** Nothing scrapes this
daemon: no `/metrics` route is served, and no scrape is configured in either
environment. A pull endpoint would be a second export path exporting the same
measurements by a different mechanism with a different failure mode, and
`playbooks/operations/cutover/probes.sh` asserts that the architecture documents and the
deployed configuration agree on its absence rather than leaving it to a reader.

The vendor-named knobs are accepted as ALIASES through cutover so a rollback to
the Zig binary keeps exporting, and they retire with that daemon. Where both a
standard name and a vendor alias are set, the standard name wins.

## The OTLP exporter substrate

One pipeline serves traces (`/v1/traces`), logs (`/v1/logs`), and metrics
(`/v1/metrics`): a lock-free MPSC ring, one shared endpoint configuration, a
persistent basic-auth client, and a supervised flush task. The flush task is
cancellable where it waits and creates no pool of its own.

- Emission is fire-and-forget: a full ring drops the entry, never blocks.
- Wake thresholds: 50 logs, 50 traces, 768 metrics (leaves 255 usable slots
  while the consumer wakes). Below threshold, entries batch until the 5 s max
  interval. Stop sets the event immediately.
- Collection removes entries before the POST, so outcomes are definite:
  non-success → `export_rejected`; timeout/transport → `export_uncertain`;
  `partialSuccess` → parse the rejected count as `partial_rejected` (the
  collector message is ignored, so backend text never enters logs); malformed
  partial body → whole batch `export_uncertain`. Each records one stderr-only
  warning.
- **No retry, deliberately.** OTLP JSON has no idempotency key; replaying
  delta metrics can double-count.

Decision records:

- [PR #549 — outbound bounding, before and after M139](https://claude.ai/code/artifact/de681e67-024d-4c08-bc04-4fa96aa58d48):
  one process-wide deadline scheduler (sorted tree, monotonic boot clock)
  replaced per-caller watchdog threads on raw file descriptors. Deadlines arm
  on a connection *generation*, so a recycled descriptor is provably a no-op;
  the owner shuts the socket down, and the blocked call returns a transport
  error. Postgres stays outside the scheduler on purpose: the pool's acquire
  and connect timeouts already bound it.
- [PR #553 — the OTLP unbounded in-flight export](https://claude.ai/code/artifact/aee9e003-6c91-40a1-9d7e-0feacdb1d810):
  a stalled Grafana endpoint can block the flush thread up to the OS TCP
  timeout, and shutdown's join waits it out. Deferred, not fixed, because
  `std.http.Client.fetch` is not cancel-safe (cancelling reintroduces the
  crash PR #553 removed) and the exporter boots before the scheduler exists.
  The scoped fix: shut the pinned socket down at the deadline via the
  scheduler, after reordering boot.

### Capacity and loss audit

Usable capacity: the ring keeps one slot empty. Rates are ceilings, not
benchmarks.

| Signal | Usable queue | Flush | Loss behavior |
|---|---:|---|---|
| logs | 2047 records; body truncated at 512 B | wake at 50 or 5 s; drain the cycle-start backlog in 50-record batches | ring drops export as `ring_full` |
| traces | 1023 spans; 12 attributes each | same as logs | same |
| OTLP metrics | 1023 samples; derived series ceiling (256 cost + runtime worst case) | wake at 768 or 5 s; coalesce label sets | overflow series export as `aggregate_cap` |
| PostHog | 1000/side, ≤ 2000 resident | 20 events or 10 s; 3 retries | full side drops the new event |

Scenario model: `R` runners, `B` billed debits/s, `C` accepted reports/s, `L`
runner log records/s, `D` control-plane records/s. Unknown rates stay
variables; the architecture bounds what the application owns.

| Signal | Scenario | Producer volume | Bound and outcome |
|---|---|---:|---|
| runner logs | any | `L` records/s, ≤ 4096 B each | local stderr only; repository network bytes are zero |
| control-plane logs | steady/burst | `D` records/s | 2047 slots absorb; overflow drops as `ring_full` |
| control-plane logs | backend outage | unchanged `D` | ring fills, later entries drop, product work continues |
| metrics | steady | idle heartbeats enqueue zero; each billed lease 1 sample, each report ≤ 5 | 4096 runner slots; 1023 sample slots |
| metrics | burst/outage | ≤ `B + 5C` samples/s | non-blocking admission; overflow drops and is counted |
| metrics | fleet growth | `R` liveness keys | 4096 exact slots at any `R`; counters overflow to `_other`, gauges drop |
| traces | steady/burst | matched requests + `C` settled runs/s | fixed 4+4+2 budget; 1023-slot ring |
| traces | fleet growth | heartbeat input grows with `R`; output does not | 10 generic spans/s process-wide |

Metric coalescing happens after ring admission, so it reduces wire series, not
enqueue pressure. The aggregator's series ceiling is derived in
the metric-family registry: the 256-series cost sub-budget plus the declared
runtime families' build-time worst case, so adding a family grows the ceiling
instead of evicting cost attribution.
`agentsfleet.telemetry.samples_dropped` covers ring and aggregation loss but
only arrives if a later export succeeds; the
`agentsfleet_otlp_entries_discarded_total{signal,reason}` counter and
`agentsfleet_otlp_queue_depth` gauge count local loss at the source. They ride
the same push, so a dead pipe is caught store-side by the
`metrics-exporter-dead` absence rule, never by the process reporting on
itself.

### Label registry — money stays in Postgres

Labels are bounded at the source and again by the 256-series flush ceiling:

| Label | Allowed values and ceiling | Overflow action |
|---|---|---|
| `runner_id` | at most 4096 exact values | counters merge into `_other`; gauges drop |
| runner `reason` and `outcome` | compile-time execution enums plus `unknown` for reason | closed sets; no dynamic overflow value |
| `gen_ai.token.type` | `input`, `output` | closed set; no overflow value |
| `agentsfleet.execution.posture` | `platform`, `self_managed` | closed set; no overflow value |
| `agentsfleet.billing.charge.type` | `receive`, `renewal`, `settle` | closed set; no overflow value |
| `gen_ai.provider.name` | exact OpenTelemetry well-known names only | unmapped provider omits the attribute and counts the omission |
| `gen_ai.request.model` | exact value, admitted while the derived series budget holds | overflow omits the attribute and counts the omission |
| `surface` (library) | `tenant_models`, `global_models`, `fleet_summary` | closed enum; no overflow value — a fourth surface is a code change, not a label |
| `stage` (library) | `next_upstream`, `auth_verify`, `pool_wait`, `authorize`, `sql`, `secret_project`, `map`, `serialize`, `cache_revision`, `cache_lookup` | closed enum; no overflow value |
| `outcome` (library) | `ok`, `invalid`, `unauthorized`, `forbidden`, `not_found`, `timeout`, `cancelled`, `dependency_error`, `internal_error` | closed enum; no overflow value |
| `cache` (library) | `hit`, `miss`, `bypass`, `stale`, `not_applicable` | closed enum; `not_applicable` is never counted — it means no cache decision was made |
| `pool_result` (library) | `acquired`, `timeout`, `cancelled`, `error` | closed enum; no overflow value |

**Workspace and tenant identity never reach a metric.** A per-process
distinct-value guard cannot cap series across replicas and restarts. Exact
per-workspace cost is a Postgres query against the ledger, which is exact
rather than bounded.

**Model attribution is derived, not guessed.** The semantic-convention layer
(`afd_observability`'s `semconv`) computes the admissible distinct
`(provider, model)` pairs from the fixed attribute sets
(postures × error-type slots × token types × charge classes) and the
aggregator passes its own ceiling in, so the two cannot disagree. A provider
with no well-known name, a pair past the budget, or a value past the payload
bound **omits the attribute and keeps the measurement**, and each omission
increments `agentsfleet_otel_attribute_omitted_total{attribute,reason}`.

All three OTLP signals share one resource (`service.name`,
`service.namespace=agentsfleet`, `service.version`, `service.instance.id` only
when a trusted id exists) and schema URL
`https://opentelemetry.io/schemas/1.43.0`. The pinned GenAI conventions commit
publishes no schema URL, so none is fabricated.

The cost families emit in the service layer, strictly after the money
transaction commits, so the exporter can never block or fail a debit. Their
kinds, units and bounds live in [`docs/metrics.census.tsv`](../metrics.census.tsv)
with every other family; what belongs HERE is why their names diverge from
the GenAI conventions, because each divergence was chosen, not accidental:

- `gen_ai.invoke_agent.duration` keeps the standard name because runner wall
  time bounds exactly one invocation, so the standard name is truthful.
- `agentsfleet.invoke_agent.token.usage` does not use the GenAI client-call
  name: usage is cumulative per invocation, so that name would be false.
- `agentsfleet.invoke_agent.cache_read.token.usage` is a non-additive subset
  of input tokens — never a third total.
- `agentsfleet.billing.credit.consumed` counts nanocredits by charge class;
  nanocredits are money, not time.
- `agentsfleet.telemetry.samples_dropped` is exporter self-observability.

Every committed debit emits once; uncommitted, stale-fenced, or replayed
operations emit nothing. Flush coalesces the evented cost families into one
**delta** dataPoint per (metric, labelset), converted to cumulative
downstream; the runtime snapshot counters are natively cumulative and need no
conversion (the metric-family registry documents the temporality split). With
the pull endpoint retired, this push is the one metrics egress — when the pipe
itself dies, the store-side `metrics-exporter-dead` absence rule is the
watchdog.
