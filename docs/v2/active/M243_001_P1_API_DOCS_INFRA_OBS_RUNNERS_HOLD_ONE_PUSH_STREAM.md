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

**Status:** IN_PROGRESS

**Implementation branch:** `feat/m243-runner-push-stream`, worktree `/workspace/agentsfleet-m243`; current `origin/main` `61820b66a` was merged normally after #817.

**Transport revision:** Nov 08, 2026. Indy approved HTTP/3 first, HTTP/2 fallback and direct runner-to-daemon connectivity. Production wiring and final verification remain due.

**Priority:** P1, operators see work start late and a kill that never stops a run; every self-hosted runner will take this path

**Categories:** API, DOCS, INFRA, OBS

**Batch:** B1, the third push-scheduler milestone; it starts after M242 merges, and M244 starts after it

**Depends on:** M241_001 (the bench's statement paths and `push_scheduler.md`); M242_001 and M242_002 (the one-statement intake rings the doorbell this stream listens to, and M242_002's folds request moves onto the stream)

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 06, 2026) from the EGM brief and source research on main 14539a73e

**Canonical architecture:** `docs/architecture/push_scheduler.md` §M243 and `docs/architecture/runner_protocol.md` (implementation design; runtime not yet implemented).

---

## Overview

**Goal (testable):** A runner host holds one authenticated stream to `agentsfleetd` that carries offers, renewals, activity, reports and cancels, so a streamed 10 s chat reads no lease target, an idle host sends no lease request, and a kill cancels a running lease from any replica.

**Problem:**
- Every idle worker sends `POST /v1/runners/me/leases` each `NO_WORK_RETRY_AFTER` (1 s, `af_core/src/timing.rs`; `afr_supervisor/src/worker_pool/mod.rs`), and each request reads the runner token, deliberately uncached (`afd_http/src/auth/guard.rs`; `docs/AUTH.md` §Validation).
- To shorten the wait, a host holds its heartbeat open up to 10 s for a doorbell ring, at most 64 per process (`afd_api_runner/src/handler/runner/wake.rs`, `afd_dragonfly/src/doorbell/mod.rs`, `HELD_BEATS_MAX`).
- Each 250 ms activity flush is a `POST` that reads the token and then the lease target (`afd_fleet/src/lease/activity/mod.rs`): 40 posts and 80 statements per 10 s chat; M241's bench counts the 40 target reads.
- A kill stops nothing already running. It writes nothing to `runner.leases`, and no cancel channel exists (`runner_fleet.md` §Steer, kill, pause).
- Runners initiate connections to a directly reachable daemon endpoint and hold no broker credential (Indy, Discovery). Cloudflare is not required.

**Solution summary:** Each runner opens one authenticated session to a directly reachable `agentsfleetd` endpoint.
HTTP/3 is preferred; HTTP/2 carries the same protocol when HTTP/3 cannot connect.
The daemon sends offers, heartbeat answers, renewal answers, report acknowledgements, folded input, cancels and `go_away` over that session.

The runner sends hello, credits, heartbeats, renewal batches, activity, reports and folds requests.
Postgres holds cancellation markers and connection ownership, so a lost publish cannot erase either decision.
Pub/sub moves behind `afd_bus::Bus`, with Dragonfly as its first implementation and NATS added by M244.

The gateway claims work when a notification arrives or credits rise.
The poll, held heartbeat, per-flush activity posts and ten retired routes are deleted.
The Nov 09 authoring correction also moves model calls, proposals, renewal and completion onto this native session.
Postgres retains saved results and accounting; unacknowledged runner reports remain only in memory.
Connection failures, fallback, disconnected duration and recovery are measured and traced.

**Design flow:**

```
agentsfleet-runner ──direct HTTP/3; HTTP/2 fallback──► agentsfleetd replica (gateway)
 hello{running, credits, kinds}  ─►  token read once; connection_id written; live leases read (rebuild)
                                 ◄─  welcome · cancel for leases Postgres no longer holds or has marked
 admission commits ─► Bus wake(shard) ─► every replica: session with credits ─► Plane::claim ─► offer
 every 5 s, one probe per replica: work waiting with no live lease? ─► a claim pass
 heartbeat every 10 s ─► Runners::heartbeat (liveness, holds, policy, status, token hash, connection_id)
 renew batch every 5 s ─► Plane::renew per lease ─► renewed{deadline | refusal}
 activity ─► lease target cached at offer or rebuild ─► Bus tail (0 statements)
 report ─► Plane::report (lease-keyed, idempotent) ─► report_ack ─► runner releases pending memory
 kill ─► cancel_requested_at in the kill transaction ─► Bus runner command ─► owning replica ─► cancel
 operator edit, revoke, rotate ─► Bus recheck ─► owning replica re-reads the row ─► answer or close
 drain ─► go_away ─► runner reconnects after a jittered delay
```

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): one pushed stream per runner host, behind a bus seam
- **Intent (one sentence):** work is pushed to a runner, kill stops a running run, and idle workers send no lease polls.
- **Authoring handshake:** Indy wants the lease path rebuilt for scale before production, with self-hosted runners pointing at the cloud daemon. This milestone replaces polling with one pushed stream per host and builds the bus seam M244 needs. ASSUMPTIONS I'M MAKING:
  1. M242_001 and M242_002 merge first, and M242_002's folds request keeps its meaning when it moves onto the stream.
  2. No self-hosted runner exists yet, so runner and daemon upgrade together. A runner of an older build gets `404` on its old routes and takes no work until upgraded.
  3. Batched renewal here is framing: each lease keeps its own renewal statements until M245 makes it one statement per host.
  4. The bench drives a gateway session in process, so M241's statement paths measure what the stream sends.
  5. Schema changes edit each existing table's file. No new ALTER file or unlisted table is added.
