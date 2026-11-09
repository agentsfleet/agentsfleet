---
type: reference
audience: contributor
verified: 2026-11-09
product_version: 0.75.0
executable: false
---

# M244 inventory: what Dragonfly does today, and where each job goes

## What it is

This page lists every job `afd_dragonfly` does in production code, with its callers and its M244 destination.
It was read on `feat/m244-core-nats` after merging M243 checkpoint `55a6e51fd`.
Paths are under `rustd/crates/daemon/` unless they start with another root.

## Why it exists

The [M244 spec](../../v2/done/M244_001_P1_DOCS_INFRA_OBS_CORE_NATS_REPLACES_DRAGONFLY.md) deletes Dragonfly.
Each job must first move to core NATS, if it is a disposable signal, or to Postgres, if it is a durable or compare-and-swap fact.
This page is the checklist §7's deletion is graded against.

## Facts that change the plan

- **No production crate uses `afd_bus` yet.** It is a workspace member (`rustd/Cargo.toml:40`) with no dependents. Every caller below talks to `afd_dragonfly` directly. M243 owns that migration.
- **The tail bench calls Dragonfly directly** (`afd_bench/src/lane/tail/mod.rs:127,132`). The approved checkpoint baseline therefore needs the tail lane on `afd_bus::Handle` first.
- **The anomaly key is built once, outside the rule loop** (`afd_gate/src/gate/anomaly.rs:60`). Every rule increments one counter, and only the first window sets the expiry (`afd_dragonfly/src/kv.rs:62-66`). §5's Dimension 5.9 fixes it.
- **The gate decision mirror has no production writer.** `fleet:gate:response:*` is read at `afd_gate/src/gate/store/mod.rs:193`. Only tests write it (`afd_gate/tests/integration_gate_faults.rs:164`, `store/tests.rs:30`).
- **The outbound stream is never trimmed in production.** `OutboundQueue::trim` (`afd_dragonfly/src/outbound/mod.rs:334`) has only a test caller (`afd_dragonfly/tests/retention_faults.rs:68`).
- **Runner commands have no producer or consumer yet.** `Subject::RunnerCommand` exists only in `afd_bus` (`subject.rs:13`, `dragonfly/mod.rs:95`).

## Disposable signals: core NATS (§2, §6)

| Job | Dragonfly today | Production callers | M244 |
|---|---|---|---|
| Fleet live tail | `SPUBLISH fleet:{id}:activity` (`streams/mod.rs:14-15`, `streams/tail.rs:16-98`) | Publishers: `afd_fleet/src/lease/{activity/mod.rs:149,bracket.rs:106}`, `afd_gate/src/gate/park.rs:167`, `afd_approval/src/inbox/{announce.rs:59,resolve/mod.rs:233}`, `afd_events/src/{activity.rs:112,steer/mod.rs:259}`. Subscribers: `afd_sse/src/{live.rs:85,fanin.rs:164}` | `Subject::FleetTail` on NATS; §6 sends `catching_up` after subscribe |
| Draft progress | `draft:{id}:events`, spelled in `afd_authoring/src/model/progress/mod.rs:26` | Publisher `afd_authoring/src/model/progress/publish.rs:94`. Subscriber `afd_api_tenant/src/handler/draft/stream/mod.rs:51` | `Subject::DraftEvents`; §6 adds `draft.resync` |
| Wake doorbell | `fleet:wake:{N}`, 16 shards, 64 hints (`doorbell/mod.rs:25-218`) | `afd_admission/src/wake.rs:7`, `afd_approval/src/inbox/resolve/mod.rs:175`; listen `main/src/plane/mod.rs:271`; take `afd_fleet/src/lease/assign/ledger.rs:50` | `afd_bus::wake` over `Subject::Wake`, moved by M243 |
| Runner commands | none | none | `Subject::RunnerCommand`, wired by M243 |
| Outbound ping | none | none | new `af.outbound.wake` subject (§4); needs a `Subject` variant |
| Subscription hub | one pub/sub connection, capacity 256, 5 s redial ceiling (`hub/mod.rs:75,88,207`) | start `main/src/serve/optional.rs:32`; shutdown `serve/runtime.rs:204` | `afd_bus::nats` hub, one connection per process |

## Durable and compare-and-swap facts: Postgres (§4, §5)

