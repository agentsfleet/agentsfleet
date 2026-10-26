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
**Priority:** P1 — operator-facing: production sizing ($80 vs $198 a month for three daemon machines) and the resting lease-poll cost both need per-stage evidence that today exists only by hand-reading debug logs
**Categories:** API, INFRA, OBS, UI
**Batch:** B1 — §1 and §3 run in parallel; §2 needs D1 re-confirmed against the corrected risk; §4 needs §1–§3 emitting
**Branch:** feat/m233-round-trip-stage-timing
**Baseline revision:** d6d279cfde5f3c6ec5eb75b088fa1d2bac1af371
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 26, 2026), from a measured dev investigation; revised after the Stage 5 design review recorded in Discovery
**Canonical architecture:** `docs/architecture/observability.md` §Traces, §"`agentsfleet-runner` — a collector of its own"

---

## Overview

**Goal (testable):** For any dev event, the round-trip row of the observability dashboard, filtered by fleet and event, shows the daemon-clock window from steer start to report end as contiguous segments whose durations sum to that window, with the runner's existing segments as drill-downs, and the browser's Send-to-`event_complete` time exceeds that window by no more than the edge legs `Server-Timing` and the browser marks account for, within 10%.

**Problem:** After §0, three dev chats on `shared-cpu-2x` took 1.53–1.94 s from Send to `event_complete` (Oct 26, 2026, 05:11 UTC). Where those seconds go is not visible. Each daemon request is one root span with no event identity on it. The runner exports nothing because its collector was never built. The Next.js steer route reports nothing. The existing `agentsfleet_fleet_delivery_stage_seconds{stage}` gives two coarse aggregates (`event_to_lease`, `lease_to_first_chunk`) and no per-event view. Attributing one chat took debug-level SQL logs, Postgres timestamps and a browser console.

**Solution summary:** The five daemon verbs one event crosses attach the event's identity and their stage durations to the request span they already emit, as span attributes set from the handlers; no new metric family. The decided runner collector is built on the bare-metal host for traces and runner metrics, accepting connections only from the runner's own process; the runner's existing stage stopwatch is recorded on `runner.lease`. The steer route answers the browser with its own `Server-Timing`. The existing observability dashboard gains a round-trip row read from Tempo, joined by `agentsfleet.event.id`.

## PR Intent & comprehension handshake

