<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M243_001: Each runner host holds one authenticated stream that carries its work, and an idle host sends no lease request

**Prototype:** v2.0.0

**Milestone:** M243

**Workstream:** 001

**Date:** Nov 06, 2026

**Status:** PENDING

**Priority:** P1, operators see work start late and a kill that never stops a run; every self-hosted runner will take this path

**Categories:** API, DOCS, INFRA, OBS

**Batch:** B1, the third push-scheduler milestone; it starts after M242 merges, and M244 starts after it

**Depends on:** M241_001 (the bench's statement paths and `push_scheduler.md`); M242_001 and M242_002 (the one-statement intake rings the doorbell this stream listens to, and M242_002's folds request moves onto the stream)

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 06, 2026) from the EGM brief and source research on main 14539a73e

**Canonical architecture:** `docs/architecture/push_scheduler.md` §M243 (created by M241_001), and `docs/architecture/runner_protocol.md`, which §3 creates

---

## Overview

**Goal (testable):** A runner host holds one authenticated stream to `agentsfleetd` that carries offers, renewals, activity, reports and cancels, so a streamed 10 s chat reads no lease target, an idle host sends no lease request, and a kill cancels a running lease from any replica.

**Problem:**
- Every idle worker sends `POST /v1/runners/me/leases` each `NO_WORK_RETRY_AFTER` (1 s, `af_core/src/timing.rs`; `afr_supervisor/src/worker_pool/mod.rs`), and each request reads the runner token, deliberately uncached (`afd_http/src/auth/guard.rs`; `docs/AUTH.md` §Validation).
- To shorten the wait, a host holds its heartbeat open up to 10 s for a doorbell ring, at most 64 per process (`afd_api_runner/src/handler/runner/wake.rs`, `afd_dragonfly/src/doorbell/mod.rs`, `HELD_BEATS_MAX`).
- Each 250 ms activity flush is a `POST` that reads the token and then the lease target (`afd_fleet/src/lease/activity/mod.rs`): 40 posts and 80 statements per 10 s chat; M241's bench counts the 40 target reads.
- A kill stops nothing already running. It writes nothing to `runner.leases`, and no cancel channel exists (`runner_fleet.md` §Steer, kill, pause).
- Self-hosted runners must reach the cloud daemon through its public API and hold no broker credential (Indy, Discovery).

**Solution summary:** `GET /v1/runners/me/stream` upgrades to one WebSocket per host, authenticated once. Down it go offers, heartbeat answers, renewal answers, report acknowledgements, folded input, cancels and `go_away`. Up it go hello, credits, heartbeats, renewal batches, activity, reports and folds requests. A gateway in each replica claims for a host when the doorbell rings or its credits rise. Postgres marks a cancel and records which connection owns each runner, so a lost publish costs latency, never correctness. Pub/sub moves behind an `afd_bus::Bus` seam with one Dragonfly implementation, so M244 adds NATS without touching callers. The poll, the held heartbeat, the per-flush activity posts and the six retired routes are deleted.

**Design flow:**

```
agentsfleet-runner ──WebSocket over HTTPS──► cloudflared ──► agentsfleetd replica (gateway)
 hello{running, credits, kinds}  ─►  token read once; connection_id written; live leases read (rebuild)
                                 ◄─  welcome · cancel for leases Postgres no longer holds or has marked
 admission commits ─► Bus wake(shard) ─► every replica: session with credits ─► Plane::claim ─► offer
 every 5 s, one probe per replica: work waiting with no live lease? ─► a claim pass
 heartbeat every 10 s ─► Runners::heartbeat (liveness, holds, policy, status, token hash, connection_id)
 renew batch every 5 s ─► Plane::renew per lease ─► renewed{deadline | refusal}
 activity ─► lease target cached at offer or rebuild ─► Bus tail (0 statements)
 report ─► Plane::report (lease-keyed, idempotent) ─► report_ack ─► runner spool deletes it
 kill ─► cancel_requested_at in the kill transaction ─► Bus runner command ─► owning replica ─► cancel
 operator edit, revoke, rotate ─► Bus recheck ─► owning replica re-reads the row ─► answer or close
 drain ─► go_away ─► runner reconnects after a jittered delay
```

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): one pushed stream per runner host, behind a bus seam
- **Intent (one sentence):** work reaches a runner host the moment it is admitted, a kill stops a running run, and an idle host costs nothing.
- **Authoring handshake:** Indy wants the lease path rebuilt for scale before production, with self-hosted runners pointing at the cloud daemon. This milestone replaces polling with one pushed stream per host and builds the bus seam M244 needs. ASSUMPTIONS I'M MAKING:
  1. M242_001 and M242_002 merge first, and M242_002's folds request keeps its meaning when it moves onto the stream.
  2. No self-hosted runner exists yet, so runner and daemon upgrade together. A runner of an older build gets `404` on its old routes and takes no work until upgraded.
  3. Batched renewal here is framing: each lease keeps its own renewal statements until M245 makes it one statement per host.
  4. The bench drives a gateway session in process, so M241's statement paths measure what the stream sends.
  5. Schema slots 2750 and 2760 are placeholders, renumbered from `origin/main` at CHORE(open), because PR #803 takes `schema/2190`–`2690`. CHORE(open) also re-verifies Files Changed against `origin/main`.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_api_runner/src/handler/runner/heartbeat.rs` and `afd_runner/src/heartbeat/mod.rs` — what one beat writes and answers; the stream carries both unchanged except `wake` and `woken`.
