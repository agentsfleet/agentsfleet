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

# M197_002: The daemon counts its fleets — a census gauge by status and a runs-started counter the operator dashboard can draw

**Prototype:** v2.0.0
**Milestone:** M197
**Workstream:** 002
**Date:** Sep 17, 2026
**Status:** PENDING
**Priority:** P1 — operator-facing; the dashboard M197_001 ships cannot say how many fleets exist or how many runs started, because no family carries either.
**Categories:** DOCS, OBS
**Batch:** B2 — after M197_001, whose dashboard asset and grader this workstream extends.
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M197_001 — its grader requires every produced family to reach a panel, and its dashboard is the asset the two new panels land in.
**Provenance:** LLM-drafted (claude-fable-5-1, Sep 17, 2026), grounded in source reads of `rustd/crates/afd_observability`, `afd_fleet`, `afd_runner` and `afd_fleet_lifecycle`
**Canonical architecture:** `docs/architecture/observability.md` §Metric family census

---

## Overview

**Goal (testable):** `agentsfleet_fleets{status}` publishes one reading per lifecycle status sampled from `core.fleets`, `agentsfleet_fleet_runs_started_total{kind}` increments once per lease granted, both grade against the census in both directions, and the development dashboard draws them.

**Problem:** an operator opening the dashboard cannot answer "how many fleets do we have, in what state, and are runs actually starting". The Rust registry declares sixty families and none is a fleet count. The closest readings are the readiness-index depth and per-runner active leases, which say what is leasable and what is held, never what exists. The one family that sounds like a start counter, `agentsfleet_fleet_triggered_total`, is declared, carried in the census, listed in the `UNPRODUCED` ledger, and incremented nowhere.

