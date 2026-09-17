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

# M197_001: Grafana reads the dev fleet — heartbeat repaired, three proven Service Level Indicators, one applied dashboard

**Prototype:** v2.0.0
**Milestone:** M197
**Workstream:** 001
**Date:** Sep 17, 2026
**Status:** DONE
**Priority:** P0 — the shipped `runner-silent` alert fires permanently by construction, and no agentsfleet dashboard has ever been applied to any Grafana stack.
**Categories:** DOCS, INFRA, OBS
**Batch:** B1 — no concurrent workstream; the observability assets are edited by nothing else.
**Branch:** `feat/m197-slo-dashboard`
**Baseline revision:** `d229f568abb48724ac77425c6f45c119d16243ea`
**Test Baseline:** unit 2713 passed / 0 failed / 513 ignored (`make test-unit-all`) · integration 493 passed / 0 failed (`make test-integration-rustd`, live Postgres + Dragonfly) · lint exit 0 · version 0.48.0. Measured on this branch at `0145ebbac` via `orly gate pr`.
**Baseline evidence:** `orly gate pr` on `feat/m197-slo-dashboard`; its `cmd.verify.*` rows all exit 0. Comparison revision `d229f568a` carried no observability change, so the delta is the whole of this branch's test growth: +11 unit tests.
**Depends on:** none
**Provenance:** LLM-drafted (claude-opus-5, Sep 17, 2026), grounded in live reads of the development Grafana stack and `rustd/crates/afd_observability` source
**Canonical architecture:** `docs/architecture/observability.md` §Metric family census

---

## Overview

**Goal (testable):** the `agentsfleet-runtime-dev` dashboard exists in the development Grafana stack carrying the six alert rules, counted by the grader's named constant rather than a literal, with error-budget burn shipped as PANELS, every panel resolves against a family `rustd/crates` actually produces, `runner-silent` fires only when a runner is genuinely overdue, and three Service Level Indicators (SLIs) carry targets derived from measured series rather than invented numbers.

**Problem:** an operator has nothing to look at. The repository carries a nine-panel dashboard and six alerts that have never been applied — a live read of the stack returns one folder (`GrafanaCloud`), thirteen stock dashboards, and zero provisioned alert rules. Two of the shipped assets are wrong in ways that would have been discovered on the first page: the runner heartbeat panel renders an epoch timestamp as an age, and the `runner-silent` rule compares that same epoch against ninety seconds, so it alerts continuously for every runner that is working correctly.

