<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M241_001: The push-scheduler decisions are recorded, and every number its milestones are judged by is measured on the rig

**Prototype:** v2.0.0

**Milestone:** M241

**Workstream:** 001

**Date:** Nov 06, 2026

**Status:** IN_PROGRESS

**Priority:** P1, the push-scheduler milestones change how every chat reaches a runner, and none of them can prove a gain without a baseline measured first

**Categories:** DOCS, INFRA, OBS

**Batch:** B1, the first milestone of the push-scheduler programme; M242 onward start after it ships

**Depends on:** none

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 06, 2026) from the EGM design brief reviewed twice on Nov 06, 2026

**Canonical architecture:** `docs/architecture/push_scheduler.md`, which §2 creates; `docs/architecture/lease_flow.md` and `runner_fleet.md` describe the system it measures

---

## Overview

**Goal (testable):** `bench-lease`, its `burst` shape and `bench-tail` write statements per chat by path, burst pickup latency, and tail and wake p99 into committed baselines, and `push_scheduler.md` records them beside the decisions and milestone 4's pass bar.

**Problem:** The programme replaces the 1 s lease poll with a pushed runner stream, folds chat messages into running runs, and replaces Dragonfly with core NATS. Nobody can tell whether any of that is faster, because today's costs are unmeasured or mismeasured:
- `agentsfleet_lease_poll_db_roundtrips_total` counts only the ledger statements of a poll (`afd_fleet/src/lease/assign/ledger.rs`).
- `agentsfleet_lease_poll_dragonfly_commands_total` is always zero: `lease_polled` is passed `0` (`assign/mod.rs`).
- The lease drain's simulated runner leases and reports, but never renews or posts activity (`afd_bench/src/lane/lease/drain/runner.rs`), so a drained "chat" is missing most of a real one.
- No bench sends a paced burst to busy fleets; the steer lane appends as fast as it can (`lane/steer/mod.rs`).
- `bench-tail` reports p95, never p99, and no bench times a wake.
- No one has recorded what a viewer sees when Dragonfly dies. Both Fly configurations run all four processes on one machine (`docs/architecture/datastore_scaling.md:55`).

The decisions themselves live only in a brief outside the repository.

**Solution summary:** One architecture page holds Indy's decisions, the rules they supersede, the proposed tables per milestone, the sequence diagrams and the milestone list. The two undercounting poll counters are deleted, so `pg_stat_statements` in the bench is the only statement count. Three bench changes and one drill fill the page's baselines and set the bar NATS must clear in milestone 4. The two specs this programme supersedes are parked.

**Design flow:**

```
make bench-lease PROFILE=rig
   seed fleets ─► simulated runners: lease, 2 renewals, 40 activity posts, report
   pg_stat_statements per query ─► statements per lease by path ─► lease.rig.json
make bench-lease PROFILE=rig BENCH_SHAPE=burst
   50 fleets × 10 steers/s via Steer::append ─► runners hold each lease for the turn
   bench clock: append → lease received, per event ─► p50/p95/p99 ─► lease-burst.rig.json
make bench-tail PROFILE=rig
   viewer ladder ─► publish-to-receive p95 and p99 ─► tail.rig.json
   wake ladder: doorbell::publish ─► Doorbell::wait_for_ring on 1, 2, 4 hubs ─► p50, p99, missed
broker-kill drill (manual, laptop): curl viewers + paced steers ─► stop Dragonfly ─► observe ─► restart
each lane: three runs, the median run copied to bench/baselines/, spread recorded
all of it ─► docs/architecture/push_scheduler.md §Baselines and §Milestone 4 pass bar
```

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(bench): record the push-scheduler decisions and today's baselines
- **Intent (one sentence):** give every later push-scheduler milestone a recorded decision and a measured number to beat.
- **Authoring handshake:** Indy wants the lease, admission and broker path rebuilt for scale before production, in milestones, each listing its table and code changes when it starts. This first milestone records the decisions and measures today, in one Pull Request with this spec. ASSUMPTIONS I'M MAKING:
  1. The rig is Indy's laptop for this milestone, and baselines move to the api-dev machines at deployment. Laptop runs are noisy, so each lane runs three times and the median run becomes the baseline.
  2. Deleting the two poll counters is acceptable. No dashboard in the repository reads them (`git grep` across `playbooks/`, `deploy/`), and the bench already counts statements from `pg_stat_statements`.
  3. The tables in `push_scheduler.md` are a proposal per milestone. Each milestone's spec fixes its own schema when it starts.
  4. The parking of M240_001 and M222_001 rides this milestone's branch, as Indy asked.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/afd_bench/src/lane/steer/mod.rs` — how a lane appends steers in process through `afd_events::Steer::append`; §5 mirrors it with pacing.
