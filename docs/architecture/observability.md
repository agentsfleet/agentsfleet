# Observability — `agentsfleetd` is the plane; runners get a collector of their own

> Two decisions drive this file. **`agentsfleetd` owns backend-bound
> telemetry**: product analytics, cost, and every fact a runner reports over
> `/v1/runners`; its collectors, `otelcol-{dev,prod}`, serve the daemon only.

> **The dev runner has its own OpenTelemetry collector on the bare-metal host**
> (installed 2026-11-04). It exports runner spans and metric families over
> OpenTelemetry Protocol (OTLP). Logs remain on the host; forwarding requires a separate allowlist.

> The collector holds the backend key;
> the runner holds no analytics or observability credential.

Siblings: [`runner_fleet.md`](./runner_fleet.md) (plane structure),
[`data_flow.md`](./data_flow.md) (an event traced through the runtime). This
file answers: when something happens, where does the signal go, and who owns it.

## Facts

Every row is extracted from the sections below; the owner column names the section that carries the full story.

| Invariant | Value | Mechanism | Owner section |
|---|---|---|---|
| Signal paths | 3 | OTLP push through the daemon's collector · <img src="https://cdn.simpleicons.org/posthog" width="14" alt="" /> PostHog · 🐘 Postgres (money) | §The three signal paths |
| Metric namespace | `agentsfleet_` runtime families; dotted semantic-convention cost families | `docs/metrics.census.tsv` declares every exported name; `test_metric_family_registry_parity` grades the registry against it both ways | §The three signal paths |
| Runner telemetry | spans and families declared in `docs/metrics.runner.census.tsv`; dev collector installed and first real lease export verified Nov 04, 2026 | traces and metrics through the dedicated host collector; logs stay local; never through `otelcol-{env}` | §`agentsfleet-runner` — a collector of its own |
| Library read series | 124 total, the sum of the census ceilings | closed enums; a member that outgrows its family's ceiling fails `every_declared_ceiling_admits_its_label_product` | §Library read stages are metrics, not spans |
| Trace budget | daemon: none yet, one span per matched request · runner: 256 per lease, 128 per second | the daemon has no sampler, so it sheds nothing and counts nothing | §Traces |
| OTLP loss | drop, never block; no exporter retry | bounded queues; later cumulative metric collections can recover totals while the source process survives | §"The Rust daemon's export path, as built" |
| PostHog events | 11 captured, 2 declared-uncaptured | `FleetCompleted` fires only on a report's first settle; `$insert_id` = SHA-256 of `fleet_id \|\| 0x00 \|\| event_id` | §PostHog is product analytics |
| Per-runner label ceiling | 4096 exact `runner_id` slots | counters overflow to `_other`, gauges drop | §Label registry |
| Tenant identity on metrics | never | exact per-workspace cost is a Postgres ledger query, which is exact rather than bounded | §Label registry |
| Log sink | stderr always; OTLP when configured | the log exporter counts its failures without warning, so a failing exporter cannot feed itself | §The shared logging module |
| Performance gating | nothing gates on a percentile | the exported series are the evidence; a threshold that cannot fail reports success forever | §Library read stages are metrics, not spans |
| The M61 naming trap | the live OpenTelemetry (OTel) export survived `OTEL_EXPORT_REMOVAL` | check the OTLP exporters + the `OTEL_EXPORTER_OTLP_*` knobs, never the milestone name | §The M61 naming trap |
| Production wiring truth | per-surface state, each row with its code evidence | re-read the evidence column rather than trusting the row | §Signal routing |

## The three signal paths

The control plane's telemetry surfaces live in `afd_observability`.

| Path | What | Consumer |
|---|---|---|
| OTLP (push) | logs → Loki, traces → Tempo, metrics (runtime + cost families) → Mimir, **through the collector** (`deploy/fly/otelcol-{dev,prod}`), which holds the vendor credentials and owns the fan-out. Gated on an OTLP endpoint: the deploy workflows set `OTEL_EXPORTER_OTLP_ENDPOINT` to the collector, not the vendor, and `OTEL_EXPORTER_OTLP_HEADERS` to the collector's ingest credential. The daemon's **only** metrics egress: there is no pull endpoint. See §The export path | <img src="https://cdn.simpleicons.org/grafana" width="14" alt="" /> Grafana Cloud, operator dashboards |
| <img src="https://cdn.simpleicons.org/posthog" width="14" alt="" /> PostHog | nullable client, product events only | product analytics |
| Postgres | per-run execution telemetry + billing counters in `afd_billing` | the money system of record |

**One process, one registry.** Every runtime family carries the `agentsfleet_`
prefix; the evented cost families use dotted OpenTelemetry
semantic-convention names (each divergence is explained in §"Label registry — money stays in Postgres";
`docs/metrics.census.tsv` lists every family).
The registry is built from that file, and `test_metric_family_registry_parity`
grades the two against each other both ways. The prefix rule covers exported
metric families only; it does not rename `fleet_id`, log event names,
`EventKind` tags, or the Dragonfly consumer group.

**One registry row is the whole family.** A census row declares the wire
name, kind, number type, unit, temporality, label keys, histogram bounds and
series ceiling once. Producer code names the family through a `Declared<Kind>`
constant in `rustd/crates/afd_observability/src/metrics/declared/*.rs` and
claims an OpenTelemetry instrument from `Instruments`
(`metrics/instrument/mod.rs`). A claim whose kind or number type disagrees with the
census is refused at boot, and every claim is recorded, so
`Instruments::unclaimed` names any family nothing asked for. Label values come
from closed enums in `metrics/label/*.rs`. Two labels are open: `runner_id`,
bounded by the runner slot table, and the cost families' `gen_ai.request.model`,
bounded by the model catalogue (§"Label registry — money stays in Postgres").
A gauge's callback only loads a value from `metrics/observed/mod.rs` and never
touches Dragonfly or a lock, because the callback runs under the OpenTelemetry
software development kit (SDK)'s pipeline lock and a stall there silences every family at once. Adding a
family is one census row, one `Declared` constant and one producer call.

