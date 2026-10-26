<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M233_001: Every event's round trip is broken into named stages, per fleet, in Grafana

**Prototype:** v2.0.0
**Milestone:** M233
**Workstream:** 001
**Date:** Oct 26, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing: a dev chat reply takes 5.4–10 s, about 1 s of it is the model, and the rest cannot be attributed without hand-reading debug logs
**Categories:** API, INFRA, OBS, UI
**Batch:** B1 — §1 and §3 run in parallel; §2 needs decision D1; §4 needs §1–§3 emitting
**Branch:** feat/m233-round-trip-stage-timing
**Baseline revision:** d6d279cfde5f3c6ec5eb75b088fa1d2bac1af371
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 26, 2026), from a measured dev investigation recorded in Discovery
**Canonical architecture:** `docs/architecture/observability.md` §Traces, §"Library read stages are metrics, not spans", §"`agentsfleet-runner` — a collector of its own"

---

## Overview

**Goal (testable):** For any dev event, an operator filtering the round-trip dashboard by `fleet_id` and `event_id` reads the edge, admission, claim, gates, issue, billing hold, hydrate, model call, capture, settle and completion stages with durations whose sum is within 10% of the observed Send-to-`event_complete` time.

**Problem:** A chat reply on `app-dev` takes 5.4 s on a good run and 10 s on a bad one. Grafana shows one root span per daemon request and nothing from the runner, so the only way to learn that ~4.4 s of a 5.4 s reply is platform overhead was to read `sqlx::query` debug logs by trace identifier, query Postgres timestamps, and tap the Server-Sent Events (SSE) stream from a browser console. The runner exports nothing in dev, the Next.js steer route is invisible, and no metric names a stage.

**Solution summary:** `agentsfleetd` records fixed-cardinality stage timing for the five verbs one event crosses (steer, lease, hydrate, capture, report) and puts per-event stage durations, statement counts and SQL time on the request span it already emits. The decided runner collector is built on the bare-metal runner host so the runner's existing spans reach Grafana, with stage durations and time to first token added as attributes. The Next.js steer route reports its own cost to the browser and to the daemon. One dashboard reads it all, joined by `agentsfleet.event.id`.

## PR Intent & comprehension handshake