2. `rustd/crates/afd_bench/src/lane/lease/drain/runner.rs`, `lane/lease/shape.rs` and `src/bin/lease.rs` — the simulated runner §4 extends, and the shape switch and per-shape lane file §5 extends.
3. `rustd/crates/afd_bench/src/statements/mod.rs` and `src/report/mod.rs` — how statements are read today (one `sum(calls)`) and the flat `measurements` map every result key must fit.
4. `rustd/crates/afd_dragonfly/src/doorbell/mod.rs` — the public `publish`, `Doorbell::listen` and `wait_for_ring` the wake ladder drives, and the 64-hint cap.
5. `bench/baselines/README.md` — a baseline is a real run, copied over in the same commit as the change, with `scaling.md`'s table moving with it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M241_001_P1_DOCS_INFRA_OBS_PUSH_SCHEDULER_BASELINES_RECORDED.md` | CREATE, then MOVE to `active/` and `done/` | this spec |
| `docs/v2/pending/M240_001_P1_API_DOCS_INFRA_OBS_LEASE_QUEUE_ONE_STATEMENT_CLAIM.md` | MOVE to `done/` as DEFERRED | §1 |
| `docs/v2/pending/M222_001_P2_API_DOCS_OUTBOUND_PROXY_DECLARED_NOT_INHERITED.md` | MOVE to `done/` as DEFERRED | §1 |
| `docs/architecture/push_scheduler.md` | CREATE | §2, filled by §7 |
| `docs/architecture/README.md` | EDIT | §2: index row |
| `rustd/crates/afd_fleet/src/lease/assign/mod.rs`, `assign/ledger.rs`, `assign/measured.rs` | EDIT | §3: `PollCost` keeps candidates; `lease_polled` takes one count |
| `rustd/crates/afd_fleet/tests/integration_held_sandbox/poll.rs` | EDIT | §3: the idle-poll test asserts candidates, not round trips |
| `rustd/crates/afd_observability/src/metrics/declared/fleet.rs`, `producers/fleet/mod.rs`, `tests/producers_drive.rs` | EDIT | §3: two families, their instruments and the producer call go |
| `docs/metrics.census.tsv` | EDIT | §3: two rows go |
| `rustd/crates/afd_bench/src/instrument/mod.rs`, `instrument/tests.rs`, `tests/lease_instrument.rs`, `tests/ledger_claim.rs`, `src/report/compare/tests.rs`, `Cargo.toml` | EDIT | §3: `PollCounters` keeps polls and candidates; tests and the manifest comment follow |
| `rustd/crates/afd_bench/src/lane/lease/window.rs` | EDIT | §3: `roundtrips_per_lease` goes; `datastores.postgres.operations` comes from the window's `pg_stat_statements` calls |
| `rustd/crates/afd_bench/src/statements/mod.rs`, `statements/paths.rs`, `statements/tests.rs` | EDIT, CREATE | §4: per-query calls and the path table |
| `rustd/crates/afd_bench/src/lane/lease/drain/runner.rs`, `drain/mod.rs`, `drain/tests.rs` | EDIT | §4: renewals and activity per simulated turn; flat path keys |
| `rustd/crates/afd_bench/src/knobs/mod.rs`, `src/profile/mod.rs` | EDIT | §4, §5: `BENCH_TURN_MS` and `BENCH_RATE`, with rig caps |
| `rustd/crates/afd_bench/src/lane/lease/shape.rs`, `lane/lease/mod.rs`, `lane/lease/burst/mod.rs`, `burst/tests.rs`, `src/bin/lease.rs`, `src/report/lane.rs` | EDIT, CREATE | §5: `Shape::Burst` and `Lane::LeaseBurst` |
| `rustd/crates/afd_bench/src/lane/tail/mod.rs`, `tail/viewers/mod.rs`, `tail/tests.rs`, `tail/wake/mod.rs`, `tail/wake/tests.rs` | EDIT, CREATE | §6: p99 per rung, and the wake ladder |
| `make/bench.mk` | EDIT | §5, §6: help text; no new target |
| `bench/baselines/lease.rig.json`, `bench/baselines/lease-burst.rig.json`, `bench/baselines/tail.rig.json` | EDIT, CREATE | §4–§6: median-run baselines |
| `docs/architecture/scaling.md`, `observability.md`, `runner_fleet.md` | EDIT | §3: lines naming the deleted counters (`scaling.md` lease and tail rows, `observability.md` poll counters, `runner_fleet.md` poll cost); §7: the measured-ceilings rows move with the baselines |