**Solution summary:** repair the two heartbeat readings at the asset layer, add SLI and error-budget panels built only on families the Rust registry produces, add an operator view whose subject is loss and saturation, label the families that have no producer as visibly unproven rather than silently empty, write the SLI and Service Level Objective (SLO) definitions into the canonical architecture doc, and apply the result to the development stack so the dashboard is a live page rather than a JSON asset. The operator-visible outcome is a dashboard that answers "is the fleet healthy" and "is this dashboard blind" on one screen, and an alert set whose firing means something.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m197): repair the runner heartbeat readings and ship an SLO dashboard`
- **Intent (one sentence):** an operator opening Grafana for development sees fleet health, error budget, and the dashboard's own blind spots, and is paged only when a runner is actually silent.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_observability/src/metrics/produced.rs` — the `UNPRODUCED` ledger names every declared family this build has no producer for, with a sentence each. It is the authority on which panels are impossible, and it is why a family absent from Mimir is not automatically a gap.
2. `docs/metrics.census.tsv` — the single source of truth for the export; the `category` and `watch_for` columns already carry the RED/USE taxonomy and one line of operator meaning per family. Panels express the `watch_for` line, never the raw series.
3. `rustd/crates/afd_observability/src/runner.rs` — `last_seen_readings()` returns `last_seen_ms / MILLIS_PER_SECOND`, a Unix epoch in seconds. This is the source claim the heartbeat repair rests on.
4. `playbooks/operations/observability/providers/grafana/assets_check.sh` — the asset grader: minimum panel count, unique panel identifiers, every target expression containing `agentsfleet_`, pinned datasource, exactly the alert count its named constant declares, and every `agentsfleet_*` token greppable in `rustd/crates`.
5. `docs/architecture/observability.md` §Metric family census — canonical for the export path; the SLO definitions land beside the census legend they extend.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `playbooks/operations/observability/providers/grafana/assets/dashboard.json` | EDIT | gains the SLI, burn-rate, operator and producer-gap panels; panel 6 stops rendering an epoch as an age. |
| `playbooks/operations/observability/providers/grafana/assets/alerts.json` | EDIT | `runner-silent` becomes an age comparison; the set grows by the burn-rate rules. |
| `playbooks/operations/observability/providers/grafana/assets_check.sh` | EDIT | the minimum panel count moves with the new scope and four invariants become checks; the grader stays a grader. |
| `playbooks/operations/observability/providers/grafana/common.sh` | EDIT | gains the replay-floor derivation and the ONE dashboard/alert renderer the apply and the drift check now share. |
| `playbooks/operations/observability/providers/grafana/resources.sh` | EDIT | renders through the shared function instead of its own substitution copy. |
| `playbooks/operations/observability/providers/grafana/resource_verify.sh` | EDIT | same renderer; reads dashboard identity from `metadata.name`, and tolerates a panel that queries nothing. |
| `playbooks/operations/observability/providers/grafana/alerts.sh` | EDIT | same shared alert renderer. |
| `rustd/Cargo.lock` · `bun.lock` · `cli/bun.lock` · four `package.json` files | EDIT | dependency refresh Indy asked for in-session; every bump is inside its existing range. |
| `playbooks/operations/observability/observability_test.sh` | EDIT | new asset expectations get a self-test row. |
| `playbooks/operations/observability/observability_assets_test.sh` | CREATE | asset CONTENT tests, split out when the original suite passed the length cap. |
| `playbooks/operations/observability/observability_refusals_test.sh` | CREATE | grader REFUSAL tests — each breaks a copy of the assets one way and requires rejection by name. |
| `playbooks/operations/observability/observability_test_support.sh` | CREATE | the stubs, helpers and the one parallel runner all four suites share. |
| `rustd/crates/afd_fleet/src/lease/assign/diagnostics.rs` | CREATE | what the assignment pass says when it cannot proceed, split from `assign.rs` when workstream 002 pushed it past the cap. |
| `rustd/crates/afd_fleet/src/lease/assign.rs` | EDIT | its diagnostics leave; 369 → 279. |
| `playbooks/operations/observability/observability_verify_test.sh` | EDIT | the drift check's stub dashboard tracks the real asset shape. |
| `playbooks/operations/observability/001_playbook.md` | EDIT | Acceptance gains the SLO rows and the shared-tenant warning an operator must read before applying to production. |
| `docs/metrics.census.tsv` | EDIT | arrives with M197_002 merged into this branch: two produced families added, the never-incremented trigger counter retired. Not edited by this workstream's own commits. |
| `docs/architecture/observability.md` | EDIT | new §Service Level Objectives section: the three SLIs, their expressions, their targets and the provenance of each target. |
| `docs/v2/pending/M197_001_P0_DOCS_INFRA_OBS_SLO_DASHBOARD_AND_HEARTBEAT_REPAIR.md` | CREATE | this spec. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (the runner offline threshold and every burn-rate window are named constants derived from source, never literals pasted into a shell script), **NDC** (no panel for a family with no producer unless it is explicitly a declared-gap panel), **ORP** (the panel identifier renumbering must leave no dangling `panelId` in `alerts.json`), **FLL** (the asset JSON files are data, but the shell graders that read them stay inside the function and file caps).
- **`dispatch/write_shell.md`** — the three grader scripts are `*.sh`; quoted expansions, array arguments and temp-file cleanup apply to every edit in them.
- **`dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md`** — the architecture doc edit is published prose and reads the documentation rules before the narrower guides.
- **`dispatch/name_architecture.md`** — the SLO section names durable operator concepts; the architecture doc wins over anything this spec infers.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| DOC READ GATE | yes — `docs/architecture/observability.md` and the shell façade both trigger | one `📖 DOC READ:` proof-line per triggered document per turn, logged through `audits/doc-read.sh log` with the applied section cited. |
| UFS GATE | yes — thresholds and windows enter the assets | every threshold is a named placeholder the apply step substitutes from source (the `__RUNNER_OFFLINE_SECONDS__` pattern already in `common.sh`), never a pasted number. |
| LENGTH GATE | yes — `assets_check.sh` grows | the grader's new expectations go in a named function; the file stays under the cap or splits by concern. |
| MILESTONE-ID GATE | yes — new asset and doc content | `m197` identifiers on the commits and any new log or comment anchor. |
| SCHEMA GUARD | no — no `schema/*.sql` in the blast radius | N/A. |
| UI GATE / DESIGN TOKEN GATE | no — no `*.ts`/`*.tsx` in the blast radius | N/A. |
| ZIG GATE / PUB GATE | no — no `*.zig` edited; `constants.zig` is read, never written | N/A. |
| File & Function Length (≤350/≤50/≤70) | yes for the shell graders | split the asset grader's checks into named functions rather than growing one block. |

