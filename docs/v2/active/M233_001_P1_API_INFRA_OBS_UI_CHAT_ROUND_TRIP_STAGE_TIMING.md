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

# M233_001: Every chat event's round trip is broken into named stages, per fleet, in Grafana

**Prototype:** v2.0.0
**Milestone:** M233
**Workstream:** 001
**Date:** Oct 26, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing: the production daemon size (three `shared-cpu-4x` 4 GB machines at about $80 a month, or three `performance-2x` 4 GB at $198 a month; source in Discovery) is chosen without per-event evidence of where a chat's time goes
**Categories:** API, INFRA, OBS, UI
**Batch:** B1 — single workstream; Section prerequisites are stated in each Section
**Branch:** feat/m233-round-trip-stage-timing
**Baseline revision:** d6d279cfde5f3c6ec5eb75b088fa1d2bac1af371
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 26, 2026), from timing measurements taken on dev (Discovery, Consults); revised after the Stage 5 and Stage 7 design reviews (`egm-review/`)
**Canonical architecture:** `docs/architecture/observability.md` §Traces, §"`agentsfleet-runner` — a collector of its own", §"Signal routing"

---

## Overview

**Goal (testable):** For N consecutive dev chat probes, (1) the steer, lease, hydrate, capture and report spans and the runner's `runner.lease` span are all found for each event; (2) in every verb span, span duration minus the sum of its stage attributes is under the larger of 10% and 5 ms; and (3) browser `af:accepted − af:send` ≥ route `auth + upstream` ≥ daemon steer span duration. Edge out (the `event_complete` frame's trip to the browser) is shown as a labelled remainder and is not checked.

**Problem:** An operator cannot see where a chat's 1.5–1.9 s goes. Three dev chats took 1.53–1.94 s from Send to `event_complete` on Oct 26, 2026 at 05:11 UTC. Splitting one chat into stages took debug-level SQL logs, Postgres timestamps, a browser console and a shell on the runner host. Grafana shows each daemon request as an unrelated span, nothing from the runner, and nothing from the Next.js steer route.

**Solution summary:** Today the daemon's request spans carry no event identity, the runner's stage stopwatch reaches only its host's journal, and the edge reports nothing. This spec adds:
- **Daemon:** the handlers for the five daemon calls a chat passes through (steer, lease, hydrate, capture, report) write the event or fleet identity and per-stage durations onto the request span. A span layer installed outside the reload slot carries them to the exporter. No metric family is added.
- **Runner host:** the runner collector, whose design `observability.md` records as decided (Oct 4, 2026), is built for traces and runner metrics. An nftables rule admits only root-owned host processes, which excludes sandboxes. The runner's existing stopwatch is written to `runner.lease`.
- **Edge:** the Next.js steer route returns `Server-Timing`, and the browser records `performance` marks.
- **Dashboard:** the observability dashboard gains a round-trip row read from Tempo.

§0 carries owner-requested dev changes (sizing, log level, Dragonfly downsizing, a rule edit) that are not part of the stage-timing design.

**Risks and limits:**
- **Collector key:** the key sits on a multi-tenant host whose sandboxes share one kernel (D1, Discovery).
- **Admission rule:** it depends on sandboxes running under a non-root host user (§2 PLAN check).
- **Collector resources:** they share the runner host with tenant workloads (§2 limits).
- **Clocks:** dev runs one daemon machine. Production gaps rely on clock agreement across machines (each span shows `service.instance.id`).
- **Scale:** the per-event view lasts as long as Tempo keeps traces, and event lookup is built for dev scale, not production volume.

## PR Intent & comprehension handshake