- **Implementer handshake:** the runner initiates the connection; HTTP/3 and HTTP/2 share session, lease and recovery rules. Private Fly and external runners use direct endpoints.
- **Current scope:** both carriers belong in M243. WebSocket, polling, compatibility routes and Cloudflare requirements are removed. Review the revised transport before runtime edits.

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_api_runner/src/handler/runner/heartbeat.rs` and `afd_runner/src/heartbeat/mod.rs` — what one beat writes and answers; the stream carries both unchanged except `wake` and `woken`.
2. `rustd/crates/daemon/afd_runner/src/capacity/mod.rs` — `lock` refuses a non-active or degraded host inside every issue transaction, and a revoked or degraded one inside every renewal; that is the grant re-check `docs/AUTH.md` requires once auth stops running per request.
3. `rustd/crates/daemon/afd_dragonfly/src/hub/mod.rs` and `doorbell/mod.rs` — `Received`, the gap signal, and the ring the gateway listens to.
4. `rustd/crates/runner/afr_supervisor/src/lease_loop/settle.rs` and `report/delivery.rs` — workers retain current reports in memory until acknowledgement.
5. `https://github.com/openai/codex/blob/cc7ba33/codex-rs/app-server-transport/src/transport/remote_control/websocket.rs` (Indy's checkout at `~/Projects/oss/rs/codex`) — an outbound WebSocket client with pings, a pong timeout and jittered reconnect.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M243_001_P1_API_DOCS_INFRA_OBS_RUNNERS_HOLD_ONE_PUSH_STREAM.md` | CREATE, then MOVE to `active/` and `done/` | this spec |
| `docs/architecture/runner_protocol.md`, `docs/architecture/README.md` | CREATE, EDIT | §3: frames, ordering, rebuild, cancel, supersede, timings; §1's transport evidence; index row |
| `docs/reviews/m243/` | CREATE, DELETE | Owner audit closure; reconcile design into this spec and canonical architecture, then remove the review packet |
| `rustd/crates/daemon/afd_runner/src/capacity/**`, `rustd/crates/daemon/afd_gateway/src/connection.rs`, `rustd/crates/daemon/afd_http/src/auth/guard.rs`, `rustd/crates/daemon/afd_http/src/route/mod.rs` | EDIT, CREATE | atomic session authority and native connection-local secondary-lane binding |
| `rustd/crates/daemon/{afd_admission,afd_approval,afd_authoring,afd_events,afd_fleet,afd_gate,afd_sse,afd_api_tenant,afd_http,afd_api_runner,afd_api_operator,afd_api,afd_bench}/Cargo.toml` and their bus-owning constructors and module indexes | EDIT | inject the one boot-selected bus into every caller; remove direct pub/sub dependencies where unused |
| `rustd/crates/daemon/main/Cargo.toml`, `rustd/crates/daemon/main/tests/daemon_suite.rs`, `rustd/crates/daemon/main/tests/support/{e2e_wire,e2e_tail,e2e_speaking,e2e_schedules}.rs`, `rustd/crates/protocol/af_wire/tests/wire_suite.rs` | EDIT | register carrier/protocol tests and convert shared harnesses that call retired routes |
| `docs/architecture/push_scheduler.md` | EDIT | §M243 status; retires "no cancel channel" and "not a general scheduler"; §8's baseline row |
| `docs/architecture/datastore_scaling.md` | EDIT | Indy's requested NATS decision record; preserve the current deployment description until M244 replaces it |
| `docs/architecture/runner_fleet.md`, `lease_flow.md`, `data_flow.md`, `scaling.md`, `observability.md`, `api_resources.md`, `runner_execution.md`, `user_flow.md`, `scenarios/fleet-to-fleet.md`, `scenarios/slack-incident-responder.md`, `scenarios/two-fleets-github-slack.md` | EDIT | §7: the held heartbeat, the poll, the retired routes, kill and the idle-cost knob, described as the stream; preserve complete renewal and cleanup budgets |
| `docs/AUTH.md` | EDIT | §7: auth once per connection, revocation pushed with its backstop, the grant re-check |
| `docs/metrics.census.tsv` | EDIT | §4, §7: stream health, fallback and recovery families in; wake answers out |
| `public/openapi.json` | EDIT | §7: regenerated from `rustd/` with `--no-banner` |
| `ui/packages/app/tests/approvals-list/resolve.test.ts` | EDIT | Fix the reported CI race by awaiting the visible error before asserting its content |
| `rustd/crates/daemon/afd_state/Cargo.toml`, `tests/directory/runners.rs`, `tests/support/directory_fixtures.rs` | EDIT | Native UUIDv7 fixtures; a raw UUID fault proves malformed runner identifiers fail the strengthened schema constraint |
| `schema/20261108123200_runner_leases.sql`, `schema/20261108123100_runner_runners.sql`, `rustd/crates/daemon/afd_db/src/migration.rs` | EDIT | §3: edit table files for the cancel marker and owning connection; recheck names after the data-layer follow-up lands |
| `rustd/Cargo.toml`, `rustd/Cargo.lock`, `rustd/crates/daemon/afd_bus/**` | EDIT, CREATE | §2: bus, wake and Dragonfly adapter; §1, §6: native HTTP/3 and HTTP/2 carrier dependencies |
| `rustd/crates/daemon/afd_dragonfly/src/lib.rs`, `doorbell/**` | EDIT, DELETE | §2: the doorbell moves to `afd_bus::wake` |
| `rustd/crates/daemon/afd_admission/src/wake.rs`, `afd_approval/src/inbox/announce.rs`, `afd_approval/src/inbox/resolve/mod.rs`, `afd_authoring/src/model/progress/context.rs`, `publish.rs`, `afd_events/src/activity.rs`, `afd_events/src/steer/mod.rs`, `afd_fleet/src/lease/bracket.rs`, `afd_gate/src/gate/park.rs`, `afd_sse/src/fanin.rs`, `live.rs`, `stream_limit.rs`, `afd_api_tenant/src/handler/draft/stream/mod.rs` | EDIT | §2: every production publisher and subscriber goes through `afd_bus::Handle` |
| test harnesses that build a hub: `afd_api/tests/harness/**`, `afd_api/tests/support/**`, `afd_sse/tests/**`, `afd_approval/tests/integration_inbox_*.rs`, `afd_authoring/tests/draft/model_calls/progress/fixture.rs`, `afd_events/tests/integration_backfill.rs`, `afd_gate/tests/integration_gate_tail.rs`, `afd_fleet/tests/integration_wall_counters.rs`, `agentsfleetd/tests/integration_cluster_outcomes.rs`, `afd_bench/src/datastores/mod.rs`, `afd_bench/src/lane/tail/**` | EDIT | §2: build an `afd_bus::Handle` instead |
| `rustd/crates/protocol/af_wire/src/protocol/**`, `paths.rs`, `runner/mod.rs`, `lease/request.rs`, `lease/response.rs`, `af_wire/tests/{routes,strictness,validation_holds,claim_selection}.rs` | CREATE, EDIT | §3: frames; `RUNNER_STREAM`; `wake`, `woken`, `LeaseRequest` and `LeaseResponse` go |
| `rustd/crates/foundation/af_core/src/timing.rs`, `limits.rs`, `error_code/fleet.rs`, `error_code/mod.rs`, `problem/fleet.rs` | EDIT | §3: stream timings and assertions; three codes; §7: three constants go |
| `rustd/crates/foundation/af_validate/src/{lib,serialized}.rs`, `rustd/crates/runner/afr_supervisor/src/deadline.rs` and its six callers | EDIT, DELETE | Owner reuse audit: share allocation-free JSON sizing; delete the forwarding time helper and call foundation methods directly |
| `rustd/crates/daemon/afd_gateway/**` | CREATE | §4: registry, session, offer passes, target cache, rebuild, recheck, HTTP/2 and HTTP/3 server adapters; exact planned files in `runner_protocol.md` |
| `rustd/crates/daemon/afd_http/src/route/runner.rs`, `afd_http/src/services/mod.rs` and the `Services` trait file, `services/leasing/mod.rs`, `services/leasing/lease_service.rs` | EDIT | §4: the stream route; `gateway()`; `wait_for_work` and `activity` leave `Leasing` |
| `rustd/crates/daemon/afd_api_runner/src/handler/runner/stream.rs`, `mod.rs`, `usage.rs`, `lib.rs`, `openapi.rs` | CREATE, EDIT | §4: session opening, version check, ceiling permit, drain registration |
| `rustd/crates/daemon/afd_fleet/src/lease/claim.rs`, `activity/mod.rs`, `store.rs`, `answer/mod.rs`, `renew/mod.rs`, `rebuild.rs`, `sql/select_runner_live_leases.sql`, `sql/select_waiting_admission.sql`, `sql/select_lease_for_renew.sql`, `sql/activity.rs` | EDIT, CREATE | §4: claim answers its target; rebuild read; the fallback probe; §5: renewal reads the cancel marker |
| `rustd/crates/daemon/afd_runner/src/heartbeat/mod.rs`, `sql/runner/select_runner_assigned_policy.sql`, `sql/runner/write_runner_connection.sql`, `sql/runner/mod.rs` | EDIT, CREATE | §4, §5: the beat's read carries status, token-hash match and `connection_id`; connect writes it |
| `rustd/crates/daemon/afd_authoring/src/creation/patch.rs`, `creation/sql/mark_leases_cancel_requested.sql`, `afd_api_tenant/src/handler/fleet/native/patch.rs` | EDIT, CREATE | §5: the kill marks running leases in its transaction; the handler publishes `Cancel` after commit |
| `rustd/crates/daemon/afd_fleet/src/task/lease.rs`, `task/sql.rs`, `task/sql/lock_issue_scope.sql`, `afd_fleet/tests/issue_lifecycle.rs` | EDIT, CREATE | §5: issuance rechecks active fleet state after waiting behind Kill; reports retain their settlement path |
| `rustd/crates/daemon/afd_api_operator/src/handler/operator/runner_patch.rs`, `runner_delete.rs` | EDIT | §5: publish `Recheck` after every runner-row write |
| `rustd/crates/protocol/af_observability/src/metrics/declared/fleet.rs`, `rustd/crates/protocol/af_observability/src/metrics/label/fleet.rs`, `rustd/crates/protocol/af_observability/src/producers/fleet/mod.rs`, `rustd/crates/protocol/af_observability/src/producers/fleet/runner/stream.rs` | EDIT, CREATE | §4, §6, §7: stream health, carrier fallback, recovery, queue pressure and offer outcomes; wake answers out |
| `rustd/crates/daemon/main/src/plane/mod.rs`, `serve/mod.rs`, `serve/runtime/mod.rs`, `serve/optional.rs`, `serve/drain/mod.rs`, `serve/drain/tests.rs` | EDIT | §2, §4: build the bus and gateway once; `go_away` on drain; the doorbell close goes |
| `rustd/crates/daemon/main/tests/integration_runner_stream/**` | CREATE | §4–§6: reconnect, kill across replicas, revocation, rotation, supersede, blackhole, chat statements |
| `rustd/crates/daemon/main/tests/integration_runner_e2e/**`, `integration_rust_runner*.rs`, `integration_runner_activity*.rs`, `integration_runner_messages*.rs`, `integration_runner_shapes.rs` | EDIT | §6: drive leases over the stream |
| `rustd/crates/daemon/main/tests/telemetry_boot.rs`, `rustd/crates/daemon/afd_fleet/tests/integration_report_owes_destination/{mod,stage}.rs` | EDIT | Native identity in the boot fixture; split receipt assertions from fixture construction and preserve its external text identifier |
| `rustd/crates/daemon/afd_api/tests/route_inventory.rs`, `route_meta_total.rs`, `harness/stubs_runner.rs`, `afd_api/src/openapi/stability/mod.rs` | EDIT | §4, §7: one route in, six out |
| `rustd/crates/daemon/afd_api/tests/runner_plane/native_opening.rs`, `runner_plane/decision_lifecycle/fixture.rs`, `runner_plane/decision_lifecycle/fixture/native_report.rs` | CREATE, EDIT | Native protocol/capacity refusals and acknowledged reports retain their completion-analytics proof |
| `rustd/crates/runner/afr_supervisor/src/downlink/**`, `uplink/**` | CREATE | §6: connect, hello, reconnect, dispatch; the prioritised writer |
| `rustd/crates/runner/afr_supervisor/src/{worker_pool,renew,activity,report,drainer,lease_loop,client,halt,verbs}/**`, `rustd/crates/runner/afr_supervisor/src/{lib,shutdown}.rs`, `rustd/crates/runner/afr_supervisor/src/test_util/mod.rs`, `rustd/crates/runner/afr_supervisor/Cargo.toml` | EDIT | §6: workers take offers; renewals batch per host; activity, reports and folds ride the uplink; the existing inbox implementation is `verbs::FencedVerbs`; shutdown retains one lease interval |
| `rustd/crates/runner/afr_supervisor/src/test_support/daemon/**`, `agentsfleet_runner/tests/fake_daemon.rs`, `afr_sandbox/examples/kernel_lane/supervisor_plane.rs`, `supervisor_checks.rs` | EDIT | §6: the fake daemons speak the stream |
| `rustd/crates/daemon/afd_bench/src/lane/lease/drain/{runner,window,mod,tests}.rs`, `lane/lease/saturation/production.rs`, `lane/lease/seed/mod.rs`, `make/bench.mk`, `bench/baselines/lease.rig.json` | EDIT | §8: the drain drives a session; idle totals replace per-poll ratios; median baseline |
| retired: `afd_api_runner/src/handler/runner/{heartbeat,wake,wake_tests,renew,activity,report,folds}.rs`, `handler/runner/lease/**`, `afr_supervisor/src/heartbeat/**`, `afd_api/tests/integration_runner_heartbeat.rs`, `agentsfleetd/tests/integration_runner_wake.rs`, `afd_approval/tests/integration_grant_request.rs` | DELETE, EDIT | §7: pull path and tests; fold client methods currently live in `afr_supervisor/src/client/lease_verbs/mod.rs` |
| `rustd/crates/daemon/main/src/serve/runner_stream.rs`, `rustd/crates/daemon/main/src/preflight/runner_listener.rs` and their parent module indexes | CREATE, EDIT | direct TLS listener and deployment configuration; exact transport mapping is fixed by §1 |
| `rustd/crates/daemon/afd_http/src/auth/{guard,request,runner_stream,mod}.rs`, `handler/refusal/mod.rs`, `services/{http_services,runner_stream,mod}.rs`, `services/leasing/lease_service.rs` | CREATE, EDIT, MOVE | Indy's middleware and naming review; explicit result types and one scope guard |
| `rustd/crates/daemon/afd_sse/src/stream_limit.rs` and all renamed-type callers in the exact review inventory | MOVE, EDIT | `StreamLimit`, `StreamPermit`, `LeaseService` and `AdmittedFleetWork` replace ambiguous names without aliases |
| `rustd/crates/daemon/afd_fleet/src/lease/{offer,deliver/issue,report/observations}.rs`, `report/observations/tests.rs`, `src/error/{kind,lift,classify,permanence}.rs` | CREATE, EDIT | Typed offer metadata and observation errors preserve saved timestamps and usage failures |
| `rustd/crates/daemon/main/src/preflight/{config,mod,read}.rs`, `plane/{config,services,tests}.rs` | EDIT | Resolve wake and memory settings before startup; invalid values remain absent until admission |
| `rustd/crates/daemon/afd_bench/src/error/{kind,mod,accessors}.rs` | EDIT | Preserve failed concurrency conversion rather than substituting a maximum value |
| `deploy/fly/agentsfleetd-dev/fly.toml`, `deploy/fly/agentsfleetd-prod/fly.toml` | Approved Nov 09 by Indy | private runner listener and certificate delivery; public ingress is outside this milestone |
| `~/Projects/docs` branch `chore/m243-runner-stream-changelog`: `runners.mdx`, `runners/install.mdx`, `cli/agentsfleet.mdx`, `api-reference/error-codes.mdx`, `changelog.mdx` | EDIT | §7: direct endpoint and network prerequisites, carrier fallback, kill behaviour and errors; API pages after merge |

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
| `make lint-all` (clippy `-D warnings`, `cargo shear`) | yes, new bus and gateway crates plus carrier dependencies | each dependency lands with its first caller (HLP) |
| Route inventory and meta-total tests | yes, one route in, six out | `INVENTORY` and the counts change in the commit that changes the routes |
| Metrics census and producer tests | yes | census rows, declarations and producers move together |
| Rust aggregate and patch coverage | yes | `make test-coverage-rustd`: at least 98% overall lines and 97.5% changed lines; include the kernel report when merging shards |
| OpenAPI and error-registry parity | yes | regenerate `public/openapi.json`; each code lands with its registry row and problem entry |
| Linux-only lanes | yes, `afr_sandbox/examples/kernel_lane` | linted and tested on Linux in a Codex cloud task |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, the session and the downlink grow | split by concern: `session/{hello,frames,recheck}.rs`, `downlink/{connect,dispatch,reconnect}.rs` |

