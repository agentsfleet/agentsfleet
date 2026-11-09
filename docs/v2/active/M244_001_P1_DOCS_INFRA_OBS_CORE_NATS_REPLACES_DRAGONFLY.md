<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M244_001: Core NATS carries every signal, Postgres holds every fact, and Dragonfly is gone

**Prototype:** v2.0.0

**Milestone:** M244

**Workstream:** 001

**Date:** Nov 06, 2026

**Status:** IN_PROGRESS

**Implementation branch:** `feat/m244-core-nats`, worktree `~/Projects/agentsfleet-m244`, cut from `origin/main` `61820b66a` on Nov 08, 2026. It adopts M243 checkpoints from `feat/m243-runner-push-stream` by normal merge, recorded in issue #818; M244 merges after M243.

**Priority:** P1, a lost datastore machine today freezes every viewer's tail with no signal and costs every Dragonfly node at once

**Categories:** DOCS, INFRA, OBS

**Batch:** B1, the fourth push-scheduler milestone; M245 starts after it merges

**Depends on:** M243_001, whose Bus seam every publisher and subscriber already uses, with a Dragonfly implementation this spec measures and then replaces; M241_001, whose bench lanes, baselines and `push_scheduler.md` this spec extends

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 06, 2026) from the EGM brief and source research on main 14539a73e

**Canonical architecture:** `docs/architecture/push_scheduler.md` §Milestones (the M244 row), §Where each fact lives and §When a signal is lost, created by M241_001; `docs/architecture/datastore_scaling.md` holds the datastore decision record this spec rewrites

---

## Overview

**Goal (testable):** NATS tail and wake p99 stay within the greater of Dragonfly's plus 10% and Dragonfly's plus 1 ms on the same laptop, killing one of three NATS nodes sends every viewer `catching_up` and recovers within 10 s, and no Dragonfly code, service or rule remains.

**Problem:**
- Both Fly configurations run all four Dragonfly processes on one machine (`docs/architecture/datastore_scaling.md:55`), and self-hosted multi-shard Dragonfly manages no failover (`:36`).
- A viewer cannot tell a dead datastore from a quiet fleet. With no hub the tail is `stream::pending()` (`afd_sse/src/live.rs:104`), and the 15 s heartbeat keeps it looking healthy (`afd_sse/src/lib.rs:53`).
- A failed tail publish is only logged as `tail_frame_dropped` (`afd_dragonfly/src/streams/tail.rs:20`), and a draft stream whose subscription ends drops it silently (`afd_api_tenant/src/handler/draft/stream/body.rs:126`).
- Dragonfly also holds facts that need compare-and-swap or durability: the outbound stream (`afd_dragonfly/src/outbound/mod.rs:64,68`), device-login sessions behind three Lua scripts (`afd_dragonfly/src/session/`), OAuth nonces (`afd_connector/src/state/nonce.rs:49-71`), gate references (`afd_gate/src/gate/store/mod.rs:137,171`) and the anomaly counter (`afd_dragonfly/src/kv.rs:62-76`).
- The anomaly brake computes one key outside its rule loop (`afd_gate/src/gate/anomaly.rs:60`), so every rule increments the same counter and only the first increment sets its window.

**Solution summary:** Dragonfly's tail and wake p99 and a kill drill are measured first, and the pass bar is written. A core NATS implementation goes behind M243's Bus, and three NATS nodes join compose. NATS is measured against the bar on the same laptop before anything is deleted. Then every durable or compare-and-swap job moves into Postgres. No tail freezes silently. `afd_dragonfly`, its service, its scripts and its rules are deleted. A rolling cutover delivers each owed reply once.

**Design flow:**