- **PR title (eventual):** Break each chat's round trip into stages, per fleet, in Grafana
- **Intent (one sentence):** Indy (project owner) can open one dashboard, pick a fleet and a chat event, and see where the time between Send and the reply went, so optimisations and production sizing are chosen from evidence.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/observability.md` — §Traces (leases are separate root spans joined by attribute; the daemon has no span budget, only `SPAN_QUEUE`), §"`agentsfleet-runner` — a collector of its own", and §"Signal routing" (the fail-closed rules for runner logs, which this spec leaves out).
2. `rustd/crates/agentsfleetd/src/logs.rs` — the subscriber layout; the OpenTelemetry span bridge sits in a `reload` slot that refuses downcasts, which is why §1 moves the span layer out of it.
3. `rustd/crates/afr_supervisor/src/lease_loop/segments.rs` and `rustd/crates/afd_observability/src/metrics/label/fleet.rs` — the existing stage names: the runner's `Segments` and the daemon's four `DeliveryStage` members. They are reused, not duplicated.
4. `rustd/crates/afr_sandbox/src/bubblewrap_engine/mod.rs` — sandboxes run under `sandbox_ids` when the runner is root, which the collector's admission rule depends on.
5. `deploy/baremetal/agentsfleet-runner.service` and `deploy/fly/otelcol-dev/config.yml` — the runner unit the collector sits beside, and the collector configuration to copy, minus its public binds and logs pipeline.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `deploy/fly/agentsfleetd-dev/fly.toml` | EDIT | §0: dev daemon `shared-cpu-2x` 1 GB |
| `deploy/fly/dragonfly-dev/fly.toml` | EDIT | §0: dev Dragonfly `shared-cpu-1x` 1 GB |
| `.github/workflows/deploy-dev-fly.yml` | EDIT | §0: dev deploys set `AGENTSFLEET_LOG_LEVEL="info"` |
| `scripts/check_workflow_rules.py` | EDIT | §0: rule `workflow-daemon-log-level` pins `info` (owner-approved gate edit) |
| `scripts/check_workflow_rules_test.py` | EDIT | §0: fixture and seeded violation follow the pinned value |
| `docs/architecture/scaling.md` | EDIT | §0: dev cluster size; dev database region once verified |
| `rustd/crates/agentsfleetd/src/logs.rs` | EDIT | §1: install the `tracing_opentelemetry` span layer permanently, over a global-provider tracer, outside the reload slot |
| `rustd/crates/agentsfleetd/src/telemetry/mod.rs` | EDIT | §1: `attach` installs the global tracer provider |
| `rustd/crates/afd_observability/Cargo.toml` | EDIT | §1: add `tracing-opentelemetry` (workspace 0.34.0) |
| `rustd/crates/afd_observability/src/stage_clock.rs` | CREATE | §1: per-request stage clock over closed per-verb stage enums; writes `agentsfleet.<verb>.<stage>_ms` to the current span |
| `rustd/crates/afd_observability/src/lib.rs` | EDIT | §1: export the stage clock |
| `rustd/crates/afd_observability/src/semconv/mod.rs` | EDIT | §1: the attribute keys, spelled once |
| `rustd/crates/afd_api_tenant/src/handler/fleet/message_steer/mod.rs` | EDIT | §1: steer stages and event identity |
| `rustd/crates/afd_api_runner/src/handler/runner/lease/mod.rs` | EDIT | §1: lease stages and the issued event's identity |
| `rustd/crates/afd_api_runner/src/handler/runner/memory.rs` | EDIT | §1: hydrate and capture stages and fleet identity |
| `rustd/crates/afd_api_runner/src/handler/runner/report.rs` | EDIT | §1: report stages and the reported event's identity |
| `rustd/crates/afd_fleet/src/lease/pull/mod.rs` | EDIT | §1: lease stage marks on the passed clock (boundaries fixed at Stage 9) |
| `rustd/crates/afd_fleet/src/lease/pull/poll.rs` | EDIT | §1: lease stage marks (boundaries fixed at Stage 9) |
| `rustd/crates/afd_fleet/src/lease/deliver/mod.rs` | EDIT | §1: billing hold, issue and thread marks |
| `rustd/crates/afd_fleet/src/lease/report/mod.rs` | EDIT | §1: load, settle, billing capture, billing release, announce marks |
| `rustd/crates/afr_supervisor/src/lease_loop/segments.rs` | EDIT | §2: write the existing segments to `runner.lease` when the lease concludes |
| `rustd/crates/afr_supervisor/src/identity/mod.rs` | EDIT | §2: `runner.lease` declares the segment fields as `Empty` |
| `deploy/baremetal/otelcol-runner.yml` | CREATE | §2: loopback OTLP receiver, traces and metrics pipelines, limits, Grafana exporter; `health_check` on loopback; no logs pipeline |
| `deploy/baremetal/otelcol-runner.service` | CREATE | §2: systemd unit with hardening and `MemoryMax=`; credential from a root-only environment file |
| `deploy/baremetal/otelcol-runner.nft` | CREATE | §2: loopback port 4318 accepted only from uid 0 |
| `deploy/baremetal/agentsfleet-runner.service` | EDIT | §2: `OTEL_EXPORTER_OTLP_ENDPOINT` names the loopback collector |
| `deploy/baremetal/service.sh` | EDIT | §2: load the rule; refuse to start the collector without it |
| `deploy/baremetal/service_test.sh` | EDIT | §2: unit, rule and endpoint assertions |
| `deploy/baremetal/deploy.sh` | EDIT | §2: fetch `otelcol-contrib` at the pinned version and verify its SHA-256 |
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
| `playbooks/operations/observability/002_round_trip_playbook.md` | CREATE | §4: how to read one event's round trip, with the tiling script fallback |
| `playbooks/README.md` | EDIT | §4: inventory row for the new playbook |
| `docs/architecture/observability.md` | EDIT | §1–§4: span layer placement, runner collector built for traces and metrics (logs not yet), stage attributes, edge `Server-Timing` |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`**, applied as follows:
  - **UFS:** stage names and attribute keys are named constants in `semconv`.
  - **FLL:** file and function length limits; the clock is passed in, so call sites stay short.
  - **NDC:** no dead code.
  - **ORP:** orphan sweep on every rename.
  - **TST-NAM:** test names carry no milestone ID.
  - **ARCH:** attribute keys consult `observability.md` before they are spelled.
  - **TSC:** TypeScript conventions in the route and hook.