## Prior-Art / Reference Implementations

- **Reference:** the Google Site Reliability Engineering (SRE) workbook's multiwindow, multi-burn-rate alerting shape — two windows per severity, long window for the budget claim and short window for the reset. Followed for the burn-rate panels and rules; diverged from on thresholds, because the workbook's 2%/5%-of-budget numbers assume a request stream this deployment does not have.
- **Reference:** `playbooks/operations/observability/providers/grafana/assets/dashboard.json` as shipped — the panel object shape, the pinned `__PROMETHEUS_UID__` datasource, the `__ENVIRONMENT__` templating and the `stat`/`timeseries` option blocks are mirrored exactly, so one asset stays deployable to both environments.

## Sections (implementation slices)

### §1 — The heartbeat reads as an age

The `runner-silent` rule compares a Unix epoch against ninety seconds and is therefore always true; the matching panel plots the same epoch with a seconds unit and renders tens of thousands of years. Both are repaired at the asset layer by subtracting the evaluation time, because the metric's own semantics are correct and the census documents them — the readers are what is wrong. **Implementation default:** `time() - agentsfleet_runner_last_seen_seconds` rather than changing the producer, because the epoch is the more useful primitive and `RUNNER_OFFLINE_AFTER_MS` already derives the threshold at apply time.

- **Dimension 1.1** — DONE —the `runner-silent` expression compares an age, not an epoch, and does not fire for a runner heartbeating now → Test `test_should_repair_the_heartbeat_readings`
- **Dimension 1.2** — DONE —panel 6 plots seconds since last heartbeat, bounded by the derived offline threshold → Test `test_should_repair_the_heartbeat_readings`
- **Dimension 1.3** — DONE —a runner whose last heartbeat predates the threshold still trips the rule → Test `test_should_reject_an_epoch_read_without_subtraction`

### §2 — Three Service Level Indicators, each grounded in a produced family

Only families the Rust registry actually feeds may carry an SLI. Admission availability is the good-events ratio over `agentsfleet_admissions_total{outcome}`; pickup latency reads `agentsfleet_admission_backlog_oldest_age_seconds`, whose census `watch_for` line names the replay floor as its own threshold; runner error rate is `agentsfleet_runner_executions_total{outcome}` against itself, with `agentsfleet_runner_failures_total{reason}` as the attribution panel beside it. **Implementation default:** every ratio numerator and denominator is wrapped so an absent delta counter reads as zero rather than "No data", because a counter that has never incremented publishes no series and an empty panel is indistinguishable from a broken one.

- **Dimension 2.1** — DONE —admission availability renders a ratio in `[0,1]` when admissions exist and `1` in BOTH empty states — family absent, and family registered at zero — never "No data" → Test `test_should_guard_slo_ratios_against_zero`
- **Dimension 2.2** — DONE —pickup latency panel carries the source-derived replay-floor threshold, not a literal → Test `test_should_derive_the_replay_floor_from_source`
- **Dimension 2.3** — DONE —runner success reads executions by outcome, never divides by zero, and reaches `1` in both empty states; its error-rate panel is the complement and reaches `0` in the same two → Test `test_should_guard_slo_ratios_against_zero`
- **Dimension 2.4** — DONE —every SLI target recorded in the architecture doc names the measurement that produced it → Test `test_should_cover_slo_in_the_playbook`

### §3 — Error budget, and the honesty about it

Burn-rate panels follow the workbook's multiwindow shape, and every one of them carries a visible unproven marker until the development deployment has accumulated a distribution worth setting a target against. Measured now: fourteen admissions and ten runner executions since the daemon's most recent restart. A target computed from ten events is arithmetic, not an objective, and the panel says so on its face. **Implementation default:** burn rate ships as panels and not as alert rules — the request was for panels, and adding rules would have meant relaxing the grader's exact-count assertion to admit them.

- **Dimension 3.1** — DONE —each burn-rate panel pairs a long and a short window → Test `test_should_mark_burn_rate_panels_unproven`
- **Dimension 3.2** — DONE —every burn-rate panel description carries the unproven marker → Test `test_should_mark_burn_rate_panels_unproven`

### §4 — The operator view: what is saturated, retrying, and silently dropping

