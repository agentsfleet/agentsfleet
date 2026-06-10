# Observability — `zombied` is the plane, the runner is bare

> One decision drives this whole file: **`zombied` is the observability plane; the
> host-resident `zombie-runner` is deliberately bare.** Metrics, traces, and product
> analytics all originate in (or are reported back to) the control plane. A runner
> emits local logs and reports liveness/results over the `/v1/runners` protocol —
> nothing else. Everything below is a consequence of that split. Read it before
> reaching for a metric on the runner side; there isn't one by design.

This is a sibling of [`runner_fleet.md`](./runner_fleet.md) (the control-plane /
execution-plane structure) and [`data_flow.md`](./data_flow.md) (an event traced
through the runtime). This file answers a narrower question: *when something
happens, where does the signal go, and who owns it.*

---

## `zombied` — the observability plane

All of `zombied`'s telemetry lives under `src/zombied/observability/`. Four
independent signal paths, each with a different consumer:

- **Prometheus metrics (pull).** The `zombie_*` metric families — counters,
  histograms, and gauges for external-call retries/failures, API backpressure and
  in-flight depth, execution, the runner fleet, workspace tokens, the Redis pool,
  run limits, and the signup funnel — render at the pull endpoint
  `GET /metrics` (`src/zombied/http/handlers/health.zig`, via `metrics_render.zig`).
  Nothing pushes; Prometheus scrapes.

- **OpenTelemetry (OTel) logs + traces — LIVE, exported direct.** `otel_logs.zig`
  and `otel_traces.zig` are real OpenTelemetry Protocol (OTLP) / JSON exporters: a
  ring buffer drained by a background flush thread that POSTs to Grafana Cloud
  (logs to Loki, traces to Tempo), gated on the `GRAFANA_OTLP_*` environment. Every
  structured log line fans out to the OTLP sink in addition to stderr. **There is no
  OTel collector** — the app exports straight to Grafana Cloud with no intermediary
  hop. Dashboards live in `deploy/grafana/`; the scrape/agent setup is in
  `playbooks/operations/observability/`.

- **PostHog — product analytics.** A nullable client (see
  [`scaling.md`](./scaling.md) for where it sits in the request path). Present in
  `zombied`, absent in the runner.

- **Postgres execution telemetry.** Per-run accounting in
  `src/zombied/state/` (execution telemetry + the billing/credit-pool counters) —
  durable, queryable, the system of record for what a run cost.

### The M61 naming trap (read before you "remove" OTel)

The milestone named **`OTEL_EXPORT_REMOVAL`** did **not** remove the live OTel
export. It deleted a *different*, genuinely-dead trio (`otel_export` /
`otel_histogram` / `otel_json`) and **kept `otel_logs` and `otel_traces` wired**.
The name reads like "we stopped exporting OTel" — we did not. Before touching
anything OTel-shaped, confirm against `otel_logs.zig` / `otel_traces.zig` and the
`GRAFANA_OTLP_*` gate, not against the milestone name.

---

## `zombie-runner` — deliberately bare

The host-resident runner (`src/runner/`) carries **no** metrics, OTel, PostHog, or
telemetry of its own — the lone `record_metric` hook is a no-op stub. It:

- emits **logfmt** logs locally (operator reads them on the host), and
- reports **liveness and results** to `zombied` over the `/v1/runners` protocol
  (heartbeat / `/renew` / result-report).

The server side owns the runner's observable state: `zombied` holds
`metrics_runner.zig` and derives fleet liveness itself (see `runner_fleet.md`). This
is intentional — a runner is cattle (`runner_fleet.md`, "Runners are cattle, not
pets"); it holds no datastore credentials and runs no exporter. Pushing telemetry
infrastructure onto the runner would re-couple it to the very backends the split
removed.

---

## The shared logging module

The real logger is the named **`log`** module at `src/lib/logging/` — shared by both
binaries (it is in `src/lib/`, so both `build.zig` and `build_runner.zig` wire it as
a named module). Its shape makes conformance by construction:

- `mod.zig` — the body builder; callers write `log.scoped(.tag).level("event", .{…})`.
- `envelope.zig` — wraps every line with the required keys (`ts_ms=`, `level=`,
  `scope=`) and scrubs newlines to close log-injection.
- `sinks.zig` — fans each line out to stderr **and** the OTLP sink, with a 4 KiB
  buffer and a `truncated=true` marker when a line overflows.

Because the envelope and fan-out are enforced in the module, any call site that uses
`log.scoped(...).level(...)` is conformant for free — there is no per-call
discipline to remember. The field-level standard those calls must satisfy lives in
[`../LOGGING_STANDARD.md`](../LOGGING_STANDARD.md); this file covers *where the
signal goes*, that file covers *what a line must contain*.