- `.orly/docs/LOGGING_STANDARD.md` — stage timing is a span attribute, not a new log line.
- `.orly/dispatch/write_rust.md` and `docs/RUST_ERROR_STANDARD.md` — the stage clock adds no fallible public signature; any that appears uses `afd_core::error_shell!`.
- `.orly/dispatch/write_ts_adhere_bun.md` — route and hook edits.
- `.orly/dispatch/write_shell.md` — `service.sh`, `deploy.sh` and playbook edits.
- `.orly/dispatch/name_architecture.md` — every attribute key is checked against `docs/architecture/observability.md` before it is spelled.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UFS GATE | yes — stage names, attribute keys, the collector port | Named constants in `semconv` and the stage clock; the port is one variable in `service.sh` |
| LOGGING GATE | yes — handlers and `logs.rs` on the logging path | No new log lines; the span layer moves, the log layers do not |
| UI GATE / DESIGN TOKEN GATE | no — no rendered component changes | `performance` marks only |
| SCHEMA GUARD | no — no schema change | Identity joins by attribute and time window |
| Architecture consult | yes — new attribute keys; span layer placement; collector status | Recorded in Discovery; `observability.md` updated in the same PR |
| File & Function Length (≤350/≤50/≤70) | yes — `pull/mod.rs` and `report/mod.rs` are near the cap | The clock is passed in; each call site adds one mark |
| Harness edit (`check_workflow_rules.py`) | yes — §0 changes the value the rule pins | Owner approval recorded in Discovery; the dead-`LOG_LEVEL` ban is unchanged |

## Prior-Art / Reference Implementations

