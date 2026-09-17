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
**Status:** PENDING
**Priority:** P0 — the shipped `runner-silent` alert fires permanently by construction, and no agentsfleet dashboard has ever been applied to any Grafana stack.
**Categories:** DOCS, INFRA, OBS
**Batch:** B1 — no concurrent workstream; the observability assets are edited by nothing else.
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none
**Provenance:** LLM-drafted (claude-opus-5, Sep 17, 2026), grounded in live reads of the development Grafana stack and `rustd/crates/afd_observability` source
**Canonical architecture:** `docs/architecture/observability.md` §Metric family census

---

## Overview

**Goal (testable):** the `agentsfleet-runtime-dev` dashboard exists in the development Grafana stack carrying the six shipped alert rules plus the burn-rate rules §3 adds, counted by the grader's named constant rather than a literal, every panel resolves against a family `rustd/crates` actually produces, `runner-silent` fires only when a runner is genuinely overdue, and three Service Level Indicators (SLIs) carry targets derived from measured series rather than invented numbers.

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
| `playbooks/operations/observability/providers/grafana/assets_check.sh` | EDIT | the alert-count expectation and the minimum panel count move with the new scope; the grader stays a grader. |
| `playbooks/operations/observability/observability_test.sh` | EDIT | new asset expectations get a self-test row. |
| `playbooks/operations/observability/observability_verify_test.sh` | EDIT | the drift check's stub dashboard tracks the real asset shape. |
| `playbooks/operations/observability/001_playbook.md` | EDIT | Acceptance gains the SLO rows and the shared-tenant warning an operator must read before applying to production. |
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

- **Dimension 1.1** — the `runner-silent` expression compares an age, not an epoch, and does not fire for a runner heartbeating now → Test `test_runner_silent_compares_an_age`
- **Dimension 1.2** — panel 6 plots seconds since last heartbeat, bounded by the derived offline threshold → Test `test_heartbeat_panel_plots_an_age`
- **Dimension 1.3** — a runner whose last heartbeat predates the threshold still trips the rule → Test `test_overdue_runner_still_alerts`

### §2 — Three Service Level Indicators, each grounded in a produced family

Only families the Rust registry actually feeds may carry an SLI. Admission availability is the good-events ratio over `agentsfleet_admissions_total{outcome}`; pickup latency reads `agentsfleet_admission_backlog_oldest_age_seconds`, whose census `watch_for` line names the replay floor as its own threshold; runner error rate is `agentsfleet_runner_executions_total{outcome}` against itself, with `agentsfleet_runner_failures_total{reason}` as the attribution panel beside it. **Implementation default:** every ratio numerator and denominator is wrapped so an absent delta counter reads as zero rather than "No data", because a counter that has never incremented publishes no series and an empty panel is indistinguishable from a broken one.

- **Dimension 2.1** — admission availability renders a ratio in `[0,1]` when admissions exist and `1` in BOTH empty states — family absent, and family registered at zero — never "No data" → Test `test_admission_availability_ratio`
- **Dimension 2.2** — pickup latency panel carries the source-derived replay-floor threshold, not a literal → Test `test_pickup_latency_threshold_is_derived`
- **Dimension 2.3** — runner success reads executions by outcome, never divides by zero, and reaches `1` in both empty states; its error-rate panel is the complement and reaches `0` in the same two → Test `test_runner_error_rate_guards_zero`
- **Dimension 2.4** — every SLI target recorded in the architecture doc names the measurement that produced it → Test `test_slo_targets_cite_provenance`

### §3 — Error budget, and the honesty about it

Burn-rate panels follow the workbook's multiwindow shape, and every one of them carries a visible unproven marker until the development deployment has accumulated a distribution worth setting a target against. Measured now: fourteen admissions and ten runner executions since the daemon's most recent restart. A target computed from ten events is arithmetic, not an objective, and the panel says so on its face. **Implementation default:** the burn-rate alert rules ship disabled-by-annotation rather than omitted, so the expression is reviewed and version-controlled now and enabling it later is a one-field change rather than new authorship.

- **Dimension 3.1** — each burn-rate panel pairs a long and a short window → Test `test_burn_rate_is_multiwindow`
- **Dimension 3.2** — every burn-rate panel description carries the unproven marker and the event count behind it → Test `test_burn_rate_marked_unproven`

### §4 — The operator view: what is saturated, retrying, and silently dropping