**Solution summary:** a supervised census sweeper samples `core.fleets` grouped by status on a fixed cadence and publishes five snapshot cells an observable gauge reads; the lease grant records one counter increment labelled by whether the run is fresh or a reclaim of a lapsed holder; the dead trigger family leaves the registry, the census and the ledger together; the census gains two rows and the label-product test covers both; and the M197_001 dashboard gains a fleet row that reads them. The operator-visible outcome is a top row that says "N fleets, M active, runs starting at R per minute" from measured series.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m197): count fleets by status and runs started, on the dashboard`
- **Intent (one sentence):** an operator sees how many fleets exist in each state and whether runs are starting, from series the daemon measures rather than infers.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_observability/src/metrics/observed.rs` — the snapshot cell. Nothing is read inside a collection callback; a publisher writes on its own cadence and a failed read withdraws rather than publishing zero. The census gauge is five of these.
2. `rustd/crates/afd_observability/src/producers/fleet.rs` — the fleet producer: the `Handles` struct, the observable-gauge registration in `claim`, and the `*_observed` publish functions the new cells mirror. `producers/fleet/admission.rs` is the labelled-counter shape.
3. `rustd/crates/afd_runner/src/sweep/mod.rs` and `sweep/retention.rs` — the `Sweep` trait and a sweeper that owns its interval and its statements; `rustd/crates/agentsfleetd/src/sweepers.rs` is the only place a sweeper is spawned, and `tests/integration_serve.rs` asserts that inventory.
4. `rustd/crates/afd_fleet/src/lease/assign.rs` — `try_candidate` is the single grant point: a reclaim of a lapsed holder returns early, a fresh read follows. Both are runs started; they carry different labels.
5. `rustd/crates/afd_observability/src/metrics/label/fleet.rs` and `label/tests.rs` — the `closed_set!` macro and `label_products`, which the census ceiling test reads. Every labelled family added here gets a row there.
6. `docs/metrics.census.tsv` — the single source of truth for the export; the registry test grades it against the declared families in both directions, so a family cannot exist without a row or a row without a family.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_observability/src/metrics/declared/fleet.rs` | EDIT | declares `FLEETS` and `FLEET_RUNS_STARTED_TOTAL`; `FLEET_TRIGGERED_TOTAL` leaves. |
| `rustd/crates/afd_observability/src/metrics/label/fleet.rs` | EDIT | two closed sets: the five lifecycle statuses, and the two run-start kinds. |
| `rustd/crates/afd_observability/src/metrics/label/tests.rs` | EDIT | `label_products` gains both families so the ceiling test covers them. |
| `rustd/crates/afd_observability/src/semconv.rs` | EDIT | the `status` and `kind` label keys, named once. |
| `rustd/crates/afd_observability/src/producers/fleet.rs` | EDIT | five census cells, the observable gauge over them, the runs-started counter handle, and the two publish functions. |
| `rustd/crates/afd_observability/src/metrics/produced.rs` | EDIT | the trigger family's `UNPRODUCED` entry leaves with the family. |
| `rustd/crates/afd_fleet_lifecycle/src/lib.rs` | EDIT | a test asserting `FleetStatus` and the label set agree member for member. |
| `rustd/crates/afd_fleet/src/lease/assign.rs` | EDIT | records the grant, labelled fresh or reclaimed. |
| `rustd/crates/afd_runner/src/sweep/census.rs` | CREATE | the census sweeper: one grouped count, five cells published, all five withdrawn on a failed read. |
| `rustd/crates/afd_runner/src/sweep/mod.rs` | EDIT | the module line. |
| `rustd/crates/afd_runner/src/sql/sweep.rs` | EDIT | the schema-qualified grouped count, beside retention's statements. |
| `rustd/crates/afd_runner/src/sweep/tests.rs` | EDIT | the sweeper's unit rows. |
| `rustd/crates/agentsfleetd/src/sweepers.rs` | EDIT | spawns the census sweeper under the supervisor, with its named constant. |
| `rustd/crates/agentsfleetd/tests/integration_serve.rs` | EDIT | the supervisor inventory assertion gains the new name. |
| `docs/metrics.census.tsv` | EDIT | two rows added, one removed. |
| `docs/architecture/observability.md` | EDIT | the sentence naming the trigger family as declared-but-unemitted goes; the fleet census sweeper is named where the sweepers are. |
| `playbooks/operations/observability/providers/grafana/assets/dashboard.json` | EDIT | a fleet row: fleets by status, runs started by kind, pickup ratio against appended admissions. |
| `playbooks/operations/observability/observability_test.sh` | EDIT | the new panels get self-test rows. |
| `docs/v2/pending/M197_002_P1_DOCS_OBS_FLEET_CENSUS_AND_RUN_START_FAMILIES.md` | CREATE | this spec. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NDC** (the trigger family is dead code at the registry, the census and the ledger; it leaves in the same diff that adds its replacement), **NLR** (touching the declared module means the dead family does not survive it), **UFS** (the sweeper interval, the label keys and every status spelling are named constants; the SQL statement is a named constant in the crate's `sql` module), **NSQ** (`core.fleets` is schema-qualified in the statement), **ORP** (the removed family's wire name is grepped to zero across `rustd/`, `docs/` and `playbooks/`), **TST-NAM** (no milestone identifier in a test name), **TCF** (the label-agreement test must fail when a status is added to one side only).
- **`dispatch/write_rust.md`** — every edited file is `*.rs`; preserved error variants (`afd_runner::error` composes with `#[from]`, the sweeper adds no new kind) and deterministic tests (the census sweeper is tested against a seeded table, never a sleep).
- **`docs/RUST_ERROR_STANDARD.md`** — the sweeper's statement failure lifts through the crate's existing `query` context helper; no `map_err` to a string.
- **`docs/LOGGING_STANDARD.md`** — the sweeper's pass is logged by the `Sweep` loop; a status string the closed set does not carry is logged at `warn` with the spelling, once per pass, and never becomes a label.
- **`dispatch/write_shell.md`** — `observability_test.sh` gains rows; quoted expansions and array arguments apply.
- **`dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md`** — the architecture doc edit is published prose.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| DOC READ GATE | yes — the Rust façade, the shell façade, the logging standard and the architecture doc all trigger | one `📖 DOC READ:` proof-line per triggered document per turn, logged through `audits/doc-read.sh log` with the applied section cited. |
| UFS GATE | yes — interval, label keys, status spellings, the SQL statement | every one is a named constant; the status spellings are the closed set's, declared once. |
| LENGTH GATE | yes — `producers/fleet.rs` is the widest file touched | the census cells and their gauge go in a `producers/fleet/census.rs` submodule if the parent approaches the cap, mirroring `admission.rs`. |
| MILESTONE-ID GATE | yes — commits carry `m197`; no production identifier does | `m197` on commit subjects only; test names and constants stay milestone-free. |
| LOGGING GATE | yes — one new `warn` in the sweeper | the event name is a constant; fields are hoisted before the macro per the standard's coverage note. |
| SCHEMA GUARD | no — `core.fleets` is read, never altered | N/A. |
| UI GATE / DESIGN TOKEN GATE | no — no `*.ts`/`*.tsx` in the blast radius | N/A. |
| ZIG GATE / PUB GATE | no — no `*.zig` edited | N/A. |
| File & Function Length (≤350/≤50/≤70) | yes for `producers/fleet.rs` and `sweepers.rs` | split by concern as above; `sweepers.rs` gains one constant and one spawn. |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_observability/src/producers/fleet.rs` — the `READY_DEPTH` cell and its gauge registration are the exact shape the census follows, five times over with a label each. Followed exactly; the only divergence is that a census publishes a zero for a status the query did not return, because a successful grouped count that omits a status has measured zero of them.
- **Reference:** `rustd/crates/afd_runner/src/sweep/retention.rs` — a sweeper that owns its interval and its statements and reports through `Swept`. Followed for structure; diverged from on pacing, because a census has no backlog to drain and never shortens its own gap.
- **Reference:** `rustd/crates/afd_observability/src/producers/fleet/admission.rs` — the labelled counter recorded through `installed()`. Followed exactly for the runs-started counter.
- **Reference:** `rustd/crates/afd_observability/src/runner.rs` `last_seen_readings` — labelled readings built from atomics, never from a lock or a datastore. Followed: the census callback loads five cells and allocates five readings.

## Sections (implementation slices)

### §1 — The census gauge: five cells a sweeper fills

`agentsfleet_fleets{status}` is an observable gauge whose callback loads five snapshot cells, one per `FleetStatus` member, and publishes a labelled reading for each cell that is valid. A supervised sweeper fills them: one schema-qualified `SELECT status, COUNT(*) FROM core.fleets GROUP BY status`, every status the closed set carries mapped from the result, statuses absent from the result published as zero, and the whole set withdrawn when the statement fails. **Implementation default:** the sweeper lives in `afd_runner::sweep` beside the others because the `Sweep` trait and the supervisor spawn are there, and it depends only on `afd_db` and `afd_observability`, both already dependencies of that crate. The interval is a named constant of thirty seconds: a fleet count changes on install and edit, both operator-paced, and a tighter cadence would spend a Postgres round trip on a number that did not move.

- **Dimension 1.1** — a seeded table with two active, one paused and one stopped fleet publishes readings `2`, `1`, `1`, and `0` for the two statuses the table lacks → Test `test_census_publishes_every_status`
- **Dimension 1.2** — a failed statement withdraws all five cells and the callback publishes no reading → Test `test_census_withdraws_on_failed_read`
- **Dimension 1.3** — a status string the closed set does not carry is logged once and produces no label; the four known statuses still publish → Test `test_census_refuses_an_unknown_status`
- **Dimension 1.4** — the sweeper is spawned under the supervisor with its own name and the inventory assertion names it → Test `test_census_sweeper_is_supervised`
- **Dimension 1.5** — the gauge's ceiling in the census equals the closed set's length, graded by the label-product test → Test `every_declared_ceiling_admits_its_label_product`

### §2 — Runs started, counted at the one grant point

`agentsfleet_fleet_runs_started_total{kind}` increments in `try_candidate` on each `Some` it returns: `reclaimed` when a lapsed holder's event is re-leased under a higher fence, `fresh` when a new entry is read. A poll that returns `None` records nothing, because nothing started. **Implementation default:** two label values rather than two families, because an operator reads them as one line split by cause — a rising `reclaimed` share is runners dying mid-run, which is the first thing the split has to make visible — and a single family keeps `sum()` honest as the count of runs that began.

- **Dimension 2.1** — a fresh grant increments `kind="fresh"` exactly once → Test `test_fresh_grant_counts_one_run`
- **Dimension 2.2** — a reclaim over a lapsed holder increments `kind="reclaimed"` exactly once and `fresh` not at all → Test `test_reclaim_counts_as_reclaimed`
- **Dimension 2.3** — a poll that finds nothing leasable, and one whose claim loses to a live holder, increment neither → Test `test_empty_poll_starts_no_run`

### §3 — The trigger family retires

`agentsfleet_fleet_triggered_total` is declared, carried in the census, listed in `UNPRODUCED`, and incremented nowhere. Its `watch_for` line, "trigger volume", is `agentsfleet_admissions_total{outcome="appended"}` under another name: every trigger this daemon accepts is an admission, and the appended outcome is the one that reached the stream. A second family for the same count would be a second line an operator has to reconcile. It leaves from the declared module, the census, the ledger and the architecture doc in this diff. The PostHog `FleetTriggered` product event is a different thing on a different path and is not touched.

- **Dimension 3.1** — the wire name is absent from `rustd/`, `docs/` and `playbooks/` after the diff → Test `test_trigger_family_is_gone`
- **Dimension 3.2** — the `UNPRODUCED` ledger no longer carries it and the registry still grades both directions clean → Test `test_ledger_and_census_agree_after_retirement`

### §4 — The census and the registry move together

Two rows enter `docs/metrics.census.tsv` in census order beside the fleet families, one leaves, and the registry test that grades the file against the declared families passes in both directions. The label-product test covers both new families. `FleetStatus` in `afd_fleet_lifecycle` and the status label set in `afd_observability` cannot share a type, because the lifecycle crate depends on the observability crate and not the other way round, so a test in the lifecycle crate asserts the two agree member for member and fails on a status added to either side alone.

- **Dimension 4.1** — the census carries `agentsfleet_fleets` as a `u64` gauge with `status`, `fixed:5`, `live_read yes`, and `agentsfleet_fleet_runs_started_total` as a cumulative `u64` counter with `kind`, `fixed:2` → Test `test_census_rows_match_declarations`
- **Dimension 4.2** — adding a sixth `FleetStatus` member without the label fails the agreement test, and the reverse does too → Test `test_status_label_set_mirrors_lifecycle`
- **Dimension 4.3** — the readiness-depth gauge still publishes from the lease poll and the census never touches its cell → Test `test_ready_depth_gauge_unchanged`

### §5 — On the dashboard, where the operator looks first

On the M197_001 asset, a fleet row at the top: fleets by status as a stat per status, runs started per minute by kind, and the pickup ratio `fresh runs ÷ appended admissions` in the complement form M197_001 fixed for its ratios, so an idle deployment reads `1` and never "No data". The grader M197_001 ships requires every produced family to reach a panel; these two reach theirs here, in the same diff that produces them, so the grader never sees a produced family without a panel. **Implementation default:** the pickup ratio's numerator is `kind="fresh"` only, because a reclaim is a run that already started once and counting it again would let a dying runner inflate the ratio.

- **Dimension 5.1** — a panel targets `agentsfleet_fleets` by status and one targets `agentsfleet_fleet_runs_started_total` by kind; the asset grader passes → Test `test_fleet_row_reads_both_families`
- **Dimension 5.2** — the pickup ratio reads `1` with no series on either side and a ratio in `[0,1]` otherwise → Test `test_pickup_ratio_guards_empty`
- **Dimension 5.3** — every panel the M197_001 asset carried keeps its identifier and family → Test `test_prior_panels_survive`

## Interfaces

```
New families (docs/metrics.census.tsv rows, byte-exact wire names):
  agentsfleet_fleets                     gauge   u64  {fleet}  -           status  -  fixed:5  yes  traffic
    watch_for: installed fleets by lifecycle status; active is the leasable
               set, a rising paused count is the anomaly gate holding work
  agentsfleet_fleet_runs_started_total   counter u64  1        cumulative  kind    -  fixed:2  no   traffic
    watch_for: leases granted; fresh ÷ appended admissions is pickup, a
               rising reclaimed share is runners dying mid-run