- **PR title (eventual):** Break each event's round trip into stages, per fleet, in Grafana
- **Intent (one sentence):** Indy can open one dashboard, pick a fleet and an event, and see where the time between Send and the reply went, so optimisations and production sizing are chosen from evidence.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/observability.md` — §Traces (one span per request; leases are separate roots joined by attribute), §"`agentsfleet-runner` — a collector of its own" (the decided collector and its fail-closed privacy rules for logs).
2. `rustd/crates/afr_supervisor/src/lease_loop/segments.rs` — the runner's existing stopwatch (`memory`, `sandbox`, `model`, `tools`, `report`, `first_token`); §2 records it, adding no second vocabulary.
3. `rustd/crates/afd_observability/src/metrics/label/fleet.rs` — the existing `DeliveryStage` family (`event_to_lease`, `lease_to_first_chunk`); left unchanged and charted beside the new row.
4. `rustd/crates/afd_api/src/router/trace.rs` — the generic request span; this spec leaves it unchanged and sets attributes from the five handlers instead.
5. `deploy/fly/otelcol-dev/config.yml` — the collector shape to mirror, including its warning that an unauthenticated receiver is an open relay.
6. `docs/architecture/runner_execution.md` — the Facts table: under `allow_all` a sandbox shares the host's network namespace, which is why §2 restricts the receiver to the runner's cgroup.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `deploy/fly/agentsfleetd-dev/fly.toml` | EDIT | §0: dev daemon `shared-cpu-2x` 1 GB |
| `deploy/fly/dragonfly-dev/fly.toml` | EDIT | §0: dev Dragonfly `shared-cpu-1x` 1 GB |
| `.github/workflows/deploy-dev-fly.yml` | EDIT | §0: dev deploys set `AGENTSFLEET_LOG_LEVEL="info"` |
| `scripts/check_workflow_rules.py` | EDIT | §0: rule `workflow-daemon-log-level` pins `info` (owner-approved gate edit) |
| `scripts/check_workflow_rules_test.py` | EDIT | §0: fixture and seeded violation follow the pinned value |
| `docs/architecture/scaling.md` | EDIT | §0: dev cluster size statement |
| `rustd/crates/afd_observability/Cargo.toml` | EDIT | §1: add `tracing-opentelemetry` (workspace 0.34.0) for `OpenTelemetrySpanExt` |
| `rustd/crates/afd_observability/src/stage_clock.rs` | CREATE | §1: per-request stage clock over closed per-verb stage enums; writes `agentsfleet.<verb>.<stage>_ms` to the current span |
| `rustd/crates/afd_observability/src/lib.rs` | EDIT | §1: export the stage clock |
| `rustd/crates/afd_observability/src/semconv/mod.rs` | EDIT | §1: the attribute keys, spelled once |
| `rustd/crates/afd_api_tenant/src/handler/fleet/message_steer/mod.rs` | EDIT | §1: steer stages and event identity |
| `rustd/crates/afd_api_runner/src/handler/runner/lease/mod.rs` | EDIT | §1: lease stages and the issued event's identity |
| `rustd/crates/afd_api_runner/src/handler/runner/memory.rs` | EDIT | §1: hydrate and capture stages |
| `rustd/crates/afd_api_runner/src/handler/runner/report.rs` | EDIT | §1: report stages and the reported event's identity |
| `rustd/crates/afd_fleet/src/lease/pull/mod.rs` | EDIT | §1: claim, record, provider, gates marks on the passed clock |
| `rustd/crates/afd_fleet/src/lease/pull/poll.rs` | EDIT | §1: issue and thread marks |
| `rustd/crates/afd_fleet/src/lease/deliver/mod.rs` | EDIT | §1: billing hold mark |
| `rustd/crates/afd_fleet/src/lease/report/mod.rs` | EDIT | §1: load, settle, billing capture, billing release, announce marks |
| `rustd/crates/afr_supervisor/src/lease_loop/segments.rs` | EDIT | §2: record the existing segments on `runner.lease` when the lease concludes |
| `rustd/crates/afr_supervisor/src/identity/mod.rs` | EDIT | §2: `runner.lease` declares the segment fields as `Empty` |
| `deploy/baremetal/otelcol-runner.yml` | CREATE | §2: loopback OTLP receiver, traces and metrics pipelines, Grafana exporter; no log receiver |
| `deploy/baremetal/otelcol-runner.service` | CREATE | §2: systemd unit; credential from a root-only environment file |
| `deploy/baremetal/otelcol-runner.nft` | CREATE | §2: accept loopback 4318 only from the runner's own cgroup |
| `deploy/baremetal/agentsfleet-runner.service` | EDIT | §2: `OTEL_EXPORTER_OTLP_ENDPOINT` names the loopback collector |
| `deploy/baremetal/service.sh` | EDIT | §2: install the units and the rule; refuse to start the collector without the rule |
| `deploy/baremetal/service_test.sh` | EDIT | §2: unit, rule and endpoint assertions |
| `deploy/baremetal/deploy.sh` | EDIT | §2: fetch `otelcol-contrib` at a pinned version and verify its SHA-256 |
| `deploy/baremetal/deploy_test.sh` | EDIT | §2: checksum-refusal test |
| `playbooks/lib/runner/common.sh` | EDIT | §2: read the collector credential from the vault at deploy |
| `playbooks/lib/runner/deploy.sh` | EDIT | §2: write the credential to the host's root-only environment file |
| `playbooks/lib/runner/runner_test.sh` | EDIT | §2: the credential is written without being printed |
| `ui/packages/app/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/messages/route.ts` | EDIT | §3: time credential and upstream; answer `Server-Timing` |
| `ui/packages/app/components/domain/useFleetMessageDelivery.ts` | EDIT | §3: `performance` marks for send and accepted |
| `ui/packages/app/lib/streaming/fleet-stream-reply-registry.ts` | EDIT | §3: `performance` marks for first frame and completion |
| `ui/packages/app/tests/steer-route-timing.test.ts` | CREATE | §3: route timing header tests |
| `playbooks/operations/observability/assets/dashboard.json` | EDIT | §4: round-trip row (Tempo panels via a datasource variable) |
| `playbooks/operations/observability/01_assets_check.sh` | EDIT | §4: accept the Tempo datasource variable beside the Prometheus UID |
| `playbooks/operations/observability/002_round_trip_playbook.md` | CREATE | §4: how to read one event's round trip |
| `playbooks/README.md` | EDIT | §4: inventory row for the new playbook |
| `docs/architecture/observability.md` | EDIT | §2–§4: runner collector built for traces and metrics (logs not yet), stage attributes, edge `Server-Timing` |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — UFS (stage names and attribute keys are named constants in `semconv`), TIM (every duration is measured on one process's monotonic clock; no cross-process clock subtraction), FLL, NDC, ORP, TST-NAM, ARCH (attribute keys consult `observability.md` first), TSC.
- `.orly/docs/LOGGING_STANDARD.md` — stage timing is a span attribute, not a new log line.
- `.orly/dispatch/write_rust.md` and `docs/RUST_ERROR_STANDARD.md` — the stage clock adds no fallible public signature; any that appears uses `afd_core::error_shell!`.
- `.orly/dispatch/write_ts_adhere_bun.md` — route and hook edits.
- `.orly/dispatch/write_shell.md` — `service.sh`, `deploy.sh` and playbook edits.
- `.orly/dispatch/name_architecture.md` — every attribute key is checked against `docs/architecture/observability.md` before it is spelled.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UFS GATE | yes — stage names, attribute keys, the collector port | Named constants in `semconv` and the stage clock; the port is one variable in `service.sh` |
| LOGGING GATE | yes — handlers on the logging path | No new log lines |
| UI GATE / DESIGN TOKEN GATE | no — no rendered component changes | `performance` marks only |
| SCHEMA GUARD | no — no schema change | Attribute joins replace any trace-context column |
| Architecture consult | yes — new attribute keys; the collector's built status | Recorded in Discovery; `observability.md` updated in the same PR |
| File & Function Length (≤350/≤50/≤70) | yes — `pull/mod.rs` and `report/mod.rs` are near the cap | The clock is passed in; each call site adds one mark |
| Harness edit (`check_workflow_rules.py`) | yes — §0 changes the value the rule pins | Owner approval recorded in Discovery; the dead-`LOG_LEVEL` ban is unchanged |

## Prior-Art / Reference Implementations

- **Reference:** `fleet.delivery` (`rustd/crates/afd_observability/src/delivery/mod.rs`) — correlation identity on a span, never on a metric; this spec follows it.
- **Reference:** the runner's `Timeline`/`Segments` (`afr_supervisor/src/lease_loop/segments.rs`) and `DeliveryStage` — existing stage vocabularies, reused rather than duplicated.
- **Reference:** W3C `Server-Timing` (https://www.w3.org/TR/server-timing/) — the browser reads `PerformanceResourceTiming.serverTiming` on a same-origin response.
- **Reference:** nftables `socket cgroupv2` matching (https://wiki.nftables.org/wiki-nftables/index.php/Matching_packet_metainformation) — the standard way to admit a local socket by its owning cgroup.

## Sections (implementation slices)

### §0 — Dev runs where latency is measurable — IN_PROGRESS

Every measurement this spec enables is meaningless on a machine frozen in 80 ms slices. A shared vCPU's baseline is 5 ms per 80 ms; the dev daemon rests at about 7–8% of a core, so on `shared-cpu-1x` its burst balance stayed empty and every request was throttled. Dev moves to `shared-cpu-2x` 1 GB (12.5% baseline) and logs at `info`. The Dragonfly downsizing and the rule edit ride along as owner-requested cost and hygiene changes; they are not part of the stage-timing design. The live machines were resized on Oct 26, 2026; this Section makes the deploy configuration agree.

- **Dimension 0.1** — DONE — Dev deploys set `AGENTSFLEET_LOG_LEVEL="info"`, the rule pins it, and the dead `LOG_LEVEL` spelling stays refused → Test `test_moved_rules_reject_seeded_violations`
- **Dimension 0.2** — DONE — Both dev `fly.toml` sizes match the live machines → Test `manual_dev_sizes_match_live`
- **Dimension 0.3** — After an hour at rest the dev daemon's burst balance stays above zero and throttle time is zero → Test `manual_dev_daemon_unthrottled_at_rest`

### §1 — The daemon names every stage an event crosses

Five verbs carry one event: steer (`POST …/fleets/{fleet_id}/messages`), lease (`POST /v1/runners/me/leases`), hydrate and capture (`/v1/runners/me/memory/{fleet_id}`), and report (`POST /v1/runners/me/reports`). Each handler creates a stage clock, passes it into the domain call, and writes the reached stages to the current request span as `agentsfleet.<verb>.<stage>_ms`, with `agentsfleet.event.id` and `gen_ai.agent.id` (the fleet). Stages: steer `auth, admit, announce`; lease `claim, record, provider, gates, issue, billing_hold, thread`; hydrate `fence, memory_read`; capture `fence, memory_write`; report `load, settle, billing_capture, billing_release, announce`. Keys are qualified by verb, so no name means two things on spans sharing an event. **Implementation default:** attributes are set with `tracing_opentelemetry::OpenTelemetrySpanExt::set_attribute` on the current span, because `trace.rs` is the generic layer every route passes through and must not learn the fleet domain's vocabulary; tests read spans from an in-memory OpenTelemetry exporter. No metric family is added: the goal is per event, which only spans answer, and the existing `DeliveryStage` aggregates stay as they are.

- **Dimension 1.1** — Each verb writes each stage it reached exactly once, and nothing for stages it did not reach → Test `stage_clock_writes_each_reached_stage_once`
- **Dimension 1.2** — A refused or empty lease poll writes `claim` only and no event identity → Test `empty_poll_writes_claim_without_identity`
- **Dimension 1.3** — A real admit-lease-report cycle against live Postgres yields steer, lease and report spans carrying the same `agentsfleet.event.id` → Test `event_identity_joins_steer_lease_and_report_spans`
- **Dimension 1.4** — `trace.rs` is unchanged and its span still carries the route template, never a raw path → Test `request_span_carries_template_not_path`

### §2 — The runner's spans reach Grafana, and only the runner can send them

The runner already exports `runner.lease`, `invoke_agent`, `chat` and `execute_tool` when `OTEL_EXPORTER_OTLP_ENDPOINT` names a collector (M214_001). The collector was decided (Indy, Oct 04, 2026) and never built. This Section builds it on the bare-metal host for traces and runner metrics only; journald logs wait for the privacy rules `observability.md` requires. Under `allow_all` a sandbox shares the host's network namespace, so a plain loopback receiver would let tenant code post telemetry under the platform's credential. The receiver therefore listens on `127.0.0.1:4318` behind an nftables rule that accepts only sockets owned by the runner's own cgroup (`agentsfleet-runner.service/runner`, from `DelegateSubgroup=runner`); lease cgroups are siblings and are refused. `service.sh` refuses to start the collector if the rule cannot be loaded. `runner.lease` gains the existing stopwatch's `memory`, `sandbox`, `model`, `tools`, `report` and `first_token` as `agentsfleet.runner.<segment>_ms`, written once when the lease concludes; no new runner vocabulary and no new span. The collector binary is `otelcol-contrib` at the version `deploy/fly/otelcol-dev/Dockerfile` pins, fetched by `deploy.sh` and refused on a SHA-256 mismatch. **Prerequisite:** D1 re-confirmed against the corrected risk (Discovery). **PLAN check:** the dev host's kernel supports `socket cgroupv2` matching (`nft` loads the rule); if it does not, stop and surface it, because the receiver must not run unrestricted.

- **Dimension 2.1** — The runner unit's endpoint is loopback, the collector unit reads its credential from a root-only file, and the runner unit carries no Grafana variable → Test `runner_unit_targets_loopback_collector`
- **Dimension 2.2** — The rule admits the runner's cgroup and refuses any other on port 4318, and `service.sh` refuses to start the collector without it → Test `collector_rule_admits_only_the_runner_cgroup`
- **Dimension 2.3** — On the dev host, a sandbox under `allow_all` cannot deliver to `127.0.0.1:4318` → Test `manual_sandbox_cannot_post_to_collector`
- **Dimension 2.4** — `runner.lease` carries the segments the lease reached, written once at conclusion → Test `lease_span_records_existing_segments`
- **Dimension 2.5** — `deploy.sh` refuses an `otelcol-contrib` download whose SHA-256 differs from the pin → Test `collector_download_refused_on_checksum_mismatch`
- **Dimension 2.6** — On the dev host, a chat event's `runner.lease` span is queryable in Tempo by `agentsfleet.event.id` → Test `manual_runner_span_reaches_tempo`

### §3 — The edge reports its own cost on one clock

In the throttled measurement, 584 ms passed between the browser's Send and the daemon's first byte. The steer route times `credential()` and the upstream call and answers `Server-Timing: auth;dur=…, upstream;dur=…`. Network and tunnel time is `upstream` minus the daemon's steer span duration: two durations, each on one clock, joined by the event identifier the 202 returns. The browser sets `performance` marks `af:send`, `af:accepted`, `af:first-frame` and `af:complete`, keyed by event identifier. Nothing is added to the daemon's public request surface.

- **Dimension 3.1** — The steer route answers with `Server-Timing` naming `auth` and `upstream` → Test `steer_route_answers_server_timing`
- **Dimension 3.2** — A refused or failed upstream still answers with `auth` timing and an unchanged status and body → Test `server_timing_never_changes_the_answer`
- **Dimension 3.3** — The browser leaves the four marks for one send, in order → Test `delivery_marks_send_accepted_first_frame_complete`

### §4 — One timeline, one row, one procedure

The critical path, on the daemon's clock, tiles end to end; drill-downs nest inside one segment and are never added to the sum:

```
 browser Send ─ edge in (Server-Timing: auth, upstream − steer span) ─┐
 ┌─────────────────────── daemon clock: tiles, sums to the window ──────────────────────┐
 │ steer │ queue (event_to_lease) │ lease │ gap A │ hydrate │ gap B │ capture │ gap C │ report │
 └──────────────────────────────────────────────────────────────────────────────────────┘
   gap A, B, C = time between daemon verbs: runner work plus network
   drill-down inside gap B: runner model, tools, first_token (runner.lease)
   drill-down inside each verb: its agentsfleet.<verb>.<stage>_ms
 └─ edge out: event_complete publish → browser frame (browser af:complete − window − edge in)