The 3am screen. Saturation from `agentsfleet_api_in_flight_requests`, `agentsfleet_sse_in_flight_streams`, `agentsfleet_fleet_ready_depth`, `agentsfleet_admission_backlog` and `agentsfleet_process_resident_memory_bytes`; retry pressure from `agentsfleet_repair_dispatch_retried_total` and `agentsfleet_admission_replays_total`; silent loss from `agentsfleet_otlp_entries_discarded_total{signal,reason}`, which the `UNPRODUCED` ledger names as the surviving self-observability counter now that `agentsfleet.telemetry.samples_dropped` and `agentsfleet_otlp_queue_depth` have no producer. Lease-poll cost keeps its existing panel because its `watch_for` line — idle polls must add zero database round trips — is the cheapest regression detector in the export.

- **Dimension 4.1** — DONE —the operator row reads discarded telemetry entries by signal and reason → Test `test_should_read_telemetry_loss`
- **Dimension 4.2** — DONE —saturation panels each carry the cap or the census guidance they are read against → Test `test_should_read_telemetry_loss`

### §5 — Declared gaps are on the screen, not in a footnote

Twelve declared families have no producer, and two of them are exactly the self-observability signals an operator would reach for first. Rather than omit them, the dashboard carries one panel naming each impossible SLI, the family behind it, and the `UNPRODUCED` reason — so the gap is visible at 3am instead of discoverable by grep. The same panel carries the shared-tenant warning: development and production resolve to one Grafana stack, one namespace, one datasource and one ingest credential, and no series carries an environment attribute, so this dashboard is development-only by the accident that production runs no machines.

- **Dimension 5.1** — DONE —the declared-gap panel names each still-impossible SLI and its `UNPRODUCED` reason; a gap the milestone closes leaves the panel in the same diff → Test `test_should_reject_a_gap_panel_for_a_produced_family`
- **Dimension 5.2** — DONE —the shared-tenant warning names the missing resource attribute and the successor work → Test `test_should_warn_about_the_shared_tenant`
- **Dimension 5.3** — DONE —the grader refuses a gap panel naming a family the `UNPRODUCED` ledger does not carry → Test `test_should_reject_a_gap_panel_for_a_produced_family`
- **Dimension 5.4** — DONE —the nine shipped panels keep their identifiers and their families across the diff → Test `test_should_keep_the_shipped_panels`
- **Dimension 5.6** — DONE —the grader's alert-count constant equals the number of rules in `alerts.json`, so the set can grow without the grader going stale → Test `test_should_match_the_alert_count_constant`

### §6 — Applied, then verified as applied

The asset becomes a live page. The playbook's existing three-command sequence owns the apply; this Section's work is that the apply succeeds against the development stack and the read-only drift check passes afterwards, and that the playbook's Acceptance list grows the rows a reader needs to confirm it. Production is explicitly not applied: the same assets would land in the same stack against the same datasource, and until the environment attribute exists that is one dashboard wearing two names.

- **Dimension 6.1** — DONE —the development folder, dashboard and alert rules exist in the stack after apply → Test `test_should_cover_slo_in_the_playbook`
- **Dimension 6.2** — DONE —the drift check passes against the applied resources → Test `test_should_accept_matching_resources`
- **Dimension 6.3** — DONE —the playbook's Acceptance list names the SLO rows and the shared-tenant warning → Test `test_should_cover_slo_in_the_playbook`
- **Dimension 6.4** — DONE —a second apply against unchanged assets mutates nothing → Test `test_should_update_existing_resources_with_versions`

## Interfaces