## Applicable Rules

- **`code.md`** — NDC, NLR, FLL, CODE-SEAM
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR
- **`tests.md`** — TEST-REAL, TEST-LANE, TEST-COUNTER, TEST-DELTA
- **`domains/logging.md`** — OBS, LOG-EVENT
- **`docs.md`** — DOC-02, DOC-03
- **`spec.md`** — SPEC-STATUS, SPEC-DEFER, SPEC-DECIDE

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `docs.language` at `orly gate verify` and `orly gate pr` | yes, new and edited architecture prose | sentences under 25 words, paragraphs of at most three sentences; scan before each commit |
| `make lint-all` (clippy `-D warnings`, `cargo shear`) | yes, `afd_bench`, `afd_fleet` and `afd_observability` change | `lint-rustd` per Section |
| Metrics census and producer tests | yes, two families go | census rows, declarations and producer go in one commit |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, `statements/` and `lane/tail/` grow | the path table in `statements/paths.rs`; the wake ladder in `tail/wake/` |

## Prior-Art / Reference Implementations

- **Reference:** `afd_bench/src/lane/steer/` — in-process steers through `Steer::append`; `burst` adds pacing per fleet. Divergence: runners drain while steers arrive.
- **Reference:** `afd_bench/src/lane/lease/saturation/` and `Lane::LeaseSaturation` — a second lease shape with its own lane file, the pattern `Lane::LeaseBurst` follows.
- **Reference:** `afd_bench/src/lane/tail/viewers/` — the ladder the wake ladder mirrors, including the rule that the sender never outruns its slowest receiver.
- **Reference:** `docs/architecture/datastore_scaling.md` — dated decisions with the owner's words and the evidence that decided them.

## Sections (implementation slices)

### §1 — Park the specs this programme supersedes

Apply the two parking commits saved at `/Users/kishore/Projects/agentsfleet/.gstack/egm/scheduler/park_patches/` (gitignored); the first is also on `origin/chore/m240-001-park`. M240_001 and M222_001 move to `done/` as DEFERRED, each with Indy's words, the reason and the reopen condition in Discovery. Correct M240_001's quote time to 12:59 PM, when Indy sent it. **Implementation default:** apply the patches rather than rewrite them, because the standards review of that diff found no rule findings.

- **Dimension 1.1** — DONE — both specs sit in `done/` as DEFERRED → Test R1's `grep -c` (manual): prints `1` for each file; evidence in the Pull Request Session Notes.
- **Dimension 1.2** — DONE — no live page points at the pending paths → Test `git grep -n "pending/M240_001" -- docs/architecture rustd` (manual): prints nothing.

### §2 — `push_scheduler.md` records the decisions and the target design

One new architecture page holds:
- Indy's decisions with his words and dates (Discovery lists them).
- The superseded rules, each with its retiring milestone: the Dragonfly datastore rule in `.orly/LOCAL.md` (M244); "config is never cached" at `runner_fleet.md:57,555` (M245); "no cancel channel" at `runner_fleet.md:545` (M243); "not a general scheduler" at `runner_fleet.md:53,167` (M243); timers with no central deadline scheduler at `concurrency.md:272-279` (M245).
- The fact-to-store table: Postgres accepts every message and fences every run; core NATS carries only what a Postgres backstop repairs.
- Proposed tables per milestone, named at the architecture level.
- Sequence diagrams: accept then start, fold or wait while compacting; runner stream connect and reconnect; host-batched renewal; tail fan-out with the gap signal; outbound claiming; respawn after a lapsed lease.
- The per-subject failure table, the milestone list, §Baselines and §Milestone 4 pass bar.