2. `rustd/crates/daemon/afd_runner/src/capacity/mod.rs` — `lock` refuses a non-active or degraded host inside every issue transaction, and a revoked or degraded one inside every renewal; that is the grant re-check `docs/AUTH.md` requires once auth stops running per request.
3. `rustd/crates/daemon/afd_dragonfly/src/hub/mod.rs` and `doorbell/mod.rs` — `Received`, the gap signal, and the ring the gateway listens to.
4. `rustd/crates/runner/afr_supervisor/src/report_spool/mod.rs` and `drainer/mod.rs` — the durable report spool the stream's acknowledgement completes.
5. `https://github.com/openai/codex/blob/cc7ba33/codex-rs/app-server-transport/src/transport/remote_control/websocket.rs` (Indy's checkout at `~/Projects/oss/rs/codex`) — an outbound WebSocket client with pings, a pong timeout and jittered reconnect.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M243_001_P1_API_DOCS_INFRA_OBS_RUNNERS_HOLD_ONE_PUSH_STREAM.md` | CREATE, then MOVE to `active/` and `done/` | this spec |
| `docs/architecture/runner_protocol.md`, `docs/architecture/README.md` | CREATE, EDIT | §3: frames, ordering, rebuild, cancel, supersede, timings; §1's transport evidence; index row |
| `docs/architecture/push_scheduler.md` | EDIT | §M243 status; retires "no cancel channel" and "not a general scheduler"; §8's baseline row |
| `docs/architecture/runner_fleet.md`, `lease_flow.md`, `data_flow.md`, `scaling.md`, `observability.md`, `api_resources.md`, `runner_execution.md`, `user_flow.md`, `scenarios/fleet-to-fleet.md`, `scenarios/slack-incident-responder.md` | EDIT | §7: the held heartbeat, the poll, the retired routes, kill and the idle-cost knob, described as the stream |
| `docs/AUTH.md` | EDIT | §7: auth once per connection, revocation pushed with its backstop, the grant re-check |
| `docs/metrics.census.tsv` | EDIT | §4, §7: two families in, one out |
| `public/openapi.json` | EDIT | §7: regenerated from `rustd/` with `--no-banner` |
| `schema/2750_runner_leases_cancel_requested.sql`, `schema/2760_runner_runners_connection.sql`, `rustd/crates/daemon/afd_db/src/migration.rs` | CREATE, EDIT | §3: the cancel marker and the owning connection |
| `rustd/Cargo.toml`, `rustd/crates/daemon/afd_bus/**` | EDIT, CREATE | §2: the bus crate, its `wake` module and its Dragonfly implementation in `src/dragonfly/`; axum `ws`; `tokio-tungstenite` for the runner |
| `rustd/crates/daemon/afd_dragonfly/src/lib.rs`, `doorbell/**` | EDIT, DELETE | §2: the doorbell moves to `afd_bus::wake` |
| `rustd/crates/daemon/afd_admission/src/wake.rs`, `afd_approval/src/inbox/announce.rs`, `afd_approval/src/inbox/resolve/mod.rs`, `afd_authoring/src/model/progress/context.rs`, `publish.rs`, `afd_events/src/activity.rs`, `afd_events/src/steer/mod.rs`, `afd_fleet/src/lease/bracket.rs`, `afd_gate/src/gate/park.rs`, `afd_sse/src/fanin.rs`, `live.rs`, `ceiling.rs`, `afd_api_tenant/src/handler/draft/stream/mod.rs` | EDIT | §2: every production publisher and subscriber goes through `afd_bus::Handle` |
| test harnesses that build a hub: `afd_api/tests/harness/**`, `afd_api/tests/support/**`, `afd_sse/tests/**`, `afd_approval/tests/integration_inbox_*.rs`, `afd_authoring/tests/draft/model_calls/progress/fixture.rs`, `afd_events/tests/integration_backfill.rs`, `afd_gate/tests/integration_gate_tail.rs`, `afd_fleet/tests/integration_wall_counters.rs`, `agentsfleetd/tests/integration_cluster_outcomes.rs`, `afd_bench/src/datastores/mod.rs`, `afd_bench/src/lane/tail/**` | EDIT | §2: build an `afd_bus::Handle` instead |
| `rustd/crates/protocol/af_wire/src/protocol/**`, `paths.rs`, `runner/mod.rs`, `lease/request.rs`, `lease/response.rs`, `af_wire/tests/{routes,strictness,validation_holds,claim_selection}.rs` | CREATE, EDIT | §3: frames; `RUNNER_STREAM`; `wake`, `woken`, `LeaseRequest` and `LeaseResponse` go |
| `rustd/crates/foundation/af_core/src/timing.rs`, `limits.rs`, `error_code/fleet.rs`, `error_code/mod.rs`, `problem/fleet.rs` | EDIT | §3: stream timings and assertions; three codes; §7: three constants go |
| `rustd/crates/daemon/afd_gateway/**` | CREATE | §4: registry, session, offer passes, target cache, rebuild, recheck, report path and its telemetry |
| `rustd/crates/daemon/afd_http/src/route/runner.rs`, `afd_http/src/services/mod.rs` and the `Services` trait file, `services/leasing/mod.rs`, `services/leasing/plane.rs` | EDIT | §4: the stream route; `gateway()`; `wait_for_work` and `activity` leave `Leasing` |
| `rustd/crates/daemon/afd_api_runner/src/handler/runner/stream.rs`, `mod.rs`, `usage.rs`, `lib.rs`, `openapi.rs` | CREATE, EDIT | §4: upgrade, version check, ceiling permit, drain registration |
| `rustd/crates/daemon/afd_fleet/src/lease/claim.rs`, `activity/mod.rs`, `store.rs`, `answer/mod.rs`, `renew/mod.rs`, `rebuild.rs`, `sql/select_runner_live_leases.sql`, `sql/select_waiting_admission.sql`, `sql/select_lease_for_renew.sql`, `sql/activity.rs` | EDIT, CREATE | §4: claim answers its target; rebuild read; the fallback probe; §5: renewal reads the cancel marker |
| `rustd/crates/daemon/afd_runner/src/heartbeat/mod.rs`, `sql/runner/select_runner_assigned_policy.sql`, `sql/runner/write_runner_connection.sql`, `sql/runner/mod.rs` | EDIT, CREATE | §4, §5: the beat's read carries status, token-hash match and `connection_id`; connect writes it |
| `rustd/crates/daemon/afd_authoring/src/creation/patch.rs`, `creation/sql/mark_leases_cancel_requested.sql`, `afd_api_tenant/src/handler/fleet/native/patch.rs` | EDIT, CREATE | §5: the kill marks running leases in its transaction; the handler publishes `Cancel` after commit |
| `rustd/crates/daemon/afd_api_operator/src/handler/operator/runner_patch.rs`, `runner_delete.rs` | EDIT | §5: publish `Recheck` after every runner-row write |
| `rustd/crates/protocol/af_observability/src/metrics/declared/fleet.rs`, `metrics/label/**`, `producers/fleet/mod.rs` | EDIT | §4, §7: stream closes and offer claims in; wake answers out |
| `rustd/crates/daemon/agentsfleetd/src/plane/mod.rs`, `serve/mod.rs`, `serve/runtime.rs`, `serve/optional.rs`, `serve/drain/mod.rs`, `serve/drain/tests.rs` | EDIT | §2, §4: build the bus and gateway once; `go_away` on drain; the doorbell close goes |
| `rustd/crates/daemon/agentsfleetd/tests/integration_runner_stream/**` | CREATE | §4–§6: reconnect, kill across replicas, revocation, rotation, supersede, blackhole, chat statements |
| `rustd/crates/daemon/agentsfleetd/tests/integration_runner_e2e/**`, `integration_rust_runner*.rs`, `integration_runner_activity*.rs`, `integration_runner_messages*.rs`, `integration_runner_shapes.rs` | EDIT | §6: drive leases over the stream |
| `rustd/crates/daemon/afd_api/tests/route_inventory.rs`, `route_meta_total.rs`, `harness/stubs_runner.rs`, `afd_api/src/openapi/stability/mod.rs` | EDIT | §4, §7: one route in, six out |
| `rustd/crates/runner/afr_supervisor/src/downlink/**`, `uplink/**` | CREATE | §6: connect, hello, reconnect, dispatch; the prioritised writer |
| `rustd/crates/runner/afr_supervisor/src/{worker_pool,renew,activity,report,drainer,lease_loop,client,halt}/**`, `inbox.rs`, `lib.rs`, `test_util.rs`, `Cargo.toml` | EDIT | §6: workers take offers; renewals batch per host; activity, reports and folds ride the uplink |
| `rustd/crates/runner/afr_supervisor/src/test_support/daemon/**`, `agentsfleet_runner/tests/fake_daemon.rs`, `afr_sandbox/examples/kernel_lane/supervisor_plane.rs`, `supervisor_checks.rs` | EDIT | §6: the fake daemons speak the stream |
| `rustd/crates/daemon/afd_bench/src/lane/lease/drain/{runner,window,mod,tests}.rs`, `lane/lease/saturation/production.rs`, `lane/lease/seed/mod.rs`, `make/bench.mk`, `bench/baselines/lease.rig.json` | EDIT | §8: the drain drives a session; idle totals replace per-poll ratios; median baseline |
| retired: `afd_api_runner/src/handler/runner/{heartbeat,wake,wake_tests,renew,activity,report,folds}.rs`, `handler/runner/lease/**`, `afr_supervisor/src/client/folds.rs`, `afr_supervisor/src/heartbeat/**`, `afd_api/tests/integration_runner_heartbeat.rs`, `agentsfleetd/tests/integration_runner_wake.rs`, `afd_approval/tests/integration_grant_request.rs` | DELETE, EDIT | §7: the pull path and its tests; the grant-request test's comment names a deleted constant |
| `deploy/fly/cloudflared-dev/config.yml`, `cloudflared-prod/config.yml` | EDIT only if §1 requires it | an `originRequest` setting; needs Indy's approval (Discovery, open blocker) |
| `~/Projects/docs` branch `chore/m243-runner-stream-changelog`: `runners.mdx`, `runners/install.mdx`, `cli/agentsfleet.mdx`, `api-reference/error-codes.mdx`, `changelog.mdx` | EDIT | §7: heartbeat wording, proxies must pass a WebSocket upgrade, kill stops a running run, three codes; API pages for the route after merge |

## Applicable Rules

- **`code.md`** — NLG, NDC, NLR, HLP, ORP, CODE-SEAM, TIM, OWN, FLS, FLL, TGU, NTP, ECL, NTE, CODE-MSID
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-RACE, RS-UNWRAP, RS-FEATURE, RS-REF, RS-ENV
- **`domains/http.md`** — HTTP-ROUTE, HTTP-CLASS, HTTP-METHOD, HTTP-GARDE, HTTP-VALID, HTTP-EXTRACT
- **`domains/http-surface.md`** — HTTP-OPENAPI, HTTP-ANNOTATE, HTTP-SURFACE, HTTP-BREAK, EP4
- **`domains/http-responses.md`** — HTTP-REGISTRY, HTTP-NEWCODE, HTTP-TITLE, HTTP-STATUS, HTTP-DETAIL
- **`domains/auth.md`** — AUTH-DOC, AUTH-GUARD, AUTH-NOLOG, AUTH-LOGID
- **`domains/sql.md`** — SCH, MIG, SQL-SLOT, SQL-READER, SQL-TSTYPE, SQL-TIMECOL, STS, SQL-FILE, SQL-PREPARE, SQL-TIMEOUT, SQL-STAMP
- **`domains/logging.md`** — LOG-EVENT, LOG-PAIR, LOG-HOT, LOG-CODE, OBS
- **`domains/architecture.md`** — ARCH-LAND, ARCH-HOMES, ARCH-PUBLISHED
- **`tests.md`** — TEST-REAL, TEST-INJECT, TEST-PARTIAL, TEST-ACKLOSS, TEST-RESIDUAL, TEST-RACE, TEST-COUNTER, TEST-SCOPE, TEST-NEG, STR, TEST-PORT, TEST-LANE, TFX
- **`docs.md`** — DOC-02, DOC-03, DOC-11
- **`spec.md`** — SPEC-DECIDE, SPEC-DEFER, SPEC-PIN

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `docs.language` at `orly gate verify` and `orly gate pr` | yes, a new page and edited prose | sentences under 25 words, paragraphs of at most three sentences; scan before each commit |
| `make lint-all` (clippy `-D warnings`, `cargo shear`) | yes, two new crates and two new dependencies | each dependency lands with its first caller (HLP) |
| Route inventory and meta-total tests | yes, one route in, six out | `INVENTORY` and the counts change in the commit that changes the routes |
| Metrics census and producer tests | yes | census rows, declarations and producers move together |
| OpenAPI and error-registry parity | yes | regenerate `public/openapi.json`; each code lands with its registry row and problem entry |
| Linux-only lanes | yes, `afr_sandbox/examples/kernel_lane` | linted and tested on Linux in a Codex cloud task |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, the session and the downlink grow | split by concern: `session/{hello,frames,recheck}.rs`, `downlink/{connect,dispatch,reconnect}.rs` |

## Prior-Art / Reference Implementations

- **Reference:** `codex-rs/app-server-transport/src/transport/remote_control/websocket.rs` (`tokio-tungstenite` 0.28, `proxy` and `rustls-tls-native-roots`) — client pings, a pong timeout, jittered reconnect. Divergence: no replay buffer, because Postgres rebuilds the state.
- **Reference:** `afd_api_tenant/src/handler/stream/revocable/mod.rs` — a held stream that re-checks access; the session re-checks on each heartbeat instead of a timer.
- **Reference:** `afr_supervisor/src/client/mod.rs` — `RunnerApi` as an `#[async_trait]` trait object, the shape `afd_bus::Bus` takes.
- **Reference:** `docs/architecture/push_scheduler.md` §M243 — the target design this spec builds.

## Sections (implementation slices)

### §1 — The transport is chosen on evidence through a Cloudflare tunnel

Production ingress is one `cloudflared` tunnel in front of the daemons (`deploy/fly/cloudflared-prod/config.yml`), whose behaviour with long streams nobody has measured. On Indy's laptop, run `websocat` as an echo server behind `cloudflared tunnel --url` and connect a `websocat` client through it. Record the `cloudflared` and `websocat` versions, then run each check below. **Implementation default:** a quick tunnel, because it crosses the same Cloudflare edge without touching the deployed tunnel's configuration, whose edit needs Indy's approval. WebSocket is chosen when all three checks pass. Otherwise the same checks run against an HTTP/2 stream with a streamed request body, and that wins if it passes. If neither passes, the work stops and Indy is asked. The evidence becomes `runner_protocol.md` §Transport evidence and is rerun when `cloudflared`, `tokio-tungstenite` or the tunnel configuration changes.

- **Dimension 1.1** — pings keep an idle stream open → Test `tunnel_idle_spike` (manual): with a ping every 15 s, a stream idle for 120 s stays open; with no ping, the cut time is recorded. A run shorter than 120 s proves nothing.
- **Dimension 1.2** — a tunnel restart surfaces, never hangs → Test `tunnel_restart_spike` (manual): killing `cloudflared` mid-stream ends the client's stream within 30 s, by a close frame or a ping timeout.
- **Dimension 1.3** — a report-sized frame passes → Test `tunnel_frame_spike` (manual): a 2 MiB text message echoes back byte-identical.

### §2 — Pub/sub goes behind one bus seam

`afd_bus` holds the `Bus` trait (`publish`, `publish_batch`, `subscribe`) and the typed `Subject`: a fleet's tail, a draft's events, a wake shard and a runner's commands. Each implementation spells its own names. `Subscription` and `Received { Message, Lagged, Gap }` move from `afd_dragonfly::hub` with their meaning unchanged. `Command { Cancel { lease_id }, Recheck }` is the typed runner-command payload. The doorbell moves whole into `afd_bus::wake`, keeping 16 shards, 64 hints and "ring after commit, count a lost ring". §7 deletes its hold. The Dragonfly implementation lives in `afd_bus/src/dragonfly/`, where M244 deletes it, over `afd_dragonfly`'s client and hub. It keeps today's channel spellings and `SPUBLISH`/`SSUBSCRIBE`, and hash-tags the runner id so one runner's commands land in one slot. Boot builds one `afd_bus::Handle` and hands it to every publisher and subscriber. **Implementation default:** the handle wraps `Arc<dyn Bus>`, because boot chooses the implementation once and M244 swaps it without threading a type parameter through every store; nothing downcasts, so NTE's concern does not arise. **Implementation default:** a daemon whose hub did not start still subscribes to nothing, as `afd_sse/src/live.rs` does today, because M244's frozen-screen fix owns that change and its A/B compares the same work.

- **Dimension 2.1** — no production crate names a Dragonfly pub/sub type → Test `git grep -n -w -e SubscriptionHub -e publish_tail -e "doorbell::publish" -- 'rustd/crates/*/src/*' ':!rustd/crates/daemon/afd_dragonfly' ':!rustd/crates/daemon/afd_bus'` (manual): prints nothing; today it prints hits in `afd_sse`, `afd_admission` and `afd_approval`.
- **Dimension 2.2** — a lost subscription reaches a bus subscriber as a gap → Test `a_repaired_node_gaps_every_bus_subscription` (integration, ported from the hub's gap fault test): after a node repair, a subscription opened through `afd_bus` reads `Received::Gap`.
- **Dimension 2.3** — runner commands cross replicas and stay addressed → Test `a_runner_command_crosses_replicas` (integration): `Cancel` published through one hub reaches a subscription for that runner on a second hub; a subscription for another runner reads nothing within 1 s.
- **Dimension 2.4** — a lost ring is counted, never refused → Test `a_failed_ring_counts_and_admits` (integration, ported from `doorbell/tests.rs`): with the publish deadline forced to elapse, the admission commits and `agentsfleet_fleet_ready_wake_failures_total{kind="ring"}` rises by 1.

### §3 — Protocol, schema and timings are fixed before the code that uses them

`af_wire::protocol` declares the frames in Interfaces as JSON text tagged by `type`. Up frames refuse unknown fields; their bounds are `garde` on the type, reusing today's bounds for heartbeat, activity and report bodies, with `MAX_WORKERS` capping `running` and renewal entries. `STREAM_FRAME_BYTES_MAX` caps up frames at 2 MiB, axum's default request-body limit the runner routes use today, so any report that fits the route fits a frame. `runner_protocol.md` records the `Cargo.lock` versions of axum's WebSocket stack and `tokio-tungstenite`, and §1's spike reruns when either changes. Two schema slots mirror the `ADD COLUMN` slot `schema/2130_authoring_dispatch_deadline.sql`. `runner.leases.cancel_requested_at` is a NULL marker: the kill transaction writes it, and renewal and rebuild read it. `runner.runners.connection_id` is UUIDv7 or NULL: connect writes it, and the heartbeat read compares it. Neither slot adds an index: rebuild uses `idx_leases_runner_id_status`, and the kill mark uses `idx_leases_fleet_id_status_fencing_token`. Timing constants and their assertions go into `af_core/src/timing.rs` (Interfaces). UZ-RUN-021, -022 and -023 land as family constants, `REGISTRY` rows and problem entries in one commit. `runner_protocol.md` is written in this Section, before the gateway (ARCH-LAND).

- **Dimension 3.1** — an up frame with an unknown field is refused → Test `an_up_frame_with_an_unknown_field_is_refused` (unit): a `renew_batch` frame with `"extra":1` fails to decode; the same frame without it decodes.
- **Dimension 3.2** — bounds hold → Test `a_renew_batch_past_max_workers_is_refused` (unit): `MAX_WORKERS + 1` entries are refused naming the bound; `MAX_WORKERS` entries pass.
- **Dimension 3.3** — the timings leave a full renewal window after a reconnect → Test `stream_timings_leave_a_renewal_window_after_reconnect` (unit): asserts each Interfaces relation against the production constants (TFX), and fails if `STREAM_ANSWER_TIMEOUT` is raised to 15 s.
- **Dimension 3.4** — the new statements prepare and the slots migrate → Test `test_every_sql_file_prepares` (integration, `afd_db/tests/integration_migrate/statements.rs`): passes with the four new `.sql` files and two more slots in the migration list.

### §4 — The gateway serves the stream

`GET /v1/runners/me/stream` is `Guard::RunnerBearer`, `RouteClass::Stream`, scoped `runner:self`. The handler authenticates once and refuses an unknown `Sec-WebSocket-Protocol` before upgrading. The session then owns its stream-ceiling permit and its drain registration until it closes. **Implementation default:** `GET …/stream`, because a WebSocket upgrade is a `GET` (Request for Comments (RFC) 6455), and every held connection here ends in `stream` (`…/events/stream`). **Implementation default:** the stream shares `SSE_MAX_STREAMS` (256 a replica, `agentsfleetd/src/preflight/knobs.rs`), because about 150 hosts spread over the replicas use a fraction of it; a refused upgrade answers `503` with `Retry-After`.

`afd_gateway` keeps one registry per replica. On `hello`, the session writes its `connection_id` and commits, publishes `Recheck` for the runner, and reads the runner's live leases (`select_runner_live_leases.sql`) into its lease-target cache. It answers `welcome`. A lease in `running` that Postgres no longer holds gets `cancel` with UZ-RUN-011, and a marked lease gets `cancel` with UZ-RUN-022. A lease Postgres holds that `running` omits is left to lapse. **Implementation default:** lapse, because its body carried secrets the daemon never stored, and a second expiry writer would race renewal; the worst case is `LEASE_TTL` plus `OFFER_FALLBACK_TICK`, only when a cut lands between issue and delivery.

Offers come from claim passes. A pass walks the local sessions with credits: first those whose last heartbeat holds the ring's fleet, then the rest, longest-waiting first. For each, it runs `Plane::claim` with that session's holds and pushes `offer` while claims issue leases, and moves on at the first empty claim. A pass covering k rings ends after k leases or at the end of the walk. An empty pass so costs one claim per session with credits, the fan-out each ring causes today when every held beat polls once. Rings during a pass coalesce into one more pass, and a rise in a session's credits runs a pass for that session alone.

Every `OFFER_FALLBACK_TICK`, each replica runs one probe: does any unsettled admission wait with no live lease on its fleet (`select_waiting_admission.sql`, the candidate read's runner-free core)? A hit runs a pass. This catches a lost ring and a lease that lapsed with no ring, at one statement per replica per tick whatever the runner count. A `Gap` or `Lagged` on a wake shard counts as a ring; on a runner's command subject it runs `Recheck` and re-reads that session's cancel marks. Credits are a hint: the host lock's census decides whether a lease fits. A session whose last heartbeat answered degraded gets no pass, and the issue transaction's host lock refuses a degraded or non-active host whatever the session believes.

The other frames:
- **heartbeat** runs `Runners::heartbeat` and answers it. Its policy read is widened to return status, whether the stored token hash still equals the connection's, and `connection_id` (CODE-SEAM).
- **renew_batch** runs `Plane::renew` per entry, in order, and answers each lease's deadline or refusal code.
- **activity** publishes through the cache with no statement; only an unknown lease id is read from Postgres, as today.
- **report** runs `Plane::report` and answers `report_ack` with the saved result or a refusal code that says whether a retry helps. The report's spend and delivery telemetry moves here from the deleted handler.
- **folds** runs M242_002's `Leasing::fold` under the lease's fence and answers `folded`. **Implementation default:** request and answer, not an unasked push, because the runner consumes input only at a model-call boundary, so a push would arrive no sooner.

Authoring leases arrive as offers, because `Plane::claim` already picks either kind; their four per-lease routes stay HTTP. On drain the registry sends `go_away`, stops offering, answers frames already read and closes, bounded by `DRAIN_TIMEOUT`.

- **Dimension 4.1** — a streamed chat reads the token once and no lease target → Test `a_streamed_chat_reads_no_lease_target` (integration, a private database filtered by `dbid` as `afd_events/tests/queued/snapshot.rs` does): one lease, 2 renewal batches, 40 activity frames and a report over the stream make 1 call of the runner-token statement and 0 of the lease-target statement. Over HTTP the lease-target statement runs 40 times.
- **Dimension 4.2** — an unknown protocol is refused before the upgrade → Test `an_unknown_protocol_version_is_refused` (integration): `agentsfleet.runner.v9` answers `400` UZ-RUN-023 and no `101`.
- **Dimension 4.3** — reconnect rebuilds from Postgres → Test `reconnect_rebuilds_active_leases_from_postgres` (integration): Postgres holds leases A, B and marked M; `hello` lists A, C and M. A renews, C gets `cancel` UZ-RUN-011, M gets `cancel` UZ-RUN-022, and B is offered to nobody until it lapses.
- **Dimension 4.4** — rings coalesce and passes stay bounded → Test `rings_coalesce_into_one_claim_pass` (unit, paused time): 100 rings during one in-flight claim run exactly one more pass. A one-ring pass ends at its first lease, and an empty pass over 5 sessions makes 5 claims.
- **Dimension 4.5** — the fallback costs one probe a tick per replica → Test `the_fallback_tick_probes_once_per_replica` (unit, paused time): 50 idle sessions with credits, an empty ledger and three ticks make 3 probes and 0 claims; a waiting admission makes the next tick run a pass.
- **Dimension 4.6** — a drain answers what it read, then sends `go_away` → Test `draining_sends_go_away_and_waits_for_answers` (integration): a drain that starts with a renewal in flight sends `renewed`, then `go_away`, then closes before `DRAIN_TIMEOUT`.
- **Dimension 4.7** — a report resent after a cut settles once → Test `a_report_resent_after_a_cut_settles_once` (integration): the same report on two connections writes one usage line, and both acknowledgements carry the same saved result.
- **Dimension 4.8** — a grant re-checks the host inside its transaction → Test `a_grant_rechecks_the_host_inside_its_transaction` (integration): a host flipped degraded after `hello`, with a session still believing it healthy, is issued nothing and its claimed admission returns to the ledger.
- **Dimension 4.9** — a stream past the ceiling is refused with a retry hint → Test `a_stream_past_the_ceiling_is_refused_with_retry` (integration): with `SSE_MAX_STREAMS=1` and one dashboard stream open, the upgrade answers `503` with `Retry-After`.

### §5 — Cancel, revocation and supersede are a Postgres write plus a push

Each push names its Postgres authority, so pub/sub's at-most-once delivery costs latency, never correctness.

- **Kill.** The kill transaction in `afd_authoring/src/creation/patch.rs` marks the fleet's active leases in the same transaction (`mark_leases_cancel_requested.sql`). After commit, the handler publishes `Cancel` on each owning runner's subject. The replica holding that stream pushes `cancel`, and the runner cuts the lease with the class a terminal renewal refusal uses. **Implementation default:** that class, because both mean the daemon ended the lease, and a new class would change report and billing readers for no new decision. Backstops: renewal refuses a marked lease with UZ-RUN-022, and rebuild cancels it. Pause keeps today's behaviour.
- **Revocation, rotation, cordon, drain, policy edits, delete.** After each runner-row write commits, the operator handler publishes `Recheck`. The owning session re-runs the heartbeat's read without its writes. A non-active status closes with UZ-RUN-009, a changed token hash or a missing row with UZ-RUN-001, and anything else pushes the new `heartbeat` answer. Backstop: the next heartbeat runs the same read, and a session with no heartbeat for two intervals closes. Every grant and renewal re-checks inside its transaction (§4).
- **Supersede.** `runner.runners.connection_id` is the authority; connect overwrites it, and the last writer wins. An older session that reads another id sends `go_away` UZ-RUN-021 and closes, and a runner that receives it halts. A reconnecting runner has already dropped its old socket, so only a second process sharing the token sees UZ-RUN-021. It halts instead of the two flapping.

- **Dimension 5.1** — a kill cancels a running lease from another replica → Test `killing_a_fleet_cancels_its_running_lease_across_replicas` (integration, two `Supervisor` daemons on the lane's Postgres and Dragonfly): the stream on replica B, the kill sent to A; the runner gets `cancel` within 1 s, the row carries `cancel_requested_at`, and the report settles with the renewal-refusal class.
- **Dimension 5.2** — a lost cancel push ends at the next renewal → Test `a_cancel_whose_push_is_lost_ends_at_the_next_renewal` (integration, a bus that drops runner commands): the next `renewed` answers UZ-RUN-022 for that lease and a deadline for its sibling.
- **Dimension 5.3** — revocation closes the stream → Test `revoking_a_runner_closes_its_stream` (integration): after a revoke the stream closes within 1 s, the reconnect answers `401` UZ-RUN-009, and the runner halts.
- **Dimension 5.4** — a lost revocation push closes at the next heartbeat → Test `a_revocation_whose_push_is_lost_closes_at_the_next_heartbeat` (integration, a bus that drops runner commands): the stream closes on the next heartbeat with UZ-RUN-009, and no offer reaches it after the revoke commits.
- **Dimension 5.5** — rotation closes the stream on the old token → Test `rotating_a_token_closes_the_stream_on_the_old_token` (integration): after `rotate`, the old stream closes with UZ-RUN-001 and the new token connects.
- **Dimension 5.6** — a second connection supersedes the first → Test `a_second_connection_supersedes_the_first` (integration, two replicas): the first stream gets `go_away` UZ-RUN-021 and closes, and only the second receives offers.

### §6 — The runner takes work from the stream

`afr_supervisor` opens the stream with `tokio-tungstenite`, as Codex's client does. A host with an environment proxy connects through it, as the runner's HTTP client does today (`afr_supervisor/src/client/http.rs`); Codex enables the `proxy` feature for this, and Dimension 6.7 proves it here. It sends `hello` with its running leases, accepted kinds and idle workers, `ready` whenever its idle-worker count changes, and a `heartbeat` every served interval. It pings every `STREAM_PING_INTERVAL`. A ping or renewal unanswered within `STREAM_ANSWER_TIMEOUT` counts as a dead stream, and the runner reconnects after a uniform delay up to `RECONNECT_JITTER_MAX`, then exponentially to 30 s, as Codex caps it. The downlink dispatches each frame:
- `offer` to an idle worker; workers never poll.
- `cancel` to that lease's interrupt token (`lease_loop/mod.rs`).
- `folded` to the lease's inbox (M242_002's `LeaseInbox`); `heartbeat` answers to the assignment watch.
- `go_away` triggers a reconnect, except UZ-RUN-021, which halts. A `401` at upgrade halts through `Halt::stops_on`, as today.

The uplink writes in a fixed priority: control frames and folds, then reports, then renewals, then activity. Activity keeps its 64 KiB batches and drops past `MAX_BATCHES_HELD`, as today. Renewals batch per host each `RENEWAL_TICK`. The report spool deletes a report only on its `report_ack`, and replays it after a reconnect. A refusal that says a retry cannot help is discarded, as today.

- **Dimension 6.1** — a blackholed stream recovers inside the lease → Test `a_blackholed_stream_reconnects_and_reports_once` (integration, an in-process Transmission Control Protocol (TCP) proxy that stops forwarding mid-lease): the runner detects the cut within `STREAM_ANSWER_TIMEOUT`, reconnects, and renews before the deadline. The report spooled during the cut settles exactly once.
- **Dimension 6.2** — an activity flood never delays a renewal → Test `an_activity_flood_does_not_delay_renewal` (unit): with 1,000 activity batches queued, a renewal queued after them is the next frame written.
- **Dimension 6.3** — reconnects spread out → Test `reconnects_spread_over_the_jitter_window` (unit, paused time): 100 runners told `go_away` at once reconnect within `RECONNECT_JITTER_MAX`, no more than 10 in any tenth of it.
- **Dimension 6.4** — a superseded runner halts → Test `a_superseded_runner_halts` (unit): `go_away` with UZ-RUN-021 stops the runner, and it opens no new connection.
- **Dimension 6.5** — an idle runner asks for nothing → Test `an_idle_runner_sends_no_lease_request` (unit, paused time): an idle runner over 60 s writes only heartbeats and pings, and no frame asks for work.
- **Dimension 6.6** — a runner runs a lease end to end over the stream → Test `a_runner_runs_a_lease_over_the_stream` (e2e, the `integration_runner_e2e` harness): an admitted event reaches the supervisor as an `offer`, streams activity to the tail, and settles its report.
- **Dimension 6.7** — a host behind an outbound proxy still connects → Test `the_stream_connects_through_an_environment_proxy` (unit, an in-process CONNECT proxy and an injected `MapEnv`): with `HTTPS_PROXY` set, the upgrade arrives through the proxy; with it unset, the proxy sees nothing.

### §7 — The pull path is deleted, and the docs describe the stream

Six routes go: `POST /v1/runners/me/heartbeats`, `/leases`, `/leases/{lease_id}/renew`, `/leases/{lease_id}/activity`, `/leases/{lease_id}/folds` (M242_002) and `/reports`, with their handlers, client verbs and tests. The doorbell's hold (`wait_for_ring`, `Waited`, the held count, `close`) goes with `Leasing::wait_for_work`, `NO_WORK_RETRY_AFTER`, `WAKE_HOLD_MAX`, `HELD_BEATS_MAX`, `wake`, `woken`, `LeaseRequest`, `LeaseResponse` and `agentsfleet_runner_wake_answers_total`. **Implementation default:** the removed routes answer `404`, following the pre-2.0.0 precedent in M145_001 and M228_001; `VERSION` is 0.73.0, and there is no compatibility period (NLG; Indy, Discovery). The architecture pages, `docs/AUTH.md` and the docs repository change in the same Pull Request (ARCH-HOMES, ARCH-PUBLISHED).

- **Dimension 7.1** — the retired routes are gone → Test `the_retired_runner_routes_answer_404` (integration): each of the six `POST`s answers `404`, while `GET /v1/runners/me` still answers `200`.
- **Dimension 7.2** — no census row outlives its producer → Test `every_census_family_has_a_producer` (unit): passes with the wake-answers row gone and the two new rows present.

### §8 — The bench proves the stream's cost

The drain's simulated runner (`afd_bench/src/lane/lease/drain/runner.rs`) drives an `afd_gateway` session in process: offers in; renewal batches, M242_002's folds requests, 40 activity frames and a report up. M241's statement paths classify the result unchanged. The idle window keeps every simulated runner's session connected with credits and no admissions for `IDLE_WINDOW`. It records `drain_idle_statements` and `drain_idle_dragonfly_calls`, and the per-poll keys go, since no poll exists. Heartbeats and the per-replica fallback probe sit outside the bench, which runs no registry; Dimension 4.5 bounds the probe. Three interleaved runs follow a reset rig, and the median run is copied to `bench/baselines/lease.rig.json`. The other two values go into `push_scheduler.md` §Baselines.

- **Dimension 8.1** — a streamed chat's activity path is near zero and an idle host costs nothing → Test `make bench-lease PROFILE=rig` (integration): `drain_statements_per_lease_activity` is under 1, against M241's 40. `drain_idle_statements` and `drain_idle_dragonfly_calls` are 0, against today's one Dragonfly call per idle poll.
- **Dimension 8.2** — an idle window with no polls reports totals, never a ratio → Test `an_idle_window_reports_totals_not_ratios` (unit): a window of 0 polls writes `drain_idle_statements` 0 and no `idle_statements_per_poll` key.
- **Dimension 8.3** — baselines are median runs → Test `baselines_are_median_runs` (manual): §Baselines lists three values per headline key, and the committed file holds the middle one.

## Interfaces

- **Route:** `GET /v1/runners/me/stream` with `Authorization: Bearer agt_r…` and `Sec-WebSocket-Protocol: agentsfleet.runner.v1`. It answers `101`, `400` UZ-RUN-023, `401` UZ-RUN-001 or UZ-RUN-009, `503` with `Retry-After` at the ceiling, and `503` UZ-AUTH-004 when the token read cannot answer.
- **Frames**, JSON text tagged by `type`; bodies reuse today's wire types:
  - up: `hello{accepted_kinds, running[lease_id], credits}`, `ready{credits}`, `heartbeat{HeartbeatRequest without wake}`, `renew_batch{leases[{lease_id, RenewRequest}]}` (`Up::RenewBatch`), `activity{lease_id, frames}`, `report{ReportRequest}`, `folds{lease_id, FoldRequest}`.
  - down: `welcome{heartbeat_interval_ms}`, `offer{TaggedLease}`, `heartbeat{HeartbeatResponse without woken}`, `renewed{leases[{lease_id, lease_expires_at | error_code}]}`, `report_ack{lease_id, ReportResponse | error_code, retryable}`, `folded{lease_id, Folds | error_code}`, `cancel{lease_id, error_code}`, `go_away{error_code?}`.

```
↑ {"type":"hello","accepted_kinds":["fleet","authoring"],"running":[],"credits":4}
↓ {"type":"welcome","heartbeat_interval_ms":10000}
↓ {"type":"offer","lease":{…TaggedLease, as the poll answers today…}}
↓ {"type":"cancel","lease_id":"0190f2c4-…","error_code":"UZ-RUN-022"}
```

- **Bus:** `afd_bus::Bus` with `publish(&Subject, &str)`, `publish_batch(&Subject, &[String])` and `subscribe(&Subject) -> Subscription`. `Subject` is `FleetTail(fleet_id) | DraftEvents(draft_id) | Wake(shard) | RunnerCommand(runner_id)`. `Received` is `Message | Lagged(u64) | Gap`, and `Command` is `Cancel{lease_id} | Recheck`.
- **Schema:** `runner.leases.cancel_requested_at TIMESTAMPTZ(3) NULL`; `runner.runners.connection_id UUID NULL`, with a UUIDv7 check when set.
- **Error codes:** UZ-RUN-021 "Runner stream superseded" (409); UZ-RUN-022 "Lease cancelled" (409); UZ-RUN-023 "Runner protocol unsupported" (400).
- **Timing** (`af_core/src/timing.rs`): `STREAM_PING_INTERVAL` 15 s, the Server-Sent Events (SSE) keepalive; `STREAM_ANSWER_TIMEOUT` 5 s; `RECONNECT_JITTER_MAX` 2 s; `OFFER_FALLBACK_TICK` 5 s; and `PROXY_IDLE_FLOOR` 60 s, the Amazon Web Services (AWS) Application Load Balancer's default idle timeout, below any cut §1 measures. Asserted: `2 × STREAM_PING_INTERVAL < PROXY_IDLE_FLOOR`; `RENEWAL_TICK + STREAM_ANSWER_TIMEOUT + RECONNECT_JITTER_MAX < LEASE_TTL − RENEWAL_WINDOW`; `OFFER_FALLBACK_TICK < LEASE_TTL`; `2 × HEARTBEAT_INTERVAL < RUNNER_OFFLINE_AFTER`; and, in `agentsfleetd`, `RECONNECT_JITTER_MAX < DRAIN_TIMEOUT`.
- **Removed:** the six routes in §7, and `retry_after_ms` with `LeaseResponse`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Proxy idles out a stream | failure | the tunnel's idle cut | pings every 15 s keep it open; a cut reconnects | `tunnel_idle_spike` |
| Stream blackholed mid-lease | failure | network blip, half-open socket | detected within 5 s; reconnect and renew before the deadline; the report lands once | `a_blackholed_stream_reconnects_and_reports_once` |
| Unknown protocol version | failure | a runner build the daemon does not speak | `400` UZ-RUN-023 before upgrade | `an_unknown_protocol_version_is_refused` |
| Malformed or oversized up frame | failure | a broken runner build | the frame fails decode and the session closes, writing nothing; past 2 MiB the WebSocket layer closes it | `an_up_frame_with_an_unknown_field_is_refused` |
| Ring lost | failure | publish timeout or a hub gap | the next tick's probe finds the work and runs a pass within 5 s; the loss is counted | `the_fallback_tick_probes_once_per_replica`, `a_failed_ring_counts_and_admits` |
| Burst of rings | capacity | 50 fleets at 10 messages a second | rings coalesce into passes, each bounded by its leases or one claim per session | `rings_coalesce_into_one_claim_pass` |
| Offer lost in a cut | race | the stream dies between issue and delivery | the lease lapses and the next tick's probe reclaims it | `reconnect_rebuilds_active_leases_from_postgres` |
| Cancel push lost | failure | pub/sub is at most once | the next renewal refuses with UZ-RUN-022 | `a_cancel_whose_push_is_lost_ends_at_the_next_renewal` |
| Runner away during a kill | failure | disconnected at the kill | rebuild sends `cancel` on reconnect | `reconnect_rebuilds_active_leases_from_postgres` |
| Revocation push lost | security | pub/sub is at most once | closed at the next heartbeat; grants refused in their transaction | `a_revocation_whose_push_is_lost_closes_at_the_next_heartbeat` |
| Token rotated | security | operator rotation | the old token's stream closes with UZ-RUN-001 | `rotating_a_token_closes_the_stream_on_the_old_token` |
| Degraded host | security | capability lost mid-stream | no offers; the host lock refuses inside the issue | `a_grant_rechecks_the_host_inside_its_transaction` |
| Two processes share a token | race | a duplicated host | the newer connection wins; the older halts with UZ-RUN-021 | `a_second_connection_supersedes_the_first`, `a_superseded_runner_halts` |
| Report reply lost | replay | a cut after the commit | the spool replays; the lease-keyed report answers its saved result, charged once | `a_report_resent_after_a_cut_settles_once` |
| Rolling deploy | failure | drain | answers in flight are sent, then `go_away`; the runner reconnects elsewhere | `draining_sends_go_away_and_waits_for_answers` |
| Mass reconnect | capacity | a tunnel restart | reconnects spread over the jitter window | `reconnects_spread_over_the_jitter_window` |
| Stream ceiling reached | capacity | dashboards hold every slot | `503` with `Retry-After`; the runner retries | `a_stream_past_the_ceiling_is_refused_with_retry` |
| Activity flood | capacity | a chatty run | renewals are written first; activity past the cap drops | `an_activity_flood_does_not_delay_renewal` |

## Invariants

1. No lease is granted to a non-active or degraded host, and none is renewed for a revoked or degraded one — `afd_runner::capacity::lock` decides inside the issue and renewal transactions, whatever the session believes; `a_grant_rechecks_the_host_inside_its_transaction` is the counterexample.
2. A cancel outlives a lost publish — `cancel_requested_at` commits before the publish, and renewal and rebuild read it; `a_cancel_whose_push_is_lost_ends_at_the_next_renewal` fails if renewal ignores it.
3. One runner has one serving stream — `connection_id` is last-writer-wins, and every heartbeat read compares it; `a_second_connection_supersedes_the_first` fails if the comparison goes.
4. A reconnect fits inside a lease — the `const` assertions in `af_core/src/timing.rs` fail the build when a value moves out of order, and `stream_timings_leave_a_renewal_window_after_reconnect` restates them.
5. A report leaves the runner spool only on its acknowledgement — the spool's delete takes the `report_ack`; `a_blackholed_stream_reconnects_and_reports_once` fails on a lost or doubled report.
6. Renewals never queue behind activity — the uplink's biased select; `an_activity_flood_does_not_delay_renewal` fails if the order flips.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `agentsfleet_runner_stream_closes_total` | operator | a session closes | `reason`: client, silent, superseded, revoked, draining, protocol | no token, no tenant identifier | `revoking_a_runner_closes_its_stream` |
| `agentsfleet_runner_offer_claims_total` | operator | a claim pass runs one claim | `outcome`: leased, empty | none needed | `rings_coalesce_into_one_claim_pass` |
| `agentsfleet_runner_wake_answers_total` (removed) | operator | never again | none | none | `every_census_family_has_a_producer` |
| `runner_stream_started`, `runner_stream_completed`, `runner_stream_failed` (log events) | operator | connect, clean close, error close | `runner_id`, `connection_id`, `error_code`, `duration_ms` | runner identifiers only; no token or workspace identifier at `info` | `a_second_connection_supersedes_the_first` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The transport choice rests on recorded tunnel evidence (§1) | `grep -c "^## Transport evidence" docs/architecture/runner_protocol.md` | `1` | P0 | |
| R2 | No production crate names a Dragonfly pub/sub type (§2) | `git grep -n -w -e SubscriptionHub -e publish_tail -e "doorbell::publish" -- 'rustd/crates/*/src/*' ':!rustd/crates/daemon/afd_dragonfly' ':!rustd/crates/daemon/afd_bus'` | no output | P0 | |
| R3 | The protocol page and edited docs read clean (§3, §7) | `orly gate verify` | exit 0 | P0 | |
| R4 | A streamed chat reads no lease target (§4) | `make test-integration-rustd` | output holds `a_streamed_chat_reads_no_lease_target ... ok` | P0 | |
| R5 | A reconnect rebuilds from Postgres (§4) | `make test-integration-rustd` | output holds `reconnect_rebuilds_active_leases_from_postgres ... ok` | P0 | |
| R6 | A kill cancels a running lease across replicas (§5) | `make test-integration-rustd` | output holds `killing_a_fleet_cancels_its_running_lease_across_replicas ... ok` | P0 | |
| R7 | Revocation closes the stream (§5) | `make test-integration-rustd` | output holds `revoking_a_runner_closes_its_stream ... ok` | P0 | |
| R8 | A second connection supersedes the first (§5) | `make test-integration-rustd` | output holds `a_second_connection_supersedes_the_first ... ok` | P0 | |
| R9 | A blackholed stream recovers and reports once (§6) | `make test-integration-rustd` | output holds `a_blackholed_stream_reconnects_and_reports_once ... ok` | P0 | |
| R10 | The pull path is gone (§7) | `git grep -n -w -e RUNNER_HEARTBEATS -e wait_for_work -e WAKE_HOLD_MAX -e HELD_BEATS_MAX -e NO_WORK_RETRY_AFTER -- ':!docs/v2'` | no output | P0 | |
| R11 | A streamed chat's activity path is near zero (§8) | `jq -e ".measurements.drain_statements_per_lease_activity < 1" bench/baselines/lease.rig.json` | `true` | P0 | |
| R12 | An idle host costs no statement (§8) | `jq -e ".measurements.drain_idle_statements == 0" bench/baselines/lease.rig.json` | `true` | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `RUNNER_HEARTBEATS`, `RUNNER_REPORTS`, `LEASE_ACTIVITY`, `LEASE_RENEW`, `LEASE_FOLDS` | `git grep -n -w -e RUNNER_HEARTBEATS -e RUNNER_REPORTS -e LEASE_ACTIVITY -e LEASE_RENEW -e LEASE_FOLDS -- ':!docs/v2'` | no output |
| `wait_for_work`, `wait_for_ring`, `woken` | `git grep -n -w -e wait_for_work -e wait_for_ring -e woken -- rustd` | no output |
| `NO_WORK_RETRY_AFTER`, `WAKE_HOLD_MAX`, `HELD_BEATS_MAX` | `git grep -n -w -e NO_WORK_RETRY_AFTER -e WAKE_HOLD_MAX -e HELD_BEATS_MAX -- ':!docs/v2'` | no output |
| `RUNNER_WAKE_ANSWERS_TOTAL`, `agentsfleet_runner_wake_answers_total` | `git grep -n -w -e RUNNER_WAKE_ANSWERS_TOTAL -e agentsfleet_runner_wake_answers_total -- ':!docs/v2'` | no output |
| `LeaseRequest`, `LeaseResponse` | `git grep -n -w -e LeaseRequest -e LeaseResponse -- rustd public` | no output |
| `IDLE_STATEMENTS_PER_POLL`, `idle_statements_per_poll` | `git grep -n -w -e IDLE_STATEMENTS_PER_POLL -e idle_statements_per_poll -- ':!docs/v2'` | no output |
| retired handler, test and module files | `git ls-files rustd/crates/daemon/afd_api_runner/src/handler/runner/heartbeat.rs rustd/crates/daemon/afd_api_runner/src/handler/runner/wake.rs rustd/crates/daemon/afd_api_runner/src/handler/runner/lease rustd/crates/runner/afr_supervisor/src/heartbeat rustd/crates/daemon/afd_dragonfly/src/doorbell rustd/crates/daemon/agentsfleetd/tests/integration_runner_wake.rs` | no output |

Hits under `docs/v2/` are records and stay. CHORE(open) reruns each search on `origin/main` and adds any new hit to Files Changed.

Schema this milestone leaves with no runtime reader or writer is dropped in a new slot in this Pull Request, under Indy's blanket approval (Discovery). Each drop gets a row above, with a `git grep -n -w` over `rustd`, `ui` and `cli` proving nothing still reads or writes it.

## Out of Scope

- One statement per host renewal: M245. Each lease keeps its own renewal statements here.
- The NATS implementation behind `afd_bus`: M244.
- The authoring lease's per-lease routes (renew, activity, model calls, reports): they stay HTTP with per-request auth. Authoring leases still arrive as offers.
- A pause that cancels a running lease: pause keeps today's behaviour.
- Aiming a ring at one eligible runner: an empty pass still costs one claim per session with credits, as each ring costs today, counted by `agentsfleet_runner_offer_claims_total`. Not planned until that count matters.
- Customer-runner trust (M246) and the lapsed-host respawn signal (M247).
- A drill on the deployed dev tunnel: it runs with acceptance after merge. §1's spike and §6's proxy test are this Pull Request's proof.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator kills a fleet mid-run and its run stops within a second, and a host idle all day sends no lease request yet picks up the next message as soon as it is admitted.
2. **Preserved user behaviour** — enrolment, the `agt_r` token, policy edits from the dashboard, live tail, reports and billing, and authoring.
3. **Optimal-way check** — one outbound connection per host through the public API is the most direct path. The remaining gap is per-lease renewal statements, which M245 closes.
4. **Rebuild-vs-iterate** — iterate: the lease plane, the report path and the spool stay, and only transport, wake and cancel change.
5. **What we build** — the stream route and gateway, the frames, the runner's downlink and uplink, the cancel marker, the connection owner, and the bus seam.
6. **What we do NOT build** — a replay buffer (Postgres rebuilds), compatibility routes (pre-production), NATS (M244), and single-runner ring routing (unmeasured).
7. **Fit with existing features** — compounds with M242's folding and M244's broker swap. It must not destabilise the report and billing path, which keeps its statements and its lease-keyed retry.
8. **Surface order** — runner-first; no user interface (UI) change. The docs repository's runner pages change with the release.
9. **Dashboard restraint** — no "connected" badge; liveness stays `last_seen_at`, until the stream-close metric shows operators need one.
10. **Confused-user next step** — the runner logs `runner_stream_failed` with its `error_code`, and `runners/install.mdx` says a proxy must pass a WebSocket upgrade.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the spike first, so the transport rests on evidence. The bus seam next, since it stands alone. Then protocol and schema, the daemon, cancel and auth, the runner, deletion, and the bench last, measuring the finished path.
- **Alternatives considered:**
  - An HTTP/2 bidirectional stream: the fallback, since Cloudflare may buffer request bodies (unverified until §1).
  - A down-only stream with `POST` uplinks: rejected, because each post keeps per-request auth.
  - A gateway replay buffer: rejected, because Postgres already holds every lease and mark.
  - Per-session fallback polling: rejected, because it makes idle cost grow with runners.
  - A cancel by publish alone: rejected, because pub/sub is at most once.
- **Patch-vs-refactor verdict:** a **refactor** of the runner transport, because every runner verb changes carrier while the lease plane's statements stay.

## Discovery (consult log)

- **Consults** — source review on `main` `14539a73e` plus M241's branch, Nov 06, 2026:
  - Auth admits only `active` runners (`afd_state/src/credentials/rows.rs`), so cordon and drain refuse every call today; the stream keeps that by closing.
  - `capacity::lock` already refuses a non-active or degraded host inside issue, and a revoked or degraded one inside renewal (`afd_runner/src/capacity/mod.rs`); `docs/AUTH.md` requires exactly that check once auth stops running per request.
  - The report is lease-keyed and idempotent ("Repeats return the saved result without another charge", `afd_api_runner/src/handler/runner/report.rs`). The runner already spools and replays reports (`afr_supervisor/src/report_spool/`, `drainer/`).
  - Kill marks nothing on `runner.leases` (`afd_authoring/src/creation/patch.rs`; `runner_fleet.md` §Steer, kill, pause).
  - `Plane::claim` picks either work kind (`afd_fleet/src/lease/claim.rs`), so authoring offers cost nothing extra.
  - axum has no `ws` feature and no `tungstenite` crate is in `Cargo.lock` (`rustd/Cargo.toml`); `SSE_MAX_STREAMS` defaults to 256; `DRAIN_TIMEOUT` is 15 s.
  - The tunnel sets no `originRequest` (`deploy/fly/cloudflared-prod/config.yml`), and the daemon has no public Fly service.
- **Reports decision (agent):** reports ride the stream with the existing spool and an acknowledgement. The spool and the lease-keyed retry already exist, so only the carrier changes. Keeping the HTTP report would keep per-request auth for the message that matters most.
- **Cross-spec note:** M242_002's Out of Scope expects M243 to push input with the read as backstop. §4 moves the read onto the stream and declines the push; the review reconciles that line.
- **Authoring decision (agent):** authoring leases arrive as offers; their four per-lease routes stay HTTP, because they hold their own spool, receipts and the model-call proxy.
- **Owner decisions** —
  > Indy (Nov 06, 2026: 08:50 AM): "And we are not in production yet, so i want to rethink the approach on admissions, lease, and so on as i am focussed on scale. If something fails in one host bare metal, it should automatically cordon and respawn in another runner" — context: no compatibility period for the retired routes.
  > Indy (Nov 06, 2026: 10:19 AM): "This will be on by default durable_tasks_v1. ... my plan also was to have folks spin a runner and point to the agentsfleetd (in cloud) and keep using it." — context: one stream through the public API, with no broker credential on the runner.
  > Indy (Nov 06, 2026: 10:43 AM): "1. Yes placement rules are via the tags and labels 2. Respawn across ownership - this is a tenant setting ... Based on the scope i would like to build this ... 4. Yes, but donot overengineer now, we can improve this." — context: no replay buffer and no single-runner ring routing.
  > Indy (Nov 06, 2026: 11:57 AM): "Yes i am ok with postgres store and accepting messages, fences runs" and "The message has to wait. I assume you show message like compacting" and "assume atleast 50 fleets" — context: the cancel marker and connection owner live in Postgres; fences stay the authority.
  > Indy (Nov 06, 2026: 01:18 PM): "Yes i agree on NATS." — context: the bus seam built here, so M244 adds NATS behind it.
  > Indy (Nov 06, 2026: 02:13 PM): "And i prefer we create all the spects and close M241 and push a PR?" — context: this spec lands with M241.
- **Programme decisions (relayed in the authoring brief, Nov 06, 2026; not owner quotes):** one outbound stream per host through the public API, and self-hosted runners never touch the broker. The frame lists above; auth once per connection; revocation closes the stream; each grant re-checks the host. Rebuild from Postgres with no replay buffer, and a cancel is a Postgres mark plus a push. The bus seam is built here. WebSocket is the default, decided by §1's spike, with HTTP/2 as the written fallback. No compatibility period for the `POST` routes.
  > Indy (Nov 06, 2026: 04:18 PM): "yes" — context: blanket approval, asked as "Should each milestone drop the tables and columns it leaves with no reader or writer?": a milestone drops, in a new slot in its own Pull Request, any table, column or index it leaves with no runtime reader or writer (`.orly/standards/domains/sql.md` destructive-change approval).
- **Model reviews** — none yet; the Elephant-Goldfish Model (EGM) review runs before this spec leaves `pending/`.
- **Open blockers** —
  1. Indy: do you approve an `originRequest` edit to `deploy/fly/cloudflared-dev/config.yml` and `cloudflared-prod/config.yml` if §1 shows the tunnel needs one that the 15 s ping does not cover? If not, a failing WebSocket check falls back to HTTP/2 and never to a configuration change.
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
