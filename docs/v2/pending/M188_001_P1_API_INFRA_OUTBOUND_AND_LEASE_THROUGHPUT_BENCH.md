<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M188_001: Measured throughput ceilings for steer, lease and delivery, to a million fleets

**Prototype:** v2.0.0
**Milestone:** M188
**Workstream:** 001
**Date:** Sep 05, 2026
**Status:** PENDING
**Priority:** P1 — the scaling argument for connector delivery and lease issuance is being made from estimates, and the next change is either a mud-patch or a refactor nobody can justify.
**Categories:** API, INFRA
**Batch:** B1 — standalone; no other workstream shares its files.
**Branch:** {feat/mNN-name — added at CHORE(open)}
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
**Depends on:** none — the fix that motivated it (outbound reply deadline and read backoff) is already merged.
**Provenance:** agent-generated (pre-spec, this session's dev-environment investigation)
**Canonical architecture:** `docs/architecture/scaling.md`

---

## Overview

**Goal (testable):** four benchmark lanes report, as numbers a rubric can compare, what the steer, lease, delivery and cardinality paths sustain — each attributing its cost to Redis or to Postgres — under a profile that reaches a million fleets on a dedicated rig and stays inside declared caps on the deployed development and production environments.

**Problem:** nobody can say what any of these paths cost, or which datastore gives out first. Delivery is serial, one entry per read, and holds its only worker through a retry ladder. Lease issuance spends a hand-counted number of Postgres round trips per poll and keeps one Redis stream and consumer group per fleet. Steer ingress writes Postgres, appends a stream and marks a global hash on every message. None of it has been observed above a handful of fleets, and the environment it runs in has never carried concurrent load. Every scaling claim about this system currently ends in an estimate.

**Solution summary:** four lanes driving the real production types, each reporting a rate, a latency distribution, and a per-datastore breakdown so saturation can be attributed rather than guessed. One profile system decides scale and safety: the dedicated rig goes to a million fleets, the deployed environments run bounded synthetic load inside a fixture tenant with hard caps and an abort switch. Results are files with committed baselines. The lanes measure; they change no production behaviour and ship no product surface.

## PR Intent & comprehension handshake

- **PR title (eventual):** bench: measured ceilings for steer, lease, delivery and cardinality
- **Intent (one sentence):** an engineer can answer "what breaks first, at what rate, and in which datastore" for the paths a fleet's life runs through, on the rig for the million-fleet question and safely against a deployed environment for the reality check.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_fleet/src/lease/assign.rs` — `select_counted` already tallies `candidates_scanned` and `database_roundtrips` per poll and publishes them through `producers::fleet::lease_polled`. The lease lane reads that instrument; it never counts queries itself.
2. `rustd/crates/afd_outbound/src/worker.rs` — the delivery loop under measurement. Its module note states the read order, what cancellation stops, and why a dropped read leaves a pending entry; the lane must not measure around any of it.
3. `rustd/crates/afd_events/src/steer.rs` — one steer is a Postgres write, a stream append and a readiness mark. The ingress lane's per-datastore attribution is derived from this shape.
4. `rustd/crates/afd_outbound/tests/support/hanging_queue.rs` — the loopback server pattern to model the stub vendor on; it already parses RESP and counts what it was asked.
5. `make/test-infra.mk` — the compose Postgres and Redis the integration lane resets per run, and the rig the million-fleet profile uses. A second datastore rig is the thing this repository has one of on purpose.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `make/bench.mk` | EDIT | gains the four lanes and the profile plumbing; the existing loadgen target is untouched |
| `bench/harness/profile.rs` | CREATE | profile definitions, caps, the production acknowledgement, and the abort switch |
| `bench/harness/report.rs` | CREATE | the result-file shape, per-datastore attribution, and the comparison |
| `bench/harness/fixture.rs` | CREATE | fixture tenancy, run prefixes, and the sweep every deployed run ends with |
| `bench/steer/main.rs` | CREATE | drives concurrent steers, reports accepted rate and where the cost landed |
| `bench/outbound/main.rs` | CREATE | drives the real `Worker` against a stub vendor, reports rate and head-of-line cost |
| `bench/lease/main.rs` | CREATE | drives the real lease path, reports rate and round trips per lease |
| `bench/cardinality/main.rs` | CREATE | reports Redis and Postgres cost per fleet at population, to a million on the rig |
| `bench/baselines/*.json` | CREATE | one committed baseline per lane per profile |
| `rustd/Cargo.toml` | EDIT | registers the bench binaries as workspace members |
| `.github/workflows/bench.yml` | EDIT | runs the lanes on the rig and, on dispatch, against a named deployed environment |
| `docs/architecture/scaling.md` | EDIT | records the measured ceilings beside the design they grade |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (every cap, threshold, duration and key name a named constant), **FLL** (four binaries plus a shared harness; split driver from reporter from the first commit), **NDC** (a lane measuring nothing a rubric row reads is dead code), **TST-NAM** (no milestone identifiers in bench source), **ECL** (a datastore that will not answer is unavailable, never a zero reported as a measurement), **PRI** (fixture content is generated, never echoed from a tenant row).
- `dispatch/write_rust.md` — one error type per crate with its `Result` alias, `#[from]` composition, no `map_err` that stringifies its own cause.
- `docs/LOGGING_STANDARD.md` — every emitted line carries a snake_case `event`; measurements go to a file, never to a log line a rubric would parse.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| LENGTH GATE (≤350 file / ≤50 fn / ≤70 method) | yes — a shared harness and four binaries | driver, reporter and fixture live in separate modules from the first commit, not split later under gate pressure |
| UFS GATE | yes — caps, durations, key names, profile names | each is a named constant with a doc line saying what chose the value |
| LOGGING GATE | yes — new `tracing` call sites | `event = "verb_noun"`, fields hoisted into locals, measurements written to a file |
| MILESTONE ID GATE | yes — new source files | no milestone, section or dimension identifiers anywhere in bench source |
| RUST ERR (ERR-RS) | yes — new fallible surfaces | one error type per bench crate with a `Result` alias beside it |
| SCHEMA GUARD | no — reads and writes through existing repositories only | N/A |
| UI GATE / DESIGN TOKEN GATE | no — no TypeScript or UI surface | N/A |

## Prior-Art / Reference Implementations

- **Reference:** `make/bench.mk` — the existing incident lane is the shape to mirror: a make target, a committed baseline, a reproducibility line, and artifacts uploaded by a dispatch-only workflow.
- **Reference:** `ui/packages/app/tests/e2e/acceptance/fixtures/teardown.ts` — the acceptance suite already sweeps fixture fleets by run prefix inside a fixture workspace. The deployed profiles reuse that discipline; the sweep is the reason a production run is acceptable at all.
- **Divergence, named:** the existing Tier-2 loadgen shells out to `hey` against a URL. Only the steer lane is HTTP-shaped, and even it needs per-datastore attribution the loadgen cannot produce, so these lanes drive the types directly.

## Sections (implementation slices)

### §1 — Profiles, caps and the safety rails

One profile decides scale, target and blast radius, so no lane can be pointed somewhere dangerous by a stray parameter. **Implementation default:** three profiles — `rig` (compose datastores, unbounded, the million-fleet target), `dev` (deployed development, small caps because capacity is small), `prod` (deployed production, smallest caps and an explicit acknowledgement variable). A parameter above the active profile's cap is refused before any connection opens.

- **Dimension 1.1** — a lane run above its profile's cap exits non-zero naming the cap, before opening a connection → Test `test_a_parameter_above_the_profile_cap_is_refused`
- **Dimension 1.2** — the production profile refuses to run without its explicit acknowledgement variable set → Test `test_production_requires_an_explicit_acknowledgement`
- **Dimension 1.3** — every deployed run creates only inside a fixture workspace under a unique run prefix, and ends with a sweep whose removed count is reported → Test `test_a_deployed_run_sweeps_everything_it_created`
- **Dimension 1.4** — a run whose observed error rate crosses the profile's abort threshold stops early and reports the abort rather than continuing to load the target → Test `test_a_run_aborts_when_the_target_starts_failing`

### §2 — Steer ingress under concurrency

Establishes how many concurrent steers the system accepts and where each one costs. One steer is a Postgres write, a stream append and a readiness mark, and the readiness index is a single global hash every steer writes — the first shared structure a million fleets contend on.

- **Dimension 2.1** — N concurrent steer submitters report sustained accepted steers per second and a p95 acceptance latency → Test `test_steer_bench_reports_a_rate_and_a_p95`
- **Dimension 2.2** — the same run attributes its cost between Postgres and Redis, as commands issued and time spent in each → Test `test_steer_bench_attributes_cost_between_datastores`
- **Dimension 2.3** — the run reports readiness-index depth over time, so growth outrunning drain is visible as a number rather than as a stall → Test `test_steer_bench_reports_readiness_depth_over_time`

### §3 — Lease issuance under ready-depth and runner concurrency

Establishes leases per second and the Postgres cost of each, at ready-depths and runner counts the deployment has never seen. This is the path a million fleets reach first, because every runner polls it continuously whether or not there is work.

- **Dimension 3.1** — a run with K ready fleets and R concurrent runners reports sustained leases per second and p95 poll latency → Test `test_lease_bench_reports_a_rate_and_a_p95`
- **Dimension 3.2** — the run reports mean and p95 Postgres round trips per issued lease, read from the daemon's own counter → Test `test_lease_bench_reports_roundtrips_per_lease`
- **Dimension 3.3** — with runners outnumbering ready fleets, the wasted-claim rate is reported: polls that sampled a fleet another runner already held → Test `test_lease_bench_reports_wasted_claim_rate`
- **Dimension 3.4** — idle cost is reported separately: the Redis and Postgres commands a poll issues when nothing is ready, which is what a million idle fleets actually cost → Test `test_lease_bench_reports_idle_poll_cost`

### §4 — Delivery ceiling and head-of-line cost

Establishes what one worker sustains and whether one slow destination blocks unrelated ones. **Implementation default:** the stub vendor answers on a per-destination delay drawn from the profile, so a scripted-slow or scripted-failing destination mixes into a healthy population without changing the driver.

- **Dimension 4.1** — a run of queued jobs against a uniformly fast stub reports sustained jobs per second and p95 end-to-end delivery latency → Test `test_outbound_bench_reports_a_rate_and_a_p95`
- **Dimension 4.2** — with one destination scripted slow, the latency of the OTHER destinations is separately reported, making head-of-line blocking a measured quantity → Test `test_outbound_bench_isolates_the_slow_destination_cost`
- **Dimension 4.3** — with one destination scripted retryable, the fraction of the window the worker spent in its retry ladder is reported → Test `test_outbound_bench_reports_retry_occupancy`

### §5 — Cardinality to a million fleets

Establishes what an idle fleet costs when there are a million of them. The per-fleet stream and consumer group design is sound at the scale it has run at; whether it survives six orders of magnitude more is an empirical question with no answer in this repository. **Implementation default:** the `rig` profile creates the full population; the deployed profiles measure the population already present and report the same shape, because creating a million streams in a shared environment is not a measurement anyone consented to.

- **Dimension 5.1** — on the rig, a fleet population reports Redis memory before and after and the per-fleet delta, at each step of a declared ladder up to a million → Test `test_cardinality_bench_reports_memory_per_fleet_across_the_ladder`
- **Dimension 5.2** — with the population present, a readiness peek and a single-stream read report their latency, so cardinality's cost on the hot path is measured rather than assumed → Test `test_cardinality_bench_reports_hot_path_latency_under_cardinality`
- **Dimension 5.3** — the same run reports the Postgres side: table sizes and the candidate query's plan and latency at population → Test `test_cardinality_bench_reports_postgres_cost_at_population`
- **Dimension 5.4** — on a deployed profile the lane observes rather than creates, and says so in its result → Test `test_a_deployed_cardinality_run_creates_nothing`

### §6 — Reporting, attribution and baselines

A number nobody compares against is a number nobody reads, and a number with no datastore attached does not say what to fix. **Implementation default:** a lane fails only on a missing or unreadable result, never on a regression — these run on shared runners and against shared environments where a throughput threshold would be a flake generator; the comparison prints a delta a human reads.

- **Dimension 6.1** — every lane writes one result file carrying lane, profile, parameters, measurements, and a per-datastore breakdown → Test `test_each_lane_writes_a_parseable_result`
- **Dimension 6.2** — a comparison command prints the delta between a result and its per-profile baseline and exits zero regardless of direction → Test `test_the_comparison_reports_a_delta_without_gating`
- **Dimension 6.3** — the dispatch-only workflow runs every lane on the rig, accepts a deployed environment as an input, and uploads every result → Test `test_the_workflow_runs_the_rig_and_accepts_an_environment`

## Interfaces

```
make bench-steer        PROFILE=<rig|dev|prod> [BENCH_CONCURRENCY=<n>] → bench/results/steer.<profile>.json
make bench-lease        PROFILE=<…> [BENCH_FLEETS=<n>] [BENCH_RUNNERS=<n>] → bench/results/lease.<profile>.json
make bench-outbound     PROFILE=<…> [BENCH_JOBS=<n>] [BENCH_SLOW_FRACTION=<0..1>] → bench/results/outbound.<profile>.json
make bench-cardinality  PROFILE=<…> [BENCH_FLEETS=<n>]                 → bench/results/cardinality.<profile>.json
make bench-compare      LANE=<steer|lease|outbound|cardinality> PROFILE=<…> → stdout delta, exit 0

prod profile additionally requires an explicit acknowledgement variable; without it every lane refuses.

result file shape (every lane):
{
  "lane": "lease", "profile": "rig", "created": false,
  "parameters":   { "fleets": 1000000, "runners": 64 },
  "measurements": { "rate_per_second": 0.0, "p95_ms": 0.0 },
  "datastores":   { "redis":    { "commands": 0, "time_ms": 0.0 },
                    "postgres": { "roundtrips": 0, "time_ms": 0.0 } },
  "fixture":      { "run_prefix": "bench-…", "created": 0, "swept": 0 }
}
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Datastore absent | the lane ran without its rig up, or a deployed target is unreachable | exits non-zero naming the datastore and the target that starts it; writes no result |
| Datastore dies mid-run | container restart, managed-service failover | exits non-zero; a partial result is never written, so no truncated run is read as a measurement |
| Target starts refusing | the deployed environment saturates under the lane's own load | the abort threshold stops the run, the result records the abort and the rate reached before it |
| Cap exceeded | a parameter above the active profile's ceiling | refused before any connection opens, naming the cap and the profile |
| Production without acknowledgement | someone ran the prod profile by tab-completion | refused before any connection opens, naming the variable it needs |
| Fixture left behind | the run died between create and sweep | the sweep is idempotent and prefix-scoped; a following run sweeps orphans first and reports the count |
| Baseline missing | a first run on a new lane or profile | the comparison prints absence and exits zero; it never invents one |
| Result unparseable | a partial write, or a shape change without a baseline change | the comparison exits non-zero naming the file |
| Run shorter than warmup | a duration below the profile's warmup floor | exits non-zero; a rate measured across a cold cache is not a rate |

## Invariants

1. A lane reports only what it measured — enforced by writing to a temporary path and renaming onto the result path only on the success branch.
2. Every lane drives production types, never a copy — enforced by the binaries depending on `afd_outbound`, `afd_fleet` and `afd_events` and constructing their types directly; a reimplementation would not compile against those signatures.
3. Nothing a deployed run creates outlives it — enforced by every created object carrying the run prefix and the sweep running on every exit path, with its removed count in the result.
4. No lane gates on a throughput threshold — enforced by the comparison having no non-zero exit for a regression, so a shared-environment slowdown can never be read as a broken build.
5. Postgres round trips per lease come from the daemon's own instrument — enforced by reading `producers::fleet::lease_polled` rather than wrapping queries.
6. A profile's caps bound every lane it runs — enforced by the caps living on the profile the lane resolves before it opens a connection, not on each lane's own parameters.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | the lanes read existing operator gauges and add none | run parameters, measurements, datastore counters, fixture counts | fixture tenancy only; generated content, never a tenant row echoed back | `test_each_lane_writes_a_parseable_result` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_a_parameter_above_the_profile_cap_is_refused` | a fleet count above the dev cap exits non-zero naming the cap, with no connection opened |
| 1.2 | unit | `test_production_requires_an_explicit_acknowledgement` | the prod profile without its variable exits non-zero naming the variable |
| 1.3 | integration | `test_a_deployed_run_sweeps_everything_it_created` | after a bounded run, objects carrying the run prefix number zero and the result's swept count equals its created count |
| 1.4 | integration | `test_a_run_aborts_when_the_target_starts_failing` | with a target scripted to refuse above a rate, the run stops and the result records the abort |
| 2.1 | integration | `test_steer_bench_reports_a_rate_and_a_p95` | a bounded concurrent run reports accepted steers per second above zero and a finite p95 |
| 2.2 | integration | `test_steer_bench_attributes_cost_between_datastores` | the result carries non-zero Redis commands and non-zero Postgres round trips |
| 2.3 | integration | `test_steer_bench_reports_readiness_depth_over_time` | the result carries a depth series with at least two samples |
| 3.1 | integration | `test_lease_bench_reports_a_rate_and_a_p95` | a run with seeded fleets and simulated runners reports leases per second above zero |
| 3.2 | integration | `test_lease_bench_reports_roundtrips_per_lease` | mean round trips per lease is at least one and comes from the daemon's counter |
| 3.3 | integration | `test_lease_bench_reports_wasted_claim_rate` | with runners outnumbering ready fleets, the wasted-claim rate is above zero |
| 3.4 | integration | `test_lease_bench_reports_idle_poll_cost` | with nothing ready, the per-poll Redis command count is reported and the Postgres round trips are zero |
| 4.1 | integration | `test_outbound_bench_reports_a_rate_and_a_p95` | a run against a fast stub reports jobs per second above zero and a finite p95 |
| 4.2 | integration | `test_outbound_bench_isolates_the_slow_destination_cost` | with one destination slow, other destinations' latency is separately reported |
| 4.3 | integration | `test_outbound_bench_reports_retry_occupancy` | with one destination retryable, retry occupancy is above zero |
| 5.1 | integration | `test_cardinality_bench_reports_memory_per_fleet_across_the_ladder` | each ladder step reports a per-fleet memory delta above zero |
| 5.2 | integration | `test_cardinality_bench_reports_hot_path_latency_under_cardinality` | peek latency is reported with the population it was measured at |
| 5.3 | integration | `test_cardinality_bench_reports_postgres_cost_at_population` | the result carries table sizes and the candidate query's latency at population |
| 5.4 | integration | `test_a_deployed_cardinality_run_creates_nothing` | on a deployed profile the result's created flag is false and its fixture created count is zero |
| 6.1 | unit | `test_each_lane_writes_a_parseable_result` | a result from each lane parses and carries lane, profile, parameters, measurements, datastores and fixture |
| 6.2 | unit | `test_the_comparison_reports_a_delta_without_gating` | a result worse than its baseline exits zero and prints the delta with its direction |
| 6.3 | unit | `test_the_workflow_runs_the_rig_and_accepts_an_environment` | the workflow declares an environment input and uploads every result path |
| 6.1 | unit | `test_a_missing_baseline_reports_absence_rather_than_inventing_one` | comparison against an absent baseline prints absence and exits zero |
| 6.1 | unit | `test_an_unparseable_result_fails_the_comparison` | comparison against a truncated result exits non-zero naming the file |
| 1.1–6.3 | unit | `test_a_run_that_cannot_reach_its_datastore_writes_no_result` | with an unreachable datastore the lane exits non-zero and the result path does not exist afterwards |
| regression | integration | `test_the_existing_loadgen_lane_is_unchanged` | the pre-existing bench target still runs against a health fixture, untouched by the new lanes |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every lane reports a rate on the rig (§2–§5) | `make bench-steer PROFILE=rig && make bench-lease PROFILE=rig && make bench-outbound PROFILE=rig && make bench-cardinality PROFILE=rig` | exit 0, four result files written | P0 | |
| R2 | Cost is attributed to a datastore (§2, §3) | `python3 -c "import json;d=json.load(open('bench/results/lease.rig.json'))['datastores'];print(d['redis']['commands'], d['postgres']['roundtrips'])"` | two numbers, both greater than 0 | P0 | |
| R3 | Cardinality reaches the declared population (§5) | `python3 -c "import json;print(json.load(open('bench/results/cardinality.rig.json'))['parameters']['fleets'])"` | the ladder's declared maximum | P0 | |
| R4 | A deployed run creates nothing it does not sweep (§1) | `python3 -c "import json;f=json.load(open('bench/results/lease.dev.json'))['fixture'];print(f['created']==f['swept'])"` | `True` | P0 | |
| R5 | Production refuses without acknowledgement (§1) | `make bench-lease PROFILE=prod; echo $?` | non-zero | P0 | |
| R6 | A regression never fails a lane (§6) | `make bench-compare LANE=lease PROFILE=rig; echo $?` | `0` | P0 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration tier green | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Lint green | `make lint-all` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks git --no-banner` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** every S-row Verify command is copied **verbatim from `.oracle/orly.json`** (`conform`, `verify.*`) — the same set `orly gate` runs, so the rubric and the mechanical PR gate grade one boundary. The gate BLOCKs a staged pending/active spec whose rubric omits the declared `conform` or `verify.unit` command; a rubric naming a runner the repository does not declare is wrong by construction. `.oracle/orly.json` still a seed → complete it first (`dispatch/lifecycle.md` §Bootstrap); authoring against an unseeded config is the nondeterminism this rule exists to kill.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- The refactors these numbers inform: batched outbound reads, bounded-concurrency delivery, per-destination ordering, retry as delayed re-enqueue, per-vendor rate limiting, and any reduction in the lease path's Postgres round trips. This spec produces the evidence; a follow-up spends it.
- A Redis client migration. If connection lifecycle proves to be the recurring fault, that is its own spec with its own blast radius.
- Continuous benchmarking in the merge path. The lanes stay dispatch-only until their variance is known.
- Any change to production capacity, instance counts, or datastore plans. The lanes measure what is there.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an engineer proposing a change to delivery or lease issuance opens two result files, reads the sustained rate, the head-of-line cost and which datastore the time went to, and argues from those numbers instead of from an estimate.
2. **Preserved user behaviour** — everything. No production code path changes; deployed runs stay inside a fixture tenant and sweep what they create.
3. **Optimal-way check** — the most direct route would be measuring production under real customer load, which needs traffic this product does not yet have. Driving the real types, on a rig for the million-fleet question and against deployed environments for the reality check, is the closest honest substitute. The gap is named: the rig measures one process against local datastores, and the deployed profiles are too small to reach saturation.
4. **Rebuild-vs-iterate** — iterate. A benchmark sharing the integration lane's infrastructure is a harness and four binaries; a standalone rig would be a second datastore setup to keep true.
5. **What we build** — a profile system with caps and a sweep, four lanes, per-datastore attribution, result files, committed baselines, a comparison command, and the workflow wiring.
6. **What we do NOT build** — a regression gate, a dashboard, a continuous run, a load generator pointed at customer traffic, million-fleet creation in a shared environment, and every refactor these numbers will inform.
7. **Fit with existing features** — compounds with the compose datastore rig, the acceptance suite's fixture-sweep discipline, and the dispatch-only bench workflow. The one thing it must not destabilize is the deployed environments: a lane that leaves fixtures behind or outruns its caps is worse than no lane.
8. **Surface order** — N/A — no user surface. The output is a file an engineer reads and a workflow artifact.
9. **Dashboard restraint** — N/A — no user surface. Deliberately no dashboard: numbers with no trend behind them would be a chart pretending to be evidence.
10. **Confused-user next step** — a lane that cannot run prints the profile it resolved, the cap it refused, or the make target that starts its datastore, whichever applies.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one section for the safety rails every deployed run depends on, one per measured path, and one for the reporting spine they share. The paths fail differently and at different scales, so each lane is separately landable and separately readable.
- **Alternatives considered:** one end-to-end lane driving the whole daemon. Rejected because a single number tells you the system got slower, not which of four mechanisms did it — which is exactly what a refactor decision needs. Also considered: skipping measurement and refactoring delivery on the strength of the reasoning. Rejected because the reasoning yields an estimated ceiling of roughly five jobs per second per process, and whether that estimate is right or wrong by an order of magnitude decides whether the refactor is urgent or unnecessary.
- **Patch-vs-refactor verdict:** this is a **patch** because it adds measurement beside existing code and changes no behaviour. The refactor it informs is deliberately a separate spec, so the numbers exist before anyone argues about the shape.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