## Prior-Art / Reference Implementations

- **Reference:** `openai/codex` revision `748ab55ca7de0b647d9a7a9d9a8e70e00f28666d`, `codex-rs/app-server-transport/src/transport/remote_control/websocket.rs` — reader/writer supervision, bounded shutdown and backoff reset after stability. Its framing and forked dependencies are not imported.
- **Reference:** `cloudflare/quiche`, `tokio-quiche/README.md` and native HTTP/3 drivers — client and server carrier candidate. Using the crate requires no Cloudflare service.
- **Reference:** `afd_api_tenant/src/handler/stream/revocable/mod.rs` — a held stream that re-checks access; the session re-checks on each heartbeat instead of a timer.
- **Reference:** `afr_supervisor/src/client/mod.rs` — `RunnerApi` as an `#[async_trait]` trait object, the shape `afd_bus::Bus` takes.
- **Reference:** `docs/architecture/push_scheduler.md` §M243 — the target design this spec builds.

## Sections (implementation slices)

### §1 — Direct HTTP/3 and HTTP/2 rest on measured evidence

The runner initiates a connection to a directly reachable daemon endpoint.
HTTP/3 is preferred, with HTTP/2 fallback when a network or transport failure prevents connection.
Both carriers use the same authenticated session and domain services.

Private Fly routing is the supported hosted deployment path.
External operators provide endpoint reachability, trusted certificates, and outbound UDP for HTTP/3 or TCP for HTTP/2.
An external runner needs private network access when its configured daemon endpoint is private to Fly.
Cloudflare and WebSocket are not prerequisites or fallback paths.

`runner_protocol.md` records the source references, library versions, stream mapping, test commands and measured outcomes.
The first candidate is `tokio-quiche` for HTTP/3, alongside Hyper and h2 for HTTP/2, with BoringSSL serving both carriers.
The spike checks build targets, certificate verification, native duplex operation, queue bounds, cancellation and telemetry before dependency selection is fixed.

The approved four-lane mapping, native connection binding, framing bytes, request identifiers and recovery budgets live in `runner_protocol.md`.
The spike verifies those choices on both carriers before production wiring.
Authentication, certificate and protocol refusals fail closed; changing the transport must not bypass them.

- **Dimension 1.1** — each carrier stays open and exchanges both ways → Test `direct_carriers_exchange_while_upload_remains_open` (native transport integration): HTTP/3 and HTTP/2 answer application pings during at least 120 seconds of an open request body.
- **Dimension 1.2** — a failed connection never hangs recovery → Test `carrier_failure_cancels_blocked_io` (native transport integration): interrupted reads, writes and handshakes end within their declared deadlines; no connection worker survives cleanup.
- **Dimension 1.3** — a report-sized message passes → Test `direct_carriers_preserve_report_sized_messages` (native transport integration): each carrier returns a 2 MiB message byte-identical and rejects an oversized declared length before allocating it.
- **Dimension 1.4** — unavailable UDP permits HTTP/2 → Test `unreachable_http3_falls_back_to_http2` (native transport integration): drop the HTTP/3 path, establish one HTTP/2 session within the recovery budget, and record the fallback reason.
- **Dimension 1.5** — refusal cannot become a transport downgrade → Test `terminal_refusals_do_not_trigger_fallback` (native transport integration): invalid certificates, runner credentials and protocol versions fail closed, without starting another carrier.
- **Dimension 1.6** — large payloads do not starve control → Test `both_carriers_preserve_control_progress_under_pressure` (native transport integration): cancellation and renewal complete while a large report and saturated activity queue are in flight.

### §2 — Pub/sub goes behind one bus seam

`afd_bus` holds the `Bus` trait (`publish`, `publish_batch`, `subscribe`) and the typed `Subject`: a fleet's tail, a draft's events, a wake shard and a runner's commands. Each implementation spells its own names. `Subscription` and `Received { Message, Lagged, Gap }` move from `afd_dragonfly::hub` with their meaning unchanged. `Command { Cancel { lease_id }, Recheck }` is the typed runner-command payload. The doorbell moves whole into `afd_bus::wake`, keeping 16 shards, 64 notifications and "ring after commit, count a lost ring". §7 deletes its hold. The Dragonfly implementation lives in `afd_bus/src/dragonfly/`, where M244 deletes it, over `afd_dragonfly`'s client and hub. It keeps today's channel spellings and `SPUBLISH`/`SSUBSCRIBE`, and hash-tags the runner id so one runner's commands land in one slot. Boot builds one `afd_bus::Handle` and hands it to every publisher and subscriber. **Implementation default:** the handle wraps `Arc<dyn Bus>`, because boot chooses the implementation once and M244 swaps it without threading a type parameter through every store; nothing downcasts, so NTE's concern does not arise. **Implementation default:** a daemon whose hub did not start still subscribes to nothing, as `afd_sse/src/live.rs` does today, because M244's frozen-screen fix owns that change and its A/B compares the same work.