```
publish(subject) ─► afd_bus::Nats ─► 3-node core NATS (no JetStream, no KV) ─► subscribers' hub
   publish fails ─► gap debt ─► reconnect ─► Af-Gap marker on that subject
subscriber hub: reconnect ─► Gap on every subscription · slow consumer ─► Gap on every subscription
SSE: subscribe ─► hello ─► catching_up(0) once ─► UI backfills from Postgres; each Gap ─► catching_up
report commits obligation ─► af.outbound.wake (lossy) ─► worker claims its lane's head row in Postgres
                                                      └─ 30 s claim pass if the ping is lost
device login ─► identity.device_sessions (row lock, constant-time compare, one UPDATE)
OAuth connect ─► connector.oauth_nonces (DELETE … RETURNING spends once)
gate reference ─► fleet.approval_gates · anomaly brake ─► per-replica memory, one window per rule
```

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(bus): core NATS replaces Dragonfly, and Postgres holds every durable job
- **Intent (one sentence):** survive the loss of one broker machine with every viewer told and no accepted work or owed reply lost or doubled.
- **Authoring handshake:** Indy agreed to core NATS on condition it is measurably no worse, and asked that M241's tail, wake and kill measurements run here, on this milestone's code, Dragonfly first. M243 builds the Bus seam with a Dragonfly implementation; this milestone adds NATS behind it, moves the durable jobs to Postgres and deletes Dragonfly. ASSUMPTIONS I'M MAKING:
  1. M243 merged with every publish and subscribe going through `afd_bus::Bus`, its `Received` carrying `Message`, `Lagged` and `Gap`, and the doorbell and runner commands riding it. CHORE(open) checks this on `origin/main` and amends Interfaces if the shape differs. **Amended at CHORE(open), Nov 08, 2026:** M243 has not merged. Indy authorised parallel work against its draft (#819), so this branch adopts M243 checkpoints by merge and tracks interface changes in #818. At checkpoint `55a6e51fd` the Bus has `publish`, `publish_batch`, `subscribe` and an asynchronous, idempotent `shutdown`; production publishers and subscribers are not yet migrated.
  2. The rig is Indy's laptop, three interleaved runs per lane, as in M241.
  3. Pre-production, so in-flight device logins and OAuth connects may restart across the cutover, as Indy accepted at CHORE(open) (Open blockers, item 3).
  4. The daemon reaches NATS over Fly's private network in plaintext with credentials in the URL, as it reaches Dragonfly today.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_dragonfly/src/hub/mod.rs` and `hub/gap/mod.rs` — the fan-out and gap semantics the NATS hub keeps; `tests/hub_gap_faults.rs` holds the gap tests §2 ports.
2. `rustd/crates/daemon/afd_outbound/src/producer.rs` and `obligation/sql/` — the two recovery scans a still-running old replica keeps executing during the rollout; §4's ownership rule is written against them.
3. `rustd/crates/daemon/afd_dragonfly/src/session/verify_consume.lua` and `docs/AUTH_DEVICE_LOGIN.md` §Threats — the transitions, the five-attempt cap and the 60 s replay window §5 must keep.
4. `rustd/crates/daemon/afd_bench/src/lane/tail/` and `make/bench.mk` — the viewer and stream ladders §1 extends.
5. `docs/architecture/push_scheduler.md` — the subjects, backstops and fact homes this spec builds.

## Files Changed (blast radius)

Discovery search: `git grep -l -i dragonfly -- ':!docs/v2'` found 541 files outside `rustd/crates/daemon/afd_dragonfly/` on 14539a73e. Rust: 155 production and 283 test files. Elsewhere: 103 files. M243 edits some of them first, and PR #803 may merge before this starts, so CHORE(open) reruns the search on `origin/main` and folds every hit into the rows below.

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M244_001_P1_DOCS_INFRA_OBS_CORE_NATS_REPLACES_DRAGONFLY.md` | CREATE, then MOVE to `active/` and `done/` | this spec |
| `docs/reviews/m244/dragonfly-inventory.md` | CREATE | CHORE(open): every Dragonfly responsibility, its callers and the Section that moves it |
| `docs/architecture/push_scheduler.md` | EDIT | §1 the Dragonfly baseline, kill drill and pass bar; §3 the A/B verdict; §7 the retired rule marked done |
| `docs/architecture/datastore_scaling.md` | EDIT | §7: the decision record names core NATS and Postgres with Indy's words; the Dragonfly sections become dated history |
| `docs/architecture/postgres_schemas.md` | EDIT | §4, §5: the two new tables, the claim columns and their deletion-matrix rows |
| `docs/architecture/{README,concurrency,data_flow,high_level,lease_flow,observability,runner_fleet,scaling,shared_chat_verification,testing,user_flow}.md`, `scenarios/github-pr-reviewer.md`, `scenarios/two-fleets-github-slack.md` | EDIT | §7: every live Dragonfly statement now names NATS or Postgres |
| `docs/AUTH.md`, `docs/AUTH_DEVICE_LOGIN.md` | EDIT | §5: session storage, the compare's new home, the cluster rule |
| `docs/development.md`, `README.md`, `docs/RUST_ERROR_STANDARD.md` (crate table row only) | EDIT | §7 |
| `.orly/LOCAL.md` | EDIT | §7: the datastore rule, the live-datastore lane, the auth line and the probe line name NATS |
| `docs/metrics.census.tsv` | EDIT | §2, §4: two new families; the reconnect row's description |
| `schema/2800_identity_device_sessions.sql`, `schema/2810_connector_oauth_nonces.sql`, `schema/2820_fleet_obligations_claims.sql` | CREATE | §4, §5; placeholders from 2800-2840, renumbered from `origin/main` at CHORE(open) because PR #803 takes `schema/2190-2690` |
| `rustd/crates/daemon/afd_db/src/migration.rs` and its position tests | EDIT | §4, §5: three slots (MIG) |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | §2 add `async-nats` without default features; §7 drop `afd_dragonfly` and `redis` |
| `rustd/crates/daemon/afd_bus/src/nats/` (`mod.rs`, `hub.rs`, `debt.rs`, `preflight.rs`, `config.rs`, `tests.rs`), `afd_bus/Cargo.toml`, `afd_bus/tests/` | CREATE, EDIT | §2: the NATS implementation, its gap sources, its boot checks and `NATS_URL` |
| `rustd/crates/daemon/afd_bus/src/` (M243's Dragonfly implementation and its tests) | DELETE | §7 |
| `rustd/crates/daemon/afd_dragonfly/**` | DELETE | §7: the crate, its Lua scripts and its 40 test files |
| `rustd/crates/daemon/afd_outbound/src/{worker,lanes,obligation,claim}/`, `producer.rs`, `retry.rs`, `Cargo.toml`, `tests/` | EDIT, CREATE, DELETE | §4: claims, ping, claim pass; the stream reader, re-append and receipt scans go |
| `rustd/crates/daemon/afd_fleet/src/lease/obligation.rs` | EDIT | §4: the report owes the row with its holder token and pings |
| `rustd/crates/daemon/afd_tenant/src/session/` and its `sql/`, `tests/` | EDIT, CREATE | §5: sessions as rows |
| `rustd/crates/daemon/afd_connector/src/state/nonce.rs`, `registry/mod.rs`, `connect.rs`, `tests/` | EDIT | §5: nonce digests as rows |
| `rustd/crates/daemon/afd_gate/src/gate/{store,anomaly,park}*`, `tests/` | EDIT | §5: references from `approval_gates`; the mirror read goes; the brake in memory |
| `rustd/crates/daemon/afd_sse/src/{live,tail,fanin}*`, `rustd/crates/daemon/afd_api_tenant/src/handler/{stream,draft/stream}/` | EDIT | §6: `catching_up` after subscribe and on failure; `draft.resync` on a lost subscription |
| `rustd/crates/daemon/main/src/{serve,plane,preflight,sweepers}/`, `probes.rs`, `outbound.rs`, `error.rs`, `inventory.rs`, `migrate.rs`, `tests/` | EDIT | §2 boot; §4 claim pass; §5 expiry sweep; §6 readiness; §7 environment variable removal |
| `rustd/crates/{afd_admission,afd_approval,afd_authoring,afd_events,afd_fleet,afd_memory,afd_vault,afd_runner,afd_api,afd_http,afd_cron,afd_ingress,afd_identity,afd_billing,af_crypto,af_wire,afd_api_wire,afd_api_runner,afd_db}` sources, manifests and tests that name Dragonfly | EDIT | §7: error seams, test harnesses and comments follow the deletion |
| `rustd/crates/foundation/af_core/src/error_code/request.rs`, `problem/request.rs`, `error_code/mod.rs` | EDIT | §7: `STARTUP_DRAGONFLY_CONNECT` becomes `STARTUP_BUS_CONNECT`; code `UZ-STARTUP-004` and its title stay; the hint names `NATS_URL` |
| `rustd/crates/protocol/af_observability/src/metrics/declared/`, `producers/`, `tests/producers_drive.rs` | EDIT | §2, §4: two families and their producers |
| `rustd/crates/daemon/afd_bench/src/lane/tail/{viewers,wake}/`, `lane/tail/mod.rs`, `lane/outbound/`, `lane/cardinality/capacity.rs`, `lane/sweep/mod.rs`, `src/bin/tail.rs`, `Cargo.toml`, `tests/` | EDIT, CREATE | §1 p99 and the wake ladder; §2 `BENCH_BUS`; §4 the outbound lane drives claims; §7 the Dragonfly sample and raw `redis` go |
| `bench/baselines/tail.rig.json`, `bench/baselines/outbound.rig.json` | EDIT | §3, §4: NATS-era median runs |
| `docker-compose.yml`, `compose.dev-infra.yml` | EDIT | §2 three NATS nodes; mailpit owns the daemon's network namespace; §7 the Dragonfly service goes |
| `make/test-infra.mk`, `make/test-integration-rustd.mk`, `make/bench.mk` | EDIT | §2 `TEST_NATS_URL`, `TEST_NATS_CONTROL`, the exclusive filter; §7 every `TEST_DRAGONFLY_*` goes |
| `scripts/test-infra-ports.sh`, `scripts/local-dev.sh`, `local-dev-state.sh`, `local-dev-test.sh`, `local-dev-live-test.sh`, `ensure_fly_app.sh` | EDIT | §2, §7: NATS ports and seeds |
| `scripts/dragonfly-cluster.sh`, `scripts/dragonfly_cluster_ready.sh` | DELETE | §7 |
| `cli/src/lib/id.ts`, `cli/src/program/tree/flags.ts` | EDIT | §7: comments only |
| `deploy/fly/nats-dev/`, `deploy/fly/nats-prod/` | CREATE | §8, Indy's approval at CHORE(open) |
| `deploy/fly/dragonfly-dev/`, `deploy/fly/dragonfly-prod/`, `deploy/fly/agentsfleetd-prod/fly.toml` (comments) | DELETE, EDIT | §8, Indy's approval |
| `.github/workflows/deploy-dev-fly.yml`, `release.yml`, `test-integration-rustd.yml`, `test-unit-rustd.yml` | EDIT | §8, Indy's approval: `NATS_URL` from `op://`, NATS apps before the daemon, compose service names |
| `playbooks/founding/02_preflight/{001_playbook.md,02_credentials.sh,credentials_test.sh}`, `founding/01_bootstrap/001_playbook.md`, `founding/03_priming_infra/001_playbook.md`, `operations/credential_rotation/*`, `operations/ip_allowlisting/*`, `playbooks/ARCHITECTURE.md`, `playbooks/README.md`, `operations/explicit_dispatch_test.sh`, `operations/teardown/gate_env_test.sh` | EDIT | §8: `nats-{env}/api-url` items and checks |
| `playbooks/operations/teardown/dragonfly/**` | DELETE | §8: nothing is left to flush once the apps are destroyed |
| `public/openapi.json` | EDIT (regenerated) | §6: the `/readyz` description and the `queue` field description, because the bus no longer gates readiness; no frame shape changes |

Hits kept as records, apart from the table: `docs/v2/**`, `docs/reviews/**`, `playbooks/operations/acceptance/baselines/**`, the other committed `bench/baselines/*.rig.json` provenance, and `docs/RUST_ERROR_STANDARD.md:64-65` (DOC-S7). Same spelling, different meaning: `ui/packages/app/tests/fleet-tool-calls.runner.test.tsx`, a sample search query. The docs repository has no public surface to change: its only hits are the `UZ-STARTUP-004` row, whose title stays, and a historical changelog entry.

## Applicable Rules

- **`code.md`** — NDC, NLR, NLG, ORP, GRD, TIM, CAS, OWN, FLL, CODE-MSID, EMS, ECL
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE, RS-ENV, RS-RACE, RS-FEATURE, RS-TESTCONST
- **`domains/sql.md`** — SCH, MIG, SQL-SLOT, SGR, STS, SQL-READER, SQL-ID, SQL-TSTYPE, SQL-STAMP, SQL-IMMUTABLE, SQL-ERASE, SQL-FILE, SQL-TIMEOUT, SQL-DEADLINE, SQL-LOCKCLOCK, SQL-SWEEPCLOCK, SQL-PLAN, SQL-ROLE, VLT, TXN
- **`domains/auth.md`** — AUTH-DOC, CTM, CTC, GATDL, AUTH-NOLOG
- **`domains/logging.md`** — LOG-EVENT, LOG-CODE, LOG-PAIR, LOG-RENAME, OBS
- **`tests.md`** — TEST-REAL, TEST-INJECT, TEST-CONTEND, TEST-KILL, TEST-RESIDUAL, TEST-PARTIAL, TEST-NEG, TFX, TEST-COUNTER, TEST-SWITCH, TEST-LANE
- **`domains/architecture.md`** — ARCH-LAND, ARCH-HOMES
- **`docs.md`** — DOC-02, DOC-03, DOC-S7
- **`spec.md`** — SPEC-MECH, SPEC-PIN, SPEC-BLAST, SPEC-DECIDE

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` (clippy `-D warnings`, `cargo shear`) | yes, most crates change and two dependencies go | `lint-rustd` per Section; `redis` leaves each manifest in the commit that stops using it |
| Metrics census and producer tests | yes, two families arrive | census row, declaration and producer in one commit |
| Migration position tests | yes, three slots | slots and `migration.rs` in one commit |
| `docs.language` at `orly gate verify` and `orly gate pr` | yes, architecture pages and `.orly/LOCAL.md` | sentences under 25 words, paragraphs of three sentences or fewer |
| `gitleaks` | yes, compose gains a local NATS password | the same fixed local password the Dragonfly service used; no deployed secret enters the repository |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, the NATS hub and the claim module | the hub split into `hub.rs`, `debt.rs` and `preflight.rs`; the claim statement in `claim/` |

## Prior-Art / Reference Implementations

- **Reference:** `afd_dragonfly/src/hub/` — one connection per process, local fan-out, `Received::Gap` after a lost subscription. The NATS hub keeps the shape; the client's own reconnect replaces the redial and repair code.
- **Reference:** `fleet.admissions` claiming (`afd_fleet/src/lease/assign/`) — `FOR UPDATE SKIP LOCKED` over a partial index, the pattern §4's lane-head claim follows.
- **Reference:** `afd_dragonfly/src/session/verify_consume.lua` — the transitions §5 ports verbatim, as one locked transaction.
- **Reference:** https://docs.rs/async-nats/0.52.0/async_nats/ — `ServerInfo` exposes `cluster`, `jetstream`, `max_payload` and `headers`; `Event` carries `Connected`, `Disconnected` and `SlowConsumer(u64)`; a `Subscriber`'s sid is private.

## Sections (implementation slices)

### §1 — Measure Dragonfly first, and write the pass bar

This Section carries M241's old §6 and §7 as its own, on this milestone's starting code. The viewer ladder records p99 beside p95 per rung, in the `receive_p99_ms` and `stream_receive_p99_ms` series beside the p95 ones. A wake ladder publishes rings through the Bus for distinct fleets while 1, 2 and 4 hubs each run a doorbell listener, as M243 leaves it. It keeps at most 64 rings outstanding, the doorbell's hint cap, and counts a ring missed when no hub takes it within 1 s. Results are `wake_p50_ms_hubs_<n>`, `wake_p99_ms_hubs_<n>` and `wake_missed_hubs_<n>`. **Implementation default:** the wake ladder lives in the tail lane, because both time one broker hop and the A/B compares both.

`bench-tail` runs three times on Indy's laptop after a reset rig; the median run is recorded in `push_scheduler.md` §Baselines with the other two beside it. The kill drill runs on the dev stack twice:
1. `make up` starts `compose.dev-infra.yml` and this worktree's native daemon, so stopping a datastore container leaves the daemon's network alone.
2. Two viewers hold one fleet's stream (`curl -N` on `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/stream`), and one steer a second goes to that fleet.
3. `docker compose -f compose.dev-infra.yml stop dragonfly`, then observe for 60 s, then `make up`.
4. Record each viewer's frames (`event_admitted`, `catching_up`, or only heartbeats), whether each steer was accepted in Postgres, and the seconds until frames flow again.

§Baselines then gains `### Broker pass bar`, judged on per-rung medians:
- NATS tail p99 and wake p99 no worse than the greater of Dragonfly's plus 10% and Dragonfly's plus 1 ms, per rung;
- after one of three NATS nodes is killed, every viewer receives `catching_up` and resumes within 10 s with no person acting;
- no lost frame passes without a gap signal.

- **Dimension 1.1** — DONE — the summary carries p99 → Test `tail_summary_reports_p99` (unit): latencies 1..100 ms report p95 95 and p99 99.
- **Dimension 1.2** — DONE — a rung with nothing timed reports no figure → Test `rung_without_frames_reports_no_p99` (unit): an empty rung writes no `p99_ms_` key, never 0.
- **Dimension 1.3** — a late ring is missed, not delivered → Test `wake_past_deadline_counts_missed` (unit): a ring taken after 1 s counts in `wake_missed_hubs_1` and not in p99.
- **Dimension 1.4** — the ringer never passes the hint cap → Test `wake_ringer_waits_at_window` (unit, paused time): with 64 untaken rings, the 65th waits.
- **Dimension 1.5** — a live wake ladder delivers → Test `make bench-tail PROFILE=rig` (integration): the result holds `wake_p99_ms_hubs_4` and `wake_missed_hubs_4` 0.
- **Dimension 1.6** — the drill is recorded, frozen tail included → Test `broker_kill_drill_recorded` (manual): §Baselines holds "Dragonfly kill drill, run 1" and "run 2" with viewer outcomes, accepted steers, recovery seconds and commands. If no viewer receives `catching_up`, that is recorded as the defect §6 closes.
- **Dimension 1.7** — Indy approves the bar before NATS runs → Test `broker_pass_bar_approved` (manual): his reply to `### Broker pass bar` is quoted in the Pull Request Session Notes.

### §2 — Core NATS behind the Bus, and three nodes in compose

`afd_bus::nats` implements M243's Bus over `async-nats` 0.52.0 with default features off. It keeps one connection per process and fans out locally, as the Dragonfly hub does. Gaps come from three sources:
- after `Event::Connected` follows a `Disconnected`, every subscription receives `Gap` once;
- `Event::SlowConsumer` names a private sid, so every subscription on that client receives `Gap`;
- a publish that fails records a debt for its subject, and so does every subject published in the 2 s before a disconnect, the frames that may have died with the node. On reconnect each indebted subject gets one empty message with header `Af-Gap: 1`, which subscribers read as `Gap`.

The debt set is bounded at 4,096 subjects; overflow publishes one marker on `af.bus.gap`, which every hub reads, so every subscription gaps. **Implementation default:** one broadcast subject, because a publisher cannot name another process's subscriptions and NATS refuses a wildcard publish. Every adapter limit lives in `afd_bus::nats::Tuning`, a `serde` value with `garde` bounds whose defaults are the numbers in this Section. Subjects carry the dev namespace as a `dev.<ns>.` prefix. Boot reads `NATS_URL`, comma-separated seeds with credentials in the userinfo. It refuses, as `UZ-STARTUP-004`, a seed it cannot reach within the connect timeout, and a server reporting `jetstream`, no `cluster`, no `headers`, or a `max_payload` under `LARGEST_FRAME_BYTES`. `LARGEST_FRAME_BYTES` is asserted at compile time to cover the tail's 64 KiB batch and the draft's 32 KiB frame. Compose gains `nats-1`, `nats-2` and `nats-3`, one process each, routed as one cluster with `--no_advertise`, so host clients use their seed list. Mailpit owns the network namespace the compose daemon joins, so a node kill never takes the daemon's network. The daemon connects to both stores until §7, and the bench picks a Bus with `BENCH_BUS=dragonfly|nats`. **Implementation default:** gap every subscription on a slow-consumer event, because a false gap costs one backfill and a missed one freezes a screen.

- **Dimension 2.1** — DONE — a reconnect gaps every subscription → Test `reconnect_gaps_every_subscription` (integration): a TCP proxy before the subscriber's seed is cut and restored; all three subscriptions receive one `Gap`, then the next frame, and `agentsfleet_bus_gaps_total{cause="reconnect"}` rises by 3.
- **Dimension 2.2** — DONE — a slow consumer gaps → Test `slow_consumer_gaps_every_subscription_on_the_client` (integration): subscription capacity 8, the pump held, 100 frames published; each subscription receives `Gap`.
- **Dimension 2.3** — DONE — a failed publish reaches subscribers as a gap → Test `failed_publish_sends_a_gap_marker_on_reconnect` (integration): the publisher's proxy is cut and its buffer filled; after restore the subscriber receives `Gap` before the next frame.
- **Dimension 2.4** — DONE — an unsuitable server is refused → Test `boot_refuses_an_unsuitable_nats_server` (unit): `ServerInfo` with `jetstream: true`, `cluster: None`, `headers: false` or `max_payload` 32,768 each refuses with `UZ-STARTUP-004` naming the field.
- **Dimension 2.5** — DONE — a payload that looks like a marker is still a message → Test `marker_needs_the_header_not_the_payload` (unit): a message whose body is empty but has no `Af-Gap` header is `Received::Message`.

### §3 — Measure NATS on the same laptop and judge it

Three interleaved pairs of `bench-tail`, Dragonfly then NATS, run on Indy's laptop after a reset rig. The verdict compares per-rung medians of that session against the bar, with spread recorded. The NATS drill repeats §1's steps with `stop nats-2`. A failing rung stops the milestone here, before anything is deleted, and Indy decides; the bar is his. The integration lane gains `integration_bus_exclusive`, the serial group's new member, which stops a real node through `TEST_NATS_CONTROL`.

- **Dimension 3.1** — DONE — a killed node gaps its viewers, who resume → Test `node_kill_gaps_viewers_and_resumes` (integration, exclusive): subscribers on all three nodes; `nats-2` stops; its subscriber receives `Gap` within 10 s and the next frame.
- **Dimension 3.2** — DONE — no frame is lost without a gap → Test `every_lost_frame_follows_a_gap` (integration, exclusive): 1,000 numbered frames across a node stop; every hole in each subscriber's sequence is preceded by `Gap`.
- **Dimension 3.3** — NATS meets the bar → Test `broker_ab_meets_the_bar` (manual): §Baselines holds the three pairs' medians per rung and `Verdict: NATS passes` or the failing rung.
- **Dimension 3.4** — the NATS drill is recorded → Test `nats_kill_drill_recorded` (manual): two runs, each viewer's `catching_up` and resume seconds; any viewer over 10 s fails the bar.

### §4 — Outbound claiming moves to Postgres

`fleet.obligations` gains `claim_expires_at TIMESTAMPTZ(3) NOT NULL` and `next_attempt_at`, a `TIMESTAMPTZ(3)` NULL marker, and an open-lane partial index on `(provider, workspace_id, sequence)`. The two scan indexes go. `receipt` becomes the holder's token: the report writes a fresh token and `claim_expires_at = clock_timestamp()`, so a new row is free to claim. One statement claims lane heads: a row is open, due, unheld, and no earlier open row shares its `(provider, workspace_id)`; `FOR UPDATE SKIP LOCKED` picks them in `sequence` order. A claim writes a new token and `claim_expires_at = clock_timestamp() + CLAIM_TTL` (30 s). The holder renews every 10 s while delivering. Stamps, attempt counts and releases match the holder's token, so a lapsed holder's late write changes nothing. After a committed report the reporter publishes `af.outbound.wake`; each replica also runs the claim every `CLAIM_PASS` (30 s). Every claim statement runs under the shared timeout helper (SQL-TIMEOUT) and compares deadlines with `statement_timestamp()` (SQL-SWEEPCLOCK). The stream, its reader, the producer's re-append and both receipt scans are deleted.

The cutover is stop-the-world on dev: the deploy stops the old daemons, resets dev data and starts the new ones, so no stream-path row survives and no old replica runs beside a new one (Indy, M242_001 Discovery, Nov 06, 2026: 10:22 PM). With no stream-path rows, `claim_expires_at` is `NOT NULL`: the report sets it as it writes the row, and the claim has no `IS NULL` branch. Order is per lane, among committed rows; two reports racing to commit in one lane deliver in commit order, as stream appends did.

- **Dimension 4.1** — DONE — one lane stays in order with two workers → Test `two_workers_keep_one_lane_in_order` (integration): 10 rows in lane A and 10 in lane B, two claimers; A posts 1..10 in order, and no lane ever has two rows held.
- **Dimension 4.2** — DONE — a failing head blocks only its lane → Test `failing_head_blocks_only_its_lane` (integration): A's head fails retryably; A's second row is never claimed before it, and B drains.
- **Dimension 4.3** — DONE — a dead holder's row is retaken once → Test `lapsed_claim_is_retaken_and_the_late_stamp_refused` (integration): `claim_expires_at` backdated; a second worker delivers; the first holder's stamp updates 0 rows; one post in total.
- **Dimension 4.4** — DONE — a lost ping costs only latency → Test `lost_ping_is_caught_by_the_claim_pass` (integration): no ping is published; the next pass claims and delivers the row, and `agentsfleet_outbound_claims_total{via="scan"}` rises by at least 1 (the metric capture is process-wide and sibling tests scan too).
- **Dimension 4.5** — CUT — no stream-path rows survive the stop-the-world cutover (Indy, M242_001 Discovery, Nov 06, 2026: 10:22 PM).
- **Dimension 4.6** — CUT — no stream-path rows survive the stop-the-world cutover (Indy, M242_001 Discovery, Nov 06, 2026: 10:22 PM).
- **Dimension 4.7** — DONE — the claim uses its index → Test `claim_reads_the_open_lane_index` (integration): `EXPLAIN (ANALYZE, BUFFERS)` over 10,000 delivered and 100 open rows after `ANALYZE` names `idx_obligations_open_lane`.

### §5 — Device sessions, nonces, gate references and the anomaly brake leave the datastore

Read `docs/AUTH.md` first (AUTH-DOC). `identity.device_sessions` has one row per session: id is the session's uuidv7, and the columns mirror `SessionState`, hex fields as `BYTEA`. `status` is enforced by `SessionStatus`, with no literal `CHECK`. The row holds no plaintext secret: the ciphertext is sealed to a CLI key the daemon never holds, and the code is a peppered HMAC. The pepper stays in process memory (`AUTH_DEVICE_LOGIN.md` threats 1 and 4). Grants mirror `vault.secrets`: the runtime role writes; the read-only roles are revoked. Its deletion-matrix row: swept after expiry, with no cascade from a user. Verify is one transaction: lock the row, read `clock_timestamp()` once, classify as the script did, and compare the digest in constant time in Rust (CTM, CTC). Then write the new state in one `UPDATE` and commit before the response. Approve, abort and abort-all are each one conditional `UPDATE … RETURNING`, a zero-row answer classified by a read-only select. Each transition re-stamps `expires_at` as the scripts re-stamp the key's life.

`connector.oauth_nonces` stores the nonce's SHA-256 under its provider with `expires_at = clock_timestamp() + STATE_TTL_SECONDS`; it is immutable (SQL-IMMUTABLE). A spend is `DELETE … WHERE expires_at > clock_timestamp() RETURNING`, so exactly one caller sees a row. A sweeper deletes expired sessions and nonces in batches. Gate references come from `fleet.approval_gates` through `select_event_gate.sql` on `idx_approval_gates_fleet_id_event_id`; the reference write and the response mirror read go. The anomaly brake counts in memory per replica, keyed by fleet, tool, action and rule window. It is bounded at `ANOMALY_KEYS_MAX` with oldest-first eviction, and it still fails open.

- **Dimension 5.1** — DONE — wrong codes lock out at five under contention → Test `concurrent_wrong_codes_lock_out_at_five` (integration): 100 wrong codes from a barrier; 4 `InvalidCode`, 1 `RateLimited`, 95 `Aborted`; the row reads attempts 5 and `aborted`.
- **Dimension 5.2** — DONE — a code redeems once under contention → Test `concurrent_redemptions_consume_once` (integration): 100 correct verifies with one fingerprint; 1 `Success`, 99 `Replay`.
- **Dimension 5.3** — DONE — abort racing consume ends in one terminal state → Test `abort_and_consume_race_end_in_one_state` (integration): 50 aborts and 50 verifies; the row is either `consumed` with one `Success` or `aborted` with none.
- **Dimension 5.4** — DONE — replay answers only the same request → Test `replay_window_answers_only_the_same_request` (integration): same fingerprint inside 60 s is `Replay`; another fingerprint, or after `replay_expires_at` is backdated, is `Consumed`.
- **Dimension 5.5** — DONE — an expired session is refused, then swept → Test `expired_session_is_refused_and_swept` (integration): `expires_at` backdated; verify answers `Expired`; one sweep pass removes the row.
- **Dimension 5.6** — DONE — a nonce spends once → Test `nonce_spends_once_under_contention` (integration): 100 spends from a barrier; one `true`, 99 `false`.
- **Dimension 5.7** — DONE — an expired nonce is refused → Test `expired_nonce_is_refused` (integration): `expires_at` backdated; spend answers `false` and deletes nothing.
- **Dimension 5.8** — DONE — a parked event finds its gate with no cache → Test `parked_event_finds_its_gate_from_the_table` (integration): park, then `recorded` returns the gate's action id from `approval_gates`.
- **Dimension 5.9** — DONE — each rule counts its own window → Test `each_rule_counts_its_own_window` (unit, paused time): rules 3 per 60 s and 10 per 3,600 s; the third call in 60 s trips; after 61 s idle, a call counts 1 for the first rule and 4 for the second. The shared key fails it.
- **Dimension 5.10** — DONE — the brake's memory stays bounded → Test `anomaly_keys_stay_bounded` (unit): `ANOMALY_KEYS_MAX + 1` distinct actions keep `ANOMALY_KEYS_MAX` entries, the oldest evicted.

### §6 — No tail freezes silently

A fleet tail sends `hello`, then one `catching_up` with `dropped: 0` once the subscription's flush returns. The UI's existing backfill (`ui/packages/app/lib/streaming/fleet-stream-dispatch.ts`) then reads `fleet.events`, covering frames published before interest reached the publisher's node. A subscribe that fails sends `catching_up` and retries with backoff, never `stream::pending()`; the backoff is `afd_sse::Resubscribe`. A subscription that errors means the bus itself closed, at drain, so the tail ends and the client reconnects to a live replica. A draft stream whose subscription ends sends `draft.resync` and resubscribes. `/readyz` stops gating on the Bus: NATS carries only signals with Postgres backstops, so a broker outage costs latency, and pulling every replica would turn it into a full outage. Boot still refuses an unreachable bus (§2). **Implementation default:** the backfill rides the documented `catching_up` signal, because the UI already handles it and no frame shape changes.

- **Dimension 6.1** — DONE — a tail opens with one catching_up → Test `tail_sends_catching_up_once_after_subscribing` (unit, over the bus's detached server; the fleet-stream transport fixture asserts the same opening on every live suite): the first three frames are `hello`, `catching_up` with `dropped` 0, then the published `event_admitted`.
- **Dimension 6.2** — an interest race is covered → Test `frame_before_interest_is_covered_by_backfill` (integration): a steer admitted on node 1 just after a node 3 subscription; the viewer receives the frame, or `catching_up` and then the row from the events route.
- **Dimension 6.3** — DONE — a failed subscribe is not silent → Test `tail_subscribe_failure_sends_catching_up` (unit): a closed Bus client; the viewer receives `catching_up` within 1 s, never only heartbeats.
- **Dimension 6.4** — DONE — a lost draft subscription resyncs → Test `draft_stream_resyncs_when_its_subscription_ends` (integration): the subscription is closed; the stream sends `draft.resync` and the next progress event.
- **Dimension 6.5** — DONE for the probe; §7 re-points it at NATS — readiness survives a broker outage → Test `readyz_stays_ready_through_a_bus_outage` (integration): all seeds behind a cut proxy; `/readyz` answers 200 and a steer is accepted. Until §7 makes NATS the production bus, the test probes an unreachable bus seed. `test_admitted_publish_failure_still_accepts` proves that a steer is accepted when its publish fails.

### §7 — Delete Dragonfly, and replace its rules

The Dragonfly Bus implementation, `afd_dragonfly`, the `redis` dependency, the compose service, `TEST_DRAGONFLY_*`, `BENCH_BUS` and the cluster scripts go; every error seam lifts the Bus or `afd_db` error instead. `bench-outbound` and the cardinality lane run on the new paths, and their median runs replace the baselines. `.orly/LOCAL.md`'s datastore bullet states: core NATS, three nodes in one region, no JetStream or KV, `NATS_URL`, Postgres for every durable fact, boot refusals. Its lane, auth and probe lines follow, and it names no retired store. `datastore_scaling.md` records the decision with Indy's words, which its own rule requires before the destination changes. Its Dragonfly history stays as dated sections that name no removed identifier, so R6 stays clean.

- **Dimension 7.1** — nothing names Dragonfly in code or tooling → Test R5's `git grep` (manual): prints nothing.
- **Dimension 7.2** — an unreachable bus names its environment variable → Test `boot_names_nats_url_when_the_bus_is_unreachable` (integration, `agentsfleetd/tests/preflight.rs`): a dead seed exits non-zero with `UZ-STARTUP-004` and `NATS_URL` in the line.
- **Dimension 7.3** — DONE — a missing environment variable is refused before any connect → Test `boot_refuses_an_unset_nats_url` (unit): an empty `NATS_URL` refuses with `UZ-STARTUP-001`.
- **Dimension 7.4** — the pages read clean → Test `orly gate verify` (integration): 0 `docs.language` findings on the edited pages and `.orly/LOCAL.md`.

### §8 — Deploy NATS, and roll the daemon over (blocked on Indy at CHORE(open))

`nats-prod` is a Fly app of three `shared-cpu-2x` 1 GB machines and `nats-dev` one of two `shared-cpu-1x` 512 MB machines, both in `iad`, core-only, with no volume. Each node reads its password from the one `api-url` item. The deploy workflows ensure the NATS app before the daemon and read `NATS_URL` from `op://<vault>/nats-<env>/api-url`. The release command migrates before the first new machine starts. During the rolling overlap:

| Job | What overlapping replicas do | Why nothing doubles |
|---|---|---|
| Outbound replies | old replicas deliver what they queued; new replicas claim from Postgres | §4's ownership rule; Dimensions 4.5 and 4.6 |
| Tail, draft | a viewer on a new replica misses old publishers' frames | `catching_up` after subscribe and the reconnect backfill; nothing is delivered twice |
| Wake, runner commands | rings between versions are lost | the gateway's offer tick and M243's cancel mark |
| Gate references | old replicas also write `approval_gates` | the table was always written first |
| Device logins, OAuth connects | a flow started on an old replica fails as missing on a new one | the person starts again (Open blockers) |
| Anomaly brake | counters restart at zero | fails open, as an unreadable counter does today |

Once no old machine runs, Indy destroys `dragonfly-dev` and `dragonfly-prod`.

- **Dimension 8.1** — the dev rollover delivers each reply once → Test `dev_cutover_delivers_each_reply_once` (manual): during the dev deploy, `outbound_delivery_delivered` events grouped by obligation id never exceed 1; the query and counts land in the Session Notes.
- **Dimension 8.2** — dev survives a machine stop → Test `dev_machine_kill_drill` (manual): `fly machine stop` on one `nats-dev` machine; two viewers receive `catching_up` and resume within 10 s.
- **Dimension 8.3** — the old apps are gone → Test `dragonfly_apps_destroyed` (manual): Indy runs the destroy; `fly apps list` output in the Session Notes names neither.

## Interfaces

- **Subjects:** `af.fleet.<fleet_id>.tail`, `af.draft.<draft_id>.events`, `af.wake.<shard>` (16 shards, CRC-16/XMODEM of the fleet id, as `doorbell/mod.rs` computes), `af.runner.<runner_id>.cmd`, `af.outbound.wake`. A dev namespace prefixes `dev.<ns>.`. A gap marker is an empty message with header `Af-Gap: 1`; `af.bus.gap` carries the overflow marker every hub reads.
- **Environment:** `NATS_URL` replaces `DRAGONFLY_URL`, `DRAGONFLY_REQUEST_TIMEOUT_MS`, `DRAGONFLY_CONNECT_TIMEOUT_MS` and `DRAGONFLY_TLS_CA_CERT_FILE`. Example: `nats://agentsfleet:agentsfleet@127.0.0.1:7001,nats://agentsfleet:agentsfleet@127.0.0.1:7002`. Lanes read `TEST_NATS_URL` and `BENCH_NATS_URL`.
- **Streams:** frame shapes and route descriptions are unchanged. A fleet tail adds one `catching_up` after `hello`, inside its documented meaning.
- **Errors:** `UZ-STARTUP-004` keeps its title, "Event service unavailable"; its constant becomes `STARTUP_BUS_CONNECT` and its hint names `NATS_URL`. No new code.
- **Tables:** `identity.device_sessions`, `connector.oauth_nonces`; `fleet.obligations.claim_expires_at` and `next_attempt_at`; `receipt` now holds the holder's token.
- **Bench keys:** `tail.rig.json` gains `p99_ms_<rung>` and the `wake_*_hubs_<n>` keys; its provenance image comes from the `nats-1` service.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Subscriber's node dies | failure | node loss, network blip | reconnect to a peer; viewers see `catching_up` and backfill | `node_kill_gaps_viewers_and_resumes` |
| Publisher's node dies mid-flight | failure | node loss | recent subjects are marked; subscribers see `Gap` | `failed_publish_sends_a_gap_marker_on_reconnect` |
| Slow consumer | capacity | hub pump stalls | every subscription on the client gaps | `slow_consumer_gaps_every_subscription_on_the_client` |
| Interest not yet propagated | race | subscribe on another node | `catching_up` after subscribe; UI backfill | `frame_before_interest_is_covered_by_backfill` |
| Unsuitable server | failure | JetStream on, no cluster, small payload | boot refuses with `UZ-STARTUP-004` | `boot_refuses_an_unsuitable_nats_server` |
| Broker unreachable at boot | dependency unavailable | dead seeds | boot refuses naming `NATS_URL`; other replicas serve | `boot_names_nats_url_when_the_bus_is_unreachable` |
| Broker outage after boot | dependency unavailable | all nodes down | replicas stay ready; steers accepted; tails send `catching_up` | `readyz_stays_ready_through_a_bus_outage` |
| Outbound ping lost | network blip | at-most-once publish | claim pass delivers within 30 s | `lost_ping_is_caught_by_the_claim_pass` |
| Worker dies holding a claim | timeout | `SIGKILL` mid-delivery | claim lapses; retaken; late stamp refused | `lapsed_claim_is_retaken_and_the_late_stamp_refused` |
| Lane head keeps failing | failure | destination refuses | only that lane waits; abandoned after `MAX_DELIVERY_CYCLES` | `failing_head_blocks_only_its_lane` |
| Online code brute force | security | concurrent wrong codes | locked out at five; the handler's answers are unchanged | `concurrent_wrong_codes_lock_out_at_five` |
| Ciphertext replay | replay | repeated verify | one `Success`; same request replays inside 60 s | `concurrent_redemptions_consume_once` |
| OAuth callback replay | replay | a nonce presented twice | one spend succeeds | `nonce_spends_once_under_contention` |
| Expired nonce | timeout | callback after ten minutes | refused; the person starts again | `expired_nonce_is_refused` |
| Anomaly memory growth | capacity | many distinct actions | oldest key evicted at the cap | `anomaly_keys_stay_bounded` |
| Draft subscription ends | failure | client closed | `draft.resync`, then resubscribe | `draft_stream_resyncs_when_its_subscription_ends` |

## Invariants

1. NATS holds no state. `async-nats` is declared without default features, so `async_nats::jetstream` does not compile, and boot refuses a server reporting `jetstream`; R4 and `boot_refuses_an_unsuitable_nats_server` hold it.
2. A lane has at most one held row. The claim statement's earlier-open-row test and `SKIP LOCKED` decide it, every write matches the holder's token, and the claim commits before the post; `two_workers_keep_one_lane_in_order` is the counterexample test.
3. A device code redeems once and five wrong codes abort the session. The row lock is the authority, and the transition commits before the response; `concurrent_wrong_codes_lock_out_at_five` and `concurrent_redemptions_consume_once` hold it.
4. A nonce spends once: `DELETE … RETURNING` answers one caller; `nonce_spends_once_under_contention`.
5. Every lost frame is preceded by a gap at the subscriber; `every_lost_frame_follows_a_gap`.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `agentsfleet_bus_gaps_total` | operator | a subscription receives `Gap` | `cause`: `reconnect`, `slow_consumer`, `publish_debt` | no subject, fleet or draft id | `reconnect_gaps_every_subscription` |
| `agentsfleet_outbound_claims_total` | operator | a claim takes rows; a rising `scan` share means lost pings | `via`: `ping`, `scan` | no destination or workspace | `lost_ping_is_caught_by_the_claim_pass` |
| `agentsfleet_sse_hub_reconnects_total` (description only) | operator | the NATS client reconnects | none | none | `every_census_family_has_a_producer` |
| `bus_gap_marked`, `outbound_claim_lapsed`, `bus_unavailable_at_readyz` log events | operator | a debt is paid; a holder's late write updates nothing; readiness skips the Bus | counts, durations, `error_code` | no session id, nonce, code digest or URL credentials | `failed_publish_sends_a_gap_marker_on_reconnect`, `lapsed_claim_is_retaken_and_the_late_stamp_refused`, `readyz_stays_ready_through_a_bus_outage` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The Dragonfly baseline, drill and bar are recorded first (§1) | `grep -c "^### Broker pass bar" docs/architecture/push_scheduler.md` | `1` | P0 | |
| R2 | NATS meets the bar on the same laptop (§3) | `grep -c "^Verdict: NATS passes" docs/architecture/push_scheduler.md` | `1` | P0 | |
| R3 | Tail and wake p99 are lane baselines (§1, §3) | `grep -c wake_p99_ms_hubs_4 bench/baselines/tail.rig.json` | `1` | P0 | |
| R4 | No JetStream code exists (Invariant 1) | `git grep -n "async_nats::jetstream" -- rustd` | no output | P0 | |
| R5 | Dragonfly is gone from code and tooling (§7) | `git grep -n -i dragonfly -- rustd make scripts docker-compose.yml compose.dev-infra.yml cli README.md .orly/LOCAL.md` | no output | P0 | |
| R6 | The crate and its environment variables are gone everywhere (§7, §8) | `git grep -n -w -e afd_dragonfly -e DRAGONFLY_URL -e TEST_DRAGONFLY_URL -- . ':!docs/v2' ':!docs/reviews' ':!bench/baselines' ':!playbooks/operations/acceptance' ':!docs/RUST_ERROR_STANDARD.md'` | no output | P0 | |
| R7 | The decision record quotes Indy (§7) | `grep -c "Yes i agree on NATS" docs/architecture/datastore_scaling.md` | `1` | P0 | |
| R8 | The repository rule names NATS (§7) | `grep -c "NATS_URL" .orly/LOCAL.md` | `1` or more | P0 | |
| R9 | Sessions and nonces are Postgres slots (§5) | `grep -c -e identity_device_sessions -e connector_oauth_nonces rustd/crates/daemon/afd_db/src/migration.rs` | `2` | P0 | |
| R10 | Claims read the open-lane index (§4) | `git grep -c -w idx_obligations_open_lane -- schema` | one slot, count `1` | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `rustd/crates/daemon/afd_dragonfly/**`, `SessionStore` | `git grep -n -w -e afd_dragonfly -e SessionStore -- rustd` | no output |
| `OutboundQueue`, `OutboundReader`, `outbound_consumer`, `MIN_AGE`, `select_unreceipted`, `reappend_receipt`, `receipt_delivery` | `git grep -n -w -e OutboundQueue -e OutboundReader -e outbound_consumer -e MIN_AGE -e select_unreceipted -e reappend_receipt -e receipt_delivery -- rustd` | no output; Dimension 4.6's fixtures are named `RETIRED_UNRECEIPTED_SCAN` and `RETIRED_LOST_SCAN` |
| `increment_in_window`, `key::response`, `key::event_ref`, `key::anomaly` | `git grep -n -e increment_in_window -e "key::response" -e "key::event_ref" -e "key::anomaly" -- rustd` | no output |
| `STARTUP_DRAGONFLY_CONNECT` | `git grep -n -w STARTUP_DRAGONFLY_CONNECT` | no output |
| `scripts/dragonfly-cluster.sh`, `scripts/dragonfly_cluster_ready.sh`, `playbooks/operations/teardown/dragonfly/` | `git grep -n -e dragonfly-cluster.sh -e dragonfly_cluster_ready.sh -e teardown/dragonfly -- ':!docs/v2'` | no output |
| `BENCH_BUS`, `integration_hub_exclusive` | `git grep -n -w -e BENCH_BUS -e integration_hub_exclusive` | no output outside `docs/v2` |

Schema this milestone leaves with no runtime reader or writer is dropped in a new slot in this Pull Request, under Indy's blanket approval (Discovery). Each drop gets a row above, with a `git grep -n -w` over `rustd`, `ui` and `cli` proving nothing still reads or writes it.

## Out of Scope

- JetStream, NATS KV and the object store: Postgres holds every durable fact (Indy, Discovery).
- Throughput work on the outbound path beyond the port: M245 counts statements.
- TLS between the daemon and NATS: the private network carries it in plaintext, as it carries Dragonfly today.
- Multi-region NATS, leaf nodes and Synadia Cloud: not planned.
- Persisting chunks or progress for backfill: a gap recovers durable rows only, as today.

---

## Product Clarity (authoring record)

N/A: no user surface. Routes, frames, CLI commands and error titles are unchanged; §6's extra `catching_up` sits inside its documented meaning.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** measure, add NATS, judge, then move and delete. The verdict lands while both implementations exist, so a failing bar reverts cheaply. The Postgres moves come before the deletion because each removes one Dragonfly caller.
- **Alternatives considered:** JetStream for outbound and KV for sessions, rejected because NATS would then hold state to replicate and back up; a scripted laptop kill in the bench, rejected in M241 as unstable on a shared compose node, so the exclusive integration group carries the automated proof; mapping a slow consumer to its own subscription, impossible while async-nats keeps the sid private; claiming any open row rather than lane heads, rejected because it reorders replies to one workspace.
- **Patch-vs-refactor verdict:** a **refactor** because one store is replaced and five of its jobs change homes, behind a seam M243 already cut.

## Discovery (consult log)

- **Readiness text (implementation finding, Nov 09, 2026: 03:40 AM)**: the Files Changed row said §6 leaves `public/openapi.json` unchanged. That was wrong. Once the bus stops gating, the `/readyz` sentence "Returns 200 when the database and event queue are available" is false. The route text and the `Readiness.queue` description now say the field reports the bus and does not gate. The wire shape is unchanged: `queue` stays as a reported field. The docs-repository page follows on its own branch at CHORE(close).

- **Consults** — Source review on `main` `14539a73e`, Nov 06, 2026:
  - `afd_dragonfly` has 19 importing crates plus the workspace manifest (`rustd/Cargo.toml:129`); `redis` 1.8.1 is pinned in `Cargo.lock`; `async-nats` is absent.
  - The retired unreceipted scan has no `delivered_at` filter (`afd_outbound/src/obligation/sql/select_unreceipted.sql`). A new path that wrote no `receipt` would see every delivered row re-sent by an old replica. §4's token closes that.
  - The anomaly key is built once outside the rule loop (`afd_gate/src/gate/anomaly.rs:60`).
  - The gate response mirror has no production writer; only `afd_gate/tests/integration_gate_faults.rs:160` writes it.
  - Both prod and dev daemons deploy with `release_command = "/usr/local/bin/agentsfleetd migrate"`, prod with `strategy = "rolling"` (`deploy/fly/agentsfleetd-prod/fly.toml:68-70`).
  - `datastore_scaling.md:27` requires an explicit user decision to change the datastore destination.
  - async-nats 0.51.0 on docs.rs (0.52.0 re-checked at CHORE(open), same fields): `ServerInfo` has `cluster`, `jetstream`, `headers` and `max_payload`; `Subscriber`'s fields are private. SPEC-PIN: the crate version and the `nats` image tag are pinned at §2's first commit; `node_kill_gaps_viewers_and_resumes` and `every_lost_frame_follows_a_gap` rerun when either changes.
  - Settled at §2: `nats-server` 2.15.2 accepts `--no_advertise` and `--cluster_name` (`nats --help`). Compose runs `nats:2.15.2-alpine`, the same release with `wget` for the `/healthz` check.
  - Found at §2: `async-nats` 0.52.0 authenticates only from `ConnectOptions` (`connector.rs:631`) and ignores a URL's userinfo. `Config` lifts the seeds' shared user and password and refuses seeds that disagree.
  - Found at §2: during an outage the client queues publishes and flushes them after reconnect, ahead of the owed marker. The adapter therefore fails a publish from disconnect until the markers are paid, so Dimension 2.3's gap precedes the next frame.
  - Active M233_002 edits `deploy/fly/dragonfly-dev/fly.toml`; CHORE(open) checks whether it merged before §8 deletes that file.
- **SPEC-BOUNDARY consult** — device sessions move inside this spec, because Dragonfly cannot be deleted while they live there. Endpoints, codes and the flow are unchanged; §5 lands as its own commit so its review reads `docs/AUTH.md` and the threat table against one diff.
- **Legacy consult (§4's ownership rule)** — superseded: the cutover is stop-the-world and dev data is reset, so the `claim_expires_at IS NULL` branch is not built (Indy, M242_001 Discovery, Nov 06, 2026: 10:22 PM).
- **Owner decisions** —
  > Indy (Nov 06, 2026: 08:50 AM): "And we are not in production yet, so i want to rethink the approach on admissions, lease, and so on as i am focussed on scale. If something fails in one host bare metal, it should automatically cordon and respawn in another runner" — context: failure handling is the reason for this milestone.
  > Indy (Nov 06, 2026: 11:57 AM): "Yes i am ok with postgres store and accepting messages, fences runs" — context: Postgres takes every durable job.
  > Indy (Nov 06, 2026: 01:11 PM): "keep Dragonfly, unless you stop being a synchophat and convince me that NATS is good, with the different edge cases indy hasnt thought through" then (01:18 PM) "Yes i agree on NATS." — context: the datastore decision `datastore_scaling.md:27` asks for; R7 quotes it there.
  > Indy (Nov 06, 2026: 10:43 AM): "4. Yes, but donot overengineer now, we can improve this." — context: one connection per process, one claim statement, no new topology setting.
  > Indy (Nov 06, 2026: 01:31 PM): "i prefer to have it in my laptop, upon deployment we move to the api-dev machines." — context: §1 and §3 run on the laptop.
  > Indy (Nov 06, 2026: 02:13 PM): "Move 6-7 to M244 but when we hit M244 we would have change code here) is this still fine to measure." — context: §1 carries M241's old §6 and §7 Dimensions as its own, Dragonfly measured first on this milestone's code.
  > Indy (Nov 06, 2026: 02:13 PM): "And i prefer we create all the spects and close M241 and push a PR?" — context: this spec lands in `pending/` with M241.
  > Indy (Nov 06, 2026: 04:18 PM): "yes" — context: blanket approval, asked as "Should each milestone drop the tables and columns it leaves with no reader or writer?": a milestone drops, in a new slot in its own Pull Request, any table, column or index it leaves with no runtime reader or writer (`.orly/standards/domains/sql.md` destructive-change approval).
- **Broker verdict (Nov 09, 2026: 10:32 AM)** — NATS passes; Indy signed off.
  > Indy (Nov 09, 2026, morning), on the first A/B, where the publisher ran 64 frames ahead and NATS failed 14 of 16 rungs: chose "Fair method, rerun" — context: one frame in flight on the viewer ladder; the stream ladder timed from the burst's start; `catching_up` counted as a lag only after a viewer is subscribed.
  > Indy (Nov 09, 2026, morning): "we need to look at the scale, on who does better?" — context: the stream ladder's burst now publishes concurrently, as fleets do. That exposed a full-record rescan in `afd_bus::nats::debt`, fixed in `202b08ffd` (4,096 streams: 32.0 ms to 9.8 ms p99).
  > Indy (Nov 09, 2026: 10:32 AM): "I would accept the 1 bench miss and signoff NATS." — context: 15 of 16 rungs pass the bar; the miss is 1 viewer at 64 KiB, NATS p99 2.76 ms against a bar of 1.46 ms. A size sweep ruled out the client's 65,535-byte read buffer; the cause is open.
  - Bench: 3 interleaved Dragonfly-then-NATS pairs of `make bench-tail PROFILE=rig` on Indy's laptop, medians, on this branch with M243 `0b87ec9ca` merged and the uncommitted lane edits above (`BENCH_BUS`, p99 series, `WINDOW` 1, concurrent stream burst). 1,024 streams: Dragonfly 4.18 ms, NATS 2.26 ms; 4,096: 20.83 ms against 9.83 ms. No frame undelivered and no lag on either bus.
  - Kill drills, two runs per bus, local daemon (`make up`), 2 viewers on one fleet, a steer every 10 s through a 60 s stop. Dragonfly: heartbeats only during the outage, `catching_up` 2.8 to 3.0 s after the container restarted. NATS (daemon switched to NATS locally, reverted; the stopped node was the daemon's own, `nats-3` then `nats-2`): `catching_up` 0.25 to 0.3 s after the stop, and all 6 outage steers delivered live. Every steer of all four runs reached `fleet.admissions` (19 per run). Viewers signed in as a dev Clerk drill user created by the acceptance fixtures and deleted afterwards.
  - `push_scheduler.md` §Baselines receives these numbers once #819 lands, because both milestones edit that page.
- **Post-quantum TLS on rustls (Nov 09, 2026)** — the workspace pinned rustls with `default-features = false`, which drops its `prefer-post-quantum` default, so every rustls client (sqlx, reqwest, async-nats, lettre, object_store) offered classical X25519 first. `rustd/Cargo.toml` now enables the feature; `tls_posture::rustls_negotiates_the_hybrid_post_quantum_key_exchange` proves a handshake picks X25519MLKEM768 and fails as X25519 without it. BoringSSL stays in `af_stream` because tokio-quiche requires it.
  > Indy (Nov 09, 2026): "You said its a 1 line change - rustls prefer-post-quantum inside M244 so why need a spec?" — context: folded into M244 rather than a separate spec.
  > Indy (Nov 09, 2026): "Yes i would keep boringssl in runner and migrate later to 1 crate" — context: one TLS library is later work; sqlx, reqwest and async-nats have no BoringSSL backend, so the supported single-library route is rustls on AWS-LC with quinn for QUIC.
- **Model reviews** — none yet. The EGM spec review runs before this spec leaves `pending/`.
- **Open blockers** — answered at CHORE(open) except item 2, which is Indy's action:
  1. Approve, at CHORE(open), edits to `deploy/fly/*` (new `nats-dev` and `nats-prod`, three machines each; `dragonfly-*` deleted) and to `.github/workflows/{deploy-dev-fly,release,test-integration-rustd,test-unit-rustd}.yml`.
  2. Create the 1Password items `nats-dev/api-url` and `nats-prod/api-url` in each runtime vault; §8 and the founding preflight checks read them.
  3. Accept that a device login or OAuth connect in flight across the cutover fails as missing and must be restarted.
  4. Accept that the anomaly brake counts per replica and resets on restart: a runaway spread over R replicas can repeat up to R times its threshold before a pause.
  5. Authorise destroying the `dragonfly-dev` and `dragonfly-prod` Fly apps once no old daemon machine runs (Dimension 8.3).
- **CHORE(open), Nov 08, 2026** — Indy's start instruction: "start M244_001 with the orly-build skill", with parallel work authorised while M243 changes (#818). `afd_bus` starts from M243 checkpoint `55a6e51fd`.
- **CHORE(open) owner decisions** —
  > Indy (Nov 08, 2026: 11:52 PM): "I see 0.52.0 for async-nats" then "so use that" — context: SPEC-PIN moves to `async-nats` 0.52.0; its `ServerInfo` and `Event` match the fields §2 reads.
  > Indy (Nov 08, 2026: 11:58 PM), on blocker 3: "Yes i accept the reset, its no big deal. We just did one in PR#817 where the db was cleaned. So you are free to do so." — context: no bridge for in-flight device logins or OAuth connects.
  > Indy (Nov 08, 2026: 11:58 PM), on blocker 4: chose "Per-replica memory" — context: the brake counts per replica, one window per rule, fails open.
  > Indy (Nov 08, 2026: 11:58 PM), on blockers 1 and 5: chose "Edits and destroy" — context: §8 may edit `deploy/fly/*` and the four workflows after §3 passes; Indy runs the app destroy (Dimension 8.3).
  > Indy (Nov 08, 2026: 11:58 PM), on the pass bar: "I think review the numbers of dragonfly now, with the current leve of af_bus:Bus adaptor or Dragonfly in local mac." Then he chose "Plan + 1 ms floor" — context: Dimension 1.7's bar is signed off as written in §1, with a 1 ms floor. Dragonfly is measured now through the checkpoint's Bus adapter, recorded as a checkpoint baseline, not as completed M243. The verdict rests on §3's interleaved pairs on one build.
  > Indy (Nov 09, 2026: 12:02 AM): chose "Core cluster, JS off" — context: `nats:2.15.2`, three nodes clustered by routes, JetStream off in development and production; Invariant 1 stands. Clustering is core NATS, so the A/B compares clustered Pub/Sub with clustered core NATS.
  > Indy (Nov 09, 2026: 12:20 AM), asking "why do you need 3?", then chose "Prod 3, dev 2" — context: core NATS has no quorum, so two nodes survive one loss; prod keeps a third for a crash during a NATS deploy. The laptop drill still kills one of three.
  > Indy (Nov 09, 2026: 12:20 AM): chose "api-url only" — context: the 1Password item `nats-<env>/api-url` is the only credential; the NATS image derives its server password from that URL.
  > Indy (Nov 09, 2026: 12:28 AM), asked whether 8.1 to 8.3 may be graded after merge: "Can you test this in local mac", then chose "Local 8.1+8.2, defer 8.3" — context: 8.1 is rehearsed on the laptop (main on Dragonfly, stop, migrate, this branch on NATS, each reply delivered once) and 8.2 on a two-node compose matching dev, both before merge; the deploy re-checks them. Only 8.3, Indy's app destroy, is graded after merge.
  > Indy (Nov 09, 2026: 12:40 AM): "try and avoid hard coding … I want this to be fed via the configuration so i can have a toml based configuration added alter." — context: the channel capacity, native buffer, connect and publish deadlines, debt window and debt bound are fields of `Tuning`, deserialisable and `garde`-bounded; a file loader comes later.
  > Indy (Nov 09, 2026: 12:35 AM): "donot use that term KNOB use releavnt ones" — context: constants are `URL_ENV` and `NAMESPACE_ENV`; prose says "environment variable" or "setting".
  > Indy (Nov 09, 2026: 12:04 AM): "If NATS is a winner then you will proceed on a PR, if not you need to prepare me a report with a rubric pros and cons for indy to signoff nats." — context: §3's failing-rung path produces that report before any deletion.
  > Indy (Nov 09, 2026: 12:05 AM): chose "Run when adapter ready" — context: §3's verdict runs on this branch's Bus-level tail lane once the NATS adapter is ready, with `BENCH_BUS` swapping only the adapter. One interleaved pair reruns after M243 merges, before the Pull Request leaves draft.
- **Deferrals and transfers** — none. M241_001 §6 and the drill half of its §7 arrive here as §1, mapped Dimension for Dimension (M241 6.1-6.4 → 1.1, 1.3-1.5; 7.2-7.3 → 1.6), under Indy's 02:13 PM quote above.