```

The dashboard's round-trip row reads Tempo through a datasource variable (`tempo`), so no new vault field is needed. Its `fleet` and `event` variables list the five verb spans and `runner.lease` for one event and chart the tiled segments, next to the existing `DeliveryStage` panel. `002_round_trip_playbook.md` gives the procedure and the TraceQL, and points statement-level questions at PlanetScale Insights and the `afd_bench` lanes.

- **Dimension 4.1** — The asset check accepts the Tempo datasource variable and still refuses any other datasource → Test `assets_check_accepts_tempo_variable_only`
- **Dimension 4.2** — For one event the tiled segments sum to the daemon window by construction → Test `round_trip_segments_tile_the_daemon_window`
- **Dimension 4.3** — On dev, one probe's browser Send-to-`event_complete` minus the daemon window is within 10% of the edge legs `Server-Timing` and the marks report → Test `manual_round_trip_edges_account_for_the_rest`

## Interfaces

```
Daemon request span attributes (set by the five handlers; trace.rs unchanged):
  agentsfleet.event.id, gen_ai.agent.id
  agentsfleet.steer.{auth,admit,announce}_ms
  agentsfleet.lease.{claim,record,provider,gates,issue,billing_hold,thread}_ms
  agentsfleet.hydrate.{fence,memory_read}_ms
  agentsfleet.capture.{fence,memory_write}_ms
  agentsfleet.report.{load,settle,billing_capture,billing_release,announce}_ms