Removed family:
  agentsfleet_fleet_triggered_total      (declared, census, UNPRODUCED — all three)

Label sets (closed, afd_observability::metrics::label::fleet):
  status: installing | active | paused | stopped | killed   (FleetStatus::as_str spellings)
  kind:   fresh | reclaimed

Publish surface (afd_observability::producers::fleet):
  fleet_census_observed(counts: &[(FleetStatusLabel, u64)])   all five, or withdraw
  fleet_census_withdrawn()
  run_started(kind: RunStart)

Sweeper (afd_runner::sweep::census, supervised as "sweeper:fleet-census"):
  interval FLEET_CENSUS_INTERVAL = 30s (named constant)
  statement sql::sweep::FLEET_CENSUS: SELECT status, COUNT(*) FROM core.fleets GROUP BY status

Dashboard pickup ratio (same complement form as M197_001 §2):
  (1 - ((sum(agentsfleet_admissions_total{outcome="appended"})
         - (sum(agentsfleet_fleet_runs_started_total{kind="fresh"}) or vector(0)))
        / clamp_min(sum(agentsfleet_admissions_total{outcome="appended"}), 1))) or vector(1)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Statement fails | Postgres unreachable or the pool exhausted during a census pass | all five cells withdrawn; the gauge publishes nothing; the `Sweep` loop logs `sweep_failed`; the next pass republishes. The operator sees a gap, never a zero. |