The 3am screen. Saturation from `agentsfleet_api_in_flight_requests`, `agentsfleet_sse_in_flight_streams`, `agentsfleet_fleet_ready_depth`, `agentsfleet_admission_backlog` and `agentsfleet_process_resident_memory_bytes`; retry pressure from `agentsfleet_repair_dispatch_retried_total` and `agentsfleet_admission_replays_total`; silent loss from `agentsfleet_otlp_entries_discarded_total{signal,reason}`, which the `UNPRODUCED` ledger names as the surviving self-observability counter now that `agentsfleet.telemetry.samples_dropped` and `agentsfleet_otlp_queue_depth` have no producer. Lease-poll cost keeps its existing panel because its `watch_for` line — idle polls must add zero database round trips — is the cheapest regression detector in the export.

- **Dimension 4.1** — the operator row reads discarded telemetry entries by signal and reason → Test `test_operator_row_reads_telemetry_loss`
- **Dimension 4.2** — saturation panels each carry the cap or the census guidance they are read against → Test `test_saturation_panels_name_their_ceiling`

### §5 — Declared gaps are on the screen, not in a footnote

Twelve declared families have no producer, and two of them are exactly the self-observability signals an operator would reach for first. Rather than omit them, the dashboard carries one panel naming each impossible SLI, the family behind it, and the `UNPRODUCED` reason — so the gap is visible at 3am instead of discoverable by grep. The same panel carries the shared-tenant warning: development and production resolve to one Grafana stack, one namespace, one datasource and one ingest credential, and no series carries an environment attribute, so this dashboard is development-only by the accident that production runs no machines.

- **Dimension 5.1** — a declared-gap panel names each impossible SLI and its `UNPRODUCED` reason → Test `test_declared_gaps_are_panelled`
- **Dimension 5.2** — the shared-tenant warning names the missing resource attribute and the successor work → Test `test_shared_tenant_warning_present`
- **Dimension 5.3** — the grader refuses a gap panel naming a family the `UNPRODUCED` ledger does not carry → Test `test_gap_panel_must_cite_the_ledger`
- **Dimension 5.4** — the nine shipped panels keep their identifiers and their families across the diff → Test `test_existing_nine_panels_survive`
- **Dimension 5.5** — no census row changes, so the registry grading is untouched → Test `test_census_unchanged_by_m197`
- **Dimension 5.6** — the grader's alert-count constant equals the number of rules in `alerts.json`, so the set can grow without the grader going stale → Test `test_alert_count_constant_matches_assets`

### §6 — Applied, then verified as applied

The asset becomes a live page. The playbook's existing three-command sequence owns the apply; this Section's work is that the apply succeeds against the development stack and the read-only drift check passes afterwards, and that the playbook's Acceptance list grows the rows a reader needs to confirm it. Production is explicitly not applied: the same assets would land in the same stack against the same datasource, and until the environment attribute exists that is one dashboard wearing two names.