- **PR title (eventual):** Break each event's round trip into stages, per fleet, in Grafana
- **Intent (one sentence):** Indy can open one dashboard, pick a fleet and an event, and see where every millisecond between Send and the reply went, so optimisations are chosen from evidence.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/observability.md` — §Traces (one span per request, attribute joins, no W3C context across the runner protocol), §"Library read stages are metrics, not spans" (the stage-family shape to mirror), §"`agentsfleet-runner` — a collector of its own" (the decided collector), §"Label registry".
2. `rustd/crates/afd_observability/src/metrics/label/library.rs` — closed stage enums with census ceilings; the precedent the event stage families copy.
3. `rustd/crates/afd_api/src/router/trace.rs` — the request span that gains per-event attributes; route templates only, never raw paths.
4. `rustd/crates/afr_agent/src/spans.rs` and `rustd/crates/afr_telemetry/src/lib.rs` — the runner's four spans, span budget and endpoint rules.
5. `deploy/fly/otelcol-dev/config.yml` — the daemon collector whose receiver/exporter shape the runner collector mirrors, minus the ingest auth a loopback receiver does not need.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_observability/src/metrics/label/event_stage.rs` | CREATE | Closed `Verb` and `Stage` enums and the verb→stage table |
| `rustd/crates/afd_observability/src/metrics/label/mod.rs` | EDIT | Register the event stage labels |
| `rustd/crates/afd_observability/src/producers/event_stage.rs` | CREATE | `stage_finished(verb, stage, duration)` and the per-request tally recorder |
| `rustd/crates/afd_observability/src/producers/mod.rs` | EDIT | Export the producer |
| `rustd/crates/afd_observability/src/metrics/registry/mod.rs` | EDIT | Declare the four event stage families |
| `rustd/crates/afd_observability/src/semconv/mod.rs` | EDIT | Stage, SQL-tally and edge attribute keys |
| `docs/metrics.census.tsv` | EDIT | Census rows and `fixed:<n>` ceilings for the new families |
| `rustd/crates/afd_api/src/router/trace.rs` | EDIT | Request span declares the event, fleet, SQL-tally and stage fields as `Empty` |
| `rustd/crates/afd_api/src/router/sql_tally.rs` | CREATE | `tracing` layer summing `sqlx::query` events into the enclosing request span |
| `rustd/crates/agentsfleetd/src/telemetry/mod.rs` | EDIT | Install the tally layer beside the exporter |
| `rustd/crates/afd_api_tenant/src/handler/fleet/message_steer/mod.rs` | EDIT | Steer stages; read the bounded edge headers |
| `rustd/crates/afd_api_tenant/src/handler/fleet/message_steer/edge.rs` | CREATE | `garde`-bounded edge timing header type |
| `rustd/crates/afd_fleet/src/lease/pull/mod.rs` | EDIT | Claim, record, provider, gates stages |
| `rustd/crates/afd_fleet/src/lease/pull/poll.rs` | EDIT | Issue and thread stages; event and fleet identity on the span |
| `rustd/crates/afd_fleet/src/lease/deliver/mod.rs` | EDIT | Billing hold stage |
| `rustd/crates/afd_fleet/src/lease/report/mod.rs` | EDIT | Load, settle, billing capture, billing release, announce stages |
| `rustd/crates/afd_api_runner/src/handler/runner/memory.rs` | EDIT | Hydrate and capture stages |
| `rustd/crates/afd_api_runner/src/handler/runner/lease/mod.rs` | EDIT | Record the issued event's identity on the request span |
| `rustd/crates/afd_api_runner/src/handler/runner/report.rs` | EDIT | Record the reported event's identity on the request span |
| `rustd/crates/afr_supervisor/src/lease_loop/mod.rs` | EDIT | Runner stage durations on `runner.lease` |
| `rustd/crates/afr_supervisor/src/activity/mod.rs` | EDIT | First flush delay on `runner.lease` |
| `rustd/crates/afr_agent/src/spans.rs` | EDIT | `chat` declares the first-token field |
| `rustd/crates/afr_agent/src/loop/model_turn.rs` | EDIT | Record time to first streamed token |
| `deploy/baremetal/otelcol-runner.yml` | CREATE | Runner collector: loopback OTLP receiver, journald receiver, Grafana exporter |
| `deploy/baremetal/otelcol-runner.service` | CREATE | systemd unit; credential from an environment file only root reads |
| `deploy/baremetal/agentsfleet-runner.service` | EDIT | `OTEL_EXPORTER_OTLP_ENDPOINT` names the loopback collector |
| `deploy/baremetal/service.sh` | EDIT | Install and enable the collector unit |
| `deploy/baremetal/service_test.sh` | EDIT | Unit install and endpoint assertions |
| `ui/packages/app/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/messages/route.ts` | EDIT | Time credential and upstream; `Server-Timing`; forward edge headers |
| `ui/packages/app/lib/api/fleets.ts` | EDIT | `steerFleet` accepts the edge headers |
| `ui/packages/app/components/domain/useFleetMessageDelivery.ts` | EDIT | `performance` marks for send and accepted |
| `ui/packages/app/lib/streaming/fleet-stream-reply-registry.ts` | EDIT | `performance` marks for first frame and completion |
| `ui/packages/app/tests/steer-route-timing.test.ts` | CREATE | Route timing and header tests |
| `playbooks/operations/observability/assets/round_trip_dashboard.json` | CREATE | The round-trip dashboard |
| `playbooks/operations/observability/03_apply.sh` | EDIT | Apply the second dashboard |
| `playbooks/operations/observability/01_assets_check.sh` | EDIT | Validate the second dashboard asset |
| `playbooks/operations/observability/002_round_trip_playbook.md` | CREATE | How to read one event's round trip |
| `docs/architecture/observability.md` | EDIT | Event stage families, runner collector built, edge timing |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — UFS (stage names, header names and bounds are named constants), NSQ (no new SQL; any read stays schema-qualified), ECL (an unparseable edge header is ignored, never an error), TIM (every clock is monotonic inside a process; the cross-process edge delta is clamped), FLL, NDC, ORP, TST-NAM, ARCH (attribute and family names consult the observability architecture first), TSC.
- `.orly/docs/LOGGING_STANDARD.md` — stage timing is a metric and an attribute, not a new log line.
- `.orly/dispatch/write_rust.md` and `docs/RUST_ERROR_STANDARD.md` — the tally layer and the edge type add no fallible public signature; any that appears uses `afd_core::error_shell!`.
- `.orly/dispatch/write_ts_adhere_bun.md` — route and hook edits.
- `.orly/dispatch/write_shell.md` — `service.sh` edits.
- `.orly/docs/REST_API_DESIGN_GUIDELINES.md` §8 — the edge headers are untrusted input bounded with `garde`.
- `.orly/dispatch/name_architecture.md` — every new attribute key and family name is checked against `docs/architecture/observability.md` before it is spelled.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UFS GATE | yes — stage names, header names, bounds | Every literal is a named constant in the label module or the route |
| LOGGING GATE | yes — producers touch the logging path | No new log lines; stage data is metric plus span attribute |
| UI GATE / DESIGN TOKEN GATE | no — no rendered component changes | `performance` marks only; no markup |
| SCHEMA GUARD | no — no schema change | Attribute joins replace any trace-context column |
| Architecture consult | yes — new families and keys | Consult recorded in Discovery; `observability.md` updated in the same PR |
| File & Function Length (≤350/≤50/≤70) | yes — `pull/mod.rs` and `report/mod.rs` are near the cap | Stage recording lives in `producers/event_stage.rs`; call sites add one guard each |