| Job | Dragonfly today | Production callers | M244 |
|---|---|---|---|
| Outbound delivery | stream `connector:outbound`, group `connector_workers` (`outbound/mod.rs:66-334`, `outbound/reader.rs:78,102`) | enqueue `afd_fleet/src/lease/obligation.rs:163`, re-append `afd_outbound/src/producer.rs:184`; worker `afd_outbound/src/worker/mod.rs:160,280,301`; ack `lanes/mod.rs:249`; spawn `main/src/outbound.rs:58-91` | claim columns on `fleet.obligations`, lane-head claim, 30 s pass |
| Device-login sessions | `auth:session:{id}`, 300 s life, 60 s replay, five attempts, three Lua scripts (`session/mod.rs:73-116`, `session/operations.rs:29-176`) | `afd_tenant/src/session/mod.rs:122-271`; built at `main/src/plane/surfaces.rs:145` | `identity.device_sessions`, one locked transaction per verify |
| OAuth nonces | `SET connect:{provider}:nonce:{hex} EX 600`, spent by `DEL` (`kv.rs:96,123`) | `afd_connector/src/state/nonce.rs:49-70`, from `connect.rs:187,271` | `connector.oauth_nonces`, spent by `DELETE … RETURNING` |
| Gate event reference | `fleet:gate:byevent:{fleet}:{event}`, falls back to Postgres (`afd_gate/src/gate/store/mod.rs:136-175`) | write `gate/park.rs:128`; read `gate/pass.rs:136` | read `fleet.approval_gates` only; the write goes |
| Gate decision mirror | `fleet:gate:response:{action}`, read only (`store/mod.rs:189-221`) | read `gate/pass.rs:162` | deleted; Postgres already answers every read |
| Anomaly brake | Lua `INCR` then `EXPIRE` (`kv.rs:62-76`) | `afd_gate/src/gate/anomaly.rs:60,88`, from `gate/first.rs:56` | per-replica bounded memory, one window per rule |

## Boot, readiness and configuration (§2, §6, §7)

| Job | Where |
|---|---|
| Configuration | `afd_dragonfly/src/config.rs:16-115`: `DRAGONFLY_URL`, `DRAGONFLY_TLS_CA_CERT_FILE`, `DRAGONFLY_CONNECT_TIMEOUT_MS`, `DRAGONFLY_REQUEST_TIMEOUT_MS`; namespace `AGENTSFLEET_DEV_NAMESPACE` (`namespace/mod.rs:6`) |
| Connect and preflight | `main/src/serve/runtime.rs:113-129`; cluster, `SSUBSCRIBE`, `noeviction` and cache-mode checks in `afd_dragonfly/src/preflight/mod.rs:72-174` |
| Readiness | `main/src/probes.rs:44-80`; `/readyz` gating in `afd_api/src/router/probes.rs:57,66` |
| Startup error | `UZ-STARTUP-004` at `af_core/src/error_code/request.rs:209`, hint at `af_core/src/problem/request.rs:327` |

## Not on Dragonfly

Ingress deduplication (`afd_ingress/src/deliver.rs:3`) and billing (`afd_billing/src/store.rs:26`) are already Postgres.
No lock, leader election or rate limiter uses Dragonfly besides the anomaly counter and the five-attempt verify cap.

## Tooling and manifests §7 removes

- Make: `make/test-infra.mk`, `make/test-integration-rustd.mk`, `make/bench.mk`.
- Compose: `docker-compose.yml`, `compose.dev-infra.yml`.
- Scripts: `scripts/dragonfly-cluster.sh` and `dragonfly_cluster_ready.sh` go; `local-dev*.sh`, `test-infra-ports.sh` and `ensure_fly_app.sh` are edited.
- `afd_dragonfly` is a normal dependency of 18 crates and a test dependency of `afd_http` and `afd_api`; `redis` appears in `afd_dragonfly`, `afd_bench` and `afd_outbound` (dev).
- Test suites read `TEST_DRAGONFLY_URL`, `TEST_DRAGONFLY_CA_CERT`, `TEST_DRAGONFLY_TLS_URL`, `TEST_DRAGONFLY_BAD_CA` and `TEST_DRAGONFLY_CONTROL`; 117 test files outside the crate import it.

## Related pages

- [Datastore scaling](../../architecture/datastore_scaling.md)
- [Push scheduler](../../architecture/push_scheduler.md)
- [M243 implementation sign-off](../m243/implementation-signoff.md)