Runner span attributes (runner.lease, written once at conclusion):
  agentsfleet.runner.{memory,sandbox,model,tools,report,first_token}_ms

Runner collector (bare-metal host):
  receiver   127.0.0.1:4318 OTLP/HTTP, traces and metrics only
  admission  nftables: tcp dport 4318 to loopback accepted only from
             socket cgroupv2 agentsfleet-runner.service/runner; others dropped
  credential root-only environment file, from op://ZMB_CD_DEV/grafana-dev/*

Edge (steer route → browser):
  Server-Timing: auth;dur=<ms>, upstream;dur=<ms>
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Sandbox posts telemetry | Tenant code under `allow_all` connects to `127.0.0.1:4318` | nftables drops the connection; the collector never sees it; proven by `manual_sandbox_cannot_post_to_collector` |
| Rule cannot load | Host kernel lacks `socket cgroupv2` matching | `service.sh` refuses to start the collector and names the rule; the runner runs with export disabled and logs `telemetry_export_disabled` |
| Collector down | Collector stopped or crashed | Runner keeps running; `agentsfleet_runner_otlp_entries_discarded_total` rises |
| Credential missing on host | Environment file absent | Collector unit fails to start naming the file; runner unaffected |
| Tampered collector download | Release asset changed | `deploy.sh` refuses on SHA-256 mismatch; the previous binary stays |
| Exit before a stage | Refusal, parked work, stale fence | Only reached stages are written |
| Upstream failure at the edge | Daemon unreachable or refuses | Route answers its existing status and body plus `auth` timing |
| Span queue overflow | Burst beyond `SPAN_QUEUE` | Attributes ride existing spans; overflow behaviour is unchanged |