The page states the target as the design (SPEC-FRAME). **Implementation default:** no column listings, because each milestone's spec fixes its own schema when it starts (Indy, Discovery).

- **Dimension 2.1** — the page reads clean → Test `orly gate verify` (integration): 0 `docs.language` findings on `docs/architecture/push_scheduler.md`.
- **Dimension 2.2** — Indy approves the decisions section → Test `decisions_approved` (manual): Indy reads §Decisions and §Superseded rules; his reply is quoted in the Pull Request Session Notes.

### §3 — Delete the poll counters that undercount

`PollCost` keeps `candidates_scanned`, and `lease_polled` takes that count alone. `agentsfleet_lease_poll_db_roundtrips_total` and `agentsfleet_lease_poll_dragonfly_commands_total` go from the declarations, the producer, the census, `PollCounters`, `window.rs` and the tests that name them. `window.rs` fills `datastores.postgres.operations` from the window's `pg_stat_statements` calls instead. The architecture lines naming the counters change in the same commit. **Implementation default:** delete rather than repair, because M240's hand-threaded tally still recorded 19 of 36 statements: the provider, credential and estimate crates send statements the poll never sees.

- **Dimension 3.1** — no producer is left for a census row → Test `every_census_family_has_a_producer` (unit, `afd_observability/src/producers/tests.rs`): passes with both rows gone, and fails if a row is left after its producer goes.
- **Dimension 3.2** — an idle poll still records its candidates → Test `test_an_idle_poll_examines_no_candidates` (integration, renamed from `test_an_idle_poll_costs_one_postgres_probe_and_no_dragonfly_commands`): one idle poll records `candidates_scanned` 0 and `PollCost` has no round-trip field.
- **Dimension 3.3** — a lease window reports no counter round trips → Test `window_measurements_carry_no_counter_roundtrips` (unit): `Report.measurements` has no `roundtrips_per_lease` key, which today it does.
- **Dimension 3.4** — the window's Postgres operations come from statements → Test `window_operations_come_from_statements` (unit): a window with 120 statement calls reports `datastores.postgres.operations` 120 whatever the poll counters say.

### §4 — Statements per chat, grouped by path

`BENCH_TURN_MS` (default 10 000, rig cap 60 000) sets one simulated turn. Per lease, the drain's runner sends what that turn sends: `floor(turn / 5 000)` renewals through `Leases::extend` (renewal every 5 s, `afd_core/src/timing.rs:37`) and `turn / 250` activity posts through the activity service (`afr_supervisor/src/activity/mod.rs:56`). In the default shape they run back to back with no sleep, so the 30 s drain window still holds. After the drain, `statements/` reads each `pg_stat_statements` row's `calls` and normalized query, and `paths.rs` attributes it to one path: `claim`, `gates`, `issue`, `renew`, `activity`, `report`, `sweep` or `other`. **Implementation default:** the table matches a distinctive fragment of each product statement's normalized text, first match wins, and a statement shared by two paths goes to the first and is listed as shared; product statement constants are private to their crates. Results are flat keys `drain_statements_per_lease_<path>`. The lane's printed summary lists the five statements with the most calls per lease.

- **Dimension 4.1** — the paths sum to the total → Test `statements_by_path_sum_to_total` (unit): rows of 10, 15 and 5 calls in three paths over 10 leases report 1.0, 1.5 and 0.5 per lease, and 3.0 in total.
- **Dimension 4.2** — an unknown statement is counted, not lost → Test `unmatched_statement_counts_as_other` (unit): a query matching no fragment adds its calls to `other`.
- **Dimension 4.3** — a shared statement is counted once → Test `shared_statement_counts_in_its_first_path` (unit): a statement matching two fragments adds its calls to the first path only.
- **Dimension 4.4** — zero leases never divide → Test `no_leases_reports_no_ratio` (unit): a drain with 0 leases reports no per-lease keys rather than infinity.
- **Dimension 4.5** — a turn shorter than one renewal renews zero times → Test `short_turn_sends_no_renewal` (unit): `BENCH_TURN_MS=1000` plans 0 renewals and 4 activity posts.
- **Dimension 4.6** — a drained chat fills its paths → Test `make bench-lease PROFILE=rig` (integration): `bench/results/lease.rig.json` holds non-zero `drain_statements_per_lease_` keys for `claim`, `issue`, `renew`, `activity` and `report`.

