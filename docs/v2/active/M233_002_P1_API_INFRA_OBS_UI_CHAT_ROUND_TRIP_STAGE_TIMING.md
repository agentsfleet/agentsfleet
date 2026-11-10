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

# M233_002: Every chat event's round trip is broken into named stages, per fleet, in Grafana

**Prototype:** v2.0.0
**Milestone:** M233
**Workstream:** 002
**Date:** Nov 04, 2026
**Status:** IN_PROGRESS
**Open work:** Daemon verb spans (§1), the runner span's segments (§2), edge timing (§3) and the dev sizing checks (§0).
**Priority:** P1 — operator-facing: the production daemon size (three `shared-cpu-4x` 4 GB machines at about $80 a month, or three `performance-2x` 4 GB at $198 a month; source in Discovery) is chosen without per-event evidence of where a chat's time goes
**Categories:** API, INFRA, OBS, UI
**Batch:** B1 — single workstream, built beside M245_001; Section prerequisites are stated in each Section
**Baseline revision:** 51d41a9e3
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M238_001 (merged in #799); M243_001 and M244_001 (merged: the runner stream and core NATS); M233_001 shipped §0.1 and §5 in #786. M245_001 runs in parallel and rewrites the functions §1 marks, so whichever merges second rebases onto the other's shape.
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 26, 2026), from timing measurements taken on dev (Discovery, Consults); revised after the Stage 5 and Stage 7 design reviews; §1 rewritten for the runner stream on Nov 10, 2026 (Claude Fable 5.2, Discovery)
**Canonical architecture:** `docs/architecture/observability.md` §Traces, §"`agentsfleet-runner` — a collector of its own", §"Signal routing"

---

## Overview

This successor owns M233_001 §1–§4 and its unfinished sizing checks. §0.1 and §5 shipped in #786, and the §4 row and the runner collector shipped since. M243 moved the lease offer, renewal and report onto the runner stream (`afd_gateway`), so §1 times stream operations, not HTTP handlers.

**Goal (testable):** One dev chat yields `fleet.steer`, `fleet.offer`, `fleet.report` and `runner.lease` spans that share one `agentsfleet.event.id`, each carrying its reached stages, and the steer route answers `Server-Timing`.

**Problem:** An operator cannot see where a chat's 1.5–1.9 s goes. Three dev chats took 1.53–1.94 s from Send to `event_complete` on Oct 26, 2026.
- Splitting one chat took debug-level SQL logs, Postgres timestamps, a browser console and a shell on the runner host.
- Grafana holds `fleet.delivery` per run and `runner.lease` per lease. Neither names a daemon stage, and the edge reports nothing.
- The offer and the report have no exported span at all: their only spans are `debug` (`afd_gateway/src/session/dispatch/mod.rs`, `session/supervisor.rs`).

**Solution summary:**
- **Daemon:** each verb starts a stage clock at its entry and records one synthesized root span when it ends, as `fleet.delivery` does. The verbs are steer, offer, report, and the fallback hydrate and capture. No tracing layer moves and no metric family is added.
- **Runner host:** the collector is installed; `runner.lease` gains its ten stopwatch segments as attributes.
- **Edge:** the steer route answers `Server-Timing`, and the browser leaves four `performance` marks per send.
- **Dashboard:** the round-trip row in `dashboard-chat.json` is shipped; §4 keeps its drift checks.