## Invariants

1. No identifier becomes a metric label — no metric family is added; the census grader is unchanged.
2. Timing never changes an HTTP answer — `server_timing_never_changes_the_answer` compares status and body with and without timing.
3. The runner holds no observability credential — `afr_telemetry`'s refusal of header knobs stays, and `runner_unit_targets_loopback_collector` asserts no Grafana variable in the runner unit.
4. Only the runner's cgroup can deliver to the collector — `collector_rule_admits_only_the_runner_cgroup` and the manual sandbox test.
5. No new span names, and `trace.rs` is unchanged — the runner's span-name allowlist test and `request_span_carries_template_not_path` stay green.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| Daemon verb stage attributes | ops | A verb's request ends | Event id, fleet id, stage durations | No prompt, reply, path or SQL text | `event_identity_joins_steer_lease_and_report_spans` |
| `runner.lease` segment attributes | ops | A lease concludes | Segment durations | No prompt or reply | `lease_span_records_existing_segments` |
| Edge `Server-Timing` | ops | Every steer response | `auth`, `upstream` durations | No identifiers in the header | `steer_route_answers_server_timing` |

No metric family is added or changed, and no PostHog event changes, so no analytics funnel playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 0.1 | unit | `test_moved_rules_reject_seeded_violations` | Fixture with `export AGENTSFLEET_LOG_LEVEL="info"` passes; `LOG_LEVEL="info"` fails rule `workflow-daemon-log-level` |
| 0.2 | manual | `manual_dev_sizes_match_live` | `fly scale show` reads `shared` 2 CPUs 1024 MB for `agentsfleetd-dev` and 1 CPU 1024 MB for `dragonfly-dev`, matching both `fly.toml` files |
| 0.3 | manual | `manual_dev_daemon_unthrottled_at_rest` | One hour after resize, `rate(fly_instance_cpu_throttle{app="agentsfleetd-dev"}[30m])` = 0 and `fly_instance_cpu_balance` > 0; values in Session Notes |
| 1.1 | unit | `stage_clock_writes_each_reached_stage_once` | A report that settles → each of five report stages written once; a report refused at `load` → only `load` written |
| 1.2 | unit | `empty_poll_writes_claim_without_identity` | Poll with no unsettled admission → only `agentsfleet.lease.claim_ms`; no `agentsfleet.event.id` |
| 1.3 | integration | `event_identity_joins_steer_lease_and_report_spans` | Live Postgres: steer, lease, report → three exported spans share one `agentsfleet.event.id` |
| 1.4 | unit | `request_span_carries_template_not_path` | Request with real identifiers → route attribute is the template; no identifier in any attribute but `agentsfleet.event.id` and `gen_ai.agent.id` |
| 2.1 | unit | `runner_unit_targets_loopback_collector` | Rendered unit → endpoint `http://127.0.0.1:4318`; no `GRAFANA_` variable; collector env file mode 0600 owner root |
| 2.2 | unit | `collector_rule_admits_only_the_runner_cgroup` | Rendered rule matches `socket cgroupv2` on the runner path for dport 4318 and drops the rest; `service.sh` with a failing `nft` load → exit non-zero, collector not started |
| 2.3 | manual | `manual_sandbox_cannot_post_to_collector` | On the dev host, a lease under `allow_all` runs `curl -m 2 http://127.0.0.1:4318/v1/traces` → connection refused or timed out; collector receives nothing; output in Session Notes |
| 2.4 | unit | `lease_span_records_existing_segments` | Lease with memory 12 ms, model 400 ms, report 30 ms → those three fields set once; `sandbox` and `tools` unset |
| 2.5 | unit | `collector_download_refused_on_checksum_mismatch` | Stubbed download with a wrong digest → exit non-zero, previous binary untouched |
| 2.6 | manual | `manual_runner_span_reaches_tempo` | Dev chat; TraceQL `{ span.agentsfleet.event.id = "<id>" }` returns `runner.lease`; screenshot in Session Notes |
| 3.1 | unit | `steer_route_answers_server_timing` | Stubbed credential 12 ms, upstream 40 ms → header names `auth` and `upstream` with durations ≥ those |
| 3.2 | unit | `server_timing_never_changes_the_answer` | Upstream refuses 409 or is unreachable → same status and body as without timing; `auth` present |
| 3.3 | unit | `delivery_marks_send_accepted_first_frame_complete` | Simulated send → four marks for one event id, in order |
| 4.1 | unit | `assets_check_accepts_tempo_variable_only` | Panel with `${tempo}` passes; panel with any other datasource UID fails naming it |
| 4.2 | unit | `round_trip_segments_tile_the_daemon_window` | Fixture spans for one event → steer, queue, lease, gaps, hydrate, capture, report sum to report end minus steer start exactly |
| 4.3 | manual | `manual_round_trip_edges_account_for_the_rest` | Indy or Aiwa runs one dev probe; browser total minus daemon window within 10% of edge in plus edge out; numbers in Session Notes |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R0 | Dev unthrottled at rest and deploying at `info` (§0) | `make check-gh-actions-valid` | exit 0 | P0 | |
| R1 | Daemon verbs write their reached stages and event identity (§1) | `cargo test -p afd_observability stage_clock` | exit 0 | P0 | |
| R2 | Only the runner can deliver to the collector (§2) | `bash deploy/baremetal/service_test.sh` | exit 0 | P0 | |
| R3 | Runner spans reach Tempo from the dev host (§2) | `manual: TraceQL { span.agentsfleet.event.id = "<probe id>" } in Grafana Tempo` | ≥1 `runner.lease` span | P0 | |
| R4 | Edge cost reaches the browser on one clock (§3) | `cd ui/packages/app && bun run test steer-route-timing` | exit 0 | P0 | |
| R5 | Dashboard asset valid; the timeline tiles (§4) | `bash playbooks/operations/observability/01_assets_check.sh` | exit 0 | P0 | |
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