### §5 — A paced burst to busy fleets

`BENCH_SHAPE=burst` writes `Lane::LeaseBurst` (`lease-burst.rig.json`). It seeds `BENCH_FLEETS` fleets (default 50) and appends `BENCH_RATE` steers a second to each (default 10) through `Steer::append` for `BENCH_WINDOW_SECONDS` (default 60, keeping 30 000 steers under `DEPLOYMENT_BACKLOG_BUDGET` of 100 000). `BENCH_RUNNERS` simulated runners (default 8) drain, each holding a lease for `BENCH_TURN_MS` before reporting, so fleets are busy while steers arrive. Today each steer is a separate run (`runner_fleet.md:541`), so this measures the queue milestone 2's folding removes. Pickup is timed on the bench's clock: append returned → lease received, keyed by event id. Results are flat keys: `burst_accepted`, `burst_refused`, `burst_unleased`, `burst_append_p95_ms`, `burst_pickup_p50_ms`, `burst_pickup_p95_ms`, `burst_pickup_p99_ms` and `burst_backlog_peak`. **Implementation default:** a refused append is counted and excluded from latency; an accepted steer with no lease at window close is counted as unleased, never dropped.

- **Dimension 5.1** — the shape parses and rejects nonsense → Test `shape_reads_burst_and_refuses_unknown` (unit): `burst` reads as `Shape::Burst`; `bursty` fails naming the allowed spellings.
- **Dimension 5.2** — unleased work is reported → Test `burst_counts_unleased_at_window_close` (unit): 10 accepted, 4 leased reports `burst_unleased` 6.
- **Dimension 5.3** — refusals stay out of latency → Test `burst_excludes_refusals_from_pickup` (unit): 3 refusals and 7 pickups of 100 ms report `burst_refused` 3 and p95 100.
- **Dimension 5.4** — pacing holds the asked rate → Test `burst_pacer_spaces_appends` (unit, paused time): 10 a second for 2 s schedules 20 appends 100 ms apart.
- **Dimension 5.5** — a live burst reports pickup → Test `make bench-lease PROFILE=rig BENCH_SHAPE=burst` (integration): the result holds `burst_pickup_p99_ms` and `burst_accepted` above 0.

### §6 — Tail and wake p99

The viewer ladder records p99 beside p95 per rung (`tail/viewers/mod.rs`), as flat keys `p99_ms_<rung>`. A wake ladder calls `afd_dragonfly::doorbell::publish` for distinct fleets while 1, 2 and 4 hubs each run a `Doorbell` through `listen` and `wait_for_ring`. It keeps at most 64 rings outstanding, the doorbell's hint cap, and counts a ring missed when no hub takes it within 1 s. Results are `wake_p50_ms_hubs_<n>`, `wake_p99_ms_hubs_<n>` and `wake_missed_hubs_<n>`. **Implementation default:** the wake ladder lives in the tail lane, because both time one broker hop and milestone 4 compares both.

- **Dimension 6.1** — the summary carries p99 → Test `tail_summary_reports_p99` (unit): latencies 1..100 ms report p95 95 and p99 99.
- **Dimension 6.2** — a late ring is missed, not delivered → Test `wake_past_deadline_counts_missed` (unit): a ring taken after 1 s counts in `wake_missed_hubs_1` and not in p99.
- **Dimension 6.3** — the ringer never passes the hint cap → Test `wake_ringer_waits_at_window` (unit, paused time): with 64 untaken rings, the 65th waits.
- **Dimension 6.4** — a live wake ladder delivers → Test `make bench-tail PROFILE=rig` (integration): the result holds `wake_p99_ms_hubs_4` and `wake_missed_hubs_4` 0.

### §7 — Three runs, the broker-kill drill and milestone 4's pass bar

Each of `bench-lease`, `bench-lease BENCH_SHAPE=burst` and `bench-tail` runs three times on Indy's laptop, interleaved, after a reset rig. A laptop drain has ranged from 38 to 167 leases/s between runs, so one run proves nothing. The run with the median headline value (drain leases per second, `burst_pickup_p95_ms`, `wake_p99_ms_hubs_4`) is copied to `bench/baselines/`, as the baselines README requires a real run. The other two runs' headline values are recorded beside it in §Baselines.