- **Reference:** `fleet.delivery` (`rustd/crates/afd_observability/src/delivery/mod.rs`) — correlation identity on a span, never on a metric. It builds its span with the OpenTelemetry SDK directly; §1 instead reaches the exporter through the `tracing` layer.
- **Reference:** the runner's `Timeline`/`Segments` and the daemon's `DeliveryStage` — existing stage names, reused.
- **Reference:** W3C `Server-Timing` (https://www.w3.org/TR/server-timing/) — the browser reads `PerformanceResourceTiming.serverTiming` on a same-origin response.
- **Reference:** nftables `meta skuid` matching (https://wiki.nftables.org/wiki-nftables/index.php/Matching_packet_metainformation) — admitting a local socket by its owner's user ID, which survives service restarts.

## Sections (implementation slices)

### §0 — Dev runs where latency is measurable — IN_PROGRESS

Every measurement this spec enables is meaningless on a machine frozen in 80 ms slices. A shared vCPU's baseline is 5 ms per 80 ms; the dev daemon rests at about 7–8% of a core, so on `shared-cpu-1x` its burst balance stayed empty and every request was throttled. Dev moves to `shared-cpu-2x` 1 GB (12.5% baseline) and logs at `info`. The Dragonfly downsizing and the rule edit are owner-requested cost and hygiene changes. The live machines were resized on Oct 26, 2026; this Section makes the deploy configuration agree.

- **Dimension 0.1** — DONE — Dev deploys set `AGENTSFLEET_LOG_LEVEL="info"`, the rule pins it, and the dead `LOG_LEVEL` spelling stays refused → Test `test_moved_rules_reject_seeded_violations`
- **Dimension 0.2** — DONE — Both dev `fly.toml` sizes match the live machines → Test `manual_dev_sizes_match_live`
- **Dimension 0.3** — After an hour at rest, neither `agentsfleetd-dev` nor `dragonfly-dev` shows throttle time, and both keep a burst balance above zero → Test `manual_dev_machines_unthrottled_at_rest`

### §1 — The daemon names every stage a chat crosses

**Prerequisite:** none.

The chat's five daemon calls are steer (`POST …/fleets/{fleet_id}/messages`), lease (`POST /v1/runners/me/leases`), hydrate and capture (`/v1/runners/me/memory/{fleet_id}`), and report (`POST /v1/runners/me/reports`). Renew, activity, tool-call, credential-mint and recall calls stay uninstrumented; their time falls inside gaps B and C (§4). Each handler creates a stage clock, passes it into the domain call, and writes the reached stages to the current request span as `agentsfleet.<verb>.<stage>_ms`.
- **Identity:** steer, lease and report carry `agentsfleet.event.id`; all five carry `gen_ai.agent.id` (the fleet). Hydrate and capture carry no lease or event (`afd_api_runner/src/handler/runner/memory.rs`). They join an event as the fleet's hydrate and capture spans between that event's lease span end and its report span start. The tiled capture is the last one before the report; earlier mid-run captures are drill-downs inside gap B.
- **Stages:** steer `auth, admit, announce`; lease `claim, record, provider, gates, issue, billing_hold, thread`; hydrate `fence, memory_read`; capture `fence, memory_write`; report `load, settle, billing_capture, billing_release, announce`. Each stage's exact code boundaries are fixed at Stage 9.

**Implementation default:** `logs.rs` installs the `tracing_opentelemetry` span layer permanently, outside the reload slot, over a tracer obtained from the OpenTelemetry global provider. `attach` sets that provider instead of filling the slot with the span bridge; the log bridge stays in the slot. Reason: `set_attribute` locates the bridge by downcasting the subscriber, and the `reload` layer refuses every downcast (`tracing-subscriber-0.3.23/src/reload.rs:188-204`), so attributes set through it are dropped. **Fallback**, if the PLAN check below fails: declare a fixed set of `Empty` fields from `semconv` on the request span in `trace.rs`, and fill them with `Span::record`. **PLAN check:** an attribute set inside a handler reaches an in-memory exporter through `logs::install` plus `attach` (the real subscriber).

- **Dimension 1.1** — Each verb writes each stage it reached exactly once, and nothing for stages it did not reach → Test `stage_clock_writes_each_reached_stage_once`
- **Dimension 1.2** — A refused or empty lease poll writes `claim` only and no event identity → Test `empty_poll_writes_claim_without_identity`
- **Dimension 1.3** — A real chat cycle against live Postgres yields steer, lease and report spans with one `agentsfleet.event.id`, and hydrate and capture spans joinable by fleet and time window → Test `event_identity_joins_the_five_verb_spans`
- **Dimension 1.4** — `trace.rs`'s span still carries the route template, never a raw path → Test `request_span_carries_template_not_path`
- **Dimension 1.5** — An attribute set inside a handler reaches the exporter through the production subscriber → Test `handler_attribute_reaches_exporter_through_real_subscriber`

### §2 — The runner's spans reach Grafana, and only root-owned host processes can send them

**Prerequisite:** D1 decided (Discovery). This Section is new infrastructure on a multi-tenant host, not a patch.

The runner already exports `runner.lease`, `invoke_agent`, `chat` and `execute_tool` when `OTEL_EXPORTER_OTLP_ENDPOINT` names a collector (M214_001). This Section builds the collector on the bare-metal host for traces and runner metrics only.
- **Admission:** under `allow_all` a sandbox shares the host's network namespace, so the receiver on `127.0.0.1:4318` sits behind an nftables output rule. Loopback TCP to 4318 is accepted only from sockets owned by uid 0 (the runner and other root host processes) and dropped for any other uid. Sandboxes run under `sandbox_ids` when the runner is root (`afr_sandbox/src/bubblewrap_engine/mod.rs`), so they are refused. The match depends on no cgroup, so it survives runner restarts. `service.sh` refuses to start the collector if the rule does not load.
- **Limits:** `memory_limiter` at 200 MiB with a 50 MiB spike allowance, a `sending_queue` of 1,000 batches, retry giving up after 300 s, and systemd `MemoryMax=256M`. These are defaults, confirmed at PLAN against the host's free memory.
- **Unit:** runs as a dedicated system user, with `NoNewPrivileges=`, `ProtectSystem=strict`, `ProtectHome=` and `PrivateTmp=`. It starts after `network-online.target`. Its credential is read from `/etc/agentsfleet/otelcol-runner.env`, mode 0600, owner root. `health_check` binds `127.0.0.1:13133`. There is no logs pipeline.
- **Binary:** `otelcol-contrib` at an explicit release recorded in `deploy.sh` (version, plus the SHA-256 of its `linux_amd64` tarball from the release's checksums file). `otelcol-dev` uses `:latest`, so there is no version to copy.
- **Runner span:** `runner.lease` gains the existing stopwatch's `memory`, `sandbox`, `model`, `tools`, `report` and `first_token` as `agentsfleet.runner.<segment>_ms`, written once when the lease concludes. The runner's "keep" step falls inside `report`.

**PLAN check:** on the dev host the runner runs as root and `sandbox_ids` is not 0. If either fails, stop and surface it.

- **Dimension 2.1** — The runner unit's endpoint is loopback, the collector unit reads its credential from a root-only file, and the runner unit carries no Grafana variable → Test `runner_unit_targets_loopback_collector`
- **Dimension 2.2** — The rule admits uid 0 and refuses any other uid on loopback 4318, and `service.sh` refuses to start the collector without it → Test `collector_rule_admits_only_root_sockets`
- **Dimension 2.3** — On the dev host, a sandbox under `allow_all` cannot deliver to `127.0.0.1:4318` → Test `manual_sandbox_cannot_post_to_collector`
- **Dimension 2.4** — After a runner restart, the runner's spans still reach the collector → Test `manual_runner_restart_keeps_export`
- **Dimension 2.5** — `runner.lease` carries the segments the lease reached, written once at conclusion → Test `lease_span_records_existing_segments`
- **Dimension 2.6** — `deploy.sh` refuses an `otelcol-contrib` download whose SHA-256 differs from the pin → Test `collector_download_refused_on_checksum_mismatch`
- **Dimension 2.7** — On the dev host, a chat event's `runner.lease` span is queryable in Tempo by `agentsfleet.event.id` → Test `manual_runner_span_reaches_tempo`

### §3 — The edge reports its own cost on one clock

**Prerequisite:** none.

In the throttled measurement, 584 ms passed between the browser's Send and the daemon's first byte. The steer route times `credential()` and the upstream call and answers `Server-Timing: auth;dur=…, upstream;dur=…`, in every environment; it is same-origin, authenticated, and carries durations only. The browser sets `performance` marks `af:send`, `af:accepted`, `af:first-frame` and `af:complete`; their keying and order rules are fixed at Stage 9. Nothing is added to the daemon's public request surface.

- **Dimension 3.1** — The steer route answers with `Server-Timing` naming `auth` and `upstream` → Test `steer_route_answers_server_timing`
- **Dimension 3.2** — A refused or failed upstream still answers with `auth` timing and an unchanged status and body → Test `server_timing_never_changes_the_answer`
- **Dimension 3.3** — The browser leaves the four marks for one send → Test `delivery_marks_send_accepted_first_frame_complete`

### §4 — One timeline, one row, one procedure

**Prerequisite:** §1–§3 emitting data, and the PLAN spike below passing.

| Segment (in order) | Source | Clock | In the sum? |
|---|---|---|---|
| Edge in | `af:accepted − af:send` (browser) and `auth + upstream` (`Server-Timing`) | browser; route | Checked by Goal (3), not summed |
| Steer | steer span duration | daemon | yes |
| Queue | lease span start − steer span end | daemon | yes |
| Lease | lease span duration | daemon | yes |
| Gap A | hydrate span start − lease span end | daemon | yes |
| Hydrate | hydrate span duration | daemon | yes |
| Gap B | tiled capture span start − hydrate span end | daemon | yes |
| Capture | tiled capture span duration | daemon | yes |
| Gap C | report span start − capture span end | daemon | yes |
| Report | report span duration | daemon | yes |
| Edge out | `af:complete` − everything above | browser | remainder, not checked |
| Drill-downs | `agentsfleet.<verb>.<stage>_ms`; `agentsfleet.runner.<segment>_ms` inside gaps A–C | per span | excluded |

The dashboard's round-trip row reads Tempo through a datasource variable (`tempo`), so no new vault field is needed. Each span shows its `service.instance.id`. Fleets are picked by ID (`gen_ai.agent.id`). The existing `DeliveryStage` panel shows no data in dev today (see Discovery), so the row stands on its own.

**PLAN spike, pass/fail:** one panel that, given an event ID, lists the spans with start offsets and computed gaps. If it fails, `002_round_trip_playbook.md` ships the TraceQL plus a small script that prints the tiled line, and the row shows the span list only.

- **Dimension 4.1** — The asset check accepts the Tempo datasource variable and still refuses any other datasource → Test `assets_check_accepts_tempo_variable_only`
- **Dimension 4.2** — For each verb span, duration minus the sum of its stage attributes is under the larger of 10% and 5 ms → Test `verb_stage_attributes_account_for_the_span`
- **Dimension 4.3** — On dev, for N consecutive probes, every span in the table is found, and `af:accepted − af:send` ≥ `auth + upstream` ≥ steer span → Test `manual_probes_satisfy_coverage_and_edge_ordering`

## Interfaces

```
Daemon request span attributes (set by the five handlers; trace.rs unchanged):
  agentsfleet.event.id (steer, lease, report), gen_ai.agent.id (all five)
  agentsfleet.steer.{auth,admit,announce}_ms
  agentsfleet.lease.{claim,record,provider,gates,issue,billing_hold,thread}_ms
  agentsfleet.hydrate.{fence,memory_read}_ms
  agentsfleet.capture.{fence,memory_write}_ms
  agentsfleet.report.{load,settle,billing_capture,billing_release,announce}_ms
  values: integer milliseconds

Runner span attributes (runner.lease, written once at conclusion):
  agentsfleet.runner.{memory,sandbox,model,tools,report,first_token}_ms   integer ms

Runner collector (bare-metal host):
  receiver   127.0.0.1:4318 OTLP/HTTP, traces and metrics only
  admission  nftables output: tcp dport 4318 to loopback accepted only for meta skuid 0
  limits     memory_limiter 200 MiB (+50 spike), sending_queue 1000, retry ≤ 300 s, MemoryMax=256M
  health     127.0.0.1:13133
  credential /etc/agentsfleet/otelcol-runner.env (0600 root), from op://ZMB_CD_DEV/grafana-dev/*

Edge (steer route → browser):
  Server-Timing: auth;dur=<ms>, upstream;dur=<ms>
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Sandbox posts telemetry | Tenant code under `allow_all` connects to `127.0.0.1:4318` | nftables drops it (non-root uid); the collector never sees it |
| Rule cannot load | `nft` missing or refuses the rule | `service.sh` refuses to start the collector and names the rule; the runner logs `telemetry_export_disabled`-class lines to journald |
| Collector down | Collector stopped or crashed | Runner keeps running. An operator sees runner series and spans stop in Grafana, and failed export lines in the runner's journal; the discard counter cannot be seen, because it travels the same path |
| Credential missing on host | Environment file absent | Collector unit fails to start naming the file; runner unaffected |
| Tampered collector download | Release asset changed | `deploy.sh` refuses on SHA-256 mismatch; the previous binary stays |
| Span attributes dropped | Span layer back inside a reload slot | `handler_attribute_reaches_exporter_through_real_subscriber` fails |
| Several captures per lease | Mid-run memory pushes | Only the last capture before the report is tiled; earlier ones are drill-downs |
| Exit before a stage | Refusal, parked work, stale fence | Only reached stages are written |
| Upstream failure at the edge | Daemon unreachable or refuses | Route answers its existing status and body plus `auth` timing |
| Span queue overflow | Burst beyond `SPAN_QUEUE` | No new spans are added; overflow behaviour is unchanged |

## Invariants

1. No identifier becomes a metric label — no metric family is added; the census grader is unchanged.
2. Timing never changes an HTTP answer's status or body — `server_timing_never_changes_the_answer` compares them with and without timing.
3. The runner holds no observability credential — `afr_telemetry`'s refusal of header knobs stays, and `runner_unit_targets_loopback_collector` asserts no Grafana variable in the runner unit.
4. Only root-owned host processes can deliver to the collector — `collector_rule_admits_only_root_sockets` and the manual sandbox test.
5. No new span names, and `trace.rs` is unchanged — the runner's span-name allowlist test and `request_span_carries_template_not_path` stay green.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| Daemon verb stage attributes | ops | A verb's request ends | Event id, fleet id, stage durations | No prompt, reply, path or SQL text | `event_identity_joins_the_five_verb_spans` |
| `runner.lease` segment attributes | ops | A lease concludes | Segment durations | No prompt or reply | `lease_span_records_existing_segments` |
| Edge `Server-Timing` | ops | Every steer response | `auth`, `upstream` durations | No identifiers in the header | `steer_route_answers_server_timing` |

No metric family is added or changed, and no PostHog event changes, so no analytics funnel playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 0.1 | unit | `test_moved_rules_reject_seeded_violations` | Fixture with `export AGENTSFLEET_LOG_LEVEL="info"` passes; `LOG_LEVEL="info"` fails rule `workflow-daemon-log-level` |
| 0.2 | manual | `manual_dev_sizes_match_live` | `fly scale show` reads `shared` 2 CPUs 1024 MB for `agentsfleetd-dev` and 1 CPU 1024 MB for `dragonfly-dev`, matching both `fly.toml` files |
| 0.3 | manual | `manual_dev_machines_unthrottled_at_rest` | One hour after resize, `rate(fly_instance_cpu_throttle[30m])` = 0 and `fly_instance_cpu_balance` > 0 for both apps; values in Session Notes |
| 1.1 | unit | `stage_clock_writes_each_reached_stage_once` | A report that settles → each of five report stages written once; a report refused at `load` → only `load` written |
| 1.2 | unit | `empty_poll_writes_claim_without_identity` | Poll with no unsettled admission → only `agentsfleet.lease.claim_ms`; no `agentsfleet.event.id` |
| 1.3 | integration | `event_identity_joins_the_five_verb_spans` | Live Postgres chat cycle → steer, lease and report share one event id; one hydrate and the last capture fall between lease end and report start for the same fleet |
| 1.4 | unit | `request_span_carries_template_not_path` | Request with real identifiers → route attribute is the template; no identifier in any attribute but `agentsfleet.event.id` and `gen_ai.agent.id` |
| 1.5 | integration | `handler_attribute_reaches_exporter_through_real_subscriber` | `logs::install` + `attach` with an in-memory exporter; a handler sets `agentsfleet.steer.admit_ms` → the exported span carries it |
| 2.1 | unit | `runner_unit_targets_loopback_collector` | Unit file → endpoint `http://127.0.0.1:4318`; no `GRAFANA_` variable; collector env file 0600 root |
| 2.2 | unit | `collector_rule_admits_only_root_sockets` | Rule file matches `meta skuid 0` for dport 4318 on loopback and drops the rest; `service.sh` with a failing `nft` load → exit non-zero, collector not started |
| 2.3 | manual | `manual_sandbox_cannot_post_to_collector` | On the dev host, a lease under `allow_all` runs `curl -m 2 http://127.0.0.1:4318/v1/traces` → refused or timed out; output in Session Notes |
| 2.4 | manual | `manual_runner_restart_keeps_export` | Restart `agentsfleet-runner`; send a chat; its `runner.lease` span reaches Tempo |
| 2.5 | unit | `lease_span_records_existing_segments` | Lease with memory 12 ms, model 400 ms, report 30 ms → those three fields set once; `sandbox` and `tools` unset |
| 2.6 | unit | `collector_download_refused_on_checksum_mismatch` | Stubbed download with a wrong digest → exit non-zero, previous binary untouched |
| 2.7 | manual | `manual_runner_span_reaches_tempo` | Dev chat; TraceQL `{ span.agentsfleet.event.id = "<id>" }` returns `runner.lease`; screenshot in Session Notes |
| 3.1 | unit | `steer_route_answers_server_timing` | Stubbed credential 12 ms, upstream 40 ms → header names `auth` and `upstream` with durations ≥ those |
| 3.2 | unit | `server_timing_never_changes_the_answer` | Upstream refuses 409 or is unreachable → same status and body as without timing; `auth` present |
| 3.3 | unit | `delivery_marks_send_accepted_first_frame_complete` | Simulated send → four marks for one send |
| 4.1 | unit | `assets_check_accepts_tempo_variable_only` | Panel with `${tempo}` passes; panel with any other datasource UID fails naming it |
| 4.2 | unit | `verb_stage_attributes_account_for_the_span` | Fixture spans: verb duration 100 ms with stages summing 96 ms → pass; summing 80 ms → fail |
| 4.3 | manual | `manual_probes_satisfy_coverage_and_edge_ordering` | N consecutive dev probes: every table span found; edge ordering holds; numbers in Session Notes |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R0 | Dev unthrottled at rest and deploying at `info` (§0) | `make check-gh-actions-valid` | exit 0 | P0 | |
| R1 | Daemon verbs write their reached stages and identity through the real subscriber (§1) | `cargo test -p afd_observability stage_clock` | exit 0 | P0 | |
| R2 | Only root-owned host processes can deliver to the collector (§2) | `bash deploy/baremetal/service_test.sh` | exit 0 | P0 | |
| R3 | Runner spans reach Tempo from the dev host, across a restart (§2) | `manual: TraceQL { span.agentsfleet.event.id = "<probe id>" } in Grafana Tempo` | ≥1 `runner.lease` span | P0 | |
| R4 | Edge cost reaches the browser on one clock (§3) | `cd ui/packages/app && bun run test steer-route-timing` | exit 0 | P0 | |
| R5 | Dashboard asset valid; verb stages account for their spans (§4) | `bash playbooks/operations/observability/01_assets_check.sh` | exit 0 | P0 | |
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

- **Runner logs through the collector.** Forwarding journald logs requires the fail-closed rules in `observability.md` §"Signal routing" (field allowlist, drop on parse failure, allowlist proof). Future work; the deferral is recorded in Discovery.
- **Per-request SQL statement counts.** A `tracing` layer cannot see `sqlx::query` events at `info`, because the level filter in `rustd/crates/agentsfleetd/src/logs.rs` is global. Statement questions use PlanetScale Insights and the `afd_bench` lanes. Future work if needed.
- **New metric families for stages.** Per-event questions are answered by spans. Future work once a production aggregate need exists.
- **The idle cost of lease polling.** It is a CPU question, measured as daemon process CPU ÷ `rate(agentsfleet_lease_polls_total)`, both already exported. Future work.
- **Making the round trip faster.** These optimisations wait for this spec's measurements: fewer transactions per event, hydrate folded into the lease, capture skipped when memory is unchanged, and the three report transactions merged. Future work.
- **The missing `DeliveryStage` series.** The declared histogram reaches no Prometheus series in dev (Discovery). Future work; the round-trip row does not depend on it.
- **Browser telemetry export.** Pending decision D2; this spec ships DevTools marks only.
- **W3C `traceparent` across the runner protocol.** `observability.md` §Traces keeps leases as separate root spans joined by attribute. Future work only if that decision changes.
- **Non-chat events.** Webhook, cron and continuation events have no browser Send; their daemon spans still carry stage attributes, but the round-trip row covers chats. Future work.
- **Production runner collector.** Dev host only; production follows the same systemd units once a production runner exists. Future work.
- **The "access needs attention" banner.** It came up in the same session; it is a separate product change. Future work.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy picks the dev acceptance fleet by ID and its last chat, and reads (illustrative values) "lease 210 ms: billing_hold 40 ms; gap B 640 ms: model 560 ms" without opening a terminal.
2. **Preserved user behaviour** — Chat send, reply and retry are unchanged. Every API response keeps its status and body; the steer route adds one `Server-Timing` header. The browser records `performance` marks and renders nothing new.
3. **Optimal-way check** — The most direct design is one distributed trace per event. `observability.md` §Traces keeps each lease as a separate root span, so this spec joins spans by `agentsfleet.event.id` and a fleet time window. That gives the same data, not one trace view.
4. **Rebuild-vs-iterate** — Iterate for §1, §3 and §4: attributes on existing spans and existing stopwatches recorded. §2 is new infrastructure, a collector whose design is already approved.
5. **What we build** — Daemon stage attributes through a span layer outside the reload slot, the runner collector admitting only root-owned sockets, runner segment attributes, edge `Server-Timing` and marks, one dashboard row, one playbook.
6. **What we do NOT build** — The items in Out of Scope; reasons are given there.
7. **Fit with existing features** — Builds on `fleet.delivery`, `DeliveryStage` and the runner's `Segments`. Adds no spans to the runner's budget (`MAX_LEASE_SPANS`, `RUNNER_SPANS_PER_SECOND`) and leaves `trace.rs` unchanged.
8. **Surface order** — Grafana only; no CLI surface, because the data lives in Grafana and the playbook names the queries.
9. **Dashboard restraint** — No percentile alert and no target line until a timing capture from production-sized hardware exists (`observability.md`: "nothing gates on a percentile").
10. **Confused-user next step** — `002_round_trip_playbook.md` names the row, the variables, the TraceQL and the fallback script.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one Section per process boundary (daemon, runner host, edge), plus one for the dashboard. §1 and §3 can merge and be tested on their own; §2 also waits on D1; §4 needs §1–§3 emitting data. §0 carries owner-requested dev changes that are not part of the stage-timing design.
- **Alternatives considered (rejected):**
  - **Child spans per stage:** on the daemon, about 19 extra spans per event would enter a 2,048-span queue that drops overflow without counting it, and add new span names; on the runner, they would spend the 256-per-lease budget.
  - **Stage metric families:** duplicate the attributes; the goal is per event.
  - **A SQL tally layer:** it cannot see `sqlx::query` at `info`, and enabling it would add one debug event per statement on the machine being measured.
  - **Edge-start headers:** the daemon would subtract the edge host's clock from its own and trust a client-supplied header.
  - **`OpenTelemetrySpanExt` through the current subscriber:** attributes are dropped behind the reload slot.
  - **Fields declared in `trace.rs`:** couples the generic router to fleet stage names; kept only as the §1 fallback.
  - **One trace per event via W3C `traceparent`:** changes the runner protocol against `observability.md` §Traces.
  - **Extending the `fleet.delivery` span with database timestamps:** one clock, but no runner or edge breakdown.
  - **nftables `socket cgroupv2`:** systemd recreates the runner's cgroup on restart, so the rule goes stale.
  - **Unix-socket receiver:** the runner's exporter accepts only `http(s)` URLs with a host, so it would need an `afd_otlp` transport change.
  - **Receiver credential:** breaks Invariant 3.
  - **Moving dev off `allow_all`:** changes what fleets can reach.
  - **A trace-context column on `fleet.admissions`:** a schema change for a join attributes already provide.
- **Patch-vs-refactor verdict:** patch for §1, §3 and §4; §2 is new infrastructure.

## Discovery (consult log)

- **Consults**
  - `docs/architecture/observability.md` §Traces, §runner collector, §Signal routing and §PostHog read (Oct 26, 2026).
  - Owner request, Indy (Oct 26, 2026), verbatim: "I want you to have logging, observability added to so we could to this as a fleet for me to check it and optimize". Agent's interpretation: a per-fleet, per-event stage breakdown in Grafana.
  - **Throttled baseline** on `shared-cpu-1x`, event `01a17627-25c5-761d-9794-f1ab0b40d9fc`, 03:00:38 UTC; daemon logs and Tempo via `gcx`, Postgres read-only:
    - Send to 202: 0.78 s, of which 50 ms in the daemon.
    - Lease request: 1.67 s over 36 SQL statements.
    - Hydrate: 0.33 s. Capture: 0.39 s.
    - Report: 1.60 s over 28 statements.
    - `event_complete`: 5.42 s after Send.
  - **Ruled out** (inference from these measurements: the cause is CPU quota throttling on `shared-cpu-1x`):
    - The database: PlanetScale Insights, branch `useast`, `insert into billing.holds` p50 and p99 0 ms server-side.
    - The pooler and Nagle's algorithm: an in-region Go probe on the daemon machine reproduced the delay with `TCP_NODELAY` on and off, on ports 6432 and 5432.
    - The network entirely: with no database, 169 of 2,000 idle 1 ms sleeps woke 70–76 ms late; with one busy thread, 1,988 of 2,000 woke late (median 158.6 ms).
  - **`performance-1x` trial:** 0 of 8,610 probe statements took ≥ 40 ms; three chats took 1.53–1.86 s.
  - **On `shared-cpu-2x` after §0:** three chats took 1.53–1.94 s (05:11 UTC, browser probe through the steer route).
  - **Runner stopwatch** (`lease_completed` lines in the dev runner's journal, `journalctl -u agentsfleet-runner`, 04:21–05:43 UTC), dev acceptance fleet after the resize:
    - Runner wall time: 700–1,100 ms.
    - Memory: 62–86 ms. Model: 346–684 ms. First token: 317–489 ms. Report: 249–347 ms.
    - `tools=0`, and no `sandbox` segment: no sandbox ran.
    - Throttled 04:21 run: memory 718 ms, report 1,883 ms.
  - **`DeliveryStage` export gap:** Prometheus has no series matching `*delivery*` (05:45 UTC), although `docs/metrics.census.tsv` declares `agentsfleet_fleet_delivery_stage_seconds` as a delta histogram.
  - **Pricing:** `https://docs.fly.io/about/pricing`, Oct 26, 2026, `iad`: `performance-2x` 4 GB $66 a month; `shared-cpu-4x` 4 GB about $26.76, derived from $6 per GB and the shared-vCPU rate; three machines each.
- **Required human decisions — pending**
  - **D1, blocks §2:**
    - **Previous choice:** Indy, Oct 26, 2026: "Reuse grafana-dev".
    - **Corrected risk:** bubblewrap sandboxes share one kernel (`runner_execution.md` §Sandbox engines). Any code-running tenant with a kernel exploit, or anyone with root on the runner host, could read the collector's key. That key can also write the daemon's telemetry, because dev and prod share one Grafana tenant. The key's scopes are to be read in Grafana.
    - **Option A:** keep `grafana-dev`.
    - **Option B:** a dedicated write-only token in `ZMB_CD_DEV`.
    - Re-confirmation pending.
  - **D2, does not block:** browser telemetry export, or DevTools marks only (the agent recommends marks only).
- **Decisions made (Oct 26, 2026)**
  - "I am okay with the bottleneck in dev, but i need you to confirm the reason, so i can focus on that in production".
  - "For development also strip down the debug level in development … bring down the sizing of dragonfly as well as it costs 60$".
  - Asked whether to pin `info` in rule `workflow-daemon-log-level` of `scripts/check_workflow_rules.py`, Indy chose "Yes, pin info" (the option the agent marked Recommended). This is the gate-edit approval, superseding the M187 call to deploy dev at `debug`.
  - Asked to move the dev daemon, Indy chose "Yes, shared-cpu-2x 1GB" (the option the agent marked Recommended).
- **Metrics review** — no metric family added; span attributes only; no analytics funnel playbook update required, because no product event changes.
- **Skill-chain outcomes**
  - **egm review, Stage 5:** report `egm-review/zoom_out_report.md`. Indy: "Okay do that". Changes are in revision `7123d5c05`.
  - **egm review, Stage 6:** report `egm-review/comprehension_report.md`; it ran against the pre-revision text, and each item carries a disposition.
  - **egm review, Stage 7:** report `egm-review/goldfish_review_report.md`. Indy: "Auto accept", applied here. Issue 22 (detailed-range items) carries to Stage 9.
  - **Pending:** `/orly-write-unit-test`, `/review`, `orly-babysit-prs`.
- **Deferrals**
  - > Indy (2026-10-26 05:09): "Okay do that" — context: Stage 5 finding 9 recommended shipping traces and runner metrics only and leaving journald log forwarding to a follow-up.
  - > Indy (2026-10-26 05:47): "Auto accept" — context: Stage 7 Issue 9 proposed recording that deferral; runner log forwarding through the collector is future work.