- **Runner logs through the collector.** journald forwarding needs the fail-closed privacy rules in `observability.md` (field allowlist, drop on parse failure, numeric limits, allowlist proof); a follow-up spec carries them.
- **Per-request SQL statement counts.** A `tracing` layer cannot see `sqlx::query` events at `info` because the daemon's level filter is global (`agentsfleetd/src/logs.rs`); statement-level questions use PlanetScale Insights and the `afd_bench` lanes.
- **New metric families for stages.** Per-event questions are answered by spans; aggregates wait for a production need.
- **Making the round trip faster.** Measured candidates (resting lease-poll cost, transaction count, hydrate folded into the lease, capture skipped when nothing changed, three report transactions) need this spec's numbers first.
- **Browser telemetry export** — decision D2; marks stay in DevTools.
- **W3C `traceparent` across the runner protocol** — `observability.md` §Traces keeps leases as separate roots joined by attribute.
- **Production runner collector** — dev host only; production follows the same units once a production runner exists.
- **The "access needs attention" banner** — a separate product change.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy picks `dev-acceptance-20261021-0517` and its last event and reads "lease 210 ms: billing_hold 40 ms; gap B 640 ms: model 560 ms" without opening a terminal.
2. **Preserved user behaviour** — Chat send, reply, retry and every API answer are byte-identical; the steer route adds one response header; the browser gains marks, not markup.
3. **Optimal-way check** — The direct shape is one distributed trace per event; the architecture keeps leases as separate roots, so attribute joins reach the same view without changing the runner protocol.
4. **Rebuild-vs-iterate** — Iterate: attributes on existing spans, existing stopwatches recorded, an already-decided collector built.
5. **What we build** — Daemon stage attributes, the runner collector restricted to the runner's cgroup, runner segment attributes, edge `Server-Timing` and marks, one dashboard row, one playbook.
6. **What we do NOT build** — Log forwarding, SQL tallies, stage metric families, performance fixes, browser export, cross-protocol trace context, a production collector.
7. **Fit with existing features** — Compounds `fleet.delivery`, `DeliveryStage` and the runner's `Segments`; must not destabilise the runner span budget or the generic request span.
8. **Surface order** — Grafana first; no CLI surface, because the data lives in Grafana and the playbook names the queries.
9. **Dashboard restraint** — No percentile alert and no target line until a provisioned capture exists (`observability.md`: "nothing gates on a percentile").
10. **Confused-user next step** — `002_round_trip_playbook.md` names the row, the variables and the TraceQL to paste.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Sections by process boundary (daemon, runner host, edge, dashboard), so each lands and proves alone. §0 carries owner-requested dev changes (sizing, log level, Dragonfly cost, the rule edit) that are not part of the stage-timing design.
- **Alternatives considered (rejected):**
  - **Child spans per stage:** stage spans would evict error spans from the budget.
  - **Stage metric families:** they would duplicate the span attributes, and the goal is per event.
  - **A SQL tally layer:** it is blind at `info`, and would cost a debug event per statement on the machine being measured.
  - **Edge-start headers on the public API:** they would compare two vendors' clocks and accept untrusted input.
  - **Fields declared in `trace.rs`:** the generic router would learn the fleet domain's vocabulary.
  - **A trace-context column on `fleet.admissions`:** a schema change for a join that attributes already provide.