The drill runs on the dev stack, twice:
1. Start the dev stack with `make up`: shared Postgres and Dragonfly from `compose.dev-infra.yml`, plus this worktree's native daemon.
2. Hold two viewers on one fleet's stream (`curl -N` on `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/stream`), and post one steer a second to that fleet.
3. Stop Dragonfly (`docker compose -f compose.dev-infra.yml stop dragonfly`) and observe for 60 s.
4. Start it again with `make up`.
5. Record what each viewer received (`event_admitted` frames, `catching_up`, or nothing behind the 15 s heartbeat), whether each steer was accepted in Postgres during the outage, and the seconds until frames flow again.

A laptop cannot execute a run (the sandbox is Linux-only), so the drill watches daemon frames, not run output.

§Milestone 4 pass bar then records, against these numbers:
- NATS tail p99 and wake p99 no worse than Dragonfly's plus 10%;
- after one of three NATS nodes is killed, every viewer receives `catching_up` and resumes within 10 s with no person acting;
- no lost frame passes without a gap signal.

- **Dimension 7.1** — baselines are median real runs → Test `baselines_are_median_runs` (manual): §Baselines lists three headline values per lane, and the committed file's value is the middle one.
- **Dimension 7.2** — the drill is recorded with evidence → Test `broker_kill_drill_recorded` (manual): §Baselines holds "Broker-kill drill, run 1" and "run 2", each with viewer outcomes, accepted steers and recovery seconds, and the commands used.
- **Dimension 7.3** — the frozen-tail defect is captured → Test `broker_kill_drill_shows_no_gap_signal` (manual): during the outage no viewer receives `catching_up`, recorded as observed; if one does, the pass bar is rewritten from what was seen.

## Interfaces

- **Bench environment:** `BENCH_SHAPE=burst`; `BENCH_RATE` (steers a second per fleet, default 10, rig cap 50); `BENCH_TURN_MS` (default 10 000, rig cap 60 000). `BENCH_FLEETS`, `BENCH_RUNNERS` and `BENCH_WINDOW_SECONDS` keep their meanings. Unknown values fail naming the allowed spellings, as `shape.rs` does today.
- **Result files:** flat keys in `measurements`, so `bench-compare` reads them unchanged. `lease.rig.json` gains `drain_statements_per_lease_<path>` and loses `roundtrips_per_lease`. `lease-burst.rig.json` is new, with the `burst_*` keys. `tail.rig.json` gains `p99_ms_<rung>` and the `wake_*` keys.
- **Metrics:** `agentsfleet_lease_poll_db_roundtrips_total` and `agentsfleet_lease_poll_dragonfly_commands_total` are removed. `agentsfleet_lease_polls_total` and `agentsfleet_lease_poll_candidates_scanned_total` are unchanged.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Unknown shape | failure | `BENCH_SHAPE=bursty` | the lane exits non-zero naming the allowed spellings | `shape_reads_burst_and_refuses_unknown` |
| Statement no fragment matches | failure | a new product statement | counted under `other`, never dropped | `unmatched_statement_counts_as_other` |
| Statement two paths match | failure | a shared statement | counted once, in the first path | `shared_statement_counts_in_its_first_path` |
| No leases in the drain | failure | a stalled rig | no per-lease keys, no division | `no_leases_reports_no_ratio` |
| Burst outruns the runners | capacity | appends faster than leases | accepted but unleased steers counted | `burst_counts_unleased_at_window_close` |
| Intake refuses steers | capacity | a fleet or the deployment past its backlog budget | refusals counted apart, out of latency | `burst_excludes_refusals_from_pickup` |
| A ring arrives late or never | failure | no hub, or a slow hub | counted missed, out of p99 | `wake_past_deadline_counts_missed` |
| Ringer floods the doorbell | capacity | more than 64 untaken rings | the ringer waits at the window | `wake_ringer_waits_at_window` |
| A census row outlives its producer | failure | a family half removed | the producer test fails | `every_census_family_has_a_producer` |
| Dragonfly dies | failure | the drill stops it | viewers' outcome and steer acceptance recorded as observed | `broker_kill_drill_shows_no_gap_signal` |

## Invariants