Runtime deployment carries no dashboard files. Grafana dashboard and alert
definitions live under
`playbooks/operations/observability/assets/`, where the
operator playbook checks, applies, and verifies them against source-owned
metrics.

The operator playbook stores separate Prometheus and Loki datasource
identifiers so its probes address the intended sources exactly. The incident
responder keeps the existing workspace Grafana credential shape (`host` and
`token`): it discovers the single Loki datasource through Grafana, then uses
that returned identifier for read-only log evidence.

### The M61 naming trap

The milestone named `OTEL_EXPORT_REMOVAL` did **not** remove the live OTel
export. It deleted a dead trio (`otel_export`/`otel_histogram`/`otel_json`) and
kept `otel_logs` and `otel_traces` wired. Before touching anything OTel-shaped,
check the OTLP exporters and the `OTEL_EXPORTER_OTLP_*` knobs, not the
milestone name.

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

## Service Level Objectives

The [DT read component](durable_tasks.md#status-and-ownership) uses the existing task operation counter and duration measurements.
Structured failures distinguish the whole-read deadline from server statement cancellation using bounded reason codes.
Task and lease identifiers belong in traces and structured events; prompts, criteria and private evidence never become metric labels.

Three candidate objectives have metric producers. Their targets remain unproven;
the table records the available measurements. Backlog age is a diagnostic until
a pickup objective is measured.

| Indicator | Good events over valid events | Target | Where the target came from |
|---|---|---|---|
| Admission availability | `agentsfleet_admissions_total{outcome=~"accepted\|replayed"}` over the same family excluding `over_budget` | 99% | unproven. Historical development measurement on 2026-09-17: 13 `appended`, 1 `replayed`, no refusals or deferrals. The current ledger producer labels committed work `accepted`. |
| Runner success | `agentsfleet_runner_executions_total{outcome="processed"}` over the same family excluding `fault="workload"` | 95% | unproven. Measured the same day: 7 `processed`, 3 `fleet_error`, the failures split 2 `startup_posture` and 1 `runner_crash` — both platform classes, so the blame split leaves the measured 70% unchanged. |
| Admitted fleets that started | `agentsfleet_fleet_runs_started_total{kind="fresh"}` over `agentsfleet_admissions_total{outcome="accepted"}` | 99% | unproven. Measure both families from a ledger-queue daemon before assigning this target. |

`agentsfleet_admission_backlog_oldest_age_seconds` reports the oldest unsettled
admission. Parked work remains unsettled, so backlog age includes time waiting
for an answer. The ledger has no replay interval that defines a pickup deadline.

`over_budget` is excluded from valid events on purpose. It is the deployment
refusing work because a budget is spent, which is the system doing what its
budgets say rather than failing to do it. Counting a deliberate refusal as an
availability miss would make the indicator worse the better the budgets work.

### Why the runner objective excludes workload faults

An error budget is a promise about what this control plane controls. `FailureClass` carries eleven
members and only five of them are ours: `startup_posture`, `runner_crash`, `transport_loss`,
`lease_expired` and `renewal_terminate`. The other six — `policy_deny`, `landlock_deny`, `oom_kill`,
`resource_kill`, `budget_breach` and `timeout_kill` — are the workload failing at something the
platform delivered correctly.

The daemon classifies them at the point of record: `fault_of` in
`afd_observability/src/producers/fleet/runner/mod.rs` maps the class to a two-member `Fault` set, and the
executions counter carries it as `fault` on the failing half only. A success has no `fault` label at
all, which is what makes `{fault!="workload"}` read as "everything the platform was accountable for"
— an absent label satisfies a not-equal matcher.

Workload faults leave the denominator rather than counting as successes. Counting them good would let
a noisy tenant dilute the ratio and hide real platform failures behind volume; counting them bad would
let one tenant's broken code spend everybody's budget. Neither is a measurement, so they are not
measured. An unclassified failure (`None`) is charged to the platform: a cause nobody could name is
not evidence against the tenant.

`fault_of` is an exhaustive match with no default arm, so a twelfth `FailureClass` fails the build
until somebody decides which side it belongs on, and
`every_failure_class_is_assigned_the_side_we_decided` records the decision where a reviewer can argue
with it one line at a time.

The all-cause rate stays on the dashboard beside the objective as a plain reading. "Did my run work"
is a fair question with a real answer; it just is not a promise.


A `replayed` admission counts as good for the same reason: the first
admission's identifier stands, so the producer got the answer it asked for.

`reclaimed` lease grants are excluded from the fleet-start numerator on the
mirror of that logic. A reclaim restarts work that was admitted once and
already counted once, so counting the restart would let a flapping runner push
the ratio above 1 — an availability figure that improves as the fleet gets less
stable is not measuring availability.

### Error budget burn

Burn rate is the error ratio divided by the error budget, so a burn of 1 spends
the budget exactly over the window and a burn of 14.4 spends 2% of a 30-day
budget in an hour. The dashboard draws two windows per severity, the long one
making the budget claim and the short one resetting it, so a burn that has
already stopped does not keep paging. Page at 14.4 over 1h and 5m; open a
ticket at 6 over 6h and 30m.

None of it is enabled. The windows are drawn and the expressions are reviewed,
and every burn panel carries an unproven marker until the deployment has served
enough events to set a target against. Ten runner executions produce a burn
rate; they do not produce an objective.

### What has no indicator, and what would have to exist

Every family the census declares has a producer
(`every_census_family_has_a_producer`); a mechanism this daemon does not run
has no census row. What follows is what no family measures yet.

**End-to-end fleet availability now has both halves.** It was the headline gap
here until M197_002 shipped `agentsfleet_fleet_runs_started_total{kind}`,
incremented at the one lease grant point.

`agentsfleet_admissions_total{outcome="accepted"}` is the ledger queue's
denominator. The stream queue used `outcome="appended"`.

The trigger counter formerly named here as the missing denominator was retired.
It had been declared for years without any caller incrementing it.

**API latency has no family at all.** The census declares seven histograms and
none of them measures an HTTP request. Requests per second, the 500 rate and a
latency percentile are all unanswerable from this export.

Each of those is a new family with a new census row, which makes it a milestone
rather than a dashboard edit.

### The two environments share one tenant

Development and production resolve to the same Grafana stack, the same
namespace, the same Prometheus datasource and the same ingest credential — the
vault items differ in name only. No series carries a `deployment.environment`
attribute either: `rustd/crates/afd_otlp/src/resource.rs` builds the resource
both binaries send from service name, namespace, version and an optional
instance identifier, and stops there.

Today every series in the store is development, because the production Fly
application runs no machines. That makes the development dashboard correct by
accident rather than by construction, and it stops being correct on the day
production first deploys: every panel would sum both environments with no query
able to separate them. Emitting `deployment.environment` is the blocker on that
deploy, and it is daemon and deployment work rather than a dashboard change.

### Nothing watches the infrastructure

The store holds 37 metric names: the 36 runtime families and `target_info`.
There are no collector, host, container or datastore families, so the
dashboards answer questions about the daemon's own work and nothing else.

The collector does not report on itself. `deploy/fly/otelcol-{dev,prod}/config.yml`
configures `service.telemetry` for logs only, so `otelcol_exporter_send_failed_*`
and the queue-size families are never read. The consequence is written into
that file already: the `sending_queue` is in-memory, an outage overlapping a
restart loses what was queued, and the daemon counts an export successful once
the collector returns 2xx, which means enqueued rather than accepted. The
daemon's `agentsfleet_otlp_entries_discarded_total` counts loss before the
collector and cannot see loss inside it.

There is also no `hostmetrics` receiver, so `agentsfleetd`, `otelcol` and
Dragonfly report no processor, memory, disk or network usage — the single
resource reading anywhere in the system is
`agentsfleet_process_resident_memory_bytes`, which the daemon takes from its own
`/proc/self/statm`.

Closing that is collector configuration rather than a census change, which
makes it much smaller than the three missing families above.

## `agentsfleet-runner` — a collector of its own

**Built: the runner exports spans and metric families when configured.** The dedicated dev collector was installed on Nov 04, 2026; production installation is not claimed. `afr_telemetry` uses the shared `afd_otlp` transport when `OTEL_EXPORTER_OTLP_ENDPOINT` names a collector.

Runner logs go to stderr; heartbeat, renewal and reports carry liveness and results over `/v1/runners`. The runner sends nothing to PostHog. The daemon owns reported runner state and fleet liveness.

Fleet-runtime leases run inside `runner.lease` spans. Private authoring takes a separate path without that span (`afr_supervisor/src/worker_pool/mod.rs`).

A followed tool GET emits `http_redirect_started` and exactly one `http_redirect_completed` or `http_redirect_failed` in the parent tool-call context (`afr_egress/src/redirect/mod.rs`). The terminal record carries hop count, upstream status, whether headers were stripped, a fixed safe reason and elapsed milliseconds.

Cancellation and the aggregate deadline end with failure. No destination path, query, signature or response body enters these records; existing `tool_refused` records remain in place.

Completed redirects log at info; failed redirects log at warn with a registered error code. A GET that exhausts its aggregate deadline also emits `http_read_deadline_exceeded` at warn, including when it expires before observing a redirect (`afr_tools/src/egress/mod.rs`). That record carries only the lease, tool, fixed reason and registered error code.

**What the runner reads.** The endpoint, protocol and timeout knobs the daemon
reads, and no header knob: `OTEL_EXPORTER_OTLP_HEADERS`, any signal's own
header knob (`OTEL_EXPORTER_OTLP_{TRACES,METRICS,LOGS}_HEADERS`, which the
exporter reads from the environment itself and prefers), or a user in the
endpoint (`http://user:secret@…`) stops `run` naming the knob. A header is
how an OTLP exporter carries a credential, and the runner holds none; the
credential belongs to the runner collector on the host, which a lease cannot
read. Only `afr_telemetry` may depend on the transport in the runner's build,
so no other runner crate can build a pipeline that carries one. A compression knob stops
`run` too, naming it: this build compresses nothing, and the exporter's own
refusal would name no knob. The endpoint must be an `http` or `https` URL with
a host and no query. `sandbox` and `probe` read no telemetry knob, so
hardening still sees a single thread. `run` logs `telemetry_export_started` with the knob's name and
the protocol, or `telemetry_export_disabled` once, and never the endpoint.

**What it exports.**

- **Spans:** `runner.lease`, `invoke_agent`, `chat` and `execute_tool`, and
  nothing else. The layer admits only spans under the `agentsfleet-runner`
  target, so no library's span leaves the host and no log record rides a span
  as an event. Each lease is its own root trace carrying
  `agentsfleet.lease.id` and `agentsfleet.event.id`. The tool span names its
  tool by the published catalog, `_other` for anything else, so a name the
  model made up never leaves the host.
- **A fixed span budget** (`rustd/crates/afr_telemetry/src/budget/mod.rs`): at
  most `MAX_LEASE_SPANS` (256) spans per lease, its root included, and
  `RUNNER_SPANS_PER_SECOND` (128) per monotonic second across the runner. A
  lease's root is never refused: it is charged to its second while the second
  has room, and kept uncharged in a second already spent, so a second's
  exports pass 128 only by the roots of leases started after it was spent. A span started under a shed span is shed too, so no
  exported span points at a parent the collector never received. The budget is
  a fixed table of lease slots claimed by compare-and-swap and one packed word
  for the second, so admitting a span never waits on another worker. A shed
  span counts in `agentsfleet_runner_spans_suppressed_total`. The batch
  processor's queue (2048 spans) and send interval (five seconds) are pinned in
  `afd_otlp` rather than read from `OTEL_BSP_*`, so 128 a second fills at most
  640 of the queue against a collector that keeps up. A collector slower than
  that fills the queue, and the SDK drops past it without counting; the failed
  exports themselves are counted (below).
- **Eight metric families of its own**, declared in
  `docs/metrics.runner.census.tsv` and graded both ways against their
  producers. Provider metrics cover turn duration and retries, by provider and outcome or reason.
  Sandbox metrics cover start duration, processes killed for memory, and sandboxes held between a fleet's leases.

  Other families cover failed memory pushes, tool-call duration by tool and outcome, and dropped live-tail frames.
  Frames drop through backpressure, a failed post, or abandonment when a lease stops waiting for a slow daemon.

  A provider label uses the shared, bounded public naming registry: compiled
  model-catalogue providers, SDK providers and OpenTelemetry names, with aliases
  deduplicated. Other configured names use `_other`
  (`afd_observability/src/semconv/provider.rs`, `afr_telemetry/src/labels/mod.rs`).
  These labels describe telemetry only; saved model settings choose the route.

  The `invoke_agent` span uses the same canonical `gen_ai.provider.name`. A tool
  label is a published catalog name or `_other`; every outcome and reason is a
  closed enum.
- **Its own losses:** spans suppressed by the budget, and
  `agentsfleet_runner_otlp_entries_discarded_total{signal,reason}` for what an
  export lost before the collector took it — the daemon's loss family, under
  the runner's name. The shared counting wrappers route a process's losses
  there when no daemon producer set is installed, so a down or misaddressed
  runner collector shows as a series and not only as a stderr line.
- **On the way out:** `run` shuts the pipelines down, delivering what they
  hold, and waits no longer than one export's timeout.
- **Never:** logs (stderr, read by the collector from the host's log store),
  prompts, replies, tool output, credentials, and no tenant, fleet, lease or
  event identifier on a metric.

**Decided 2026-10-04; traces and metrics installed in dev 2026-11-04.**

Indy: "we will run an otel collector and collect the observability log, trace, metrics".
He added: "we will have a new collector for the runner later on in the baremetal host".
The current collectors "are for the agentsfleetd daemon only".

- A new OpenTelemetry collector on the bare-metal host serves runners. It is
  separate from `otelcol-{dev,prod}` (§The export path), which stay the
  daemon's alone, and it exports to the backends itself. The dev unit and
  installer are in `deploy/baremetal/otelcol-runner.*` and `install-collector.sh`;
  Indy chose the existing `ZMB_CD_DEV/grafana-dev` key for that installation.
- The runner keeps logging to stderr alone. Journald forwarding is deferred
  until its separate allowlist is implemented; the installed collector has
  traces and metrics pipelines only.
- The runner's families carry only what no verb does. Every fact that rides a
  verb stays `agentsfleetd`'s.

Runners stay cattle (`runner_fleet.md`). A runner knows one collector endpoint
and no backend, so moving a vendor stays collector configuration.

## Signal routing

```text
BARE-METAL HOST (dev collector installed; log forwarding remains deferred)
  agentsfleet-runner
    ├─ structured stderr ──► journald ─────────► runner collector ──► backends
    ├─ spans + runner families ── OTLP ──────────►┘  (its own; never otelcol-{env})
    └─ lease / heartbeat / renew / activity / report ──► agentsfleetd

agentsfleetd (built)
  ├─ runtime metric families ─┐
  ├─ selected run span ───────┼─ bounded OTLP exporters ──► otelcol-{env} ──► Loki · Tempo · Mimir
  ├─ structured logs ─────────┘  (and stderr)              (daemon only; holds the vendor credential)
  └─ selected PostHog event ──► PostHog
```

**Why raw runner logs bypass `agentsfleetd`.** A log line is an unbounded byte
stream; heartbeat and report are bounded semantic messages. Routing logs
through the control plane would tie request and database capacity to log
volume, and would make the control plane the failure point for the diagnostics
you need when that plane is unhealthy.

**Runner collector rules (fail-closed on privacy).** The runner collector
forwards only single-line logfmt records after an allowlist: `ts_ms`, `level`,
`scope`, `event`, registered `error_code`, reviewed bounded metadata. It drops
prompts, response bodies, tokens, credentials, environment values, arbitrary
`msg` text, and anything it cannot parse. Sampling is level- or rate-based
after redaction; it never reads payload content or tenant identity. None is
deployed yet (`deploy/baremetal/` carries only `agentsfleet-runner.service`),
so a runner host's telemetry network rate is zero bytes per second today. It
ships with numeric memory, disk, retention, rate, sampling and retry limits and
the allowlist proof.

| Signal | Producer / owner | Path | Bound and loss |
|---|---|---|---|
| runner logs | runner logfmt on stderr; host owns retention | today none off the host; decided: host log store → runner collector → backends | host policy caps disk; the allowlist above; loss never blocks a run |
| runner semantic metrics | `agentsfleetd`, from accepted fleet verbs | OTLP push (streamed per-runner families) | 4096 runner slots; overflow → `_other` |
| runner own metrics | the runner (`afr_telemetry`), for facts no verb carries | OTLP → runner collector → backends | closed label sets in `docs/metrics.runner.census.tsv`; no tenant, fleet, lease or event identifier |
| runner host metrics | node exporter, if operators want it | direct to metrics backend | outside the runner API |
| runner traces | the runner's four span kinds | OTLP → runner collector → backends | 256 spans per lease, root included, and 128 per second, roots never refused, the rest counted; joins `fleet.delivery` by the `event_id` attribute, and the lease identifier tells a redelivered event's runs apart |
| control-plane logs | structured logger | stderr + OTLP through `otelcol-{env}` to Loki | the SDK batch log processor's queue (2048 records by default); a full queue drops, enqueue never blocks |
| control-plane metrics | runtime + cost families | one OTLP push; no pull endpoint | fixed labels or explicit caps |
| control-plane traces | HTTP ingress + settled delivery | OTLP to Tempo | no route policy yet: one span per matched request; the 2048-span batch queue drops past its cap without counting |
| product analytics | PostHog client | batched capture | selected business events only |

### Production wiring truth

| Surface | State | Evidence |
|---|---|---|
| structured stderr | installed, called | `rustd/crates/agentsfleetd/src/logs.rs` installs the subscriber on stderr at boot; `AGENTSFLEET_LOG_LEVEL` sets the level |
| server spans | emitted | `rustd/crates/afd_api/src/router/trace.rs` opens one `http.server.request` span per matched request, carrying the route template, the method and the status — never the raw path |
| OTLP logs | installed, called when configured | `rustd/crates/afd_otlp/src/pipelines/providers.rs` builds the log exporter `agentsfleetd`'s `telemetry/mod.rs` asks for; `logs.rs` bridges every `tracing` event the stderr layer already sees into it through the reload slot, so the two streams are one emit read twice |
| OTLP traces | installed, called when configured | the same module builds the span exporter inside `afd_observability`'s counting wrapper; `serve/exporting.rs` spawns `otlp_export`, which samples the resident set and runs the shutdown flush |
| OTLP run metrics | installed, called when configured | every census family is claimed from the registry at boot and produced at the call site that owns its mechanism (`rustd/crates/afd_observability/src/producers/`); a family with no producer fails `every_census_family_has_a_producer` |
| PostHog events | installed, called when configured | `rustd/crates/afd_observability/src/product/mod.rs`; boot opens the client, the supervised `analytics_flush` task drains it before exit |
| runner export | installed, called when configured | `run` in `rustd/crates/agentsfleet_runner/src/main.rs` builds it from `afr_telemetry` before boot, and fleet-runtime leases run inside a `runner.lease` span; authoring currently has no equivalent lease span |
| runner collector | installed in dev; real lease spans and provider histogram verified in Grafana Nov 04, 2026 | `deploy/baremetal/otelcol-runner.{yml,nft,service}` and `install-collector.sh`; localhost receiver behind root-only admission; traces and metrics only; actual-sandbox probe pending |

## Metrics stay semantic

`agentsfleetd` updates fixed in-memory state after it accepts heartbeat, lease,
and report verbs. One terminal report records five cost samples: one duration,
an input and an output token count, one cache-read token count, and one
settle-class credit amount (`rustd/crates/afd_api_runner/src/handler/runner/report.rs`,
`meter`).

Do not turn scheduler arms, activity frames, log lines, lease or event
identifiers, model text, error text, or raw runner identifiers into metric
labels. A scheduler metric is justified only as a fixed aggregate (queue depth,
fired total, stale-target total). The runner's own series carry only the
facts no verb carries (§"`agentsfleet-runner` — a collector of its own"), with
closed label sets, in `docs/metrics.runner.census.tsv` beside
`docs/metrics.census.tsv` rather than in it.

## Traces

`agentsfleetd` emits one `http.server.request` span per matched request
(`rustd/crates/afd_api/src/router/trace.rs`), plus one `fleet.delivery` span
after an accepted terminal report. It does not read the World Wide Web
Consortium (W3C) `traceparent` header yet: nothing under `rustd/crates` parses
it, so every request span is a new root and the header never rejects a request.
The Rust runner exports its spans to the runner collector
under a fixed per-lease and per-second budget (§"`agentsfleet-runner` — a
collector of its own"), and its verbs carry no trace field. Each lease is its
own root trace carrying `agentsfleet.lease.id` and `agentsfleet.event.id`, so
it joins `fleet.delivery` by attribute, and no W3C context crosses the runner
protocol.

**`fleet.delivery` is the chat round trip.** It starts at the event's creation and ends when `event_complete` publishes, both on this daemon's clock. The start is the lease row's `event_created_at`; the end is the publish instant `Plane::announce` reads.

Beside its identity the span carries one attribute per segment. `agentsfleet.delivery.queue_wait_ms` (event creation to lease issue) and `complete_ms` (report receipt to publish) are measured here. `memory_ms`, `model_ms`, `tools_ms`, `sandbox_wait_ms` and `settle_ms` come from the runner's `telemetry.segments`.

Every runner-reported figure is capped at `MAX_RUN`. For a reclaimed event, `queue_wait_ms` and `event_to_complete` include the earlier attempt, the round trip its owner waited. `runner_lease_issued`'s `queue_wait_ms` counts only fresh entries.

The segments are not a partition: the sandbox wait runs beside the model's time. A start the span cannot place falls back to the end less the reported `wall_ms`. Per-fleet p95 and p99 come from Tempo on `gen_ai.agent.id` (`playbooks/operations/observability/002_round_trip_percentiles.md`).

Fleet-wide, `agentsfleet_fleet_delivery_stage_seconds` carries `event_to_complete`, `lease_to_report` and `report_to_complete`. They sit beside its first-chunk stages and are recorded at the publish.

**`fleet.delivery` is a custom control-plane span.** It has its own root
(`rustd/crates/afd_observability/src/delivery/mod.rs`). `DELIVERY_SPAN_KEYS` in
`rustd/crates/afd_observability/src/semconv/mod.rs` declares its attributes.

The
Generative Artificial Intelligence (GenAI) keys appear where the source fact
matches: `gen_ai.operation.name=invoke_agent`, `gen_ai.agent.id` (the fleet),
`gen_ai.provider.name`, `gen_ai.request.model`, `gen_ai.usage.input_tokens`
and `gen_ai.usage.output_tokens`. Product-namespaced keys carry the rest:
`agentsfleet.execution.posture` and the correlation identifiers
`agentsfleet.event.id`, `agentsfleet.workspace.id` and
`agentsfleet.tenant.id`.

Public providers without a well-known spelling keep their own canonical
name, as the pinned OpenTelemetry reference permits. A private identifier
outside the compiled registry omits its key and counts the omission.

Correlation identity is allowed on a span because each span is a bounded per-event record.
A metric label creates a series that outlives the process, so metrics exclude correlation identity.
Prompt and response content
never becomes a span attribute.

**Route policy.** None yet. The daemon traces every matched request: it has no
head sampler and no per-class span budget, so nothing is suppressed and no
family counts suppression. The only bound is the shared batch queue, `SPAN_QUEUE` (2048 spans) in
`rustd/crates/afd_otlp/src/pipelines/mod.rs`, which drops past its cap without
counting.

Why it matters: an idle runner sends one heartbeat every 10 s
(`HEARTBEAT_INTERVAL` in `rustd/crates/afd_core/src/timing.rs`), and each
one is a matched request and therefore a span. Span volume grows with runner
count until a route policy fills the declared `reason` set (`noisy_route`,
`runner_rejection_budget`, `server_error_budget`, `sampled_success_budget`,
`sample_miss`).

### The chat path's signals

What each step a chat crosses emits, on `fde585971` plus M238. A row's gap is a signal that does not exist yet, not one that is broken.

| Step | Logs | Metrics | Trace |
|---|---|---|---|
| Steer admitted (`afd_api_tenant` steer handler, `afd_admission`) | admission refusals | `agentsfleet_admissions_total{outcome}` | request span only |
| Doorbell ring (`afd_dragonfly/src/doorbell`) | `admission_wake_ring_failed` | `agentsfleet_fleet_ready_wake_failures_total{kind}` | none |
| Held heartbeat (`afd_api_runner` heartbeat, `wake.rs`) | `runner_wake_refused` | `agentsfleet_runner_wake_answers_total{reason}` | request span only |
| Lease issued (`afd_fleet/src/lease/deliver`) | `runner_lease_issued` with `queue_wait_ms`; `lease_window_read_failed` | `agentsfleet_lease_polls_total` and candidates per poll; `delivery_stage{event_to_lease}` | request span only |
| Run (runner `lease_loop`) | `lease_completed` / `lease_failed` with every segment | runner families, exported only where the runner collector is configured | `runner.lease`, same condition |
| Report settled (`afd_fleet/src/lease/report`) | `report_settled`, `completion_published`, `memory_capture_failed` | `delivery_stage{lease_to_report, report_to_complete, event_to_complete}` | `fleet.delivery` with segment attributes |
| First chunk (`afd_fleet/src/lease/activity`) | none | `delivery_stage{lease_to_first_chunk, event_to_first_chunk, zombie_to_first_chunk}` | none |

Gaps left by M238, each named in M233_001:

- The steer and lease handlers carry no stage attributes on their request spans (§1).
- The runner's own families and spans reach no backend on dev until its collector is built (§2).
- The edge reports nothing of its own (§3).

## Library read stages are metrics, not spans

The authenticated library reads (tenant model registry, global catalogue,
fleet gallery, a workspace's onboarded entries) record stage timing as fixed-cardinality families in
the library stage registry. A six-stage read emitting spans would spend most of a
second's span admission on its own timing and evict the server-error spans the
budget protects. Stage timing is high-frequency, closed-label data; that is
what a metric is for. The trace half is one `http.server.request` span per
read. The browser client sends a fresh `traceparent` root per request
(`ui/packages/app/lib/api/client.ts`); the daemon does not read it yet (§Traces).

Label members live in the label registry below
(`rustd/crates/afd_observability/src/metrics/label/library.rs`). Each family's
series ceiling is its census `fixed:<n>` value, 124 in total, and
`every_declared_ceiling_admits_its_label_product` fails when a new enum member
makes a family's label product outgrow its ceiling.

| Family | Labels | Series |
|---|---|---|
| `agentsfleet_library_stage_duration_seconds_total` | `surface`,`stage` | 40 |
| `agentsfleet_library_stage_observations_total` | `surface`,`stage` | 40 |
| `agentsfleet_library_read_outcome_total` | `surface`,`outcome` | 36 |
| `agentsfleet_library_payload_bytes_total` | `surface` | 4 |
| `agentsfleet_library_results_total` | `surface` | 4 |

Design points, each load-bearing:

- One read touches four dimensions (surface, stage, outcome, pool result); no
  family carries all four. The cross-product is 1440 series, nearly all
  permanently zero.
- The pool family carries no `surface` label: a starving pool is
  process-wide. No library family carries tenant or request identity.
- Duration and observations are two counters, not a summary.
  `rate(duration)/rate(observations)` is the mean cost of a *span*, and `sql`
  fires twice per registry read (both sides of `secret_project`). Dividing by
  requests would halve its apparent cost.
- `secret_project` survives the read-path decryption removal with a narrowed
  meaning: it times the batch presence query and projection, and decrypts
  nothing. A regression that reintroduces per-row decryption shows up as a
  stage that suddenly costs more, not one that silently reappears.
- One outcome per request on every exit path. Each read handler calls
  `library::read_finished` once, and `library_outcome` in
  `rustd/crates/afd_http/src/handler/mod.rs` maps an unknown refusal status to
  `internal_error`, so an unclassified path surfaces as something to
  investigate, never as `ok`.

**The exported series are the evidence, and nothing gates on a percentile.** A latency
threshold in a universal check fails on a noisy runner, gets widened until it
cannot fail, then reports success forever. Percentile comparison needs a
provisioned environment with pinned pool size, warm state, and concurrency.
That capture harness is deferred to its own spec. An earlier draft shipped a
report validator and a `capture-library-performance` target; the target could
not capture, so both were removed rather than left looking like a capability.

## PostHog is product analytics, not operations telemetry

The client is optional: with no project key, `Analytics::silent` reports
nothing (`rustd/crates/afd_observability/src/product/mod.rs`). With one, the
pinned `posthog-rs` client runs on its defaults: a flush every 100 events or
5 s, and a 10 000-event queue that drops new events with one warning when
full. Failure drops analytics without failing a request. `capture` returns once
the event is queued, never on delivery, so application wording says
`submitted`, never `captured` or `delivered`.

Eleven events reach production code: `ServerStarted`, `WorkerStarted`,
`StartupFailed`, `WorkspaceCreated`, `FleetCompleted`, `ApiError`,
`AuthLoginCompleted`, `AuthRejected`, `EntitlementRejected`, `InviteEmail`,
reported under one of three names by its outcome, and `Authoring`, reported
under one of five funnel names by its step
([product_analytics.md](./product_analytics.md) §"Private chat to fleet funnel"). Two more —
`FleetTriggered` and `SignupBootstrapped` — are declared and mapped but emitted
by nothing: they appear in the telemetry enum, its property mapper and its own
tests, and in no call site. A declared event nobody captures is a dashboard
panel that stays empty for a reason no operator can see from the outside, so
either the capture lands or the variant goes. `FleetCompleted` fires only when
a report settles for the first time; a repeated report returns early, so
replays emit nothing (`rustd/crates/afd_api_runner/src/handler/runner/report.rs`).
Its `$insert_id` is the SHA-256 hex digest of `fleet_id || 0x00 || event_id`.
Scheduler mechanics, raw logs, spans, heartbeats, renewals, and activity frames never
become PostHog events.

## The shared logging module

One logging discipline serves both binaries, in three parts:

- **The emit** — a `tracing` macro with a byte-stable `event` field and named
  fields, never values folded into message text.
- **The sink** — `tracing_subscriber`'s formatter on stderr
  (`rustd/crates/agentsfleetd/src/logs.rs`,
  `rustd/crates/agentsfleet_runner/src/main.rs`); `AGENTSFLEET_LOG_LEVEL` sets
  the level in both.
- **The export** — the daemon bridges the same emits into OTLP, and its log
  exporter counts failures without warning, so a failing exporter cannot feed
  itself (§"The Rust daemon's export path, as built").

The runner's records go to stderr, where the host supervisor keeps them. Once
it is built, the runner collector reads them from the host's log store.
Field rules: `.orly/standards/domains/logging.md`, committed in this repository.

## The export path — one endpoint, and the collector owns the fan-out

**Decision (M181_001).** The Rust daemon is a pure OTLP pusher to ONE configured
endpoint, addressed by the OpenTelemetry specification's own environment names.
It does not know which backend its signal reaches, and that is the point.

Posting direct to a vendor, gated on vendor-named variables, spells vendor
identity into the daemon's own configuration. That works while there is one
backend, and it makes moving to a second one a daemon change:
new credentials, new configuration, a redeploy, and a window in which the old
and new paths are both half-configured. Fan-out to two backends at once is not
expressible at all without teaching the daemon about both.

**The collector is a Fly app per environment, reached only on the private
network.** `otelcol-{dev,prod}` hold the vendor credential and export upstream;
the daemon's endpoint names them instead of the vendor, and it still posts to
`{endpoint}/v1/{logs,traces,metrics}` — only the host on the front changed, so
the repoint is one staged string and no daemon source edit.

**Its receiver requires a credential, and that is not optional.** The private
network spans the whole organisation rather than this app pair, so an
unauthenticated receiver in front of the vendor key is a relay any workload on
that network can post through. The daemon already sends a Basic pair on every
export; the collector checks it, which is what keeps the boundary where it was
before the hop existed.

Pointing the daemon at a collector moves that decision out of the binary.
Adding a backend, splitting one signal to two destinations, or moving a vendor
becomes collector configuration, applied without redeploying the thing that
serves requests. The daemon keeps one endpoint, one credential and one failure
mode no matter how many backends exist downstream, and an infrastructure change
stays separately attributable from a binary change — which is exactly the
property the cutover needs on swap day.

The cost is honest: a collector is one more thing to run, and a collector that
is down is a signal gap the daemon cannot route around. The bounded exporter
already answers for that — a full queue drops rather than blocking a
request, which is the same behaviour it has when a vendor endpoint is
unreachable. The gap was never zero; the collector does not widen it.

**There is no pull endpoint, and there will not be one.** Nothing scrapes this
daemon: no `/metrics` route is served, and no scrape is configured in either
environment. A pull endpoint would be a second export path exporting the same
measurements by a different mechanism with a different failure mode.

Only the standard environment names below configure daemon export (`agentsfleetd/src/preflight/otlp/mod.rs`).

### The Rust daemon's export path, as built

`agentsfleetd` reads four knobs in preflight and nowhere else, spelled as the
OpenTelemetry specification spells them: `OTEL_EXPORTER_OTLP_ENDPOINT`,
`OTEL_EXPORTER_OTLP_HEADERS` (comma-joined `key=value` pairs),
`OTEL_EXPORTER_OTLP_PROTOCOL` (`http/protobuf` by default, `http/json`
accepted) and `OTEL_EXPORTER_OTLP_TIMEOUT` (whole milliseconds, 10 000 by
default). A vendor's own variable names configure nothing. A protocol this build does not carry (`grpc` included), a
zero or unreadable timeout, or a header pair with no `=` refuses boot at
preflight, before anything opens: a daemon that silently exported nothing
would present as a collector fault for as long as nobody checked.

No endpoint is the ordinary case and not a fault. The daemon boots, serves,
logs `startup_otel_disabled`, and supervises no export task. With one, boot
(`rustd/crates/agentsfleetd/src/telemetry/mod.rs`, through the builder both
binaries share in `rustd/crates/afd_otlp/`) builds four pipelines on the
published `opentelemetry-otlp` exporter over HTTP, each posting under its
signal path (`/v1/traces`, `/v1/metrics`, `/v1/logs`) appended to the one
endpoint, on the SDK's own batch threads with a blocking client. The periodic
metric reader collects on a plain thread with no async reactor, so an async
client awaited there would have no driver.

- **Spans** ride the SDK's batch span processor inside `afd_observability`'s
  counting wrapper, whose number is spans the exporter failed to send.
- **Log records** ride the batch log processor inside a counting wrapper of
  their own, whose number is RECORDS the exporter failed to send. It is the
  one wrapper that raises no warning: a warning about a failed log export
  becomes a log record the subscriber hands to the batch processor, which
  hands it back to the exporter that just failed, so a collector that stayed
  down would drive a feedback loop. For this signal the count is the whole
  report, and it reaches
  `agentsfleet_otlp_entries_discarded_total{signal="logs"}` like the other
  two. `logs.rs` installs the stderr subscriber in `main`, before any knob is
  read, with an empty reload slot; boot fills the slot with two bridges, one
  for spans and one for events, over the SAME emits stderr already writes.
  Stderr stays logfmt with the exporter present, absent or failed.
- **Metrics** use source-owned cumulative totals for every daemon counter and histogram.
  The census routes them to the existing cumulative provider before the shared collector endpoint.
  Collector selection cannot split a daemon's accumulation.

  The common builder also retains a delta provider for runner families declared as windows.
  Both readers collect every 5 s (`COLLECT_INTERVAL` in `rustd/crates/afd_otlp/src/pipelines/mod.rs`).
  Their counting wrappers record failed collection cycles.

  Boot claims each census family under its declared kind and series ceiling.
  The parity test rejects families without producers.
  Families without measurements receive no invented zero points.

Exporters make one attempt; a failed batch increments its counter without blocking requests.
Later cumulative exports include earlier measurements while the process survives.
`an_unreachable_collector_costs_spans_and_not_requests` checks the request-path property on a real socket.

The supervised `otlp_export` task samples the resident set every 5 s; the SDK readers own exporting.
Gauge callbacks load its cell rather than reading `/proc` under the pipeline lock.
A stalled callback would silence every family's collection.

Cancellation force-flushes every provider before dropping the pools, matching the `analytics_flush` ordering.
Flush runs on the blocking pool because the SDK call parks its thread without a timeout.
A blocked asynchronous task has no await point for the supervisor's join deadline to cancel.

Endpoint logging records only the source knob's name, never its value or the accompanying credential.

## Label registry — money stays in Postgres

Labels are bounded at the source:

| Label | Allowed values and ceiling | Overflow action |
|---|---|---|
| `runner_id` | at most 4096 exact values | counters merge into `_other`; gauges drop |
| runner `reason` and `outcome` | compile-time execution enums plus `unknown` for reason | closed sets; no dynamic overflow value |
| `gen_ai.token.type` | `input`, `output` | closed set; no overflow value |
| `agentsfleet.execution.posture` | `platform`, `self_managed` | closed set; no overflow value |
| `agentsfleet.billing.charge.type` | `receive`, `renewal`, `settle` | closed set; no overflow value |
| `gen_ai.provider.name` | canonical standard names plus public provider identities from the compiled catalogue and SDK; aliases share one emitted value | private/unrecognized names omit the attribute and count the omission |
| `gen_ai.request.model` | the saved runtime model's catalogue identity carried by the lease; catalogue updates can add identities, and the cost families set no series ceiling (`shared:cost` in the census) | none of ours; past the OpenTelemetry SDK's default of 2,000 series per stream, new model values fold into `otel.metric.overflow`, so their costs stop being told apart |
| `surface` (library) | `tenant_models`, `global_models`, `fleet_summary`, `workspace_entries` | closed enum; no overflow value — a fifth surface is a code change, not a label |
| `stage` (library) | `next_upstream`, `auth_verify`, `pool_wait`, `authorize`, `sql`, `secret_project`, `map`, `serialize`, `cache_revision`, `cache_lookup` | closed enum; no overflow value |
| `outcome` (library) | `ok`, `invalid`, `unauthorized`, `forbidden`, `not_found`, `timeout`, `cancelled`, `dependency_error`, `internal_error` | closed enum; no overflow value |

**Workspace and tenant identity never reach a metric.** A per-process
distinct-value guard cannot cap series across replicas and restarts. Exact
per-workspace cost is a Postgres query against the ledger, which is exact
rather than bounded.

**Model attribution carries no metric-family series cap.**
The cost families copy the run's model name into `gen_ai.request.model`
(`rustd/crates/afd_observability/src/producers/cost.rs`). Runtime dispatch resolves
the saved model reference through `model.platform_models` for both owner and
platform models (`rustd/crates/afd_credential/src/provider/registered/sql/mod.rs`).
Catalogue updates can add model identities over time.

The families declare no ceiling (`shared:cost` in `docs/metrics.census.tsv`).
The SDK's default limits each stream to 2,000 series.
Past that limit, new `(model, …)` combinations fold into one `otel.metric.overflow` series, losing their separate exported spend.

Exact per-model cost stays a Postgres query against `billing.usage_lines`.
A provider outside the compiled public naming registry **omits its attribute and keeps the measurement** on the `fleet.delivery` span.
The omission increments
`agentsfleet_otel_attribute_omitted_total{attribute,reason}`
(`rustd/crates/afd_observability/src/delivery/mod.rs`).

The pinned
[OpenTelemetry provider reference](https://github.com/open-telemetry/semantic-conventions-genai/blob/2e994c6d59a93bb4fc1752c5378eedb9b8e14d6b/docs/registry/attributes/gen-ai.md#gen-ai-provider-name)
permits custom values when no well-known value applies. The shared registry
therefore retains Fireworks, OpenRouter, MiniMax and other compiled public
providers. A Fireworks-hosted MiniMax model keeps Fireworks as its serving
provider and the exact MiniMax model identifier.

No model-name or protocol
heuristic chooses a provider. Historical `_other` or omitted attributes cannot
be reconstructed; corrected attribution starts after deploying the new binaries.
The `budget_exhausted` and
`value_too_long` reasons are declared and have no producer yet.

All three OTLP signals share one resource (`service.name`,
`service.namespace=agentsfleet`, `service.version`, `service.instance.id` only
when a trusted id exists; `rustd/crates/afd_otlp/src/resource.rs`). No schema
URL is set.

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

Every committed debit emits once; uncommitted, stale-fenced, or replayed operations emit nothing.
The daemon accumulates cost counters and histograms into cumulative points per metric and label set.
The collector forwards those totals without owning their accumulation.

A process restart starts a new cumulative interval.
The SDK carries each interval's start timestamp, and the trusted service instance identifies its source.
The store-side `metrics-exporter-dead` absence rule watches this sole metrics export path.

## Focused operator dashboards

The six native definitions in `playbooks/operations/observability/assets/`
cover Overview, Chat latency, API & Database, Scheduler, Models and Sandbox.
Apply and drift verification cover the whole suite. Attribute omissions are
separate from exporter loss.

Empty series mean unavailable samples, not zero.
The dev runner collector accepted its first real lease on Nov 04, 2026;
Tempo and Prometheus both received it. Sandbox/tool metrics need actual
activity, and per-model call and database/scheduler additions require their
new producers to be deployed.

Fleet identifiers stay on spans.

Pool gauges read the real boot-owned pool through a callback without I/O.
Catalogue and schedule snapshots are sampled outside collection locks, with
query budgets; failed reads withdraw data.

Model metadata is limited to 256
rows and expires after 120 seconds. Tools and image transport describe the
configured route, not measured model quality or inferred reasoning ability.