- **Patch-vs-refactor verdict:** this is a **patch** because it adds attributes to existing spans, records existing stopwatches, and builds a collector already decided.

## Discovery (consult log)

- **Consults** — `docs/architecture/observability.md` read for §Traces, §runner collector, §PostHog (Oct 26, 2026). Indy (Oct 26, 2026), direct: "I want you to have logging, observability added to so we could to this as a fleet for me to check it and optimize". Agent reading: per-fleet, per-event stage breakdown in Grafana. Throttled baseline (event `01a17627-25c5-761d-9794-f1ab0b40d9fc`, 03:00:38 UTC): Send → 202 0.78 s (daemon 50 ms); claim → runner holds lease 1.51 s across 36 statements; hydrate 0.33 s; model window ≤ 0.64 s; capture 0.39 s; report 1.60 s across 28 statements; `event_complete` at +5.42 s. Cause measured the same day: CPU quota throttling on `shared-cpu-1x`, not the database, the pooler or Nagle. PlanetScale Insights (branch `useast`) shows `insert into billing.holds` at p50/p99 0 ms server-side. An in-region probe reproduced the floor with `TCP_NODELAY` on and off, on ports 6432 and 5432. With no database at all, 169 of 2000 idle 1 ms sleeps woke 70–76 ms late, and 1988 of 2000 under one busy thread. On `performance-1x`: 0 of 8,610 probe statements ≥ 40 ms; three chats 1.53–1.86 s. On `shared-cpu-2x` after §0: three chats 1.53–1.94 s (05:11 UTC).
- **Design review (egm Stage 5, Oct 26, 2026)** — 2 Blockers, 7 Majors, 5 Nits, report `egm-review/zoom_out_report.md`; Indy: "Okay do that". This revision:
  - drops the SQL tally (blind at `info`: `logs.rs:126` filter is global);
  - restricts the collector to the runner's cgroup (under `allow_all` sandboxes share the host network: `runner_execution.md` Facts);
  - reuses `Segments` and `DeliveryStage`;
  - adds the tiled timeline;
  - uses span attributes only;
  - leaves `trace.rs` unchanged;
  - replaces edge headers with one-clock `Server-Timing`;
  - updates Problem and Priority;
  - removes journald;
  - corrects file paths, qualifies stage names, reconciles Dimension wording, and pins the collector download.