**Risks and limits:**
- **Collector key:** the key sits on a multi-tenant host whose sandboxes share one kernel (D1, Discovery).
- **Clocks:** each verb span is timed on its own replica's clock, so cross-replica gaps rely on clock agreement (each span shows `service.instance.id`).
- **Parallel build:** M245_001 rewrites the claim, issue and report functions §1 marks. A stage names a meaning, such as the report's write transaction, so its mark survives a rewrite.
- **Scale:** the per-event view lasts as long as Tempo keeps traces, and event lookup is built for dev scale.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(obs): name every stage a chat crosses, per event
- **Intent (one sentence):** Indy (project owner) can pick a fleet and a chat event in Grafana and see where the time between Send and the reply went, so optimisations and production sizing are chosen from evidence.
- **Authoring handshake:** Indy wants each chat broken into stages across browser, edge, daemon and runner. The spec rewrites §1 for the runner stream, finishes the runner span and the edge timing, and keeps the shipped row and collector. ASSUMPTIONS I'M MAKING:
  1. Synthesized root spans through the global provider reach Tempo, because `fleet.delivery` takes that path today (`af_observability/src/delivery/mod.rs`).
  2. An offer pass that selects nothing records no span. A recovery probe runs every 5 s per replica, and a span per empty pass would flood `SPAN_QUEUE`.
  3. Renewal, fold asks and activity stay uninstrumented; their time stays inside `runner.lease`.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/protocol/af_observability/src/delivery/mod.rs` — the synthesized root span §1 mirrors: built backwards from its end, through the global tracer, payload-free by construction.
2. `rustd/crates/daemon/afd_gateway/src/session/offer.rs` and `session/dispatch/report.rs` — where an offer and a report enter the daemon.
3. `rustd/crates/daemon/afd_fleet/src/lease/claim.rs`, `pull/mod.rs`, `deliver/mod.rs` and `report/mod.rs` — the functions whose boundaries the offer and report stages mark.
4. `rustd/crates/runner/afr_supervisor/src/lease_loop/segments.rs` and `identity/mod.rs` — the runner's stopwatch and the span it is written to.
5. `ui/packages/app/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/messages/route.ts` and `ui/packages/app/lib/streaming/fleet-stream-held.ts` — the steer route, and why a send's frames wait for its 202.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M233_002_P1_API_INFRA_OBS_UI_CHAT_ROUND_TRIP_STAGE_TIMING.md` | EDIT, then MOVE to `active/` and `done/` | this spec |
| `docs/architecture/observability.md`, `scaling.md` | EDIT | §1–§3: the verb spans, the runner segments, edge `Server-Timing`; §0: the dev daemon size |
| `rustd/crates/protocol/af_observability/src/verb_span/mod.rs`, `verb_span/tests.rs` | CREATE | §1: the stage clock and the synthesized verb span, with its in-memory exporter tests |
| `rustd/crates/protocol/af_observability/src/lib.rs`, `src/semconv/mod.rs`, `src/semconv/tests.rs` | EDIT | §1, §2: export the module; span names, a `VERB_SPAN_KEYS` list beside `DELIVERY_SPAN_KEYS`, and the ten runner segment keys, each checked by the key tests |
| `rustd/crates/daemon/afd_api_tenant/src/handler/fleet/message_steer/mod.rs` | EDIT | §1: the steer clock and span |
| `rustd/crates/daemon/afd_api_runner/src/handler/runner/memory.rs` | EDIT | §1: the hydrate and capture clocks and spans |
| `rustd/crates/daemon/afd_gateway/src/session/offer.rs`, `session/dispatch/report.rs`, `session/mod.rs` | EDIT | §1: the offer and report clocks and their last stages |
| `rustd/crates/daemon/afd_gateway/src/session/verb_span_tests.rs` | CREATE | §1: Dimensions 1.2 and 1.3, `#[ignore]`d live-Postgres tests beside the crate's other session tests |
| `rustd/crates/daemon/afd_fleet/src/lease/claim.rs`, `pull/mod.rs`, `deliver/mod.rs`, `report/mod.rs` | EDIT | §1: the clock passed in and one mark per stage boundary |
| `rustd/crates/daemon/main/tests/integration_verb_span_export.rs` | CREATE | §1: Dimension 1.5 through `telemetry::install` |
| `rustd/crates/runner/afr_supervisor/src/lease_loop/segments.rs`, `identity/mod.rs`, `lease_loop/tests/mod.rs`, `lease_loop/tests/span_segments.rs` | EDIT, CREATE | §2: `runner.lease` declares and records the ten segments |
| `ui/packages/app/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/messages/route.ts`, `ui/packages/app/tests/steer-route.test.ts` | EDIT | §3: time `credential()` and the upstream call; answer `Server-Timing` |
| `ui/packages/app/components/domain/useFleetMessageDelivery.ts`, `useFleetMessageDelivery.test.tsx`, `ui/packages/app/lib/streaming/fleet-stream-reply-registry.ts` | EDIT | §3: the four `performance` marks for one send |
| `playbooks/operations/observability/002_round_trip_percentiles.md` | EDIT | §1, §4: the verb spans' TraceQL beside the row's |

## Applicable Rules

- **`domains/logging.md`** — OBS, LOG-HOT, LOG-SECRET: a span attribute is never a new log line, and no payload or key reaches one.
- **`code.md`** — NDC, NLR, ORP, FLL, CODE-SEAM: the steer handler is 315 lines against a 350 cap, so its clock stays one parameter and two marks.
- **`tests.md`** — TST-NAM, TEST-INJECT, TEST-NEG, TEST-REAL, ITF: the residual test injects its clock; the join test runs on the real stack.
- **`languages/rust.md`** — ERR-RS: the clock adds no fallible public signature.
- **`languages/typescript.md`** — TS-BUNTEST, TS-TESTFILE.
- **`domains/architecture.md`** — ARCH-LAND: `observability.md` changes in the commit that adds the span names.
- **`docs.md`** — DOC-02, DOC-03.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `docs.language` at `orly gate verify` and `orly gate pr` | yes, two architecture pages and a playbook change | sentences under 25 words, three-sentence paragraphs |
| `make lint-all` | yes | workspace dependencies only; `af_observability` already depends on `opentelemetry` |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, `message_steer/mod.rs`, `pull/mod.rs`, `report/mod.rs` | the clock is passed in; each boundary adds one mark |
| Metrics census and producer test | no, no metric family changes | none |
| `make check-runner-collector-admission` | yes, §2's boundary is already shipped | stays green; no collector file changes |

## Prior-Art / Reference Implementations

