<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M139_002: Runner signal routing is bounded and evidence-backed

**Prototype:** v2.0.0
**Milestone:** M139
**Workstream:** 002
**Date:** Jul 23, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — observability must explain runner execution without turning signal volume into control-plane load
**Categories:** Observability (OBS)
**Batch:** B2 — investigation adjacent to M139_001; no scheduler owner wiring is required to begin
**Branch:** feat/m139-deadline-scheduler
**Test Baseline:** unit=2824 integration=376
**Depends on:** M139_001 for the scheduler event inventory; its remaining production wiring does not block this investigation
**Provenance:** Large Language Model (LLM)-drafted (Codex, Jul 23, 2026) from Indy's signal-routing direction and verified source inspection
**Canonical architecture:** `docs/architecture/observability.md` §agentsfleet-runner — deliberately bare

---

## Overview

**Goal (testable):** Produce one source-verified routing decision for logs, metrics, and traces across `agentsfleet-runner` and `agentsfleetd`, with a named owner, transport, volume unit, cardinality ceiling, buffering rule, drop behavior, privacy boundary, and failure behavior for every selected signal before runtime code changes.

**Problem:** `agentsfleetd` already exports structured logs, traces, and metrics through OpenTelemetry Protocol (OTLP), and it initializes PostHog product analytics, while `agentsfleet-runner` deliberately logs only to local stderr and reports bounded execution facts through existing fleet verbs. The current architecture documents parts of this split but does not give one complete answer for all three signals. Adding a generic runner-log API would duplicate a high-volume byte stream through the control plane, consume request and storage capacity, and couple runner progress to observability availability. Tracing also lacks a written decision on whether context crosses lease, heartbeat, activity, and report boundaries.

**Solution summary:** Inventory the production call graph first, model signal volume at named fleet sizes, then write the chosen topology into the existing architecture documents. Preserve local runner logs as the authoritative raw stream; centralized raw logs, if justified, use a host collector directly to the logging backend and bypass `agentsfleetd`. Derive operational metrics from existing bounded fleet verbs. Keep PostHog for selected product events only. Decide tracing from correlation value, sampling, propagation cost, and failure isolation; no per-scheduler-arm span or raw trace relay is assumed.

The eventual Pull Request (PR) for runtime changes will be specified only after this investigation identifies the smallest correct surface.

## PR Intent & comprehension handshake

- **PR title (eventual):** docs(observability): decide bounded runner signal routing
- **Intent (one sentence):** Operators get useful runner logs, metrics, and traces without routing high-volume raw telemetry through `agentsfleetd`.
- **Handshake:** Restate the signal owner and byte path for all three signals before editing. `ASSUMPTIONS I'M MAKING: 1. This workstream produces an architecture decision and a precise follow-on implementation surface, not runtime wiring. 2. Existing heartbeat, activity, and report verbs are measured before any new field is proposed. 3. A raw runner-log ingestion API is rejected unless source-backed evidence invalidates the volume concern. 4. Telemetry failure never blocks leasing, execution, heartbeat, activity, or report settlement.`

## Implementing agent — read these first