- **Required human decisions** — **D1:** the collector reuses `op://ZMB_CD_DEV/grafana-dev/{otlp-endpoint,instance-id,api-key}`. Indy (Oct 26, 2026): "Reuse grafana-dev". The original risk statement ("one leaked host exposes the daemon's ingest key") understated it: without a restriction, any `allow_all` lease could have written under that key. With the cgroup rule the residual risk is root compromise of the runner host, which then holds a key that also writes the daemon's data in a shared dev/prod tenant. **Re-confirmation against this corrected risk is pending and blocks §2.** **D2:** browser telemetry export, or DevTools marks only (agent recommends marks only). Does not block. **§0 decisions (Oct 26, 2026), direct from Indy:**
  - "I am okay with the bottleneck in dev, but i need you to confirm the reason, so i can focus on that in production".
  - "For development also strip down the debug level in development … bring down the sizing of dragonfly as well as it costs 60$".
  - Asked to pin `info` in rule `workflow-daemon-log-level` of `scripts/check_workflow_rules.py`: "Yes, pin info (Recommended)". This is the gate-edit approval, superseding the M187 call to deploy dev at `debug`.
  - Asked to move the dev daemon: "Yes, shared-cpu-2x 1GB (Recommended)".
- **Metrics review** — no metric family added; span attributes only; no analytics funnel playbook update required, because no product event changes.
- **Skill-chain outcomes** — egm review: Stage 5 complete (above). Stage 6 (comprehension) ran against the pre-revision text, and its findings are mapped onto this revision when its report lands.
- **Deferrals** — none.