- **Dimension 2.1** — no production crate names a Dragonfly pub/sub type → Test `git grep -n -w -e SubscriptionHub -e publish_tail -e "doorbell::publish" -- 'rustd/crates/*/src/*' ':!rustd/crates/daemon/afd_dragonfly' ':!rustd/crates/daemon/afd_bus'` (manual): prints nothing; today it prints hits in `afd_sse`, `afd_admission` and `afd_approval`.
- **Dimension 2.2** — a lost subscription reaches a bus subscriber as a gap → Test `a_repaired_node_gaps_every_bus_subscription` (integration, ported from the hub's gap fault test): after a node repair, a subscription opened through `afd_bus` reads `Received::Gap`.
- **Dimension 2.3** — runner commands cross replicas and stay addressed → Test `a_runner_command_crosses_replicas` (integration): `Cancel` published through one hub reaches a subscription for that runner on a second hub; a subscription for another runner reads nothing within 1 s.
- **Dimension 2.4** — a lost ring is counted, never refused → Test `a_failed_ring_counts_and_admits` (integration, ported from `doorbell/tests.rs`): with the publish deadline forced to elapse, the admission commits and `agentsfleet_fleet_ready_wake_failures_total{kind="ring"}` rises by 1.

### §3 — Protocol, schema and timings are fixed before the code that uses them

`af_wire::protocol` declares the messages in Interfaces as JSON tagged by `type`.
Up messages refuse unknown fields and declare their bounds with `garde`.
`MAX_WORKERS` caps running leases and renewal entries; `STREAM_FRAME_BYTES_MAX` caps messages at 2 MiB.

The codec enforces that cap before allocating an incoming payload.
`runner_protocol.md` records pinned carrier versions and §1's stream mapping.
The transport tests rerun when carrier or listener configuration changes.

Edit the existing table files for `runner.leases.cancel_requested_at` and `runner.runners.connection_id`.
The first is a nullable cancellation timestamp; the second is a nullable UUIDv7 connection owner.
No new table or ALTER migration file belongs here.

SQL binds and rows use `af_core::id::Uuid7` through its `sqlx` feature.
New statements use no identifier parameter casts, text-cast identifier outputs or String-typed identifier fields.
`fleet.events` is keyed by `event_id`; leases and tool calls refer to `event_id`.

Any SQL cast added or retained in changed M243 statements or schema expressions requires Indy's approval before implementation.
Present the exact expression, its reason and the typed alternative when seeking that exception.

The kill transaction writes the cancellation marker; renewal and reconnect read it.
Connect writes connection ownership; rechecks compare it.
Existing lease indexes support those reads, subject to verification after the data-layer follow-up lands.

The control request includes its previous owner in `agentsfleet-runner-session` after a successful welcome.
Automatic reconnect compares that owner inside acquisition; a newer owner causes UZ-RUN-021 and a halt.
A fresh process omits it and may supersede the previous process.

The marker remains after disconnect, while the local registry and heartbeat track serving and liveness.
Clearing it on ordinary disconnect would erase the evidence needed to refuse a superseded reconnect.

Timing constants and assertions live in `af_core/src/timing.rs`.
UZ-RUN-021, -022 and -023 land with their registry rows and problem entries.
The architecture decision lands before gateway code.

- **Dimension 3.1** — an up frame with an unknown field is refused → Test `an_up_frame_with_an_unknown_field_is_refused` (unit): a `renew_batch` frame with `"extra":1` fails to decode; the same frame without it decodes.
- **Dimension 3.2** — bounds hold → Test `a_renew_batch_past_max_workers_is_refused` (unit): `MAX_WORKERS + 1` entries are refused naming the bound; `MAX_WORKERS` entries pass.
- **Dimension 3.3** — the timings leave a full renewal window after a reconnect → Test `stream_timings_leave_a_renewal_window_after_reconnect` (unit): asserts each Interfaces relation against the production constants (TFX), and fails if `STREAM_ANSWER_TIMEOUT` is raised to 15 s.
- **Dimension 3.4 — DONE** — statements prepare against the edited table definitions → Test `test_every_sql_file_prepares` (integration, `afd_db/tests/integration_migrate/statements.rs`): passes with native UUIDv7 bindings and the cancellation and connection columns.

### §4 — The gateway serves the stream

`POST /v1/runners/me/stream` opens a bidirectional request and response over HTTP/3 or HTTP/2.
It uses `Guard::RunnerBearer`, `RouteClass::Stream`, and `runner:self` scope.
The handler authenticates the opening and refuses an unsupported `agentsfleet-runner-protocol` before returning a successful response.

The session owns its stream-ceiling permit and drain registration until it closes.
Its carrier adapter bounds reads, writes and queues independently of the gateway's domain operations.
The session shares `SSE_MAX_STREAMS`; a refused opening answers `503` with `Retry-After`.

`afd_gateway` keeps one registry per replica. On `hello`, the session writes its `connection_id` and commits, publishes `Recheck` for the runner, and reads the runner's live leases (`select_runner_live_leases.sql`) into its lease-target cache. It answers `welcome`. A lease in `running` that Postgres no longer holds gets `cancel` with UZ-RUN-011, and a marked lease gets `cancel` with UZ-RUN-022. A lease Postgres holds that `running` omits is left to lapse. **Implementation default:** lapse, because its body carried secrets the daemon never stored, and a second expiry writer would race renewal; the worst case is `LEASE_TTL` plus `OFFER_FALLBACK_TICK`, only when a cut lands between issue and delivery.

Offers come from claim passes. A pass walks the local sessions with credits: first those whose last heartbeat holds the ring's fleet, then the rest, longest-waiting first. For each, it runs `Plane::claim` with that session's holds and pushes `offer` while claims issue leases, and moves on at the first empty claim. A pass covering k rings ends after k leases or at the end of the walk. An empty pass so costs one claim per session with credits, the fan-out each ring causes today when every held beat polls once. Rings during a pass coalesce into one more pass, and a rise in a session's credits runs a pass for that session alone.

Every `OFFER_FALLBACK_TICK`, each replica runs one probe: does any unsettled admission wait with no live lease on its fleet (`select_waiting_admission.sql`, the candidate read's runner-free core)? A hit runs a pass. This catches a lost ring and a lease that lapsed with no ring, at one statement per replica per tick whatever the runner count. A `Gap` or `Lagged` on a wake shard counts as a ring; on a runner's command subject it runs `Recheck` and re-reads that session's cancel marks. Credits are a notification: the host lock's census decides whether a lease fits. A session whose last heartbeat answered degraded gets no pass, and the issue transaction's host lock refuses a degraded or non-active host whatever the session believes.

The other frames:
- **heartbeat** runs `Runners::heartbeat` and answers it. Its policy read is widened to return status, whether the stored token hash still equals the connection's, and `connection_id` (CODE-SEAM).
- **renew_batch** runs `Plane::renew` per entry and preserves each complete `RenewResponse`, including `recorded_through`, or its classified refusal.
- **activity** uses the cached target for ordinary chunks. Task observations retain their existing fenced database writes, canonical sequences and first-chunk telemetry. Cache eligibility expires with the confirmed lease deadline and clears on cancellation, refusal or report.
- **report** runs `Plane::report` and answers `report_ack` with the saved result or a refusal code that says whether a retry helps. The report's spend and delivery telemetry moves here from the deleted handler.
- **folds** runs M242_002's `Leasing::fold` under the lease's fence and answers `folded`. **Implementation default:** request and answer, not an unasked push, because the runner consumes input only at a model-call boundary, so a push would arrive no sooner.

Authoring leases arrive as offers; model calls, proposals/progress, renewals and reports use the same native session. Their four HTTP routes are removed. On drain the registry stops offering, answers accepted work, then sends `go_away` and closes within `DRAIN_TIMEOUT`.

- **Dimension 4.1** — a streamed chat reads the token once and no lease target → Test `a_streamed_chat_reads_no_lease_target` (integration, a private database filtered by `dbid` as `afd_events/tests/queued/snapshot.rs` does): one lease, 2 renewal batches, 40 activity frames and a report over the stream make 1 call of the runner-token statement and 0 of the lease-target statement. Over HTTP the lease-target statement runs 40 times.
- **Dimension 4.2** — an unknown protocol is refused before opening → Test `an_unknown_protocol_version_is_refused` (integration): `agentsfleet.runner.v9` answers `400` UZ-RUN-023, never a successful session response.
- **Dimension 4.3** — reconnect rebuilds from Postgres → Test `reconnect_rebuilds_active_leases_from_postgres` (integration): Postgres holds leases A, B and marked M; `hello` lists A, C and M. A renews, C gets `cancel` UZ-RUN-011, M gets `cancel` UZ-RUN-022, and B is offered to nobody until it lapses.
- **Dimension 4.4 — DONE** — rings coalesce and passes stay bounded → Test `rings_coalesce_into_one_claim_pass` (unit, paused time): 100 rings during one in-flight claim run exactly one more pass. `a_one_ring_pass_ends_at_its_first_lease` and `an_empty_pass_over_five_sessions_makes_five_claims` pin the positive budget and empty walk. The one-ring test also fails at its assertion when the production budget is increased by one.
- **Dimension 4.5 — DONE** — the fallback costs one probe a tick per replica → Test `the_fallback_tick_probes_once_per_replica` (unit, paused time): 50 registered idle sessions, an empty probe result and three ticks make 3 probes and 0 claims; a positive result on the fourth tick runs a pass through all 50 empty hosts. The injected probe tests clock ownership; the unchanged SQL query remains part of live integration verification.
- **Dimension 4.6** — a drain answers what it read, then sends `go_away` → Test `draining_sends_go_away_and_waits_for_answers` (integration): a drain that starts with a renewal in flight sends `renewed`, then `go_away`, then closes before `DRAIN_TIMEOUT`.
- **Dimension 4.7** — a report resent after a cut settles once → Test `a_report_resent_after_a_cut_settles_once` (integration): the same report on two connections writes one usage line, and both acknowledgements carry the same saved result.
- **Dimension 4.8** — a grant re-checks the host inside its transaction → Test `a_grant_rechecks_the_host_inside_its_transaction` (integration): a host flipped degraded after `hello`, with a session still believing it healthy, is issued nothing and its claimed admission returns to the ledger.
- **Dimension 4.9** — a stream past the ceiling is refused with a retry notification → Test `a_stream_past_the_ceiling_is_refused_with_retry` (integration): with `SSE_MAX_STREAMS=1` and one dashboard stream open, the session opening answers `503` with `Retry-After`.

### §5 — Cancel, revocation and supersede are a Postgres write plus a push

Each push names its Postgres authority, so pub/sub's at-most-once delivery costs latency, never correctness.

- **Kill.** The kill transaction in `afd_authoring/src/creation/patch.rs` marks the fleet's active leases in the same transaction (`mark_leases_cancel_requested.sql`). After commit, the handler publishes `Cancel` on each owning runner's subject. The replica holding that stream pushes `cancel`, and the runner cuts the lease with the class a terminal renewal refusal uses. **Implementation default:** that class, because both mean the daemon ended the lease, and a new class would change report and billing readers for no new decision. Backstops: renewal refuses a marked lease with UZ-RUN-022, and rebuild cancels it. Pause keeps today's behaviour.
- **Revocation, rotation, cordon, drain, policy edits, delete.** After each runner-row write commits, the operator handler publishes `Recheck`. The owning session re-runs the heartbeat's read without its writes. A non-active status closes with UZ-RUN-009, a changed token hash or a missing row with UZ-RUN-001, and anything else pushes the new `heartbeat` answer. Backstop: the next heartbeat runs the same read, and a session with no heartbeat for two intervals closes. Every grant and renewal re-checks inside its transaction (§4).
- **Supersede.** Guarded connection acquisition compares the authenticated token digest before changing `connection_id`. Issue and streamed renewal compare that digest and expected connection while holding the existing host row lock. A superseded session cannot obtain further authority even if its notification is lost. Its next recheck sends `go_away` UZ-RUN-021 and closes; a second process sharing the token halts rather than flapping.

- **Dimension 5.1** — a kill cancels a running lease from another replica → Test `killing_a_fleet_cancels_its_running_lease_across_replicas` (integration, two `Supervisor` daemons on the lane's Postgres and Dragonfly): the stream on replica B, the kill sent to A; the runner gets `cancel` within 1 s, the row carries `cancel_requested_at`, and the report settles with the renewal-refusal class.
- **Dimension 5.2** — a lost cancel push ends at the next renewal → Test `a_cancel_whose_push_is_lost_ends_at_the_next_renewal` (integration, a bus that drops runner commands): the next `renewed` answers UZ-RUN-022 for that lease and a deadline for its sibling.
- **Dimension 5.3** — revocation closes the stream → Test `revoking_a_runner_closes_its_stream` (integration): after a revoke the stream closes within 1 s, the reconnect answers `401` UZ-RUN-009, and the runner halts.
- **Dimension 5.4** — a lost revocation push closes at the next heartbeat → Test `a_revocation_whose_push_is_lost_closes_at_the_next_heartbeat` (integration, a bus that drops runner commands): the stream closes on the next heartbeat with UZ-RUN-009, and no offer reaches it after the revoke commits.
- **Dimension 5.5** — rotation closes the stream on the old token → Test `rotating_a_token_closes_the_stream_on_the_old_token` (integration): after `rotate`, the old stream closes with UZ-RUN-001 and the new token connects.
- **Dimension 5.6** — a second connection supersedes the first → Test `a_second_connection_supersedes_the_first` (integration, two replicas): the first stream gets `go_away` UZ-RUN-021 and closes, and only the second receives offers.
- **Dimension 5.7** — lost ownership notifications cannot grant authority → Test `a_stale_session_cannot_issue_or_renew_after_rotation_or_supersession` (integration): pause an old claim at the host lock, rotate or acquire a new connection, drop recheck notifications, and resume. The old transaction grants no lease or renewal; the current owner still succeeds.
- **Dimension 5.8 — DONE** — Kill invalidates an already selected admission. Tests `a_kill_committed_after_selection_prevents_a_later_issue` and `an_issue_waiting_on_a_kill_rechecks_the_committed_fleet_state` cover committed Kill and issuance waiting behind Kill. Rejected issuance writes no lease; reports retain their settlement path.

### §6 — The runner takes work from the stream

`afr_supervisor` tries HTTP/3 first and falls back to HTTP/2 on eligible connection failures.
Its proxy-aware HTTP/2 path respects the injected environment policy; HTTP/3 cannot bypass a mandatory proxy.
A healthy session keeps its chosen carrier until that session ends.

The next permitted normal connection setup tries HTTP/3 again after connection loss, daemon drain/restart or runner restart.
A healthy HTTP/2 session can remain on HTTP/2 indefinitely.
This milestone adds no periodic upgrade or hot carrier switch.

The runner sends `hello` with running leases, accepted kinds and idle workers.
It sends `ready` when credits change, heartbeats at the served interval, and application pings every `STREAM_PING_INTERVAL`.
An unanswered ping or renewal fails at `STREAM_ANSWER_TIMEOUT` after that request was sent.

Reconnect uses jittered backoff, capped at 30 seconds, with cancellation during every wait.
Short-lived connections retain backoff history; a stable connection can reset it.
The recovery budget includes carrier attempts, handshake, authentication and database rebuild.

Independent lease deadlines keep running during reconnect.
The runner stops a lease whose last confirmed deadline expires, even if connection recovery is still in progress.
The downlink dispatches each message:
- `offer` to an idle worker; workers never poll.
- `cancel` to that lease's interrupt token (`lease_loop/mod.rs`).
- `folded` to the lease's inbox (`verbs::FencedVerbs`); `heartbeat` answers to the assignment watch.
- `go_away` triggers a reconnect, except UZ-RUN-021, which halts. A `401` at session opening halts through `Halt::stops_on`.

The control writer prioritizes renewals, cancellation and pings over folds and heartbeats.
Reports and activity have separate bounded HTTP streams and writers; a report backlog cannot queue ahead of renewal.
Activity retains its 64 KiB batches and drops past `MAX_BATCHES_HELD`.

Renewals batch per host each `RENEWAL_TICK` and preserve each lease's complete `RenewResponse`, including `recorded_through`.
Each worker retains its report only in memory until its matching `report_ack`, and retries after reconnect.
Every answer echoes the session-local request identifier; disconnect fails and clears all pending callers.

- **Dimension 6.1** — a bounded blackhole recovers inside the lease → Test `a_blackholed_stream_reconnects_and_reports_once` (native integration for both carriers): inject a bounded interruption between probes and measure total detection and recovery time. Renew before the confirmed deadline and settle the report retained in worker memory once. A separate sustained-outage test, `an_outage_never_extends_a_confirmed_lease`, must stop the lease at expiry.
- **Dimension 6.2** — an activity flood never delays a renewal → Test `an_activity_flood_does_not_delay_renewal` (unit): with 1,000 activity batches queued, a renewal queued after them is the next frame written.
- **Dimension 6.3** — reconnects spread out → Test `reconnects_spread_over_the_jitter_window` (unit, paused time and injected random source): 100 runners reconnect within the initial jitter bound and occupy several time buckets. The test pins the deterministic injected sample; it does not demand an impossible perfectly even random distribution.
- **Dimension 6.4** — a superseded runner halts → Test `a_superseded_runner_halts` (unit): `go_away` with UZ-RUN-021 stops the runner, and it opens no new connection.
- **Dimension 6.5** — an idle runner asks for nothing → Test `an_idle_runner_sends_no_lease_request` (unit, paused time): an idle runner over 60 s writes only heartbeats and pings, and no frame asks for work.
- **Dimension 6.6** — a runner runs a lease end to end over the stream → Test `a_runner_runs_a_lease_over_the_stream` (e2e, the `integration_runner_e2e` harness): an admitted event reaches the supervisor as an `offer`, streams activity to the tail, and settles its report.
- **Dimension 6.7** — outbound proxy policy governs connection selection → Test `the_stream_connects_through_an_environment_proxy` (native integration, a CONNECT proxy and injected `MapEnv`): a required HTTP proxy selects the HTTP/2 path and receives the connection. No HTTP/3 attempt bypasses that policy.
- **Dimension 6.8** — connection failures and fallback are visible → Test `stream_failures_and_recovery_emit_correlated_telemetry` (native integration): collect finite transport/reason labels and correlated attempt, failure, fallback and recovery records. A sustained outage remains distinguishable from a stable connection.
- **Dimension 6.9** — usage acknowledgements survive carrier changes → Test `renewal_batches_acknowledge_usage_independently_across_reconnect` (integration): several leases each produce more than `LINES_MAX` records. Lose an answer, reconnect, replay and report. Each lease advances its own `recorded_through`; each provider request settles once.
- **Dimension 6.10** — delayed answers cannot reach another caller → Test `late_replies_do_not_complete_cancelled_or_reconnected_calls` (unit): cancel folds and renewal waits, deliver their answers late, and reconnect. New request identifiers and the new session's map retain the correct callers and bounds.
- **Dimension 6.11** — HTTP/3 returns on normal reconnect without a hot switch → Test `http3_is_reselected_only_when_the_http2_session_ends` (native integration): establish HTTP/2 with UDP blocked, restore UDP and keep the healthy session on HTTP/2; end it and prove the next permitted connection selects HTTP/3.
- **Dimension 6.12** — cancellation may overtake a work-lane offer. Test `a_cancel_received_before_its_offer_prevents_execution` delivers that ordering and asserts zero executions. Full cancellation state closes the session safely.

### §7 — The pull path is deleted, and the docs describe the stream

Six routes go: `POST /v1/runners/me/heartbeats`, `/leases`, `/leases/{lease_id}/renew`, `/leases/{lease_id}/activity`, `/leases/{lease_id}/folds` (M242_002) and `/reports`, with their handlers, client verbs and tests. The doorbell's hold (`wait_for_ring`, `Waited`, the held count, `close`) goes with `Leasing::wait_for_work`, `NO_WORK_RETRY_AFTER`, `WAKE_HOLD_MAX`, `HELD_BEATS_MAX`, `wake`, `woken`, `LeaseRequest`, `LeaseResponse` and `agentsfleet_runner_wake_answers_total`. **Implementation default:** the removed routes answer `404`, following the pre-2.0.0 precedent in M145_001 and M228_001; `VERSION` is 0.75.0, and there is no compatibility period (NLG; Indy, Discovery). The architecture pages, `docs/AUTH.md` and the docs repository change in the same Pull Request (ARCH-HOMES, ARCH-PUBLISHED).

- **Dimension 7.1** — the retired routes are gone → Test `the_retired_runner_routes_answer_404` (integration): each of the six `POST`s answers `404`, while `GET /v1/runners/me` still answers `200`.
- **Dimension 7.2** — no census row outlives its producer → Test `every_census_family_has_a_producer` (unit): passes with the wake-answers row gone and each new stream-health family paired with its producer.

### §8 — The bench proves the stream's cost

The drain's simulated runner (`afd_bench/src/lane/lease/drain/runner.rs`) drives an `afd_gateway` session in process: offers in; renewal batches, M242_002's folds requests, 40 activity frames and a report up. M241's statement paths classify the result unchanged. The idle window keeps every simulated runner's session connected with credits and no admissions for `IDLE_WINDOW`. It records `drain_idle_statements` and `drain_idle_dragonfly_calls`, and the per-poll keys go, since no poll exists. Heartbeats and the per-replica fallback probe sit outside the bench, which runs no registry; Dimension 4.5 bounds the probe. Three interleaved runs follow a reset rig, and the median run is copied to `bench/baselines/lease.rig.json`. The other two values go into `push_scheduler.md` §Baselines.

- **Dimension 8.1** — a streamed chat's activity path is near zero and an idle host costs nothing → Test `make bench-lease PROFILE=rig` (integration): `drain_statements_per_lease_activity` is under 1, against M241's 40. `drain_idle_statements` and `drain_idle_dragonfly_calls` are 0, against today's one Dragonfly call per idle poll.
- **Dimension 8.2** — an idle window with no polls reports totals, never a ratio → Test `an_idle_window_reports_totals_not_ratios` (unit): a window of 0 polls writes `drain_idle_statements` 0 and no `idle_statements_per_poll` key.
- **Dimension 8.3** — baselines are median runs → Test `baselines_are_median_runs` (manual): §Baselines lists three values per headline key, and the committed file holds the middle one.

## Interfaces

- **Route:** `POST /v1/runners/me/stream` over HTTP/3 or HTTP/2, with `Authorization: Bearer agt_r…` and `agentsfleet-runner-protocol: agentsfleet.runner.v1`. The opening answers `200`, `400` UZ-RUN-023, `401` UZ-RUN-001 or UZ-RUN-009, `503` with `Retry-After` at the ceiling, and `503` UZ-AUTH-004 when authentication storage cannot answer. §1 fixes any additional lane-binding headers before implementation.
- **Frames**, JSON text tagged by `type`; bodies reuse today's wire types:
  - up: `hello{request_id, accepted_kinds, running[lease_id], credits}`, `ready{credits}`, `heartbeat{request_id, HeartbeatRequest without wake}`, `ping{request_id}`, `renew_batch{request_id, leases[{lease_id, RenewRequest}]}` (`Up::RenewBatch`), `activity{lease_id, frames}`, `report{request_id, ReportRequest}`, `folds{request_id, lease_id, FoldRequest}`.
  - down: `welcome{request_id, connection_id, heartbeat_interval_ms}`, `offer{TaggedLease}`, `heartbeat{request_id, HeartbeatResponse without woken}`, `pong{request_id}`, `renewed{request_id, leases[{lease_id, RenewResponse | refusal}]}`, `report_ack{request_id, lease_id, ReportResponse | refusal}`, `folded{request_id, lease_id, FoldResponse | refusal}`, `cancel{lease_id, error_code}`, `go_away{error_code?}`. Every answerable up message carries the same positive request identifier; ready and activity need no reply.

The Nov 09 authoring correction adds four correlated exchanges, each carrying `request_id`, `lease_id` and its current operation body.
Work carries `authoring_model` / `authoring_model_result` and `authoring_activity` / `authoring_activity_ack`.
Control carries `authoring_renew` / `authoring_renewed`; reports carries `authoring_report` / `authoring_report_ack`.

Each authoring mutation checks the current connection owner and credential under the host lock.
The provider receipt checks authority again after physical HTTP finishes, retaining known usage while withholding fenced private output.
Accepted provider work stays tracked after a native disconnect, bounded by the original operation deadline and daemon shutdown.

The same native-session integration cases check saved model results, proposal receipt identity, renewal alongside blocked model HTTP and completion replay.
Further cases cover lost replies, changed replay content, private deletion, pending accounting, credential replacement and connection fencing.
These cases remain unverified until the code-completion review is finished and integration verification resumes.

```
↑ {"type":"hello","accepted_kinds":["fleet","authoring"],"running":[],"credits":4}
↓ {"type":"welcome","heartbeat_interval_ms":10000}
↓ {"type":"offer","lease":{…TaggedLease, as the poll answers today…}}
↓ {"type":"cancel","lease_id":"0190f2c4-…","error_code":"UZ-RUN-022"}
```

- **Bus:** `afd_bus::Bus` with `publish(&Subject, &str)`, `publish_batch(&Subject, &[String])` and `subscribe(&Subject) -> Subscription`. `Subject` is `FleetTail(fleet_id) | DraftEvents(draft_id) | Wake(shard) | RunnerCommand(runner_id)`. `Received` is `Message | Lagged(u64) | Gap`, and `Command` is `Cancel{lease_id} | Recheck`.
- **Schema:** `runner.leases.cancel_requested_at TIMESTAMPTZ(3) NULL`; `runner.runners.connection_id UUID NULL`, with a UUIDv7 check when set.
- **Error codes:** UZ-RUN-021 "Runner stream superseded" (409); UZ-RUN-022 "Lease cancelled" (409); UZ-RUN-023 "Runner protocol unsupported" (400).
- **Timing** (`af_core/src/timing.rs`): `STREAM_PING_INTERVAL` 15 s; `STREAM_ANSWER_TIMEOUT` 5 s; `RECONNECT_JITTER_MAX` 2 s; `STREAM_RECOVERY_BUDGET` 7 s, comprising HTTP/3 setup 2 s, HTTP/2 setup 2 s and opening/rebuild 3 s; `STREAM_STABLE_RESET_AFTER` 60 s; `OFFER_FALLBACK_TICK` 5 s; `PROXY_IDLE_FLOOR` 60 s. Assert `RENEWAL_TICK + STREAM_ANSWER_TIMEOUT + RECONNECT_JITTER_MAX + STREAM_RECOVERY_BUDGET < LEASE_TTL − RENEWAL_WINDOW`. Preserve the ping, heartbeat, fallback and drain relations; cap every recovery attempt by the lease's remaining safe lifetime.
- **Removed:** the six routes in §7, the four authoring lease routes, and `retry_after_ms` with `LeaseResponse`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Intermediary idles out a session | failure | a deployment's idle timeout | application pings preserve the tested idle interval; a cut triggers bounded recovery | `direct_carriers_exchange_while_upload_remains_open` |
| Stream blackholed mid-lease | failure | network blip, half-open socket | unanswered requests fail after 5 s; idle ping-only detection can take 20 s; measure complete recovery and retain independent lease expiry | `a_blackholed_stream_reconnects_and_reports_once`, `an_outage_never_extends_a_confirmed_lease` |
| HTTP/3 path unavailable | failure | blocked UDP or failed transport setup | bounded HTTP/2 fallback; one serving session; reason counted | `unreachable_http3_falls_back_to_http2` |
| Certificate or credential refusal | security | invalid endpoint identity or runner authority | fail closed; no transport bypass | `terminal_refusals_do_not_trigger_fallback` |
| Unknown protocol version | failure | a runner build the daemon does not speak | `400` UZ-RUN-023 before accepting the session | `an_unknown_protocol_version_is_refused` |
| Malformed or oversized up frame | failure | a broken runner build | strict decode fails; the bounded codec rejects messages past 2 MiB before payload allocation | `an_up_frame_with_an_unknown_field_is_refused`, `direct_carriers_preserve_report_sized_messages` |
| Ring lost | failure | publish timeout or a hub gap | the next tick's probe finds the work and runs a pass within 5 s; the loss is counted | `the_fallback_tick_probes_once_per_replica`, `a_failed_ring_counts_and_admits` |
| Burst of rings | capacity | 50 fleets at 10 messages a second | rings coalesce into passes, each bounded by its leases or one claim per session | `rings_coalesce_into_one_claim_pass` |
| Offer lost in a cut | race | the stream dies between issue and delivery | the lease lapses and the next tick's probe reclaims it | `reconnect_rebuilds_active_leases_from_postgres` |
| Cancel push lost | failure | pub/sub is at most once | the next renewal refuses with UZ-RUN-022 | `a_cancel_whose_push_is_lost_ends_at_the_next_renewal` |
| Runner away during a kill | failure | disconnected at the kill | rebuild sends `cancel` on reconnect | `reconnect_rebuilds_active_leases_from_postgres` |
| Revocation push lost | security | pub/sub is at most once | closed at the next heartbeat; grants refused in their transaction | `a_revocation_whose_push_is_lost_closes_at_the_next_heartbeat` |
| Token rotated | security | operator rotation | the old token's stream closes with UZ-RUN-001 | `rotating_a_token_closes_the_stream_on_the_old_token` |
| Degraded host | security | capability lost mid-stream | no offers; the host lock refuses inside the issue | `a_grant_rechecks_the_host_inside_its_transaction` |
| Two processes share a token | race | a duplicated host | the newer connection wins; the older halts with UZ-RUN-021 | `a_second_connection_supersedes_the_first`, `a_superseded_runner_halts` |
| Report reply lost | replay | a cut after the commit | the worker retries from memory; the lease-keyed report answers its saved result, charged once | `a_report_resent_after_a_cut_settles_once` |
| Rolling deploy | failure | drain | answers in flight are sent, then `go_away`; the runner reconnects elsewhere | `draining_sends_go_away_and_waits_for_answers` |
| Mass reconnect | capacity | a tunnel restart | reconnects spread over the jitter window | `reconnects_spread_over_the_jitter_window` |
| Stream ceiling reached | capacity | dashboards hold every slot | `503` with `Retry-After`; the runner retries | `a_stream_past_the_ceiling_is_refused_with_retry` |
| Activity flood | capacity | a chatty run | renewals are written first; activity past the cap drops | `an_activity_flood_does_not_delay_renewal` |

## Invariants

1. No lease is granted to a non-active or degraded host, and none is renewed for a revoked or degraded one — `afd_runner::capacity::lock` decides inside the issue and renewal transactions, whatever the session believes; `a_grant_rechecks_the_host_inside_its_transaction` is the counterexample.
2. A cancel outlives a lost publish — `cancel_requested_at` commits before the publish, and renewal and rebuild read it; `a_cancel_whose_push_is_lost_ends_at_the_next_renewal` fails if renewal ignores it.
3. One runner has one serving stream — `connection_id` is last-writer-wins, and every heartbeat read compares it; `a_second_connection_supersedes_the_first` fails if the comparison goes.
4. A bounded first-attempt recovery leaves renewal slack; every outage retains the confirmed expiry deadline. The timing assertion includes the full seven-second carrier/open/rebuild budget. `an_outage_never_extends_a_confirmed_lease` proves the unconditional deadline rule.
5. A worker releases its pending report memory only on acknowledgement or a terminal refusal; `a_blackholed_stream_reconnects_and_reports_once` fails on a lost or doubled report.
6. Renewals never queue behind activity — the uplink's biased select; `an_activity_flood_does_not_delay_renewal` fails if the order flips.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `agentsfleet_runner_stream_closes_total` | operator | a session closes | `reason`: client, silent, superseded, revoked, draining, protocol | no token, no tenant identifier | `revoking_a_runner_closes_its_stream` |
| `agentsfleet_runner_offer_claims_total` | operator | a claim pass runs one claim | `outcome`: leased, empty | none needed | `rings_coalesce_into_one_claim_pass` |
| Stream connection attempts, fallback and recovery families | operator | connection attempt, eligible fallback, reconnect or restored session | closed transport, reason and outcome labels; measured durations | no token, payload or unbounded identifier label | `stream_failures_and_recovery_emit_correlated_telemetry` |
| `agentsfleet_runner_wake_answers_total` (removed) | operator | never again | none | none | `every_census_family_has_a_producer` |
| `runner_stream_started`, `runner_stream_completed`, `runner_stream_failed` (log events) | operator | connect, clean close, error close | `runner_id`, `connection_id`, `error_code`, `duration_ms` | runner identifiers only; no token or workspace identifier at `info` | `a_second_connection_supersedes_the_first` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Both native carriers have recorded transport evidence (§1) | `make test-integration-rustd` and the commands/outcomes in `runner_protocol.md` | native duplex, fallback, refusal and control-pressure tests pass; an empty heading is insufficient | P0 | |
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
| R13 | Direct HTTP/3 and HTTP/2 pass native duplex and pressure tests (§1) | native transport integration through `make test-integration-rustd` | both carriers pass; recorded commands and outcomes in `runner_protocol.md` | P0 | |
| R14 | UDP failure selects HTTP/2 without bypassing refusals (§1) | `make test-integration-rustd` | fallback and fail-closed tests pass | P0 | |
| R15 | Flaky and unavailable sessions have visible telemetry (§6) | `make test-integration-rustd` | correlated failure, fallback, outage and recovery evidence | P0 | |
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
| retired handler, test and module files | `git ls-files rustd/crates/daemon/afd_api_runner/src/handler/runner/heartbeat.rs rustd/crates/daemon/afd_api_runner/src/handler/runner/wake.rs rustd/crates/daemon/afd_api_runner/src/handler/runner/lease rustd/crates/runner/afr_supervisor/src/heartbeat rustd/crates/daemon/afd_dragonfly/src/doorbell rustd/crates/daemon/main/tests/integration_runner_wake.rs` | no output |

Hits under `docs/v2/` are records and stay. CHORE(open) reruns each search on `origin/main` and adds any new hit to Files Changed.

Schema this milestone leaves with no runtime reader or writer is removed from its owning table file in this Pull Request, under Indy's blanket approval (Discovery). An unused table loses its table file; a removed column or index changes that table's file. Each removal gets a row above, with a `git grep -n -w` over `rustd`, `ui` and `cli` proving nothing still reads or writes it. The owner's current table-file layout supersedes the historical new-slot wording below.

## Out of Scope

- One statement per host renewal: M245. Each lease keeps its own renewal statements here.
- The NATS implementation behind `afd_bus`: M244.
- A pause that cancels a running lease: pause keeps today's behaviour.
- Aiming a ring at one eligible runner: an empty pass still costs one claim per session with credits, as each ring costs today, counted by `agentsfleet_runner_offer_claims_total`. Not planned until that count matters.
- Customer-runner trust (M246) and the lapsed-host respawn signal (M247).
- Deployment changes without a concrete reviewed patch. Private Fly routing needs deployment evidence; a hosted public ingress deployment is outside M243.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator kills a fleet mid-run and its run stops within a second, and a host idle all day sends no lease request yet picks up the next message as soon as it is admitted.
2. **Preserved user behaviour** — enrolment, the `agt_r` token, policy edits from the dashboard, live tail, reports and billing, and authoring.
3. **Optimal-way check** — one outbound session per host connects directly to the daemon, preferring HTTP/3 with HTTP/2 fallback. M245 reduces renewal statements.
4. **Rebuild-vs-iterate** — iterate: the lease plane and database report writes stay; transport changes and runner disk report storage is deleted.
5. **What we build** — the stream route and gateway, the frames, the runner's downlink and uplink, the cancel marker, the connection owner, and the bus seam.
6. **What we do NOT build** — a replay buffer (Postgres rebuilds), compatibility routes (pre-production), NATS (M244), and single-runner ring routing (unmeasured).
7. **Fit with existing features** — compounds with M242's folding and M244's broker swap. It must not destabilise the report and billing path, which keeps its statements and its lease-keyed retry.
8. **Surface order** — runner-first; no user interface (UI) change. The docs repository's runner pages change with the release.
9. **Dashboard restraint** — no "connected" badge; liveness stays `last_seen_at`, until the stream-close metric shows operators need one.
10. **Confused-user next step** — runner logs identify transport, refusal, fallback and recovery. `runners/install.mdx` lists direct endpoint, certificate and network prerequisites.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the spike first, so the transport rests on evidence. The bus seam next, since it stands alone. Then protocol and schema, the daemon, cancel and auth, the runner, deletion, and the bench last, measuring the finished path.
- **Alternatives considered:**
  - HTTP/3 with HTTP/2 fallback: selected by Indy. The deployment supplies a directly reachable endpoint; private networking serves colocated Fly services.
  - WebSocket or mandatory Cloudflare routing: rejected by Indy. Neither belongs in the runtime implementation.
  - A down-only stream with `POST` uplinks: rejected, because each post keeps per-request auth.
  - A gateway replay buffer: rejected, because Postgres already holds every lease and mark.
  - Per-session fallback polling: rejected, because it makes idle cost grow with runners.
  - A cancel by publish alone: rejected, because pub/sub is at most once.
- **Patch-vs-refactor verdict:** a **refactor** of the runner transport, because every runner verb changes carrier while the lease plane's statements stay.

## Discovery (consult log)

Indy changed the working order on Nov 09, 2026:

> can you complete the code first and then we can fix CI and the lint, cargo checks and so on.
>
> Ask indy for approval before running lint, test-unit and so.
>
> once you write the code just push the changes
>
> or commit them.
>
> and push to the PR

Finish and publish code to draft #819 before starting further manual verification.
Ask Indy before lint, tests, Cargo checks or other verification commands.
The draft must distinguish written code from verified behavior; retain the morning merge hold.

Indy requested a review checkpoint after the current feedback is fixed:

> So after you fix the above changed, take a check point with all the changes i have reviewed, so i can look at the next set of changes again.

The checkpoint includes the source review for RS-SENTINEL and RS-UNWRAP across the PR diff.
It preserves explicit publication errors and treats notification observation as the caller's post-commit policy.
No new milestone Dimension is marked verified by this checkpoint.

The precision review replaces census truncation with the issue statement's returned stored `created_at`.
The shared capacity clock remains unchanged.

- **Current owner decisions, Nov 08, 2026:**
  > Indy: "since i dont want to use websocket" — context: remove the earlier transport default.

  > Indy: "so even in external we could dictate that as a pre req, with HTTP/2 as fallback." — context: external operators supply direct connectivity prerequisites.

  > Indy: "and HTTP/3" — context: HTTP/3 is preferred, with HTTP/2 fallback; both belong in this milestone.

  > Indy: "Keep the specified timings and correct the detection claim to the tested bound (Recommended)" — context: preserve the 15-second ping interval and 5-second answer timeout, and measure complete fault detection.

  > Indy: "No legacy support to think about in this PR." — context: replace existing code, SQL, schema, routes and tests in place; add no compatibility surface.

- **Implementer source checks:** `origin/main` is now `61820b66a`, including #815, #816 and #817. Wire types, bounded framing, native adapters and the bus adapter are written. Gateway, runner integration and complete bus migration remain due.
- **Owner implementation approval:** Indy approved the [runner design](../../architecture/runner_protocol.md) with "Other than than the I am sign off / approve the implementation." and "Yes keep going to a PR". The shared `af_stream` crate, fourth `work` lane, private Fly deployment scope and native proof gates now govern implementation. BoringSSL is scoped to both new runner carriers; a project-wide Rustls removal remains separate.
- **Current owner constraints:** "Private Fly routing is what we will support, for external runners, i suggest we ask folks to have HTTP/3 QUIC or HTTP/2 enabled." Also: "If you find doing cast, get it signedoff / approved with indy" and "I dont wish to build the hot switch from http/2 to http/3 now". Apply those constraints to routing prerequisites, changed SQL and reconnect behavior.
- **M244 alignment:** its source specifies core NATS behind `afd_bus::Bus`, no JetStream or key-value store, neutral `Message`/`Lagged`/`Gap`, and durable facts moved to Postgres before Dragonfly deletion. M243 builds that replaceable signal boundary and keeps runners free of broker credentials.
- **Reference study:** cloned RustAPI, Quiche and Neqo outside this worktree at the owner's request. The study distinguishes established transport libraries from the new native adapter; TOON model input remains outside this stream.
- **Inventory evidence:** The Files Changed table is rechecked against current `origin/main` after each refresh. Existing table paths use #816's timestamp filenames. Recheck every final path again before closing the stream.
- **Path corrections:** folding lives in `afr_supervisor/src/client/lease_verbs/mod.rs` and `verbs::FencedVerbs`; the proposed `client/folds.rs` and `inbox.rs` do not exist.
- **Transport references:** Codex's inspected checkout is `748ab55ca7de0b647d9a7a9d9a8e70e00f28666d`. Its supervision patterns apply; its WebSocket dependencies do not.
- **Review status:** the three fresh-context Elephant-Goldfish reviewers completed their first audit. Accepted corrections fix the three-lane mapping, connection-local binding, framing, response correlation, full renewal acknowledgements, aggregate recovery budget, grant authority fencing, task activity, drain order and test registration. Native evidence remains an implementation gate, not a claimed pass.
- **Data dependency:** #816 merged as `a5155b6f8`, including `af_core/sqlx`, native UUIDv7 bindings and timestamp table files. This branch was rebased onto that main revision. New data access keeps those native identifier types.
- **Native adapter evidence:** the first production suite passed 14 tests, including four lanes per connection, framing failures and certificate refusals. Its log is `/workspace/scratch/m243-logs/native-cap-proof-tests-v1.log`. This run used a scratch dependency override; it proves neither the final dependency selection nor repository gates.
- **Dependency approval:** Indy chose “Pin workspace libc to 0.2.189 and require full Linux gates (Recommended)”. The workspace and lockfile now use that released version with published `tokio-quiche 0.20.1`. The scratch override is absent from dependency resolution.
- **Configuration request:** Indy asked, “I think these have to be configurable via toml or so”. Follow the existing typed environment resolver. `AGENTSFLEET_RUNNER_WAKE_QUEUE_CAPACITY` controls the per-replica queue, defaults to 64 and accepts 1–65,536; daemon boot wiring remains due.
- **Naming review:** Indy requested wake-queue terminology and the existing `AGENTSFLEET_` environment prefix. The buffer is `WakeQueue`; matching constants, tests, producers and new metrics use queue names. Proposed stream-listener and certificate settings use that prefix too.
- **Fairness requirement:** a replica's bounded notification queue is scheduling look-ahead, never durable ownership. Its unconditional five-second database probe must run during continuous notifications. Add a 1,000-fleet test proving older eligible work progresses with available runner capacity; the bounded-buffer test alone proves no such guarantee.
- **Coverage requirement:** Indy requested the Rust minimum gate. Main requires 98% overall line coverage and 97.5% changed-line coverage. The new crates automatically enter the substrate shard; final measurement remains due.
- **Main refresh:** #817 merged as `61820b66a` and was merged normally into this published branch without conflicts. Its approved coverage change sets the overall floor to 98%, replacing 98.5%; the patch floor remains 97.5%. No local gate was weakened.
- **Collector ownership:** the daemon installs `af_observability` producers, while the runner has its own collector and recorder. Transport startup must install the shared stream families in each process correctly. In-process capture tests alone cannot prove that a disconnected runner exports failed connection attempts.
- **Early draft request:** Indy said, “well send the draft PR when you have the check, fmt clippy lints taken care”, then “so the claude agent can start”. Publish the current bus and transport work as a draft after those checks. Remaining implementation, integration, coverage and final shipping gates stay required before merge.
- **Test environment:** process 1 is `tail`, and unreaped children caused two executor cleanup failures. The identical compiled executor suite passed 57 tests under existing `docker-init -s`; no executor source changed. Final Make lanes must run with that child reaper.
- **Bus ownership:** the adapter preserves native subscription drop behavior and shares payload bytes across consumers. `shutdown` joins the three signal operations on `Bus`, so drain releases subscriptions through the same boundary. Adapter tests and full publisher/subscriber migration remain due.

- **Consults** — source review on `main` `14539a73e` plus M241's branch, Nov 06, 2026:
  - Auth admits only `active` runners (`afd_state/src/credentials/rows.rs`), so cordon and drain refuse every call today; the stream keeps that by closing.
  - `capacity::lock` already refuses a non-active or degraded host inside issue, and a revoked or degraded one inside renewal (`afd_runner/src/capacity/mod.rs`); `docs/AUTH.md` requires exactly that check once auth stops running per request.
  - The report is lease-keyed and idempotent ("Repeats return the saved result without another charge", `afd_api_runner/src/handler/runner/report.rs`). Discovery found runner report files; Indy rejected retaining those files on Nov 09.
  - Kill marks nothing on `runner.leases` (`afd_authoring/src/creation/patch.rs`; `runner_fleet.md` §Steer, kill, pause).
  - `Plane::claim` picks either work kind (`afd_fleet/src/lease/claim.rs`), so authoring offers cost nothing extra.
  - axum has no `ws` feature and no `tungstenite` crate is in `Cargo.lock` (`rustd/Cargo.toml`); `SSE_MAX_STREAMS` defaults to 256; `DRAIN_TIMEOUT` is 15 s.
  - The tunnel sets no `originRequest` (`deploy/fly/cloudflared-prod/config.yml`), and the daemon has no public Fly service.
- **Reports decision (owner, Nov 09):** "Remove runner disk storage; retry from memory". Workers retain one current-format report until acknowledgement. Postgres owns saved completion and billing records. Process loss can lose uncommitted results and usage. No disk recovery or backward-format retry remains.
- **Cross-spec note:** M242_002's Out of Scope expects M243 to push input with the read as backstop. §4 moves the read onto the stream and declines the push; the review reconciles that line.
- **Authoring delivery (superseded by Nov 09 direction below):** authoring leases arrive as offers. Their model, activity, renewal and report operations now move onto the native session. Completed authoring reports remain in worker memory until acknowledgement. The runner stores no private report, accounting report or decision receipt files.
- **Owner decisions** —
  > Indy (Nov 06, 2026: 08:50 AM): "And we are not in production yet, so i want to rethink the approach on admissions, lease, and so on as i am focussed on scale. If something fails in one host bare metal, it should automatically cordon and respawn in another runner" — context: no compatibility period for the retired routes.
  > Indy (Nov 06, 2026: 10:19 AM): "This will be on by default durable_tasks_v1. ... my plan also was to have folks spin a runner and point to the agentsfleetd (in cloud) and keep using it." — context: one stream through the public API, with no broker credential on the runner.
  > Indy (Nov 06, 2026: 10:43 AM): "1. Yes placement rules are via the tags and labels 2. Respawn across ownership - this is a tenant setting ... Based on the scope i would like to build this ... 4. Yes, but donot overengineer now, we can improve this." — context: no replay buffer and no single-runner ring routing.
  > Indy (Nov 06, 2026: 11:57 AM): "Yes i am ok with postgres store and accepting messages, fences runs" and "The message has to wait. I assume you show message like compacting" and "assume atleast 50 fleets" — context: the cancel marker and connection owner live in Postgres; fences stay the authority.
  > Indy (Nov 06, 2026: 01:18 PM): "Yes i agree on NATS." — context: the bus seam built here, so M244 adds NATS behind it.
  > Indy (Nov 06, 2026: 02:13 PM): "And i prefer we create all the spects and close M241 and push a PR?" — context: this spec lands with M241.
- **Programme decisions (relayed in the authoring brief, Nov 06, 2026; transport replaced by the owner's Nov 08 decision):** one outbound session per host; self-hosted runners never touch the broker. Authentication happens at session opening; revocation closes the session; each grant rechecks the host. Postgres rebuilds state and holds cancellation markers. No compatibility period preserves retired routes.
  > Indy (Nov 06, 2026: 04:18 PM): "yes" — context: blanket approval, asked as "Should each milestone drop the tables and columns it leaves with no reader or writer?": a milestone drops, in a new slot in its own Pull Request, any table, column or index it leaves with no runtime reader or writer (`.orly/standards/domains/sql.md` destructive-change approval).
- **Model reviews** — implementer source-readiness review and accepted corrections appear above; Indy's new implementation design review remains pending.
- **Open implementation decisions** —
  1. §1 must prove both direct carriers and fix their stream mapping, framing and session authentication binding.
  2. Prepare a concrete private Fly listener and certificate-delivery proposal before seeking approval for deployment edits. Document external runner prerequisites without adding hosted public ingress.
  3. Reverify the complete Files Changed inventory after the data-layer follow-up lands. New SQL must use its native UUIDv7 bindings and table-file layout.
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.

### Morning handoff and provisioning

Indy said:

> so then keep the PR ready for merge and in your report tell me the instructions to run the additional needs for a private listener to be generated so indy can wake up and update the 1Password

Prepare the pull request with green checks and resolved review feedback, then hold merge for the morning provisioning handoff.
The report includes certificate-generation commands, exact vault fields, protected runtime files, environment settings, reuse choices and verification steps.
Do not publish certificate private keys or resolved secrets in the report.

### Approved post-quantum preference

Indy said:

> If it supports then i prefer to use, since it helps when the runners are public or what ever outside as they connect.

Use TLS 1.3 and prefer `X25519MLKEM768` for the native runner transports.
Verify actual negotiation in transport tests and expose its finite classification in connection observations.
Certificate signatures remain conventional unless a separate supported authentication change is approved.

### Nov 09 report-storage correction

Indy rejected runner report files, old-format retries and added storage machinery.
His selected answer was: "Remove runner disk storage; retry from memory".
He also required: "No backward compatibility crap, when you remove storage reports."

Postgres saves accepted results and billing records.
Workers retry unacknowledged reports and task-decision receipts from memory without repeating provider calls.
Process loss can leave an uncommitted result or unknown provider usage.
Grafana receives performance observations separately; traces and local files are never a billing ledger.

Source changes and unfinished feedback appear in `docs/reviews/m243/owner-feedback-audit.md`.
No verification result is claimed for this correction.

### Nov 09 authoring transport correction

Indy said: "I think the authoring job must move to the stream, that is th e whole point in making this faster? or is that part of M245 or M246 or M247? if so i can wait".

The current M245 plan changes database cost, M246 adds tenant-owned runners and protected runtime model access, and M247 adds failed-host recovery. None owns migration of the four existing authoring operations. M243 therefore carries authoring model calls, proposals/progress, renewals and reports over its existing authenticated native connection and deletes their HTTP routes, handlers and HTTP client code. This supersedes the earlier HTTP retention decision.

Model invocation keeps its existing long provider budget and physical dispatch identifier; disconnect never automatically repeats a model dispatch. Control ping, renewal and cancellation progress while a model call is pending. The daemon retains provider credentials, existing authoring validators, Postgres settlement and the shared Bus. Completed reports remain only in runner memory until acknowledgement. No report file or backward-format path is added. Transport latency remains unmeasured.

The M246 implementation must use the current authoring model-call stream boundary as its reference after M243 merges. Its tenant ownership and runtime provider-proxy work remain M246 scope.

### Nov 09 managed deployment completion

Indy corrected the deployment boundary: "deployment is part of this PR. since when merged runner that is working must work."
He directed reuse of the existing vault-reference delivery pattern and confirmed: "Already provisioned; use the private Fly names".
Daemon development and production workflows load only the leaf chain/key, stage Fly file secrets and declare their paths in the application configuration.
Runner deployments load only public CA trust and server-name, install trust and write the native origin in the existing protected environment file.
The same deployment verifies a recent native heartbeat; read-only HTTP authentication alone is insufficient.
Claude is checking the actual host routing on coordination issue 818; no deployment, vault read or key generation is authorized for this verification session.

The two environments each hold one `agentsfleet-runner-stream` vault item.
Indy accepted the CA signing key beside runtime material, readable by the CI service account, on Nov 09, 2026.
Neither runtime nor deployment jobs load that signing field.
Both issuer and listener use the approved ten-year lifetime and renew together.

### Nov 09 recovery probe clarification

M243 removes runner lease polling. Its daemon still queries Postgres once per replica every five seconds to recover missed disposable notifications.
The stream itself does not make the admission commit and broker publication atomic.

Owner (Indy, Nov 09, 2026): "Okay i think would do this in another milestone" followed by "continue with the pending work".
This defers replacing the periodic recovery probe; M243 retains Dimension 4.5 and must describe that database polling accurately.
A later design must cover publication crashes, listener gaps, capacity changes and lease expiry before removing the backstop.

Owner confirmation (Indy, Nov 09, 2026): "Okay for the above case i am ok with the recovery poll. That is fine. So keep going with yours".
The accepted failure example is a daemon crash after admission commit and before wake publication.

### Renewal recovery budget correction

Greptile found that reconnect waited for another renewal tick, despite the nine-second physical cleanup margin.
Connection changes now start a renewal batch immediately.
The grant is 35 seconds; the reserved renewal window is 15 seconds.
The complete path budgets five seconds to a tick, five for its failed answer, two for jitter and seven for reconnect.
Another five-second answer and nine-second cleanup fit before expiry.
Compile-time assertions check both relationships; the derived offline threshold is 105 seconds.

Paused-time regressions reproduced both defects separately.
Immediate renewal fixed the missed tick; the full-delay test still failed until the grant and reserved window covered cleanup.
Both regressions then passed. Full repository verification remains due.