| Unknown status in the table | a row carries a status string the closed set does not spell | the row's count is dropped and logged at `warn` once per pass with the spelling; known statuses still publish. No label is invented. |
| Sweeper not spawned | a refactor drops the spawn line | the supervisor inventory test in `integration_serve.rs` fails on the missing name. |
| Grant counted twice | a future refactor records inside both `from_reclaim` and `try_candidate` | Dimensions 2.1 and 2.2 assert exactly one increment per grant. |
| Census row drifts from declaration | a row's kind, unit or ceiling disagrees with the declared family | the registry test grades both directions at unit time and fails. |
| Ceiling under the label product | `fixed:5` edited below the closed set's length | `every_declared_ceiling_admits_its_label_product` fails; the SDK overflow bucket never fires in production. |
| Status sets diverge | a lifecycle status is added without its label, or the reverse | the agreement test in the lifecycle crate fails. |
| Panel for an unproduced family | the panels land before the producer, or the producer is reverted alone | the M197_001 grader's produced-families check fails the asset; the diff carries both or neither. |

## Invariants

1. A census reading is never a zero from a failed read — enforced by `Observed`: only `publish` sets the valid flag, and the sweeper's failure path calls `withdraw` on every cell.
2. No callback touches a datastore — enforced by construction: the gauge callback loads atomics and allocates readings; the statement runs only in the sweeper's pass.
3. Every label value is one the closed set spells — enforced by the `closed_set!` types: the sweeper maps a status string through the set and drops what it cannot map; the grant records an enum member.
4. The census and the declared registry agree in both directions — enforced by the registry test that reads the compiled-in census.
5. The lifecycle status set and the label set agree member for member — enforced by the agreement test in `afd_fleet_lifecycle`.
6. The removed family exists nowhere — enforced by the orphan grep in the Dead Code Sweep and the registry test, which would fail on a census row with no declaration.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `agentsfleet_fleets` | ops | every census pass, one reading per lifecycle status | `status` from the closed set | no workspace, fleet or tenant identifier; counts only | `test_census_publishes_every_status` |
| `agentsfleet_fleet_runs_started_total` | ops | each lease granted in `try_candidate` | `kind` ∈ {fresh, reclaimed} | no runner, fleet or event identifier on the metric; those stay on the debug log line that already exists | `test_fresh_grant_counts_one_run` |
| `agentsfleet_fleet_triggered_total` | ops | removed — never fired | — | — | `test_trigger_family_is_gone` |