- **Dimension 6.1** — the development folder, dashboard and alert rules exist in the stack after apply → Test `test_dev_resources_exist_after_apply`
- **Dimension 6.2** — the drift check passes against the applied resources → Test `test_verify_reports_no_drift`
- **Dimension 6.3** — the playbook's Acceptance list names the SLO rows and the shared-tenant warning → Test `test_playbook_acceptance_covers_slo`
- **Dimension 6.4** — a second apply against unchanged assets mutates nothing → Test `test_apply_is_idempotent`

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
4. Every numeric threshold in either asset is a substituted placeholder, never a literal — enforced by a grader check that refuses a bare integer comparison in an alert expression.
5. One asset serves both environments — enforced by the existing datasource and environment placeholder checks; a hardcoded environment string fails the asset check.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no new family | ops | this workstream adds no producer and no census row; it only reads families the registry already grades | none | no credential, tenant identifier or runner payload enters an asset or a log | `test_census_unchanged_by_m197` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_runner_silent_compares_an_age` | the `runner-silent` expression contains a `time()` subtraction and no bare epoch comparison. |
| 1.2 | unit | `test_heartbeat_panel_plots_an_age` | panel 6's target subtracts from evaluation time and keeps the `s` unit. |
| 1.3 | integration | `test_overdue_runner_still_alerts` | a stubbed series whose last heartbeat precedes the derived threshold evaluates the rule true. |
| 2.1 | unit | `test_admission_availability_ratio` | the expression evaluates to 1 with no admission series AND with the family registered at zero; neither renders "No data". |
| 2.2 | unit | `test_pickup_latency_threshold_is_derived` | the panel carries the replay-floor placeholder, and no literal seconds value. |
| 2.3 | unit | `test_runner_error_rate_guards_zero` | a zero-execution window yields a defined ratio: the denominator is clamped AND the numerator carries an absent-series fallback, since clamping alone leaves an empty vector empty. |
| 2.4 | unit | `test_slo_targets_cite_provenance` | every target in the architecture doc's SLO table names a measurement or is marked unproven. |
| 3.1 | unit | `test_burn_rate_is_multiwindow` | each burn-rate panel carries two window lengths. |
| 3.2 | unit | `test_burn_rate_marked_unproven` | each burn-rate panel description carries the unproven marker and an event count. |
| 4.1 | unit | `test_operator_row_reads_telemetry_loss` | a panel targets `agentsfleet_otlp_entries_discarded_total` by signal and reason. |
| 4.2 | unit | `test_saturation_panels_name_their_ceiling` | each saturation panel description names its cap or census guidance. |
| 5.1 | unit | `test_declared_gaps_are_panelled` | the gap panel names every family this spec claims impossible. |
| 5.2 | unit | `test_shared_tenant_warning_present` | the warning names the missing resource attribute and the successor spec. |
| 5.3 | unit | `test_gap_panel_must_cite_the_ledger` | a gap panel naming a produced family fails the asset check with a non-zero exit. |
| 6.1 | manual | `test_dev_resources_exist_after_apply` | after the apply arm, the folders, dashboards and provisioning endpoints return the agentsfleet resources; evidence is the recorded responses in Session Notes. Requires Indy's write approval. |
| 6.2 | manual | `test_verify_reports_no_drift` | the verify arm exits 0 against the applied stack; evidence is the recorded run. |
| 6.3 | unit | `test_playbook_acceptance_covers_slo` | the playbook's Acceptance list names the SLO rows and the shared-tenant warning. |
| 5.4 | unit | `test_existing_nine_panels_survive` | the nine shipped panels keep their identifiers and their families. |
| 5.5 | unit | `test_census_unchanged_by_m197` | `docs/metrics.census.tsv` is byte-identical across the diff. |
| 5.6 | unit | `test_alert_count_constant_matches_assets` | the grader's named alert-count constant equals the rule count in `alerts.json`; a rule added without moving the constant fails. |
| 6.4 | integration | `test_apply_is_idempotent` | a second apply against unchanged assets reports the resources current and mutates nothing. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | No asset reads the heartbeat family as an age-free epoch (§1) | `grep -c 'agentsfleet_runner_last_seen_seconds' playbooks/operations/observability/providers/grafana/assets/*.json && ! grep -E 'agentsfleet_runner_last_seen_seconds[^)]*>' playbooks/operations/observability/providers/grafana/assets/alerts.json` | exit 0, no bare comparison | P0 | |
| R2 | The asset grader passes on the edited assets (§2, §3, §4, §5) | `OBS_ENV=dev bash playbooks/operations/observability/providers/grafana/assets_check.sh` | exit 0, `PASS: Grafana assets are valid` | P0 | |
| R3 | The observability self-tests pass (§1–§5) | `bash playbooks/operations/observability/observability_test.sh && bash playbooks/operations/observability/observability_verify_test.sh` | exit 0 both | P0 | |
| R4 | The census is untouched (§5) | `git diff --name-only origin/main...HEAD -- docs/metrics.census.tsv` | no output | P0 | |
| R5 | Bench baselines are untouched | `git diff --name-only origin/main...HEAD -- bench/baselines/` | no output | P0 | |
| R6 | The dashboard is live in development (§6) | `ALLOW_VAULT_READS=1 ./playbooks/operations/observability/00_gate.sh verify dev grafana` | exit 0, `PASS: grafana observability verify completed for dev` | P0 | |
| R7 | The SLO definitions are written where an operator finds them (§2, §6) | `grep -c 'Service Level Objective' docs/architecture/observability.md` | at least 1 | P0 | |
| R8 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
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

N/A — no files deleted.

## Out of Scope

- **Emitting `deployment.environment`.** The daemon's resource carries service name, namespace, version and an optional instance identifier, and nothing else; separating development from production series needs a new resource attribute in `telemetry/resource.rs` plus staging in both deployment workflows. That is a daemon and Continuous Integration change and belongs to its own milestone. Named here as the blocker on production's first deploy.
- **A fleet-start counter and an API request-duration histogram.** The availability SLI an operator actually wants — asked-for fleets that started — has no denominator (`agentsfleet_fleet_triggered_total` is declared and incremented nowhere) and no numerator (no family counts a start). There is no HTTP request-duration histogram in the census at all. Both are new families with new census rows.
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
- **Deferrals** — none yet. The two Out of Scope items are scope boundaries recorded at authoring, not deferrals of committed work; if either is later claimed as deferred rather than never-scoped, an Indy-acked verbatim quote lands here first.