- **Reference:** `fleet.delivery` (`rustd/crates/protocol/af_observability/src/delivery/mod.rs`) — a root span built through `opentelemetry::global::tracer`, with an explicit start and end and only declared attributes. Divergence: the verb span is timed live by its clock, not reconstructed from a report.
- **Reference:** `producers::library::timed` (`af_observability/src/producers/library.rs`) — a stage wrapper that records on early return, for the clock's drop behaviour.
- **Reference:** the runner's `Timeline` and `Segments` — existing stage names, reused as the runner keys.
- **Reference:** W3C `Server-Timing` (https://www.w3.org/TR/server-timing/) — the browser reads `PerformanceResourceTiming.serverTiming` on a same-origin response.

## Sections (implementation slices)

### §0 — The dev daemon runs where latency is measurable — IN_PROGRESS

A shared vCPU's baseline is 5 ms per 80 ms, and on `shared-cpu-1x` every dev request was throttled. The dev daemon now declares `shared-cpu-2x` 1 GB (`deploy/fly/agentsfleetd-dev/fly.toml`). M244 deleted `dragonfly-dev`, so its half of this Section is gone.

- **Dimension 0.2** — IN_PROGRESS — the live daemon matches its file → Test `manual_dev_size_matches_live` (manual; Indy or the implementer runs `fly scale show -a agentsfleetd-dev`; output in the Pull Request Session Notes): `shared` 2 CPUs 1024 MB. A machine still on one CPU fails it.
- **Dimension 0.3** — the dev daemon is unthrottled at rest → Test `manual_dev_daemon_unthrottled_at_rest` (manual; one hour after a deploy, `rate(fly_instance_cpu_throttle[30m])` = 0 and `fly_instance_cpu_balance` > 0 for `agentsfleetd-dev`; values in the Session Notes).

### §1 — The daemon names every stage a chat crosses

**Prerequisite:** none. M245_001 may land first; the marks then move to its functions.

Each verb builds a stage clock at its entry from `af_observability::verb_span`. The clock marks each stage boundary it reaches, then records one span when the verb ends, on every exit. The span is a root, built through `opentelemetry::global::tracer(semconv::SCOPE_NAME)` as `fleet.delivery` is, starting at the clock's start and ending at its record.

- **Why synthesized:** the offer and report have no `info` span to write to, and the stream's own request span ends when its response head returns (`afd_gateway/src/session/opening/mod.rs`). A synthesized span also avoids the reload slot that drops `tracing` attributes (`daemon/main/src/logs.rs`).
- **Names and identity:** `fleet.steer`, `fleet.offer`, `fleet.report`, `fleet.hydrate` and `fleet.capture`. Steer, offer and report carry `agentsfleet.event.id`; all five carry `gen_ai.agent.id`. Hydrate and capture join an event by fleet and the time window between its offer and its report.
- **Attributes:** `agentsfleet.<verb>.<stage>_ms`, integer milliseconds, keys spelled in `semconv`. Only reached stages are written.

| Verb | Entry, where the clock starts | Stages, in order, and the boundary each ends at |
|---|---|---|
| steer | `steer` in `message_steer/mod.rs`, after its extractors | `admit` (the steering append), `announce` (`announce_admitted`) |
| offer | `SessionServices::offer` in `session/offer.rs`, passed to `LeaseService::claim_with` | `claim` (selection and the slot claim), `admit` (`admit_claimed`: delivery, payer, provider and gates), `issue` (`deliver_with`, through the issue commit), `encode` (the offer frame), `send` (the stream write) |
| report | `SessionServices::report` in `session/dispatch/report.rs`, passed to `LeaseService::report` | `load` (the lease read), `prepare` (task report and pricing), `capture` (inline memory capture), `commit` (`commit_report`), `announce` (`finalize_report`), `ack` (the acknowledgement write) |
| hydrate, capture | the handlers in `afd_api_runner/src/handler/runner/memory.rs` | `fence`, then `memory_read` or `memory_write` |

Steer's authentication runs in extractors before the clock starts, so the daemon does not time it; the edge's `auth` timing covers the app's side (§3). An offer pass that selects nothing records no span (assumption 2). **Implementation default:** the clock is a `&mut VerbClock` parameter, because a task-local would hide which functions mark.

- **Dimension 1.1** — DONE — each reached stage is written once → Test `verb_clock_writes_each_reached_stage_once` (unit, in-memory exporter): a report reaching all six stages exports one `fleet.report` span with six stage attributes. A report refused at `load` exports one span with `load_ms` only.
- **Dimension 1.2** — an empty offer pass records nothing → Test `empty_offer_pass_records_no_span` (integration, live Postgres, in-memory exporter): a pass that selects no candidate exports zero spans. A pass that recorded a `claim`-only span would fail it.
- **Dimension 1.3** — one chat's spans join by event → Test `event_identity_joins_the_verb_spans` (integration, `session/verb_span_tests.rs`, live Postgres and a gateway session from `afd_gateway` `test_util`): one chat exports `fleet.steer`, `fleet.offer` and `fleet.report` with one `agentsfleet.event.id`. A fallback hydrate joins by `gen_ai.agent.id`.
- **Dimension 1.4** — DONE — a verb span carries no payload → Test `verb_span_carries_only_declared_keys` (unit): a steer whose message is `secret-probe-7f3a` exports no attribute outside `semconv`'s verb keys, and no value containing the probe.
- **Dimension 1.5** — the span reaches the installed exporter → Test `verb_span_reaches_exporter_through_global_provider` (integration, `daemon/main/tests/integration_verb_span_export.rs`): after `telemetry::install` with an in-memory exporter, a recorded steer span is exported. A clock that took its tracer before the install exports nothing and fails it.
- **Dimension 1.6** — DONE — the stages account for the span → Test `verb_stage_attributes_account_for_the_span` (unit, injected clock): stages summing 96 ms in a 100 ms span pass; 80 ms in 100 ms fails, because the residual exceeds the larger of 10% and 5 ms.

### §2 — The runner's segments reach Grafana

**Prerequisite:** none. The collector is installed on the dev host (Discovery, Nov 04, 2026).

The collector shipped as `deploy/baremetal/otelcol-runner.{yml,service,nft}` and `install-collector.sh`. Its rule loads through the unit's `ExecStartPre`, and `make check-runner-collector-admission` proves the root-only boundary in a network namespace. What remains is the span: `runner.lease` gains the ten `Segments` as `agentsfleet.runner.<segment>_ms`, written once when the lease concludes.

The segments are `memory`, `sandbox`, `egress`, `prepare`, `checkout`, `sandbox_wait`, `model`, `tools`, `report` and `first_token`. `memory` is timed only on the fallback hydrate (`lease_loop/work.rs`). Each key joins `RUNNER_SPAN_KEYS` in `semconv`, which `semconv/tests.rs` checks.

- **Dimension 2.1** — DONE (shipped) — only root-owned sockets reach the receiver → Validation `make check-runner-collector-admission`: exit 0.
- **Dimension 2.3** — a sandbox cannot post to the collector → Test `manual_sandbox_cannot_post_to_collector` (manual; on the dev host a lease under `allow_all` runs `curl -m 2 http://127.0.0.1:4318/v1/traces`; output in the Session Notes): refused or timed out.
- **Dimension 2.5** — DONE — `runner.lease` carries the reached segments once → Test `lease_span_records_existing_segments` (unit, `lease_loop/tests/span_segments.rs`): a lease with memory 12 ms, model 400 ms and report 30 ms sets those three fields once, and leaves `sandbox` and `tools` unset.
- **Dimension 2.7** — the runner span is found by event → Test `manual_runner_span_reaches_tempo` (manual; a dev chat, then TraceQL `{ span.agentsfleet.event.id = "<id>" }`; screenshot in the Session Notes): `runner.lease` and the three daemon verb spans return.

### §3 — The edge reports its own cost on one clock

**Prerequisite:** none.

In the throttled measurement, 584 ms passed between the browser's Send and the daemon's first byte. The steer route times `credential()` and the upstream call, and answers `Server-Timing: auth;dur=…, upstream;dur=…` in every environment. It is same-origin and authenticated, and carries durations only.

The browser leaves four marks per send: `af:send`, `af:accepted`, `af:first-frame` and `af:complete`, each with `detail.operationId` set to the send's operation id. Overlapping sends therefore never share a mark. Their order holds by construction, because a send's own frames wait for its 202 (`lib/streaming/fleet-stream-held.ts`).

- **Dimension 3.1** — DONE — the route answers `Server-Timing` → Test `steer_route_answers_server_timing` (unit, `tests/steer-route.test.ts`): a stubbed credential of 12 ms and upstream of 40 ms answer a header naming `auth` and `upstream` with durations at least those.
- **Dimension 3.2** — DONE — timing never changes the answer → Test `server_timing_never_changes_the_answer` (unit, `tests/steer-route.test.ts`): an upstream 409, and an unreachable upstream, answer the same status and body as without timing, with `auth` present.
- **Dimension 3.3** — DONE — one send leaves four ordered marks → Test `delivery_marks_send_accepted_first_frame_complete` (unit, `useFleetMessageDelivery.test.tsx`): one simulated send leaves four marks with one `operationId`, in order. Two overlapping sends leave eight, four per id.
- **Dimension 3.4** — the edge and the daemon agree → Test `manual_probes_satisfy_coverage_and_edge_ordering` (manual; Indy sends 10 dev chats and reads DevTools and Tempo; values in the Session Notes): each event has all three verb spans, and `af:accepted − af:send` ≥ route `auth + upstream` ≥ `fleet.steer`'s duration.

### §4 — Round-trip percentiles from settled delivery spans

**Prerequisite:** M238_001 deployed, which it is.

The row shipped in `playbooks/operations/observability/assets/dashboard-chat.json`: eight panels read `fleet.delivery` filtered by `gen_ai.agent.id`, at most 1,000 matches each, reduced to p95 and p99. Missing attributes have no samples and are not zero. This Section adds only the verb spans' TraceQL to the playbook page.

- **Dimension 4.1** — DONE (shipped) — the asset check holds → Validation `bash playbooks/operations/observability/01_assets_check.sh`: exit 0.
- **Dimension 4.2** — DONE (shipped) — the drift check holds → Validation `bash playbooks/operations/observability/04_verify.sh` with the approved dev credentials: exit 0.
- **Dimension 4.3** — PARTIAL — the row renders → Test `manual_round_trip_row_renders` (manual; ten dev delivery spans, a screenshot in the Session Notes): p95 and p99 in the declared units, and absent segments empty. Tempo's zero-value display still blocks the rendered witness.

### Shipped predecessor scope

M233_001 Dimension 0.1 and §5 shipped in #786. Their history stays in the closed predecessor.

## Interfaces

```
Daemon verb spans (synthesized roots, global tracer, scope semconv::SCOPE_NAME):
  fleet.steer    agentsfleet.event.id, gen_ai.agent.id, agentsfleet.steer.{admit,announce}_ms
  fleet.offer    agentsfleet.event.id, gen_ai.agent.id,
                 agentsfleet.offer.{claim,admit,issue,encode,send}_ms
  fleet.report   agentsfleet.event.id, gen_ai.agent.id,
                 agentsfleet.report.{load,prepare,capture,commit,announce,ack}_ms
  fleet.hydrate  gen_ai.agent.id, agentsfleet.hydrate.{fence,memory_read}_ms
  fleet.capture  gen_ai.agent.id, agentsfleet.capture.{fence,memory_write}_ms
  values: integer milliseconds; only reached stages

Runner span attributes (runner.lease, written once at conclusion):
  agentsfleet.runner.{memory,sandbox,egress,prepare,checkout,sandbox_wait,
                      model,tools,report,first_token}_ms

Edge (steer route → browser):
  Server-Timing: auth;dur=<ms>, upstream;dur=<ms>
  performance marks af:send, af:accepted, af:first-frame, af:complete; detail.operationId
```

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Verb exits early (refusal, stale fence, parked work) | failure | a verb ends before its last stage | the span records reached stages only | `verb_clock_writes_each_reached_stage_once` |
| Empty offer passes | capacity | the 5 s recovery probe and wake passes | no span is recorded | `empty_offer_pass_records_no_span` |
| Tracer taken before install | failure | a clock built before `telemetry::install` | the clock asks for the tracer at record time, so the installed one is used | `verb_span_reaches_exporter_through_global_provider` |
| Payload in an attribute (security) | security | a stage key fed a message | only `semconv` keys are accepted | `verb_span_carries_only_declared_keys` |
| Span queue overflow | capacity | a burst beyond `SPAN_QUEUE` | the SDK drops the span as today; the verb's answer is unchanged | `verb_clock_writes_each_reached_stage_once` |
| Upstream failure at the edge (dependency unavailable) | failure | daemon unreachable or refusing | the route answers its existing status and body, plus `auth` timing | `server_timing_never_changes_the_answer` |
| Sandbox posts telemetry (security) | security | tenant code under `allow_all` connects to `127.0.0.1:4318` | nftables drops a non-root socket | `make check-runner-collector-admission` |
| Collector down | failure | collector stopped | the runner keeps running; its spans stop in Grafana | `manual_runner_span_reaches_tempo` |

## Invariants

1. **No identifier becomes a metric label.** No metric family is added, and the census grader is unchanged.
2. **Timing never changes an HTTP answer's status or body.** The route computes the header after the upstream answer is final. Test: `server_timing_never_changes_the_answer`.
3. **The runner holds no observability credential.** `afr_telemetry` still refuses header knobs, and the collector reads its credential from a root-only file.
4. **Only root-owned host processes deliver to the collector.** The nftables rule matches `meta skuid 0`. Test: `make check-runner-collector-admission`.
5. **A verb span carries only declared keys.** `VerbClock` takes a closed stage enum per verb, so a free-form key does not compile. Test: `verb_span_carries_only_declared_keys`.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `fleet.steer`, `fleet.offer`, `fleet.report`, `fleet.hydrate`, `fleet.capture` spans | operator | a verb ends, except an empty offer pass | event id, fleet id, stage durations | no prompt, reply, path or SQL text | `event_identity_joins_the_verb_spans` |
| `runner.lease` segment attributes | operator | a lease concludes | segment durations | no prompt or reply | `lease_span_records_existing_segments` |
| Edge `Server-Timing` | operator | every steer response | `auth`, `upstream` durations | no identifiers in the header | `steer_route_answers_server_timing` |

No metric family and no PostHog event changes, so no analytics funnel playbook update is required.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R0 | The dev daemon matches its declared size (§0) | `fly scale show -a agentsfleetd-dev` | a line naming `shared`, 2 CPUs and 1024 MB | P1 | |
| R1 | One chat's verb spans join by event (§1) | `make test-integration-rustd` | exit 0; `event_identity_joins_the_verb_spans`, `empty_offer_pass_records_no_span` and `verb_span_reaches_exporter_through_global_provider` print `ok` | P0 | |
| R2 | Verb spans hold reached stages and declared keys only (§1) | `make test-unit-all` | exit 0; `verb_clock_writes_each_reached_stage_once`, `verb_span_carries_only_declared_keys` and `verb_stage_attributes_account_for_the_span` print `ok` | P0 | |
| R3 | Only root-owned host processes reach the collector (§2) | `make check-runner-collector-admission` | exit 0 | P0 | |
| R4 | `runner.lease` carries its segments (§2) | `make test-unit-all` | exit 0; `lease_span_records_existing_segments` prints `ok` | P0 | |
| R5 | The edge reports its cost (§3) | `make test-unit-all` | exit 0; the `steer-route` and `useFleetMessageDelivery` suites pass | P0 | |
| R6 | The dashboard asset is valid (§4) | `bash playbooks/operations/observability/01_assets_check.sh` | exit 0 | P0 | |
| R7 | The diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | every path is in the Files Changed table | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

N/A: nothing is deleted. The old default's span layer move and its `stage_clock` were never built. Shipped authoring cleanup stays recorded in M233_001 §5.

## Out of Scope

- **Runner logs through the collector.** Forwarding journald logs requires the fail-closed rules in `observability.md` §"Signal routing" (field allowlist, drop on parse failure, allowlist proof). Future work; the deferral is recorded in Discovery.
- **Per-request SQL statement counts.** M245_001 counts them per path with `make bench-lease`; a span cannot see `sqlx::query` events at `info`.
- **Renewal, fold asks and activity.** These stream operations stay uninstrumented; their time sits inside `runner.lease`.
- **New metric families for stages.** Per-event questions are answered by spans. Future work once a production aggregate need exists.
- **Making the round trip faster.** M245_001 owns the statement and round-trip cuts; this spec measures them.
- **The missing `DeliveryStage` series.** The declared histogram reaches no Prometheus series in dev (Discovery). Future work; the round-trip row does not depend on it.
- **Browser telemetry export.** Pending decision D2; this spec ships DevTools marks only.
- **W3C `traceparent` across the runner protocol.** `observability.md` §Traces keeps leases as separate root spans joined by attribute. Future work only if that decision changes.
- **Non-chat events.** Webhook, cron and continuation events have no browser Send; their daemon spans still carry stage attributes, but the round-trip row covers chats. Future work.
- **Production runner collector.** Dev host only; production follows the same systemd units once a production runner exists. Future work.
- Shipped authoring and access-readiness work remains in M233_001 §5; this successor changes no application chat or settings surface.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy selects a fleet ID and time range, reads whole-trip and reached-segment p95/p99, and sees no timing value for absent segments without opening a terminal. Future verb-stage detail remains §1–§3 work.
2. **Preserved user behaviour** — Chat send, reply and retry are unchanged. API response envelopes stay unchanged; the steer route adds one `Server-Timing` header. Shipped authoring behavior remains unchanged by this successor.
3. **Optimal-way check** — The most direct design is one distributed trace per event. `observability.md` §Traces keeps each lease as a separate root span, so this spec joins spans by `agentsfleet.event.id` and a fleet time window. That gives the same data, not one trace view.
4. **Rebuild-vs-iterate** — Iterate for §1, §3 and §4: attributes on existing spans and existing stopwatches recorded. §2 is new infrastructure, a collector whose design is already approved.
5. **What we build** — Five synthesized daemon verb spans with stage attributes, runner segment attributes, edge `Server-Timing` and marks, and the verb spans' TraceQL in the playbook. The collector and the row are shipped.
6. **What we do NOT build** — The items in Out of Scope; reasons are given there.
7. **Fit with existing features** — Mirrors `fleet.delivery` and reuses the runner's `Segments`. Adds no spans to the runner's budget (`MAX_LEASE_SPANS`, `RUNNER_SPANS_PER_SECOND`), at most five daemon spans per event, and leaves `trace.rs` and `logs.rs` unchanged.
8. **Surface order** — Grafana only; no CLI surface, because the data lives in Grafana and the playbook names the queries.
9. **Dashboard restraint** — No percentile alert and no target line until a timing capture from production-sized hardware exists (`observability.md`: "nothing gates on a percentile").
10. **Confused-user next step** — `002_round_trip_percentiles.md` names the row, variables, TraceQL and sample limits.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one Section per process boundary (daemon, runner host, edge), plus one for the dashboard. Each is built and tested on its own. §0 carries the owner-requested dev sizing check.
- **Alternatives considered (rejected):**
  - **Child spans per stage:** on the daemon, about 19 extra spans per event would enter a 2,048-span queue that drops overflow without counting it, and add new span names; on the runner, they would spend the 256-per-lease budget.
  - **Stage metric families:** duplicate the attributes; the goal is per event.
  - **A SQL tally layer:** it cannot see `sqlx::query` at `info`, and enabling it would add one debug event per statement on the machine being measured.
  - **Edge-start headers:** the daemon would subtract the edge host's clock from its own and trust a client-supplied header.
  - **`OpenTelemetrySpanExt` through the current subscriber:** attributes are dropped behind the reload slot.
  - **A span layer outside the reload slot (the previous default):** the offer and report have no request span to write to since M243, and the layer would change `logs.rs` for no remaining caller.
  - **An `info` span per gateway operation:** it would add `tracing` spans to a hot path and still meet the reload slot.
  - **Fields declared in `trace.rs`:** couples the generic router to fleet stage names.
  - **One trace per event via W3C `traceparent`:** changes the runner protocol against `observability.md` §Traces.
  - **Extending the `fleet.delivery` span with database timestamps:** one clock, but no runner or edge breakdown.
  - **nftables `socket cgroupv2`:** systemd recreates the runner's cgroup on restart, so the rule goes stale.
  - **Unix-socket receiver:** the runner's exporter accepts only `http(s)` URLs with a host, so it would need an `af_otlp` transport change.
  - **Receiver credential:** breaks Invariant 3.
  - **Moving dev off `allow_all`:** changes what fleets can reach.
  - **A trace-context column on `fleet.admissions`:** a schema change for a join attributes already provide.
- **Patch-vs-refactor verdict:** a patch; §1 adds one module and marks, and §2's infrastructure is shipped.

## Discovery (consult log)

- **Rewritten for the runner stream, Nov 10, 2026:**
  > Indy (Nov 10, 2026): chose "Rewrite §1 for the gateway now" — context: asked how M233_002 should start beside M245_001, given §1 targeted handlers M243 removed.
  - EGM implementation-readiness and codebase-details reviews ran from a Claude Fable 5.2 session against main `51d41a9e3`: 18 findings (8 and 10, with overlap). All are applied: §1 redesigned, §0 cut to the daemon, §2 and §4 marked shipped, the rubric and rule names corrected.
  - Removed paths: `afd_api_runner/src/handler/runner/lease/mod.rs`, `report.rs` and `afd_fleet/src/lease/pull/poll.rs` no longer exist (M243); `deploy/fly/dragonfly-dev/` went with M244; `scripts/check_workflow_rules.py`, `.orly/docs/**` and `.orly/dispatch/**` are absent.
  - The collector, its rule and its checksum pin shipped as `deploy/baremetal/otelcol-runner.{yml,service,nft}` and `install-collector.sh`, with `make check-runner-collector-admission` as the proof. Dimensions 2.2, 2.4 and 2.6 fold into 2.1 and the Nov 04 installation record below.
  - The old Stage 9 boundaries are now the stage table in §1.

- **Returned to pending, Nov 10, 2026:**
  > Indy (Nov 10, 2026): "also M233_002 bring it back to pending, since nothing is active now." — context: returned from active/ to pending/; §0.1 and §5 shipped in #786, §1–§4 and §0.2–§0.3 remain open, and §0.3's dragonfly-dev check is superseded by M244, which deletes that app.

- **Row implementation, Nov 04, 2026:** §4 uses the successful plain-search query and numeric span fields from M238 evidence. The metrics query returns no series; local-blocks enablement cannot be established through the exposed Grafana proxy routes. Two segments are legitimately absent in the tool-free, memory-window sample. Fleet-wide metrics are blocked by Grafana rejecting delta metrics at ingest; Indy subsequently requested "i nee 1 PR", so the export repair ships in the same pull request as this row and the recorded evidence. The Explore search screenshot shows ten matching spans; no rendered-row screenshot is claimed. §1–§3 remain unstarted, and §0.1/§5 remains predecessor history, excluded from active files and tests.

- **Scope transfer, Nov 04, 2026:** Indy selected "Recommended: close M233_001 and move §1–§4 to M233_002". M233_001 Dimensions 1.1–1.5, 2.1–2.7 and 3.1–3.3 keep their numbers. The old asset requirement 4.1 becomes the rebased row check 4.1. Old verb accounting Dimension 4.2 moves to §1.6 and P0 rubric R7, preserving the accounting part of predecessor R5; its asset-validity part stays R5. Old coverage and edge-ordering Dimension 4.3 moves to §3.4. Current §4.2–§4.3 add row drift checks and the rendered-row witness. Rubric R0–R4 retains the predecessor mapping. Remaining §0.2–§0.3 stays tracked here so closing the predecessor loses no sizing check. §4 is rebased on the M238 delivery attributes; it does not wait for future per-verb spans.
- **Save finalization approved, Oct 27, 2026:** Indy: "The when i say save" and "You feed to the model wait for the model reply and `Saving setup...` and i see `Save done v3`", followed by "Yes correct lets go, signed off approved." The completed report's pinned candidate, including changes from this turn, is saved without another Save click; a cut response fails without saving. The existing fleet's conflicting app-selection guidance remains unsaved. > Indy (2026-10-27): "Okay whatz up, if we cant verify then lets move it up?" / "to a PR" / "undraft the PR, since i would like to merge and test" — context: publish current chat changes as a ready Pull Request with missing live proof disclosed; M233 stays active and unfinished timing work remains open.
- **Review deferred by owner:** "And i think for now remove the `Review Setup`" followed by "we will deal with it later". Remove the action from this change.
- **Incomplete fleet conversation:** Indy: "how do we make it better to even un configured fleets can receive chat". Reuse existing setup admission while required access is missing; do not admit a runtime job that cannot execute. Static credentials remain bound to an explicitly selected host.
- **Chronological activity:** Indy: "I think the chat must be the funneling point for all activity" and "Even the memory tab for a Fleet chat - is not needed". Memory sharing remains workspace-scoped; no global sharing permission is added.
- **Authoring addition:** Indy chose "Use the current M233 worktree", then approved "Yes correct lets do that" for Edit setup returning to the original authoring page. "once we ahve this fixed we need to run this on the local on the current fly.io thing you are on" and "and see the experience" authorize local verification against the existing development backend. Fonts stay unchanged. These choices supersede the prior embedded setup editor in the design and architecture documents.
- **Consults**
  - `docs/architecture/observability.md` §Traces, §runner collector, §Signal routing and §PostHog read (Oct 26, 2026).
  - Owner request, Indy (Oct 26, 2026), verbatim: "I want you to have logging, observability added to so we could to this as a fleet for me to check it and optimize". Agent's interpretation: a per-fleet, per-event stage breakdown in Grafana.
  - **Throttled baseline** on `shared-cpu-1x`, event `01a17627-25c5-761d-9794-f1ab0b40d9fc`, 03:00:38 UTC; daemon logs and Tempo via `gcx`, Postgres read-only:
    - Send to 202: 0.78 s, including 50 ms in the daemon. Lease request: 1.67 s over 36 SQL statements; hydrate: 0.33 s; capture: 0.39 s; report: 1.60 s over 28 statements; `event_complete`: 5.42 s after Send.
  - **Ruled out** (inference from these measurements: the cause is CPU quota throttling on `shared-cpu-1x`):
    - The database: PlanetScale Insights, branch `useast`, `insert into billing.holds` p50 and p99 0 ms server-side.
    - The pooler and Nagle's algorithm: an in-region Go probe on the daemon machine reproduced the delay with `TCP_NODELAY` on and off, on ports 6432 and 5432.
    - The network entirely: with no database, 169 of 2,000 idle 1 ms sleeps woke 70–76 ms late; with one busy thread, 1,988 of 2,000 woke late (median 158.6 ms).
  - **`performance-1x` trial:** 0 of 8,610 probe statements took ≥ 40 ms; three chats took 1.53–1.86 s.
  - **On `shared-cpu-2x` after §0:** three chats took 1.53–1.94 s (05:11 UTC, browser probe through the steer route).
  - **Runner stopwatch** (`lease_completed` lines in the dev runner's journal, `journalctl -u agentsfleet-runner`, 04:21–05:43 UTC), dev acceptance fleet after the resize:
    - Runner wall time: 700–1,100 ms; memory: 62–86 ms; model: 346–684 ms; first token: 317–489 ms; report: 249–347 ms. `tools=0` and no `sandbox` segment: no sandbox ran. Throttled 04:21 run: memory 718 ms, report 1,883 ms.
  - **`DeliveryStage` export gap:** Prometheus has no series matching `*delivery*` (05:45 UTC), although `docs/metrics.census.tsv` declares `agentsfleet_fleet_delivery_stage_seconds` as a delta histogram.
  - **Pricing:** `https://docs.fly.io/about/pricing`, Oct 26, 2026, `iad`: `performance-2x` 4 GB $66 a month; `shared-cpu-4x` 4 GB about $26.76, derived from $6 per GB and the shared-vCPU rate; three machines each.
- **Required human decisions — pending**
  - **D1, resolved for dev on Nov 04, 2026:**
    - **Previous choice:** Indy, Oct 26, 2026: "Reuse grafana-dev".
    - **Corrected risk:** bubblewrap sandboxes share one kernel (`runner_execution.md` §Sandbox engines). Any code-running tenant with a kernel exploit, or anyone with root on the runner host, could read the collector's key. That key can also write the daemon's telemetry, because dev and prod share one Grafana tenant. The key's scopes are to be read in Grafana.
    - **Option A:** keep `grafana-dev`.
    - **Option B:** a dedicated write-only token in `ZMB_CD_DEV`.
    - Indy reported installation from #801 commit `9b12fe0d8` using `ZMB_CD_DEV/grafana-dev`, and confirmed: "i think we are good, the sandbox wont reach yet". The key choice is resolved for this dev installation; production is not included.
  - **D2, does not block:** browser telemetry export, or DevTools marks only (the agent recommends marks only).
- **Decisions made (Oct 26, 2026)**
  - "I am okay with the bottleneck in dev, but i need you to confirm the reason, so i can focus on that in production".
  - "For development also strip down the debug level in development … bring down the sizing of dragonfly as well as it costs 60$".
  - Asked whether to pin `info` in rule `workflow-daemon-log-level` of `scripts/check_workflow_rules.py`, Indy chose "Yes, pin info" (the option the agent marked Recommended). This is the gate-edit approval, superseding the M187 call to deploy dev at `debug`.
  - Asked to move the dev daemon, Indy chose "Yes, shared-cpu-2x 1GB" (the option the agent marked Recommended). Oct 27, 2026: "Can you move tsrs to 0.20.0-ts7.1.0-beta"; update all existing compiler pins and both Bun lockfiles, then verify lint and builds.
- **Metrics review** — no metric family added; span attributes only; no analytics funnel playbook update required, because no product event changes.
- **Skill-chain outcomes**
  - **egm review, Stage 5:** report `egm-review/zoom_out_report.md`. Indy: "Okay do that". Changes are in revision `7123d5c05`.
  - **egm review, Stage 6:** report `egm-review/comprehension_report.md`; it ran against the pre-revision text, and each item carries a disposition.
  - **egm review, Stage 7:** report `egm-review/goldfish_review_report.md`. Indy: "Auto accept", applied here. Issue 22 (detailed-range items) carries to Stage 9.
  - **Pending:** `/orly-write-unit-test`, `/review`, `orly-babysit-prs`.
- **Deferrals**
  - > Indy (2026-10-26 05:09): "Okay do that" — context: Stage 5 finding 9 recommended shipping traces and runner metrics only and leaving journald log forwarding to a follow-up.
  - > Indy (2026-10-26 05:47): "Auto accept" — context: Stage 7 Issue 9 proposed recording that deferral; runner log forwarding through the collector is future work.

- **Runner collector installation, Nov 04, 2026 (Indy forwarding the host session):**
  contrib 0.164.0 active since 09:06:30 UTC as `otelcol-runner`, no restarts,
  loopback listeners only, 40 MiB under MemoryMax 256 MiB. Credential file
  `/etc/agentsfleet/otelcol-runner.env` is root:root mode 0600. Runner restarted
  at 09:07:51 UTC with no active leases and logged `telemetry_export_started`.
  Root received HTTP 200; uid 65534 and the deploy user were dropped, IPv6
  loopback was refused. First lease data at 09:18:27 UTC: 3 spans and 1 metric
  point accepted/exported, zero refused, warnings or errors. At 09:18:52 UTC
  the host session found a 995 ms `runner.lease` trace and the provider-turn
  histogram in Grafana. This session independently confirmed target_info,
  provider-turn count/sum and a runner.lease result. Real-sandbox admission
  remains pending; matching the sandbox uid is not the same witness.

- **Owner-requested cleanup:** Indy removed the standalone dashboard regression suites and dev evidence folder after the dashboard work. The production asset validator and live drift check remain. Per-request daemon stages, browser/edge timing and sizing checks remain unimplemented or unverified; this successor stays active.