1. `src/agentsfleetd/main.zig` and `src/agentsfleetd/cmd/preflight.zig` — production stderr, OTLP, and PostHog installation and lifecycle.
2. `src/agentsfleetd/observability/otel_logs.zig`, `otel_metrics.zig`, `otel_traces.zig`, and `telemetry_events.zig` — fixed buffers, drop behavior, payload shape, and declared analytics events.
3. `src/agentsfleetd/fleet/service_report.zig` and `src/agentsfleetd/observability/metrics_runner.zig` — existing report-derived metrics and bounded runner cardinality.
4. `src/runner/main.zig`, `src/runner/daemon/lease_run.zig`, and `src/runner/daemon/forwarders.zig` — local logs, terminal report, heartbeat, and activity volume boundaries.
5. `docs/architecture/observability.md`, `docs/architecture/runner_fleet.md`, and `docs/architecture/data_flow.md` — canonical process ownership and current fleet flow.
6. `docs/v2/done/M80_007_P1_API_OBS_RUNNER_FAILURE_OBSERVABILITY.md` — shipped precedent for deriving runner metrics from outbound verbs instead of scraping or reaching into runners.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/architecture/observability.md` | EDIT | Record the three-signal routing matrix, volume budgets, backend ownership, and failure isolation. |
| `docs/architecture/runner_fleet.md` | EDIT | Reconcile runner-local logs, bounded fleet verbs, and any chosen trace-context propagation. |
| `docs/architecture/data_flow.md` | NO CHANGE (reviewed) | Runtime flow does not change: this work selects and documents routing before code. |
| `docs/v2/pending/M139_003_P1_OBS_BOUNDED_SIGNAL_EXPORT.md` | CREATE | Pin the selected source changes, tests, and rollout without mixing investigation with implementation. |
| `audits/signal-routing.sh` | CREATE | Turn every completed investigation Dimension into a deterministic source-and-architecture assertion. |
| `make/quality.mk` | EDIT | Run the signal-routing drift audit from the existing `lint-all` verification lane. |

No runtime source, schema, deployment, dashboard, or public API file is in scope. The audit script and its existing-lane verification hook are the only executable artifacts.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — Observability (OBS) requires observable async failures; Ground in Source Truth (GRD) requires code-backed claims; No Dead Code (NDC) forbids speculative surfaces. Optional telemetry must degrade without blocking the product path.
- **`dispatch/name_architecture.md`** — current architecture wins until this workstream reconciles it with source evidence.
- **`dispatch/write_documentation.md`** and **`docs/DOCUMENTATION_RULES.md`** — architecture prose must state operational behavior directly and keep code identifiers exact.
- **`dispatch/write_shell.md`** and **`dispatch/write_any.md`** — the deterministic audit remains portable, bounded, milestone-free, and free of hidden mutation.
- **`docs/LOGGING_STANDARD.md`** — determines level, structure, redaction, and ownership for any recommended event.

## Applicable Gates

N/A for runtime gates. The Shell, File and Function Length, Logging, Milestone Identifier (ID), SPEC TEMPLATE, Architecture Consult, documentation, and staged secret-scan gates apply.

## Prior-Art / Reference Implementations

- **M80_007 and `metrics_runner.zig`:** runner liveness and failure signals ride outbound on existing fleet verbs; `agentsfleetd` exposes a fixed 4096-slot, allocator-free view with overflow routed to `_other`.
- **Existing OTLP exporters:** logs use a 2048-entry ring; traces and metrics use 1024-entry rings; exporters flush asynchronously and surface or count loss instead of blocking request work.
- **Existing report settlement:** one accepted terminal report records duration, token use, outcome, and failure reason, then emits metrics after durable settlement. This is the default derivation point for run-level signals.
- **Existing runner logging:** one structured local stderr sink keeps backend credentials and exporter workers out of `agentsfleet-runner`.

## Sections (implementation slices)

### §1 — Establish source truth and a volume model

Create a call-site matrix for every production log sink, OTLP exporter, PostHog capture, runner heartbeat/activity/report field, and trace context. Distinguish declared types from production calls; specifically verify whether `FleetCompleted` is captured outside tests. Model events and bytes at named concurrency points, including burst and backend-outage conditions. Every candidate route records its queue bound, flush behavior, drop accounting, and retry ceiling.

- **Dimension 1.1 — DONE** — production wiring matrix distinguishes installed, called, declared-only, and absent signals → Audit `inventory`
- **Dimension 1.2 — DONE** — volume model covers steady state, burst, backend outage, and fleet growth for runner logs, control-plane logs, metrics, and traces → Audit `volume`
- **Dimension 1.3 — DONE** — current buffer, cardinality, retry, and drop behavior is source-cited rather than inferred from prose → Audit `limits`

### §2 — Decide log ownership without a control-plane log pipe

Keep runner raw logs on stderr for local process supervision and journaling. Evaluate direct host collection only as an optional edge-to-backend path with bounded disk buffering, redaction, sampling, and outage behavior. `agentsfleetd` may log semantic facts it owns when handling heartbeat/report requests, but it does not accept or replay runner log lines. Activity frames remain user-visible run activity, not an observability tunnel.

- **Dimension 2.1 — DONE** — routing decision contains no raw runner-log ingestion endpoint or activity-frame repurposing → Audit `log-route`
- **Dimension 2.2 — DONE** — direct collection decision states local authority, buffer bound, redaction, loss behavior, and credential owner → Audit `collector`

### §3 — Decide metrics at bounded semantic boundaries

Prefer metrics derived after accepted heartbeat, lease, activity, and terminal report operations. Reuse fixed-cardinality labels and existing coalescing/export buffers. Reject runner Identifier (ID), event ID, lease ID, trace ID, model text, or error text as unbounded metric labels. Scheduler internals may expose aggregate counters for armed, fired, cancelled, stale, dropped, and queue depth only when they answer an operator question; never emit one remote metric sample per arm.

- **Dimension 3.1 — DONE** — each metric maps to an existing bounded semantic event or gives evidence for a new one → Audit `metric-source`
- **Dimension 3.2 — DONE** — every label has a cardinality ceiling and overflow/drop rule → Audit `metric-cardinality`

### §4 — Decide trace propagation and sampling

Audit the existing World Wide Web Consortium (W3C) trace context at `agentsfleetd` HTTP ingress and the runner engine's internal trace identifier. Decide whether one context should cross lease issue, renew, activity, and terminal report. Prefer run/control-plane boundary spans over scheduler-arm spans. The decision must cap spans per run, define sampling, reject malformed context without failing execution, and preserve correlation in logs without adding raw payload or secret fields.

- **Dimension 4.1 — DONE** — chosen propagation map names every boundary and the parent/child relation → Audit `trace-map`
- **Dimension 4.2 — DONE** — span budget, sampling, malformed-context behavior, and exporter-outage behavior are explicit → Audit `trace-budget`

### §5 — Record the decision and pin the next implementation

Update canonical architecture with one routing matrix and a topology diagram. The matrix covers signal, producer, local sink, network path, backend, volume unit, bound, loss visibility, privacy, and failure isolation. If runtime work remains, create M139_003 with only evidence-backed file changes and tests; otherwise record why current wiring is sufficient.

- **Dimension 5.1 — DONE** — architecture documents agree on the selected topology and ownership → Audit `architecture`
- **Dimension 5.2 — DONE** — follow-on spec or no-change verdict is mechanically traceable to every investigation finding → Audit `follow-on`

## Interfaces

- **Input:** current production call sites, exporter capacities, retry behavior, fleet verb payloads, and Indy's no-log-API constraint.
- **Decision record:** one row per signal with producer, owner, path, volume unit, queue or cardinality bound, sampling, loss accounting, privacy, and degradation.
- **Runtime interface:** none added here. Existing heartbeat, lease, activity, and report shapes remain unchanged.
- **Output:** reconciled architecture and either an executable M139_003 spec or a source-cited no-change verdict.

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Raw logs proposed through `agentsfleetd` | treating the control plane as a generic collector | Reject the route; retain local stderr and evaluate a direct host collector. |
| Declared analytics event mistaken for wiring | tests cover a type with no production capture | Mark declared-only and exclude it from current-state claims. |
| Unbounded metric labels | raw IDs or text become labels | Replace with fixed classes or reject the metric; document overflow. |
| Export queue saturation | backend outage or burst exceeds the ring | Core work continues; drops are counted or logged through the exporter self-signal. |
| Missing or malformed trace context | mixed version or invalid propagation | Start a safe local root or omit propagation; never fail execution or report. |
| Collector unavailable | edge log backend or network is down | Local process supervision remains authoritative; bounded buffering drops by policy. |
| Duplicate terminal report | retry after uncertain response | Existing settlement claim prevents duplicate run-level metrics and events. |
| Sensitive content enters a signal | payload or credential included as a property | Reject the field; allow only enumerated metadata and bounded identifiers where justified. |

## Invariants

1. No raw runner log line traverses an `agentsfleetd` ingestion API.
2. `agentsfleet-runner` receives no PostHog or backend OTLP credential as a result of this decision.
3. Heartbeat and terminal report remain bounded semantic signals; activity remains product output rather than telemetry transport.
4. Signal export and analytics are best effort and never block leasing, execution, renewal, activity, or report settlement.
5. Every route has a numeric queue, cardinality, span, sample, or byte bound plus visible loss behavior.
6. No prompt, response body, token, credential, environment value, or arbitrary error text is emitted.
7. Scheduler mechanics aggregate locally; they do not create one remote log, metric, analytics event, or span per arm.

## Metrics & Observability

| Signal | Current owner | Investigation proof | Required decision |
|--------|---------------|---------------------|-------------------|
| structured logs | runner local stderr; `agentsfleetd` stderr + OTLP | sink call graph and 2048-entry log ring | local retention, optional direct collector, loss and privacy |
| operational metrics | `agentsfleetd` report/heartbeat handlers | 4096 runner slots, 1024 OTLP samples, aggregation and drop tests | semantic source, label ceiling, coalescing and loss |
| traces | `agentsfleetd` HTTP and fleet delivery | W3C context plus 1024-entry trace ring | cross-process propagation, span budget and sampling |
| product analytics | `agentsfleetd` PostHog client | production capture matrix; declared-only events called out | selected business event only, never raw telemetry |

This workstream adds no runtime signal. Its observability output is the verified routing decision itself.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | integration | `bash audits/signal-routing.sh inventory` | every claimed producer has a non-test call site; declared-only rows are labelled. |
| 1.2 | unit | `bash audits/signal-routing.sh volume` | each signal has steady, burst, outage, and growth rows with units. |
| 1.3 | unit | `bash audits/signal-routing.sh limits` | every capacity and retry value matches a current source symbol. |
| 2.1 | integration | `bash audits/signal-routing.sh log-route` | architecture contains no runner-log API or activity reuse. |
| 2.2 | unit | `bash audits/signal-routing.sh collector` | chosen collector row contains all required ownership and loss fields. |
| 3.1 | integration | `bash audits/signal-routing.sh metric-source` | each chosen metric points to a handled semantic operation. |
| 3.2 | unit | `bash audits/signal-routing.sh metric-cardinality` | every label lists allowed values, ceiling, and overflow action. |
| 4.1 | integration | `bash audits/signal-routing.sh trace-map` | lease, renew, activity, report, and HTTP parentage are decided. |
| 4.2 | unit | `bash audits/signal-routing.sh trace-budget` | spans per run, sample rule, malformed input, and outage are covered. |
| 5.1 | integration | `bash audits/signal-routing.sh architecture` | observability and runner-fleet documents name the same owners and paths. |
| 5.2 | integration | `bash audits/signal-routing.sh follow-on` | each action finding maps to the follow-on implementation or an explicit no-change result. |

## Acceptance Rubric (single scoring surface)

| ID | Weight | Criterion | Verify command | Pass condition |
|----|--------|-----------|----------------|----------------|
| R1 | 15 | Inventory is production-backed | `bash audits/signal-routing.sh inventory` | source and architecture agree |
| R2 | 15 | Signal volume model is complete | `bash audits/signal-routing.sh volume` | all four scenarios exist for each route |
| R3 | 15 | Raw runner logs bypass the control plane | `bash audits/signal-routing.sh log-route` | rejection and chosen path are explicit |
| R4 | 10 | Volume and loss are bounded | `bash audits/signal-routing.sh limits` | source constants and documented limits agree |
| R5 | 10 | PostHog truth is reconciled | `bash audits/signal-routing.sh inventory` | installed and called states remain distinct |
| R6 | 10 | Trace boundaries are decided | `bash audits/signal-routing.sh trace-map` | each boundary has an explicit verdict |
| R7 | 10 | Failure isolation is explicit | `bash audits/signal-routing.sh architecture` | both architecture documents preserve failure isolation |
| R8 | 5 | Follow-on scope is deterministic | `bash audits/signal-routing.sh follow-on` | the pending implementation contains every selected decision |
| S1 | 5 | Documentation checks pass | `make lint-all` | exits 0 |
| S2 | 5 | Repository tests remain green | `make test-unit-all` | exits 0 |

**Pass threshold:** 100/100. Behaviour rows are indivisible; a missing route or unbounded signal returns to investigation.

## Dead Code Sweep

The audit adds no production symbol and deletes no source surface. `make lint-all` is its durable caller, so source or architecture drift fails routine repository verification after this investigation closes.

## Out of Scope

- Implementing the selected runtime changes; M139_003 owns them if needed.
- A raw runner-log ingestion endpoint or reuse of activity frames as telemetry.
- Installing or operating a host collector, logging backend, dashboard, or alert.
- Direct PostHog or OTLP credentials in `agentsfleet-runner`.
- Per-arm scheduler spans, analytics events, or remote metric samples.
- Changing lease, heartbeat, activity, or report payloads before the investigation supports it.

## Product Clarity (authoring record)

1. **Whose problem?** Operators and developers diagnosing runner execution and control-plane behavior.
2. **What can they do after this?** Select the correct signal and backend without guessing whether data crosses `agentsfleetd`.
3. **Best direct way?** Existing bounded fleet verbs for semantics; local stderr or a direct edge collector for raw logs.
4. **Larger system justified?** No. A new telemetry service or runner ingestion API has no evidence-backed need.
5. **Smallest useful version?** One source-verified matrix, volume model, topology diagram, and implementation verdict.
6. **What is explicitly not shipped?** Exporter code, new endpoints, dashboards, collectors, and payload changes.
7. **Does it compound existing strengths?** Yes — accepted report settlement, bounded metric storage, asynchronous OTLP export, and credential-free runners.
8. **User interface or command-line impact?** None.
9. **Dashboard required now?** No; routing and signal correctness precede visualization.
10. **How does an operator verify it?** Follow the architecture matrix from producer to sink and inspect the documented loss signal and bound.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** investigation-only architecture refactor. The problem spans three signal types and two processes; changing one emitter first would lock in an unmeasured route.
- **Rejected:** generic runner telemetry ingestion through `agentsfleetd`. It multiplies control-plane bytes and couples diagnostics to the service being diagnosed.
- **Rejected:** direct backend exporters inside every runner. They distribute credentials, workers, retry state, and backend policy to untrusted hosts.
- **Preserved:** existing report-derived metrics and local structured logging while tracing propagation is decided from measured value.

## Discovery (consult log)

- **Indy direction (Jul 23, 2026):** investigate logs, metrics, and tracing first, then decide the implementation approach.
- **Log-routing decision (Jul 23, 2026):** raw runner logs must not traverse an `agentsfleetd` API because their volume belongs outside the control plane.
  > Indy (Jul 23, 2026): "also it doesnt make sense to do the logs via api to agentsfleetd since its stupid. image the volume of logs" — context: raw runner log ingestion through `agentsfleetd` is prohibited.
- **Source finding:** `agentsfleetd` installs stderr plus OTLP logs, traces, and metrics; PostHog is optional and batched. `agentsfleet-runner` writes structured stderr and has no backend exporter.
- **Source finding:** `FleetCompleted` is declared and tested, but the initial call-site audit found no production capture. The investigation must confirm and record this as declared-only unless wiring appears.
- **Source finding:** report settlement already emits run metrics after acceptance; runner metric storage has a fixed 4096-runner ceiling and OTLP metric/trace queues each hold 1024 entries.
- **Architecture consult:** M80_007 already chose outbound fleet verbs over inbound runner scraping and documented multi-replica aggregation behavior.
- **Capacity finding:** logs and traces each drain 50 entries every 5 seconds. The trace path therefore sustains 10 spans per second, which 100 idle runners consume through heartbeat request spans alone.
- **Loss finding:** collection removes entries before an OTLP post. A rejected post is a definite local discard, while timeout or transport failure can leave remote delivery uncertain. Logs and traces count ring overflow internally but do not publish the count; metrics also discard beyond 256 coalesced series and expose that loss only through a later successful OTLP export.
- **Partial-success finding:** an OTLP collector can return success while rejecting individual signal items. M139_003 parses the fixed per-signal rejected count, exposes it as `partial_rejected`, discards collector text, and treats malformed or impossible counts as uncertain without replay.
- **Routing decision:** runner logs remain structured stderr; an optional host collector goes directly to Loki. `agentsfleetd` receives only bounded fleet verbs and derives operational metrics there.
- **Trace decision:** no trace field crosses the runner protocol while the runner creates no spans. Successful high-rate runner request spans are removed in M139_003; generic request spans have a hard process budget with aggregate suppression counters, and one settled `fleet.delivery` span remains per run.
- **Analytics decision:** PostHog stays business-only. M139_003 wires the declared `FleetCompleted` event after the fenced report claim and adds no scheduler mechanics.
- **Follow-on:** `M139_003_P1_OBS_BOUNDED_SIGNAL_EXPORT.md` pins route-aware trace policy, multi-batch export draining, Prometheus exporter self-metrics, and once-only completion capture.
- **Architecture note:** `data_flow.md` was reviewed and remains unchanged because M139_002 changes no runtime byte path.
- **Metrics review:** no new runtime event is added by this investigation.
- **`/write-unit-test` audit (Jul 23, 2026):** no runtime function, branch, error, or public symbol changes here. `audits/signal-routing.sh` turns every DONE Dimension into an executable source-and-architecture assertion. M139_003 owns runtime failure, concurrency, and performance tests.
- **Verification (Jul 23, 2026):** all R1-R8 source assertions passed; `bash audits/spec-template.sh --staged`, `git diff --cached --check`, `gitleaks protect --staged --redact`, `make lint-all`, and `make test-unit-all` exited zero. Live command-line acceptance tests requiring an HTTPS target remained skipped by their existing environment gate; every local unit lane passed.
- **Test delta:** baseline remains unit=2824 integration=376. Runtime test growth is zero because behavior is unchanged; the workstream adds one deterministic shell audit with eleven named cases. M139_003 specifies the required runtime test growth.
- **Native Codex review (Jul 23, 2026):** four adversarial passes corrected inert audit wiring, multiplied shutdown deadlines, OTLP partial-success blindness, missing metric-label dependents, over-broad absence scans, and fail-open scan errors. The final pass reported no actionable defects.
- **gstack review (Jul 23, 2026):** independent testing, performance, and red-team lanes reported no actionable findings.
- **Skill-driven review chain:** `/write-unit-test`, native Codex review, and gstack review are complete. Post-push review is not applicable because this investigation is not opening a standalone Pull Request (PR); M139_001 remains active on the branch.
- **Deferrals:** none.