1. Every statement call the drain's database executed is attributed to exactly one path — `paths.rs` classifies first-match with an `other` fallback, and `statements_by_path_sum_to_total` fails if any call is lost or doubled.
2. A census family exists exactly when a producer feeds it — `every_census_family_has_a_producer` fails on an orphan row.
3. The wake ringer never has more than 64 rings outstanding — the ladder awaits a take before ringing again, and `wake_ringer_waits_at_window` fails otherwise.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `agentsfleet_lease_poll_db_roundtrips_total` (removed) | operator | never again | none | none | `every_census_family_has_a_producer` |
| `agentsfleet_lease_poll_dragonfly_commands_total` (removed) | operator | never again | none | none | `every_census_family_has_a_producer` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | M240_001 is parked (§1) | `grep -c "Status:\*\* DEFERRED" docs/v2/done/M240_001_P1_API_DOCS_INFRA_OBS_LEASE_QUEUE_ONE_STATEMENT_CLAIM.md` | `1` | P1 | |
| R2 | The decisions page reads clean (§2) | `orly gate verify` | exit 0 | P0 | |
| R3 | The undercounting counters are gone (§3) | `git grep -n -w -e agentsfleet_lease_poll_db_roundtrips_total -e agentsfleet_lease_poll_dragonfly_commands_total -- ':!docs/v2'` | no output | P0 | |
| R4 | Statements per chat by path are a baseline (§4, §7) | `grep -c drain_statements_per_lease_renew bench/baselines/lease.rig.json` | `1` | P0 | |
| R5 | Burst pickup is a baseline (§5, §7) | `grep -c burst_pickup_p99_ms bench/baselines/lease-burst.rig.json` | `1` | P0 | |
| R6 | Wake p99 is a baseline (§6, §7) | `grep -c wake_p99_ms_hubs_4 bench/baselines/tail.rig.json` | `1` | P0 | |
| R7 | The drill is recorded twice (§7) | `grep -c "Broker-kill drill, run 2" docs/architecture/push_scheduler.md` | `1` | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `LEASE_POLL_DB_ROUNDTRIPS_TOTAL`, `lease_roundtrips` | `git grep -n -w -e LEASE_POLL_DB_ROUNDTRIPS_TOTAL -e lease_roundtrips -- ':!docs/v2'` | no output |
| `LEASE_POLL_DRAGONFLY_COMMANDS_TOTAL`, `lease_dragonfly_commands` | `git grep -n -w -e LEASE_POLL_DRAGONFLY_COMMANDS_TOTAL -e lease_dragonfly_commands -- ':!docs/v2'` | no output |
| `database_roundtrips`, `dragonfly_commands` | `git grep -n -w -e database_roundtrips -e dragonfly_commands -- rustd` | no output |
| `roundtrips_per_lease` | `git grep -n -w roundtrips_per_lease -- ':!docs/v2'` | no output |

Hits under `docs/v2/` are records (done specs, the parked M240_001, `docs/v2/reviews/`) and stay.

## Out of Scope

- Every behaviour change: folding and one-statement intake (M242), the runner stream (M243), NATS (M244), fewer round trips (M245), customer runners (M246), capacity and checkpoint respawn (M247). Each spec lists its table and code changes when its milestone starts (Indy, Discovery).
- Rewriting the rules in `.orly/LOCAL.md`, `runner_fleet.md` or `concurrency.md`: each changes with the milestone that changes the behaviour. Only lines naming the deleted counters change here.
- Partitions and a daemon-side fleet owner: only if measurement shows saturation below three times the 12-month peak.
- Re-measuring on the api-dev machines: at deployment (Indy, Discovery).

---

## Product Clarity (authoring record)

N/A: no user surface.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** park and record first, then delete the counters, then three measurements and a drill, so each measurement lands on honest counting and every number has a home in §2's page.
- **Alternatives considered:** repairing the poll tally, rejected because a hand-threaded count misses statements other crates send; a new `bench-burst` target, rejected because the lease lane already owns shapes; reading `event_to_lease` from the delivery-stage histogram, rejected because its buckets start at 50 ms and the bench's exporter captures only counters; a scripted broker kill, rejected because the tail lane records that killing a shared compose node is not a stable bench.
- **Patch-vs-refactor verdict:** a **patch** because it adds measurements and removes two counters without changing any request path.

## Discovery (consult log)