## Prior-Art / Reference Implementations

- **Reference:** the library stage registry (`rustd/crates/afd_observability/src/metrics/label/library.rs`, `docs/architecture/observability.md` §"Library read stages are metrics, not spans") — duration and observation counters per closed `(verb, stage)`; this spec copies it, adding no histogram.
- **Reference:** `fleet.delivery` (`rustd/crates/afd_observability/src/delivery/mod.rs`) — correlation identity on a span, never on a metric.
- **Reference:** the `X-Request-Start` queue-time convention (Heroku router, NGINX `$msec`, New Relic request queueing) — the edge stamps its receive time, the origin computes the delta and clamps skew.
- **Reference:** W3C `Server-Timing` (https://www.w3.org/TR/server-timing/) — the browser reads `PerformanceResourceTiming.serverTiming` on a same-origin response.

## Sections (implementation slices)

### §1 — The daemon names every stage an event crosses

Five verbs carry one event: steer (`POST …/fleets/{fleet_id}/messages`), lease (`POST /v1/runners/me/leases`), hydrate and capture (`/v1/runners/me/memory/{fleet_id}`), and report (`POST /v1/runners/me/reports`). Each records a closed stage set into `agentsfleet_event_stage_duration_seconds_total{verb,stage}` and `agentsfleet_event_stage_observations_total{verb,stage}`. It also records per-verb SQL cost into `agentsfleet_event_sql_statements_total{verb}` and `agentsfleet_event_sql_seconds_total{verb}`. The request span gains `agentsfleet.event.id`, `gen_ai.agent.id` (the fleet), `agentsfleet.sql.statements`, `agentsfleet.sql.duration_ms` and one `agentsfleet.stage.<stage>_ms` attribute per stage the verb declares. Stage sets: steer `auth, admit, announce`; lease `claim, record, provider, gates, issue, hold, thread`; hydrate and capture `fence, memory`; report `load, settle, capture, release, announce`. **Implementation default:** the SQL tally is a `tracing` layer reading the `sqlx::query` events sqlx already emits with `elapsed_secs`, summed into the nearest enclosing `http.server.request` span. Call sites stay untouched, and the tally sees statements whatever the log export level.

- **Dimension 1.1** — Each verb records exactly its declared stages once per request, on every exit path → Test `event_stage_records_each_declared_stage_once`
- **Dimension 1.2** — The SQL tally counts and sums the `sqlx::query` events inside one request span and none outside it → Test `sql_tally_sums_statements_inside_the_request_span`
- **Dimension 1.3** — A refused or empty lease poll records `claim` only and carries no event identity → Test `empty_poll_records_claim_without_identity`
- **Dimension 1.4** — Every family's label product fits its census ceiling → Test `every_declared_ceiling_admits_its_label_product`
- **Dimension 1.5** — A real admit-lease-report cycle against live Postgres yields one span per verb carrying the same `agentsfleet.event.id` → Test `event_identity_joins_steer_lease_and_report_spans`
- **Dimension 1.6** — The request span still carries the route template and never a raw path after gaining the new fields → Test `request_span_carries_template_not_path`

### §2 — The runner's existing spans reach Grafana from its host

The runner already exports `runner.lease`, `invoke_agent`, `chat` and `execute_tool` when `OTEL_EXPORTER_OTLP_ENDPOINT` names a collector (M214_001). The collector was decided (Indy, Oct 04, 2026, recorded in `observability.md`) and never built, so dev exports nothing. This Section builds it on the bare-metal host: an OpenTelemetry Collector with an OpenTelemetry Protocol (OTLP) receiver bound to loopback, a journald receiver for the runner's stderr, and a Grafana Cloud exporter. The runner unit points at loopback. `runner.lease` gains `agentsfleet.stage.{admit,turn_wait,hydrate,sandbox,run,settle,drain}_ms` and `agentsfleet.activity.first_flush_ms`; `chat` gains `agentsfleet.chat.first_token_ms`. Attributes only: the 256-per-lease span budget is untouched. **Prerequisite:** decision D1 (credential). **Implementation default:** collector-contrib pinned to the version `deploy/fly/otelcol-dev/Dockerfile` pins, because the two configs must read alike.

- **Dimension 2.1** — The runner unit's endpoint is loopback and the collector unit loads its credential from a root-only environment file → Test `runner_unit_targets_loopback_collector`
- **Dimension 2.2** — `chat` records time to first streamed token, and a turn that streams nothing leaves the field unset → Test `chat_records_first_token_only_when_streamed`
- **Dimension 2.3** — `runner.lease` records every runner stage it reached; a lease refused at admit records `admit` only → Test `lease_span_records_reached_stages`
- **Dimension 2.4** — On the dev host, a chat event's `runner.lease` span is queryable in Grafana Tempo by `agentsfleet.event.id` → Test `manual_runner_span_reaches_tempo`

### §3 — The edge reports its own cost

Between the browser's Send and the daemon's first byte, 584 ms went unrecorded in the measured run. The steer route times `credential()` and the upstream call. It answers the browser with `Server-Timing: auth;dur=…, upstream;dur=…`, and sends the daemon `x-agentsfleet-edge-start` (epoch microseconds when the route received the request) and `x-agentsfleet-edge-auth-ms`. The daemon records stage `edge` = its receive time minus `edge-start`, clamped to `[0, EDGE_SKEW_CEILING]`, plus `edge_auth`, both under verb `steer`. The browser sets `performance` marks `af:send`, `af:accepted`, `af:first-frame`, `af:complete` keyed by event identifier, readable in DevTools. **Implementation default:** the header type is `garde`-validated, and an absent, malformed or out-of-bound header records nothing and refuses nothing, because a public API client may send garbage and timing must never change an answer.

- **Dimension 3.1** — The steer route answers with a `Server-Timing` header naming `auth` and `upstream` → Test `steer_route_answers_server_timing`
- **Dimension 3.2** — The route forwards `edge-start` and `edge-auth-ms`, and the daemon records `edge` and `edge_auth` → Test `edge_headers_record_edge_stages`
- **Dimension 3.3** — A malformed, negative or future edge header records nothing and the steer still answers 202 → Test `malformed_edge_header_is_ignored`
- **Dimension 3.4** — The browser leaves the four marks for one send in order → Test `delivery_marks_send_accepted_first_frame_complete`

### §4 — One dashboard, one procedure, per fleet

`round_trip_dashboard.json` shows mean stage cost per verb from the counter pairs (`rate(duration)/rate(observations)`). It shows tails and per-fleet rows from Tempo's trace query language (TraceQL) over the stage attributes. Its `fleet` and `event` variables list every daemon and runner span for one event. `002_round_trip_playbook.md` states the procedure this spec's investigation performed by hand. The existing apply and check scripts carry the second asset.

- **Dimension 4.1** — The asset check validates both dashboards and refuses one that names an undeclared family → Test `assets_check_validates_round_trip_dashboard`
- **Dimension 4.2** — Every metric the dashboard queries is declared in `docs/metrics.census.tsv` → Test `round_trip_dashboard_queries_declared_families`
- **Dimension 4.3** — On dev, one probe event's stage sum is within 10% of its browser-observed Send-to-`event_complete` time → Test `manual_round_trip_stage_sum_matches_browser`

## Interfaces

```
Daemon families (closed labels; ceilings in docs/metrics.census.tsv):
  agentsfleet_event_stage_duration_seconds_total{verb,stage}   counter
  agentsfleet_event_stage_observations_total{verb,stage}       counter
  agentsfleet_event_sql_statements_total{verb}                 counter
  agentsfleet_event_sql_seconds_total{verb}                    counter
  verb  ∈ {steer, lease, hydrate, capture, report}
  stage ∈ steer{edge, edge_auth, auth, admit, announce}
          lease{claim, record, provider, gates, issue, hold, thread}
          hydrate|capture{fence, memory}
          report{load, settle, capture, release, announce}

Request span attributes (http.server.request, these five routes only):
  agentsfleet.event.id, gen_ai.agent.id,
  agentsfleet.sql.statements (int), agentsfleet.sql.duration_ms (double),
  agentsfleet.stage.<stage>_ms (double) per declared stage

Runner span attributes:
  runner.lease: agentsfleet.stage.{admit,turn_wait,hydrate,sandbox,run,settle,drain}_ms,
                agentsfleet.activity.first_flush_ms
  chat:         agentsfleet.chat.first_token_ms

Edge (Next.js steer route → agentsfleetd), both optional:
  x-agentsfleet-edge-start: <integer epoch microseconds>
  x-agentsfleet-edge-auth-ms: <decimal milliseconds, 0..EDGE_SKEW_CEILING>
Edge → browser:
  Server-Timing: auth;dur=<ms>, upstream;dur=<ms>
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Hostile edge header | API client sends huge, negative or future `edge-start` | `garde` refuses the value; nothing recorded; steer answers as before |
| Clock skew | Vercel and Fly clocks disagree | Delta clamped to `[0, EDGE_SKEW_CEILING]`; a clamped sample counts under `stage=edge` like any other, and the census note says why |
| Runner collector down | Collector stopped or crashed on the host | Runner keeps running; `agentsfleet_runner_otlp_entries_discarded_total` rises; journald keeps the logs |
| Credential missing on host | Environment file absent | Collector unit fails to start with the file named; runner unaffected |
| Exit before a stage | Refusal, parked work, stale fence | Only reached stages record; the verb still records its terminal stage once |
| Span queue overflow | Burst beyond `SPAN_QUEUE` | Attributes ride existing spans; no new spans are added, so overflow behaviour is unchanged |
| Statement outside a request | Sweeper or background task runs SQL | The tally attributes nothing; no request span, no count |

## Invariants

1. No tenant, fleet, lease or event identifier is a metric label — `every_declared_ceiling_admits_its_label_product` plus the census grader reject an open label.
2. Timing never changes an HTTP answer — the edge type's parse returns `Option`, and the handler has no branch on it besides recording.
3. The runner holds no observability credential — `afr_telemetry`'s endpoint refusal of header knobs stays, and `runner_unit_targets_loopback_collector` asserts the runner unit carries no Grafana variable.
4. No new span names — the runner's span-name allowlist test and `trace.rs`'s one-span-per-request test stay green unchanged.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `agentsfleet_event_stage_duration_seconds_total` | ops | A verb finishes a declared stage | `verb`, `stage` | Closed enums only | `event_stage_records_each_declared_stage_once` |
| `agentsfleet_event_stage_observations_total` | ops | Same | `verb`, `stage` | Closed enums only | `event_stage_records_each_declared_stage_once` |
| `agentsfleet_event_sql_statements_total` / `_seconds_total` | ops | A verb's request span closes | `verb` | No statement text | `sql_tally_sums_statements_inside_the_request_span` |
| Request span stage attributes | ops | Same | Event id, fleet id, stage durations | No prompt, reply, path or SQL text | `event_identity_joins_steer_lease_and_report_spans` |
| Runner span attributes | ops | Lease and turn end | Stage durations, first token | No prompt or reply | `chat_records_first_token_only_when_streamed` |

No PostHog event changes; the analytics funnel playbook needs no update because no product event is added or renamed.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `event_stage_records_each_declared_stage_once` | A stubbed report that settles → `load, settle, capture, release, announce` each observed once |
| 1.2 | unit | `sql_tally_sums_statements_inside_the_request_span` | Three synthetic `sqlx::query` events at 2, 3, 75 ms in a span, one outside → statements 3, duration 80 |
| 1.3 | unit | `empty_poll_records_claim_without_identity` | Poll with no unsettled admission → `claim` observed once; span has no `agentsfleet.event.id` |
| 1.4 | unit | `every_declared_ceiling_admits_its_label_product` | New enums → every family product ≤ its `fixed:<n>` |
| 1.5 | integration | `event_identity_joins_steer_lease_and_report_spans` | Live Postgres: steer, lease, report → three captured spans share one `agentsfleet.event.id` |
| 2.1 | unit | `runner_unit_targets_loopback_collector` | Rendered unit → endpoint `http://127.0.0.1:4318`; no `GRAFANA_` variable in the runner unit |
| 2.2 | unit | `chat_records_first_token_only_when_streamed` | Scripted provider streams at 40 ms → field ≥ 40; empty stream → field unset |
| 2.3 | unit | `lease_span_records_reached_stages` | Lease refused at admit → only `admit_ms` set |
| 2.4 | manual | `manual_runner_span_reaches_tempo` | Indy or Aiwa sends a dev chat; TraceQL `{ span.agentsfleet.event.id = "<id>" }` returns a `runner.lease` span; screenshot in Session Notes |
| 3.1 | unit | `steer_route_answers_server_timing` | Stubbed credential 12 ms, upstream 40 ms → header names `auth` and `upstream` with durations ≥ those |
| 3.2 | integration | `edge_headers_record_edge_stages` | Steer with `edge-start` 300 ms ago → `edge` observed ≈ 300 ms (±50) |
| 3.3 | unit | `malformed_edge_header_is_ignored` | `edge-start` = `abc`, `-5`, now + 1 h, 10⁴⁰ → 202 each; no `edge` observation |
| 3.4 | unit | `delivery_marks_send_accepted_first_frame_complete` | Simulated send → four marks for one event id, in order |
| 4.1 | unit | `assets_check_validates_round_trip_dashboard` | Asset with an unknown family → check exits non-zero naming it |
| 4.2 | unit | `round_trip_dashboard_queries_declared_families` | Every `agentsfleet_*` name in the asset appears in the census |
| 4.3 | manual | `manual_round_trip_stage_sum_matches_browser` | Indy or Aiwa runs one dev probe; stage sum vs browser Send→`event_complete` within 10%; numbers in Session Notes |
| 1.6 | unit | `request_span_carries_template_not_path` | Request to `/v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages` with real identifiers → span route is the template; no identifier appears in any attribute except `agentsfleet.event.id` and `gen_ai.agent.id` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Daemon records every declared stage and the SQL tally (§1) | `cargo test -p afd_observability event_stage` | exit 0 | P0 | |
| R2 | Runner collector ships runner spans from the dev host (§2) | `manual: TraceQL { span.agentsfleet.event.id = "<probe id>" } in Grafana Tempo` | ≥1 `runner.lease` span | P0 | |
| R3 | Edge cost reaches the browser and the daemon (§3) | `cd ui/packages/app && bun run test steer-route-timing` | exit 0 | P0 | |
| R4 | Dashboard asset valid and applied (§4) | `bash playbooks/operations/observability/01_assets_check.sh` | exit 0 | P0 | |
| R5 | Stage sum explains the round trip (§4) | `manual: one dev probe, stage sum vs browser timing` | within 10% | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version consistent | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.orly/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `.orly/dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes, so recording those results does not require another code commit and suite run. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may also be **MOVED** — see below.

**A P0 whose SCOPE moves is not a P0 shipped red.** A **transfer** moves the criterion whole into a named successor spec that carries it as its own P0. Mark such a row `MOVED to M{N}_{NNN} R{n}`; it is not ❌ when (1) the successor exists and carries the row, (2) both specs record the mapping, and (3) Discovery carries the owner's verbatim quote authorising it. A MOVED row is never rendered ✅.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- **Making the round trip faster.** Candidates measured in Discovery (the CPU-throttling floor on the daemon machine, transaction count, hydrate folded into the lease, capture skipped when nothing changed, three report transactions) need this spec's numbers first; a successor spec picks them.
- **Browser telemetry export** (Grafana Frontend Observability or similar) — decision D2; this spec leaves marks in DevTools only.
- **W3C `traceparent` across the runner protocol** — `observability.md` §Traces keeps leases as separate roots joined by attribute.
- **Production runner collector** — dev host only; production follows the same unit once a production runner exists.
- **The "access needs attention" banner** — a separate product change.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy picks `dev-acceptance-20261021-0517` and its last event on one dashboard and reads "lease 1.5 s: hold 0.4 s, gates 0.3 s" without opening a terminal.
2. **Preserved user behaviour** — Chat send, reply, retry and every API answer are byte-identical; the browser gains marks, not markup.
3. **Optimal-way check** — The direct shape is one distributed trace per event; the architecture deliberately keeps leases as separate roots, so attribute joins reach the same view without changing the runner protocol.
4. **Rebuild-vs-iterate** — Iterate: every signal rides spans and families that exist; no telemetry pipeline is replaced.
5. **What we build** — Daemon stage families and span attributes, the SQL tally layer, the runner collector, runner stage attributes, edge timing, one dashboard, one playbook.
6. **What we do NOT build** — Performance fixes, browser telemetry export, cross-protocol trace context, a production collector, per-fleet metric labels.
7. **Fit with existing features** — Compounds the library stage registry and `fleet.delivery`; must not destabilise the runner span budget.
8. **Surface order** — Grafana first; no CLI surface, because the data lives in Grafana and the playbook names the queries.
9. **Dashboard restraint** — No percentile alert and no target line until a provisioned capture exists (`observability.md`: "nothing gates on a percentile").
10. **Confused-user next step** — `002_round_trip_playbook.md` names the dashboard, the variables, and the TraceQL to paste.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four Sections by process boundary (daemon, runner host, edge, dashboard), so each lands and proves alone.
- **Alternatives considered:** child spans per stage (rejected: the library precedent shows stage spans evict error spans from the budget); histograms per stage (rejected: counter pairs give means and TraceQL gives tails from attributes, at a fraction of the series); a trace-context column on `fleet.admissions` (rejected: a schema change for a join attributes already provide).
- **Patch-vs-refactor verdict:** this is a **patch** because it adds signals to existing spans and families and builds a collector already decided.

## Discovery (consult log)

- **Consults** — `docs/architecture/observability.md` read for §Traces, §Library read stages, §runner collector, §PostHog (Oct 26, 2026). Indy (Oct 26, 2026), direct: "I want you to have logging, observability added to so we could to this as a fleet for me to check it and optimize". Agent reading: per-fleet, per-event stage breakdown in Grafana. Measured dev run (event `01a17627-25c5-761d-9794-f1ab0b40d9fc`, Oct 26, 2026 03:00:38 UTC): Send → 202 0.78 s (daemon 50 ms of it); claim → runner holds lease 1.51 s across 36 statements; hydrate 0.33 s; model window ≤ 0.64 s; capture 0.39 s; report 1.60 s across 28 statements; `event_complete` at +5.42 s. Statements split 2–10 ms vs 65–82 ms. Cause measured the same day: CPU quota throttling on the dev daemon machine (`shared-cpu-1x`, `deploy/fly/agentsfleetd-dev/fly.toml`), not the database, the pooler or Nagle. PlanetScale Insights (branch `useast`) shows `insert into billing.holds` at p50/p99 0 ms server-side; an in-region probe reproduced the floor with `TCP_NODELAY` on and off, on ports 6432 and 5432; and with no database at all, 169 of 2000 idle 1 ms sleeps woke 70–76 ms late, and 1988 of 2000 under one busy thread (median 158.6 ms, max 558 ms). Runner spans: none in Tempo (no collector).
- **Required human decisions** — **D1 (decided):** the runner collector reuses `op://ZMB_CD_DEV/grafana-dev/{otlp-endpoint,instance-id,api-key}`. Indy (Oct 26, 2026), choosing between a new write-only key and the existing one: "Reuse grafana-dev". The agent had recommended a separate key; the accepted risk is that one leaked host exposes the daemon's ingest key too. **D2:** browser telemetry export, or DevTools marks only (agent recommends marks only here). Does not block.
- **Metrics review** — four daemon families added; runner families unchanged; no analytics funnel playbook update required, because no product event changes.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