No product analytics event is added, renamed or removed; the PostHog `FleetTriggered` declaration is untouched.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_census_publishes_every_status` | a table seeded 2 active, 1 paused, 1 stopped → readings `active=2, paused=1, stopped=1, installing=0, killed=0`. |
| 1.2 | unit | `test_census_withdraws_on_failed_read` | an injected statement error → every cell `load()` is `None`; the callback returns an empty vector. |
| 1.3 | unit | `test_census_refuses_an_unknown_status` | a result row `("archived", 3)` → no reading carries `archived`; one `warn` with that spelling; the other statuses publish. |
| 1.4 | integration | `test_census_sweeper_is_supervised` | the supervisor inventory after boot contains `sweeper:fleet-census`. |
| 1.5 | unit | `every_declared_ceiling_admits_its_label_product` | `agentsfleet_fleets` ceiling ≥ 5, `agentsfleet_fleet_runs_started_total` ceiling ≥ 2. |
| 2.1 | integration | `test_fresh_grant_counts_one_run` | one ready fleet with one entry, one poll → `fresh` +1, `reclaimed` +0. |
| 2.2 | integration | `test_reclaim_counts_as_reclaimed` | a lapsed holder's active lease, one poll → `reclaimed` +1, `fresh` +0. |
| 2.3 | integration | `test_empty_poll_starts_no_run` | an empty readiness index, and a claim lost to a live holder → both labels +0. |
| 3.1 | unit | `test_trigger_family_is_gone` | `grep -rn agentsfleet_fleet_triggered_total rustd/ docs/ playbooks/` matches nothing outside this spec and `done/`. |
| 3.2 | unit | `test_ledger_and_census_agree_after_retirement` | `UNPRODUCED` has eleven entries; the registry grades the census clean in both directions. |
| 4.1 | unit | `test_census_rows_match_declarations` | the two rows carry the kind, number, unit, labels, policy and `live_read` the Interfaces block pins. |
| 4.2 | unit | `test_status_label_set_mirrors_lifecycle` | `FleetStatus` members and `FleetStatusLabel::ALL` are equal as sets of spellings; a seeded extra member on either side fails. |
| 5.1 | unit | `test_fleet_row_reads_both_families` | the asset grader passes and both wire names appear in panel targets. |
| 5.2 | unit | `test_pickup_ratio_guards_empty` | the ratio expression carries the `or vector(1)` fallback and the fresh-only numerator. |
| 5.3 | unit | `test_prior_panels_survive` | every panel identifier and family from the M197_001 asset is present after the diff. |
| 4.3 | unit | `test_ready_depth_gauge_unchanged` | regression: `agentsfleet_fleet_ready_depth` still publishes from the lease poll; the census does not touch its cell. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Both families are declared and the census grades clean both ways (§1, §2, §4) | `cargo test -p afd_observability -- registry label` | exit 0 | P0 | |
| R2 | The census sweeper is supervised and publishes (§1) | `cargo test -p agentsfleetd --test integration_serve -- sweepers` | exit 0, `sweeper:fleet-census` in the inventory | P0 | |
| R3 | The trigger family exists nowhere (§3) | `grep -rn 'agentsfleet_fleet_triggered_total' rustd/ docs/ playbooks/ \| grep -v 'docs/v2/'` | no output | P0 | |
| R4 | The asset grader passes with the fleet row (§5) | `OBS_ENV=dev bash playbooks/operations/observability/providers/grafana/assets_check.sh` | exit 0 | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration lane green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| grep -v '\.json$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes, so recording those results does not require another code commit and suite run. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may also be **MOVED** — see below.

**A P0 whose SCOPE moves is not a P0 shipped red.** Met and unmet are not the only two states a criterion has, and a gate that pretends otherwise forces an agent to invent a third. One did, twice in a day, before this clause existed.

A deferral and a transfer are different claims. A **deferral** leaves work unowned inside a closed spec, which is what the P0 gate exists to prevent — the P1 quote is as far as that goes. A **transfer** moves the criterion whole: its Dimensions, its verification and its rubric row land in a named successor spec that carries them as its own P0. Nothing is less owned afterwards; it is owned somewhere else.

Mark such a row `MOVED to M{N}_{NNN} R{n}` and it is not ❌, on three conditions, all of which must hold:

1. The successor spec **exists** and carries the criterion as a rubric row of its own. A successor that does not carry the row is a deferral wearing a new word, and fails the gate as before.
2. Both specs record the mapping — the closing spec names where each Dimension went, the successor names what it inherited. One-sided assertion is not a transfer.
3. Discovery carries the **owner's verbatim quote** authorising it, in the deferral format. An agent-authored transfer is agent-authored scope reduction.

A MOVED row is never rendered ✅. The criterion has not been met; it has changed owner, and the rubric says which.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `FLEET_TRIGGERED_TOTAL` | `grep -rn "FLEET_TRIGGERED_TOTAL" rustd/ \| head` | 0 matches |
| `agentsfleet_fleet_triggered_total` | `grep -rn "agentsfleet_fleet_triggered_total" rustd/ docs/ playbooks/ \| grep -v 'docs/v2/' \| head` | 0 matches |

## Out of Scope

- **A per-workspace fleet count.** A `workspace` label is customer-supplied cardinality with no admission bound, which is the one thing the census policy refuses. Per-workspace counts belong to the API's list endpoint, not to a metric.
- **A trigger-to-first-run latency histogram.** The event row carries `created_at` and the lease carries its grant time, so the pair is measurable, but it is a new histogram with bounds to choose and belongs to a latency workstream once pickup is visible at all.
- **Enabling any alert on the new families.** Panels only; a rule on fleet counts needs a distribution first, as M197_001 §3 says for its own.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy opens the development dashboard and the top row says how many fleets exist, how many are active, and that runs are starting, without asking an agent to run a query.
2. **Preserved user behaviour** — every family the daemon already exports keeps its name, labels and cadence; the readiness-index depth and the lease-poll cost panels are untouched; the three-command playbook sequence works unchanged.
3. **Optimal-way check** — the direct route is a count sampled from the table that owns the truth, published through the snapshot-cell rule every other gauge follows. A transition-driven counter would drift from the table on every crash; a query inside the callback would stall the pipeline. This is the shape, not a compromise.
4. **Rebuild-vs-iterate** — iterate. The producer, cell, sweeper and grader patterns are all shipped and sound; this adds two families through them and removes one that was never real.
5. **What we build** — one sweeper, five cells and a gauge, one labelled counter at the grant, two census rows, one retirement, a fleet row on the dashboard.
6. **What we do NOT build** — a per-workspace label, a pickup-latency histogram, alerts on the new families, any change to how fleets change state.
7. **Fit with existing features** — compounds with M197_001's pickup-latency and admission SLIs: the same appended-admissions denominator now has a started-runs numerator. The one thing it must not destabilize is the rule that a failed read is a gap, never a zero.
8. **Surface order** — operator-surface-first; no Command-Line Interface or web surface changes.
9. **Dashboard restraint** — counts and a ratio only; no alert rules, and the pickup ratio reads `1` on an idle deployment rather than claiming a failure nobody measured.
10. **Confused-user next step** — an operator seeing a gap in the census line reads the sweeper's `sweep_failed` log line, which names the statement; an operator seeing a status missing from the row reads the `warn` that names the unknown spelling.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five Sections in dependency order — the gauge and the counter are independent producers, the retirement is the touch-it-fix-it the declared module forces, the census-and-registry Section is the proof that binds them, and the dashboard lands last because its grader refuses a produced family without a panel and a panel without a producer.
- **Alternatives considered:** (a) increment and decrement a gauge on every lifecycle transition — rejected because a crash between the row write and the gauge update drifts the count forever and nothing reconciles it; (b) give `agentsfleet_fleet_triggered_total` a producer at the admission append — rejected because it would be `admissions_total{outcome="appended"}` under a second name; (c) fold this into M197_001 — rejected because that spec is a read-only assets change at its line budget, and a Rust producer change in the same review as a thirty-panel asset rewrite is two reviews wearing one number.
- **Patch-vs-refactor verdict:** this is a **patch** because every mechanism it uses already ships; the only new code is one sweeper, and the refactor it declines — event-driven counts — is wrong on its own terms, not merely deferred.

## Discovery (consult log)

- **Consults** — Architecture: `docs/architecture/observability.md` §Metric family census and `docs/metrics.census.tsv` read at authoring; the census's `fixed:<n>` policy and the label-product test fix the ceilings at 5 and 2. Source-verified at authoring: `FleetStatus` has five members (`afd_fleet_lifecycle/src/lib.rs`); `try_candidate` is the single grant point with a reclaim early-return (`afd_fleet/src/lease/assign.rs`); `Observed` withdraws on failure and never publishes zero for one (`afd_observability/src/metrics/observed.rs`); `agentsfleet_fleet_triggered_total` appears in the declared module, the census and `UNPRODUCED` and in no call site; the supervisor inventory is asserted in `agentsfleetd/tests/integration_serve.rs`. Indy's direction, Sep 17, 2026: "It doesnt tell me how many fleets are running and so on" and, choosing the widened option, "i only gave few examples but we have lot more metrics that needs to be dashboarded which makes sense to operator" — the dashboard breadth is M197_001's; the producers are this workstream's.
- **Metrics review** — two operator families added, one retired; no product analytics event changes, so no analytics or funnel playbook update is required.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test` at VERIFY per Section and at the boundary, `/orly-write-integration-test` at the boundary (the sweeper and the grant cross a datastore boundary), `/review` at REVIEW, `orly-babysit-prs` after each push.
- **Deferrals** — none. The Out of Scope items are scope boundaries recorded at authoring, not deferrals of committed work.