```
Grafana resource names (unchanged, set in providers/grafana/common.sh):
  folder     agentsfleet-<env>          title "agentsfleet — <environment>"
  dashboard  agentsfleet-runtime-<env>  title "agentsfleet runtime — <environment>"

Asset placeholders the apply step substitutes (unchanged shape, one added):
  __PROMETHEUS_UID__         the pinned datasource uid
  __ENVIRONMENT__            development | production
  __DASHBOARD_UID__          agentsfleet-runtime-<env>
  __RUNNER_OFFLINE_SECONDS__ derived from LEASE_TTL_MS * RUNNER_OFFLINE_AFTER_MS
  __ADMISSION_REPLAY_FLOOR_SECONDS__  derived from the admission replay floor in source

No-data convention: a ratio SLI with nothing to measure is 1, an error rate
with nothing to measure is 0, and both mean nothing has failed. Neither may
render "No data" -- an absent family and a family registered at zero are
ordinary states of a deployment this young. Clamping the denominator does not
buy that: `sum()` over an absent selector is EMPTY, not 0, `clamp_min(empty,1)`
is still empty, and empty / empty is empty. Hence the complement form, each SLI
carrying its fallback explicitly.

Service Level Indicator expressions (good events / valid events):
  admission availability
    (1 - ((sum(agentsfleet_admissions_total)
           - (sum(agentsfleet_admissions_total{outcome="appended"}) or vector(0)))
          / clamp_min(sum(agentsfleet_admissions_total), 1))) or vector(1)
  runner success
    (1 - ((sum(agentsfleet_runner_executions_total)
           - (sum(agentsfleet_runner_executions_total{outcome="processed"}) or vector(0)))
          / clamp_min(sum(agentsfleet_runner_executions_total), 1))) or vector(1)
  pickup latency (threshold SLI, not a ratio)
    max(agentsfleet_admission_backlog_oldest_age_seconds)
      < __ADMISSION_REPLAY_FLOOR_SECONDS__

  Four states each ratio answers, which is what its test asserts:
    family absent -> empty -> `or vector(1)` -> 1 . family at 0 -> 1
    all good -> 1 . some bad -> 1 - (bad / total)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Epoch read as age | a panel or rule consumes `agentsfleet_runner_last_seen_seconds` directly | the grader refuses an expression naming that family without a `time()` subtraction; the operator sees a bounded age. |
| Empty delta counter | a counter has never incremented, so no series exists and `rate()` returns nothing | the ratio's denominator is clamped; the panel reads a defined value rather than "No data". |
| Gap panel drifts | a family gains a producer but its gap panel survives | the grader cross-reads `produced.rs`; a gap panel for a produced family fails the asset check. |
| Apply against production | an operator runs the apply arm with `prod` before the environment attribute exists | the playbook's warning and the gate's environment echo name the shared tenant; the operator is told the two dashboards would share one datasource. |
| Datasource proxy unavailable | Grafana returns 503 from the datasource proxy, as observed once during this spec's authoring | the check arm exits non-zero with the curl status; the apply is not attempted against an unreadable datasource. |
| Alert count expectation stale | the rule set grows but the grader still demands exactly six | the grader's expectation is a named constant beside the asset, and the self-test asserts the two agree. |

## Invariants

1. Every `agentsfleet_*` token in either asset resolves to a family greppable in `rustd/crates` — enforced by the existing loop in `assets_check.sh`, which exits non-zero on an unowned metric.
2. No panel or alert expression reads `agentsfleet_runner_last_seen_seconds` without subtracting it from the evaluation time — enforced by a grader check, not review.
3. Every gap panel names a family the `UNPRODUCED` ledger carries — enforced by the grader reading `produced.rs`, so a family gaining a producer fails the asset check until its gap panel leaves.
4. Every threshold SOURCE owns is a substituted placeholder, never a literal — enforced by a grader check refusing a comparison against an integer of two digits or more. A Service Level Objective target is policy rather than source, so it stays a literal in the asset and carries its provenance in the panel description and in the architecture doc. Authoring assumed every number was derivable; the runner offline threshold and the replay floor are, and a 99% target is not.
5. One asset serves both environments — enforced by the existing datasource and environment placeholder checks; a hardcoded environment string fails the asset check.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no new family | ops | this workstream adds no producer and no census row; it only reads families the registry already grades | none | no credential, tenant identifier or runner payload enters an asset or a log | `test_should_leave_the_bench_baselines_untouched` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_should_repair_the_heartbeat_readings` | the `runner-silent` expression contains a `time()` subtraction and no bare epoch comparison. |
| 1.2 | unit | `test_should_repair_the_heartbeat_readings` | panel 6's target subtracts from evaluation time and keeps the `s` unit. |
| 1.3 | integration | `test_should_reject_an_epoch_read_without_subtraction` | a stubbed series whose last heartbeat precedes the derived threshold evaluates the rule true. |
| 2.1 | unit | `test_should_guard_slo_ratios_against_zero` | the expression evaluates to 1 with no admission series AND with the family registered at zero; neither renders "No data". |
| 2.2 | unit | `test_should_derive_the_replay_floor_from_source` | the panel carries the replay-floor placeholder, and no literal seconds value. |
| 2.3 | unit | `test_should_guard_slo_ratios_against_zero` | a zero-execution window yields a defined ratio: the denominator is clamped AND the numerator carries an absent-series fallback, since clamping alone leaves an empty vector empty. |
| 2.4 | unit | `test_should_cover_slo_in_the_playbook` | every target in the architecture doc's SLO table names a measurement or is marked unproven. |
| 3.1 | unit | `test_should_mark_burn_rate_panels_unproven` | each burn-rate panel carries two window lengths. |
| 3.2 | unit | `test_should_mark_burn_rate_panels_unproven` | each burn-rate panel description carries the unproven marker and an event count. |
| 4.1 | unit | `test_should_read_telemetry_loss` | a panel targets `agentsfleet_otlp_entries_discarded_total` by signal and reason. |
| 4.2 | unit | `test_should_read_telemetry_loss` | each saturation panel description names its cap or census guidance. |
| 5.1 | unit | `test_should_reject_a_gap_panel_for_a_produced_family` | the gap panel names every family this spec claims impossible. |
| 5.2 | unit | `test_should_warn_about_the_shared_tenant` | the warning names the missing resource attribute and the successor spec. |
| 5.3 | unit | `test_should_reject_a_gap_panel_for_a_produced_family` | a gap panel naming a produced family fails the asset check with a non-zero exit. |
| 6.1 | manual | `test_should_cover_slo_in_the_playbook` | after the apply arm, the folders, dashboards and provisioning endpoints return the agentsfleet resources; evidence is the recorded responses in Session Notes. Requires Indy's write approval. |
| 6.2 | manual | `test_should_accept_matching_resources` | the verify arm exits 0 against the applied stack; evidence is the recorded run. |
| 6.3 | unit | `test_should_cover_slo_in_the_playbook` | the playbook's Acceptance list names the SLO rows and the shared-tenant warning. |
| 5.4 | unit | `test_should_keep_the_shipped_panels` | the nine shipped panels keep their identifiers and their families. |
| 5.6 | unit | `test_should_match_the_alert_count_constant` | the grader's named alert-count constant equals the rule count in `alerts.json`; a rule added without moving the constant fails. |
| 6.4 | integration | `test_should_update_existing_resources_with_versions` | a second apply against unchanged assets reports the resources current and mutates nothing. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | No asset reads the heartbeat family as an age-free epoch (§1) | `grep -c 'agentsfleet_runner_last_seen_seconds' playbooks/operations/observability/providers/grafana/assets/*.json && ! grep -E 'agentsfleet_runner_last_seen_seconds[^)]*>' playbooks/operations/observability/providers/grafana/assets/alerts.json` | exit 0, no bare comparison | P0 | ✅ no expression reads the family without `time() -`; the grader enforces it and `test_should_repair_the_heartbeat_readings` proves it |
| R2 | The asset grader passes on the edited assets (§2, §3, §4, §5) | `OBS_ENV=dev bash playbooks/operations/observability/providers/grafana/assets_check.sh` | exit 0, `PASS: Grafana assets are valid` | P0 | ✅ `PASS: Grafana assets are valid and reference source-owned metrics` |
| R3 | The observability self-tests pass (§1–§5) | `bash playbooks/operations/observability/observability_test.sh && bash playbooks/operations/observability/observability_verify_test.sh` | exit 0 both | P0 | ✅ four suites in the gate run: 11 + 13 + 4 + 2, 0 failed |
| R4 | Bench baselines are untouched | `git diff --name-only origin/main...HEAD -- bench/baselines/` | no output | P0 | ✅ `git diff --name-only origin/main...HEAD -- bench/baselines` → no output |
| R5 | The dashboard is live in development (§6) | `ALLOW_VAULT_READS=1 ./playbooks/operations/observability/00_gate.sh verify dev grafana` | exit 0, `PASS: grafana observability verify completed for dev` | P0 | ✅ `PASS: development Grafana resources match the repository` — folder, 26-panel dashboard and 6 alert rules read back from the stack |
| R6 | The SLO definitions are written where an operator finds them (§2, §6) | `grep -c 'Service Level Objective' docs/architecture/observability.md` | at least 1 | P0 | ✅ `docs/architecture/observability.md` §Service Level Objectives — four indicators, each naming the measurement behind its target |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ after the three split files and the two `assign.rs` paths joined the table; 0 unlisted paths |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | ✅ `ALL GATES GREEN ── ready for VERIFY` |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ `cmd.verify.unit` exit 0 — 2713 passed, 0 failed, 513 ignored |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | ✅ `cmd.verify.lint` exit 0 — `All lint checks passed` |
| S4 | Integration lane green | `make test-integration-rustd` | exit 0 | P0 | ✅ `cmd.verify.integration` exit 0 — 492 + 1 passed, 0 failed, live Postgres + Dragonfly |
| S5 | Version sync | `make check-version` | exit 0 | P0 | ✅ `cmd.verify.version` exit 0 — `all versions match 0.48.0` |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` — 193.22 MB scanned |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| grep -v '\.json$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ green after both splits; the two files over the cap were `assign.rs` (369) and the observability suite (712) |
**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes, so recording those results does not require another code commit and suite run. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P0 may also be **MOVED** — see below.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- **Emitting `deployment.environment`.** The daemon's resource carries service name, namespace, version and an optional instance identifier, and nothing else; separating development from production series needs a new resource attribute in `telemetry/resource.rs` plus staging in both deployment workflows. That is a daemon and Continuous Integration change and belongs to its own milestone. Named here as the blocker on production's first deploy.
- **An API request-duration histogram.** The census declares three histograms and none measures an HTTP request, so requests per second, the 500 rate and a latency percentile are all unanswerable. A new family with a new census row, and the cheapest one left: `semconv.rs` already defines and bounds `ATTR_HTTP_ROUTE` and `ATTR_HTTP_RESPONSE_STATUS_CODE` for spans, so the cardinality guard is written.
- **~~A fleet-start counter~~ — LANDED in M197_002.** The availability SLI an operator actually wants is panel 34. `agentsfleet_fleet_runs_started_total{kind="fresh"}` is the numerator this spec said did not exist; `agentsfleet_admissions_total{outcome="appended"}` was always the denominator. The trigger counter named here as the missing half was retired instead of produced — declared for years, incremented nowhere.
- **Applying to production.** Deliberately not run: the same assets against the same stack and datasource would be one dashboard wearing two names until the environment attribute lands.
- **Enabling the burn-rate alert rules.** Shipped with expressions reviewed and disabled; enabling waits for a distribution.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy opens one Grafana page at 3am, sees within a second that runners are alive and nothing is being dropped, and closes it. No JSON, no `jq`, no asking an agent.
2. **Preserved user behaviour** — the three-command playbook sequence (`check`, `apply`, `verify`) keeps working exactly as documented, and the nine existing panels keep their identifiers and their families.
3. **Optimal-way check** — the most direct route to moment #1 would be an environment-scoped dashboard over environment-labelled series. The gap to that shape is one missing resource attribute, and it is acceptable now only because production runs zero machines, which makes the development-only dashboard correct today and wrong the day that changes. The warning panel is the interest paid on that.
4. **Rebuild-vs-iterate** — iterate. The asset shape, the placeholder substitution and the grader are sound; what is wrong is two expressions and what is missing is panels. A rebuild would discard a working apply path to fix a subtraction.
5. **What we build** — repaired heartbeat expressions, three SLI panels, burn-rate panels marked unproven, an operator row, a declared-gap panel, the SLO section in the architecture doc, and one applied development dashboard.
6. **What we do NOT build** — new metric families, the environment resource attribute, a production apply, enabled burn-rate rules, and any edit to `bench/baselines/`.
7. **Fit with existing features** — compounds with the census and the `UNPRODUCED` ledger, which together already answer "does this family exist and does anything feed it". The one thing it must not destabilize is the asset grader's guarantee that no panel references an unowned metric.
8. **Surface order** — operator-surface-first; there is no Command-Line Interface or web surface in this workstream.
9. **Dashboard restraint** — this is the whole point of §5. No panel claims a signal it does not have: a family with no producer gets a named gap, not an empty graph, and a burn rate over ten events is marked unproven on its face rather than rendered as a confident number.
10. **Confused-user next step** — an operator seeing an empty panel reads its description, which names either the census `watch_for` line or the `UNPRODUCED` reason; an operator whose apply fails reads the gate's own non-zero exit and the playbook's Indy-handoff list.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six Sections ordered by what breaks first — the always-firing alert leads because it is the only shipped defect, the SLIs follow because they are the ask, and the apply lands last because it should not go live until the assets are right.
- **Alternatives considered:** (a) emit `deployment.environment` first and build the dashboard on a real environment selector — correct and permanent, rejected for now because it is daemon and workflow work that delays every panel behind it, and it is named in Out of Scope as the successor; (b) build only what has live series and omit the gap panels — smaller and fully proven, rejected because the missing self-observability signals are exactly what an operator reaches for at 3am and a gap discoverable only by reading a doc is a gap nobody finds.
- **Patch-vs-refactor verdict:** this is a **patch** because the apply path, the placeholder substitution and the grader all work, and the defects are two expressions in a JSON asset. The refactor that is right in the long game — environment-labelled series — is named as its own successor rather than smuggled in here.

## Discovery (consult log)

- **Consults** — Architecture: `docs/architecture/observability.md` §Metric family census and §Capacity and loss audit read at authoring; the doc's claim that `agentsfleet.telemetry.samples_dropped` and `agentsfleet_otlp_queue_depth` count local loss is superseded by `produced.rs`, which lists both as `UNPRODUCED` — the architecture doc is updated in this workstream rather than left to contradict source. Source-verified at authoring: `runner.rs:260-272` returns an epoch; `constants.zig:59,83` derive a ninety-second offline threshold; `produced.rs` carries twelve unproduced families. Live-read at authoring against the development stack: zero agentsfleet folders, zero agentsfleet dashboards, zero provisioned alert rules; 36 of 64 census families have series; `agentsfleetd-prod` and `otelcol-prod` run no machines; the development and production vault items resolve to identical Grafana URL, namespace, datasource uid, OTLP endpoint and instance identifier.
- **Metrics review** — no analytics or funnel playbook update required: this workstream adds no product event and no metric family, and changes no census row.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test` at VERIFY per Section and at the boundary, `/review` at REVIEW, `orly-babysit-prs` after each push.
- **What the first live apply found, none of it visible from the repository.** The assets had never been applied, so four defects sat behind that. (1) The per-runner table is in-process and never evicted, so a runner heartbeating to one replica leaves a frozen reading on its sibling — measured Sep 17, 2026: runner `019fec54` read 9 s on one daemon and 1178 s on the other. Subtracting from `time()` was necessary and not sufficient; `min by (runner_id)` before the outer `max` is the second half, and without it the repaired rule still pages for a healthy fleet. (2) `resource_verify.sh` read dashboard identity from `.spec.uid`, which the Grafana v1 dashboard API answers `null` for — identity is `metadata.name`. (3) The same script iterated `.targets[]` unconditionally, and Grafana drops that field entirely for a panel that queries nothing, so a faithfully applied dashboard reported `queries drifted`. (4) Three copies of the placeholder substitution had drifted apart; they are now one `obs_render_dashboard` in `common.sh` that the apply, the drift check and the test stub all call.
- **🛑 BLOCKED — alert rules cannot be written with this credential.** `00_gate.sh apply dev grafana` created the folder and the dashboard and then returned HTTP 403 on every `POST` to `/apis/rules.alerting.grafana.app/v0alpha1/.../alertrules`. Not the feature toggle the playbook warns about: `/api/frontend/settings` reports `alertingApiServer = true`, and `GET` on the same path returns HTTP 200 with an empty list. The service account holds `alert.rules:create`, `alert.rules:write` and `alert.rules:delete` scoped to `folders:*` per `/api/access-control/user/permissions`, and the write is still refused. Needs Indy: either the service-account role gains whatever Grafana Cloud gates this write behind, or the alert path moves to `/api/v1/provisioning/alert-rules`, which this token does hold `alert.rules.provisioning:write` for and which answers `GET` 200. Probing stopped at one refused write rather than trying more shapes against a shared system.
- **`make lint-all` is red on a pre-existing gate defect, not on this diff.** `check-cutover-probes` → `zig-citations` greps `rustd/` for `.zig` paths and recurses into `rustd/target/`, matching byte sequences inside compiled `.rmeta` and `query-cache.bin` files and reporting them as dead citations. Reproduced on clean `main` at `d229f568a` with no changes: same 24 hits, same failure. Continuous Integration is green only because a fresh checkout has no build directory. Surfaced to Indy rather than repaired: the fix is one path exclusion in a gate script, and calling a gate's hit a false positive is not the agent's call.
- **Dependency refresh, requested in-session.** Indy asked for `package.json` and `Cargo.toml` currency on this branch. 30 Rust crates moved, every one a patch inside its existing `Cargo.toml` range, so only `Cargo.lock` changed. The JavaScript side raised `^` floors across four manifests, all patch or minor within the same major. `size-limit` 13 → 14 was left alone: a major bump to a bundle-size tool is its own review, not cargo for a dashboard workstream.
- **Scope handed to a follow-up: none.** Nothing committed in this workstream was moved out of it. The Out of Scope entries are boundaries drawn at authoring — the request-duration histogram was never in scope, and the fleet-start counter left that list by LANDING in workstream 002, not by being postponed. Should a later session move committed scope out of this spec, it records the owner's verbatim quote here before doing so.