- **Consults** — Source review on `main` `14539a73e`, Nov 06, 2026:
  - Poll cost counts ledger statements only (`afd_fleet/src/lease/assign/ledger.rs`), and the Dragonfly count is passed as `0` (`assign/mod.rs`).
  - The bench reads one `sum(calls)` from `pg_stat_statements` (`afd_bench/src/statements/mod.rs`); `Report` holds flat measurements (`report/mod.rs`); each shape writes its own lane file (`report/lane.rs`, `bin/lease.rs`).
  - `bench/results/` is gitignored, and committed baselines live in `bench/baselines/` (`bench/baselines/README.md`).
  - The `afd_bench` integration tests are outside the declared integration lane (`make/test-integration-rustd.mk`), so live proofs here are the bench runs themselves.
  - The doorbell's public API and 64-hint cap are in `afd_dragonfly/src/doorbell/mod.rs`; `Admissions::wake` is crate-private.
  - Both Fly configurations run four Dragonfly processes on one machine (`datastore_scaling.md:55`), and self-hosted multi-shard Dragonfly manages no failover (`datastore_scaling.md:35-37`).
- **Owner decisions** —
  > Indy (Nov 06, 2026: 08:50 AM): "And we are not in production yet, so i want to rethink the approach on admissions, lease, and so on as i am focussed on scale. If something fails in one host bare metal, it should automatically cordon and respawn in another runner" — context: the programme.
  > Indy (Nov 06, 2026: 10:19 AM): "This will be on by default durable_tasks_v1. ... my plan also was to have folks spin a runner and point to the agentsfleetd (in cloud) and keep using it." — context: durable tasks by default; runners reach the daemon through its API.
  > Indy (Nov 06, 2026: 10:43 AM): "1. Yes placement rules are via the tags and labels 2. Respawn across ownership - this is a tenant setting ... Based on the scope i would like to build this ... 4. Yes, but donot overengineer now, we can improve this." — context: placement, the respawn setting, the smallest design.
  > Indy (Nov 06, 2026: 11:57 AM): "Yes i am ok with postgres store and accepting messages, fences runs" and "The message has to wait. I assume you show message like compacting" and "assume atleast 50 fleets" — context: Postgres acceptance and fencing; waiting while compacting; §5's 50 fleets.
  > Indy (Nov 06, 2026: 01:03 PM): "i want these in the first implementation spec in the branch you do and not in a separaete chore/m240-001* branch" — context: §1.
  > Indy (Nov 06, 2026: 01:11 PM): "keep Dragonfly, unless you stop being a synchophat and convince me that NATS is good, with the different edge cases indy hasnt thought through" then (01:18 PM) "Yes i agree on NATS." — context: core NATS replaces Dragonfly in milestone 4, against §7's bar.
  > Indy (Nov 06, 2026: 01:12 PM): "Since this a underlying architechture change what milestones will be broken into. Indy will need a full list of table, and code changes before a new agents start on the milestone." then (01:18 PM) "i wanted the code changes when you start the milestone not now." — context: §2 proposes tables per milestone; each milestone's spec carries its own change list.
  > Indy (Nov 06, 2026: 01:31 PM): "i prefer to have it in my laptop, upon deployment we move to the api-dev machines." — context: the rig for §4–§7.
  > Indy (Nov 06, 2026: 01:36 PM): "what will spec 241_001 pr contain, i prefer we add all into this and start M242 and ship the M242 PR." — context: this spec ships with its build in one Pull Request.
- **Model reviews** —
  - EGM brainstorm, first pass (Claude Fable 5.1, six Goldfish lenses): 6 blockers, 16 majors, recorded in the brief; B1 and B2 accepted by Indy.
  - Second pass (Claude Opus 5.5; design, codebase-details, zoom-out): 3 blockers, 11 majors, 12 minors, all applied to the brief.
  - Standards review of the parking diff (orly-standards-reviewer): no rule findings; one wording note on R10, applied.
  - Spec review (Claude Opus 5.5; implementation-readiness, codebase-details): 3 blockers, 12 majors, 9 minors, all applied in this revision: flat result keys and per-shape lane files; the simulated turn pinned; median runs into `bench/baselines/`; full metric names in greps; the producer test in place of the census parity test; the window's operations source; the missed callers and docs; bench runs as the live proofs; `Steer::append` pacing and bench-clock pickup; the doorbell API and cap; the shared-statement rule; dev-stack drill commands; the patch path; failure Dimensions at half or more; corrected doc lines.
- **Open blockers** — none.
- **Deferrals and transfers** — none.
