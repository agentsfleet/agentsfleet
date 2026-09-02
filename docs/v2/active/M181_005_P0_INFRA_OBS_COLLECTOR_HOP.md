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

# M181_005: The collector hop in front of the daemon's OTLP export

**Prototype:** v2.0.0
**Milestone:** M181
**Workstream:** 005
**Date:** Sep 01, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — on the cutover's critical path: continuity across the swap is graded through these collectors
**Categories:** INFRA | OBS
**Batch:** B6 — the deploy configuration is independent, but the PROOF is serial behind M181_004: a collector cannot be shown to carry signals nothing is sending
**Branch:** `feat/m181-005-collectors`
**Test Baseline:** `unit=6907 integration=not-run` — `make test-unit-all` exit 0 at `ac5a00157` (rustd cargo workspace 2186 + app 2410 + website 175 + cli 1624 + design-system 512). `verify.integration` is not run at CHORE(open): the stage table declares the slow suites only when the branch carries code, and this diff carries none — deploy configuration, a runbook and a probe. It WAS run once at the boundary on Indy's instruction (`349 passed`), recorded in the Test Delta below rather than as a rubric row, since the lane grades no part of this diff.
**Depends on:** M181_001 (the probe runner this spec's evidence rows ride); **M181_004 merged** for Dimensions 1.1–1.2 — until the Rust daemon has an exporter it emits nothing, and a collector's continuity cannot be graded against an empty pipe. The collector deployment itself depends on neither
**Provenance:** LLM-drafted (Claude Opus 5, Sep 01, 2026) — §4's collector-first step of M181_002, split out on Indy's parallelization call
**Canonical architecture:** `docs/architecture/observability.md` §The three signal paths

---

## Overview

**Goal (testable):** every OTLP signal the daemon exports reaches Grafana Cloud through a collector that refuses an unauthenticated sender, on development and production, with no series renamed, dropped or decorated in transit.

**Problem:** the daemon posts telemetry direct to a vendor, so vendor identity is spelled into the daemon's own configuration: adding a backend, splitting a signal, or moving vendors becomes a daemon change with a window where both paths are half-configured, and fan-out to two backends is not expressible at all.

**This spec was drawn on a premise that turned out to be false, and the correction is the reason it exists in this shape.** It was written to stand collectors up under the *Zig* daemon first, so that infrastructure change and binary change would stay separately attributable across the cutover. The shipped `agentsfleetd` is already the Rust binary — `Dockerfile:39` copies `dist/agentsfleetd-rs-linux-${TARGETARCH}`, built by `cargo build --profile dist --bin agentsfleetd` (`release.yml:110,308`); the surviving `zig build` steps produce `agentsfleet-runner`, not the daemon. So there is no incumbent Zig export to stand in front of, the attribution argument that justified going first is void, and the honest ordering is the reverse of the one drawn: the daemon must learn to export before a hop can be proven to carry it.

**Solution summary:** deploy a collector per environment as its own Fly app on the private network, holding the vendor credential and owning the fan-out; repoint the daemon's endpoint at it by configuration, which is one staged string and no daemon source change; require a credential on the receiver, because a credentialed relay reachable by an organisation-wide private network is otherwise an open relay; and record the evidence in the cutover runbook's register so M181_006 inherits a proven telemetry path.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(deploy): the collector hop in front of the daemon's OTLP export
- **Intent (one sentence):** the daemon's telemetry leaves through a collector that owns the vendor relationship and refuses an unauthenticated sender, so the backend becomes configuration rather than a daemon change.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/observability.md` §The three signal paths — what the daemon exports and where it goes today.
2. `deploy/**` — the existing staged deploy/verify shape this rides; no new workflow shapes.
3. `playbooks/operations/cutover/001_playbook.md` — the runbook whose register records this step's evidence; M181_006 reads it.
4. M181_004's spec — the exporter this collector receives from, and the knob names the Rust daemon reads. Nothing here is provable until that lands; read its Interfaces before assuming the vendor-spelled triple survives.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `deploy/fly/otelcol-{dev,prod}/**` | CREATE | the collector app per environment — `Dockerfile` + `fly.toml` + `config.yml`, mirroring the `cloudflared-{dev,prod}` sidecar shape |
| `.github/workflows/deploy-dev-fly.yml` · `.github/workflows/release.yml` | EDIT | **where the endpoint actually lives.** `GRAFANA_OTLP_ENDPOINT` is a Fly secret staged from the vault by these two workflows (`deploy-dev-fly.yml:39,62`, `release.yml:513,539`), not a value in `deploy/`. The repoint is one changed string per environment; the collector's own upstream credentials are staged to the collector app |
| `docs/architecture/observability.md` | EDIT | §The three signal paths line 38 reads "Direct to Grafana Cloud; **no collector hop**" — the exact claim this spec falsifies. `dispatch/name_architecture.md` is no-override, so the doc is reconciled in the same diff |
| `playbooks/operations/cutover/001_playbook.md` | EDIT | the procedure and the evidence rows: collector serving, endpoint repointed, every signal arriving |
| `playbooks/operations/cutover/probes.sh` | EDIT | an executable probe for the collector path, tagged to this spec's rubric row |
| `playbooks/operations/cutover/coverage.tsv` | EDIT | the probe's row tags and this milestone's entry — **lands only in the CHORE(close) commit**, see §1's sequencing note |
| `playbooks/operations/cutover/probes_test.sh` | EDIT | the runner's self-test covers the new probe; it rides `make lint-all` via `lint-scripts` |
| `scripts/ensure_fly_app.sh` | CREATE | one parameterised deploy step for both workflows — it was inline in each, and the two copies drifted into two of the review's three critical findings. Owns the scale, the wait, the refusal, and the deployed-digest record |
| `scripts/ensure_fly_app_test.py` | CREATE | its self-test, discovered by `lint-scripts`; the refusal paths are the cases that matter, so a fake `flyctl` drives them |
| `scripts/collector_wiring_test.py` | CREATE | pins the wiring three shipped defects got wrong — which endpoint each side stages, and that the collector is ensured before the daemon deploys. Every assertion was made red against the real bug before it was trusted |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (endpoints and collector names as named values in config, not repeated literals), NDC (no collector features configured ahead of a consumer), TIM (batch/queue limits in collector config are named numbers), ECL (a backend outage during the change is an environment condition the probe distinguishes from a path defect).
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — the runbook rows are published prose.
- `dispatch/write_shell.md` — the probe additions: quoted expansions, no untrusted `eval`.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| CI/CD edit approval | yes | **GRANTED at PLAN (Indy, Sep 01, 2026)** — scope: author the repository diff (collector app, endpoint repoint, runbook rows, probe); Indy runs the Fly and Grafana Cloud rollout. No agent-run command touches staging or production. Quote in Discovery |
| LOGGING | no | no daemon source touched |
| LENGTH / UFS | yes | config files under the cap; shared endpoints named once |
| MILESTONE-ID | yes | none in config; playbook is docs (exempt) |
| SCHEMA GUARD | no | no schema change |

## Prior-Art / Reference Implementations

- **Reference:** `.github/workflows/deploy-dev*.yml` + `deploy/` — the staged deploy, verify and acceptance shape; this change reuses its verification pattern rather than inventing one.
- **Reference:** the declared-divergence register M181_001 seeded in the runbook — the evidence format this spec's rows append to.

## Sections (implementation slices)

### §1 — The collector carries every signal, and only for a caller that authenticates

One collector app per environment on the private network, holding the vendor credential and owning the fan-out. The daemon's endpoint is repointed by configuration; the backend is chosen in collector configuration with no daemon redeploy. The receiver requires a credential: the private network spans the whole organisation rather than this app pair, so without one the collector is a credentialed relay any workload on that network can post through.

- **Dimension 1.1** — every signal the daemon exports arrives at Grafana Cloud through the development collector, with no series renamed, dropped or decorated → Test `test_collector_path_carries_every_signal`
- **Dimension 1.2** — the same, on production, as a change window with a stated revert (point the endpoint back) → Test `test_collector_path_production_probe`
- **Dimension 1.3** — the runbook's register records the evidence, and the probe runner covers this spec's rubric row — an uncovered row is a red run → Test `test_runbook_probes` (the existing row-coverage assert, extended by the new tagged probe)

The dimensions above are graded from a change window. The four below are graded by the repository, and they exist because the review found three defects that every mechanical gate had already passed — each one a correctly-spelled variable meaning the wrong thing, or a correctly-formed step in the wrong place. A gate that cannot see those is not a gate for this diff.

- **Dimension 1.4** — **DONE** — the deploy step refuses rather than falling through when the collector never reaches its desired running count, so a daemon is never pointed at an app that is still starting → Tests `test_fails_when_machines_never_start` · `test_fails_when_running_count_is_below_desired` · `test_deploys_from_context_when_no_machines_exist`
- **Dimension 1.5** — **DONE** — in both environments the daemon's staged endpoint names the collector, the collector's own upstream names the vendor, and the ensure step precedes the deploy that applies the repoint → Tests `test_daemon_endpoint_points_at_the_collector` · `test_collector_upstream_points_at_the_vendor` · `test_collector_is_ensured_before_the_daemon_deploys` · `test_endpoint_names_the_collector_app_it_scales`
- **Dimension 1.6** — **DONE** — no pipeline carries an attributes, resource, transform or filter processor, and every signal has a pipeline; continuity is the deliverable, so a collector that decorates a series is the defect → Tests `test_collector_adds_no_attributes_to_any_pipeline` · `test_every_signal_has_a_pipeline`
- **Dimension 1.7** — **DONE** — the receiver refuses an unauthenticated sender, and the authenticator is registered in the service extensions rather than merely declared → Test `test_receiver_requires_authentication`

**Sequencing — the probe and its manifest rows land in the CHORE(close) commit, together, or not at all.** Three facts force this. `probes.sh:75` reads each milestone's rubric rows out of `SPEC_DONE_DIR` (`docs/v2/done`), so a `milestone	M181_005` row resolves to no spec while this one sits in `active/`. `probes.sh:189` fails any probe that declares no rubric row, so the probe cannot land ahead of its `covers` row either. And `make/quality.mk:95` runs `probes.sh --coverage` against the REAL tree inside `lint-all`, so any intermediate state is a red S3 rather than a private inconvenience. The collector app, the workflow repoint, the architecture reconciliation and the runbook rows land at EXECUTE; `probes.sh`, `coverage.tsv` and the spec's move to `done/` land in one commit at CHORE(close). `exclude	M181_005:R2` rides with them — R2 is merge-time diff scope, the same reason `M175_001:R6` and its siblings are already excluded. The self-tests need no new case: `probes_test.sh:33` derives its fixture rows from `coverage.tsv`, so a probe added to the table is covered there automatically.

## Interfaces

```
Daemon export endpoint          configuration only — repointed at the collector
Collector fan-out               backend selection lives in collector config, not the daemon
playbooks/operations/cutover/   the evidence register and the probe runner
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Dashboard gap after repoint | collector misconfigured or dropping | revert is one config change back to the direct endpoint; the probe names the dark panel; recorded before M181_006 may start |
| Silent partial delivery | one signal type dropped by collector config | the probe compares series presence per signal type, not liveness alone |
| Backend outage during the window | environment, not the change | probe distinguishes collector-unreachable from backend-unreachable (ECL); the window holds until attribution is clean |
| Evidence not recorded | change landed, register not updated | `test_runbook_probes` row-coverage assert fails — an untagged probe or unrecorded row is a red run |

## Invariants

1. No daemon source changes — the diff contains no `src/**` or `rustd/**` path; enforced by rubric R2's Files-Changed check. The repoint is configuration, and the exporter that fills it is M181_004's.
2. Rollback of this change is one configuration edit, stated in the runbook row before the change is made.
3. Every rubric row here is probe-tagged or manifest-declared — the probe runner's existing row-coverage assert.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| every exported family, now through the collector | ops | unchanged | unchanged | unchanged — the collector adds no attributes | `test_collector_path_carries_every_signal` |

No product-analytics changes; no new panels — continuity is the deliverable.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | e2e (development) | `test_collector_path_carries_every_signal` | per-signal series present through the collector; no renamed series; panel set unchanged |
| 1.2 | e2e (production window) | `test_collector_path_production_probe` | same assertion, production; revert path stated before execution |
| 1.3 | unit | `test_runbook_probes` | row-coverage: this spec's rows are tagged or manifest-declared |
| 1.4 | unit | `ensure_fly_app_test.py` (9 cases) | a fake `flyctl` drives the refusal paths: stopped machines, a running count below desired, and a from-scratch deploy that must carry its build context |
| 1.5 | unit | `collector_wiring_test.py` | each assertion made red against the real shipped defect before it was trusted |
| 1.6 | unit | `collector_wiring_test.py` | processor sets per pipeline, per environment |
| 1.7 | unit | `collector_wiring_test.py` | receiver authenticator present and registered |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The collector carries every signal to Grafana Cloud, authenticated (§1) | `bash playbooks/operations/cutover/probes.sh` | exit 0, collector rows green | P0 | ⏳ blocked, not merely pending — M181_004 must land first (the daemon exports nothing until it does), then graded from the change window, which is Indy's. The probe and its manifest rows land in the same commit, per §1's sequencing note |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table; no `src/**` or `rustd/**` path present | P0 | ✅ 11 paths, every one in the table; no `src/**` or `rustd/**` path, so Invariant 1 holds. Measured against the branch base `ac5a00157`, not `origin/main`, which is one unpushed commit behind and would otherwise attribute that commit's two spec files to this diff |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | ✅ exit 0 |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ exit 0 — 6907 (rustd 2186 + app 2410 + website 175 + cli 1624 + design-system 512) |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | ✅ exit 0 — carries `check-cutover-probes`, so the probe runner's asserts ran for real against this tree |
| S4 | Version sync | `make check-version` | exit 0 | P0 | ✅ exit 0 |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ no leaks found, 5051 commits scanned |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ⚠️ flagged, surfaced not decided — `.github/workflows/release.yml` is 904 lines. It was **861 at the branch base**, so this is a pre-existing violation the diff touched rather than created; the 43 lines added are the collector stand-up step. It is the only workflow over the cap. Splitting it is a refactor outside this spec's Files Changed and past RULE NLR's ~200-line bundling bound. Disposition is Indy's |

**Command source rule:** S1–S4 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.unit`, `verify.lint`, `verify.version`); S5–S6 are the template's repository hygiene gates. The integration lane is omitted: this diff carries no Rust and the lane grades none of it.

**The S-row letters are positional, not free.** `playbooks/operations/cutover/coverage.tsv` maps hygiene rows by LETTER across every merged milestone — `covers version *:S4`, `covers secrets *:S5`, `exclude *:S6` — and `probes.sh` derives the row set from each merged spec's own rubric table (`probes.sh:75`). This spec was drawn with `gitleaks` at S4 and no version or oversize row, which would make the runner report a phantom `M181_005:S5` (`probes.sh:161`) and reject the `*:S6` exclusion (`probes.sh:180`) the moment this milestone joined the list. Amended to the convention M179_001 and its siblings already carry.

**Test Delta (VERIFY).** `unit 6907 → 6907` (+0) against the CHORE(open) baseline, and the zero needs its explanation stated rather than assumed, because the diff DID add 16 tests. They are `scripts/*_test.py`, which `lint-scripts` discovers and `test-unit-all` never sees — 9 cases in `ensure_fly_app_test.py` and 7 in `collector_wiring_test.py`, all green inside `lint-all`. Counting them where they run rather than where the baseline was taken is the honest reading; a reader comparing 6907 to 6907 would otherwise conclude a code-adding diff shipped no tests. On the counter the baseline actually measures, the rule's concern does not apply: the rule flags zero growth on a **code-adding** diff, and this diff adds no code to those lanes — two collector app directories of Fly configuration, two workflow steps, an architecture reconciliation and runbook prose. The proof that it is nonetheless tested is `lint-all`, which runs the probe runner's three asserts against this tree, plus the eight playbook and workflow gates the diff triggers. `make test-integration-rustd` ran at the boundary on Indy's instruction: `349 passed`, unchanged by this diff and expected to be.

**One red run, classified as environment and not defect (RULE ECL), with the evidence rather than the story.** A boundary run failed `integration_pool::test_pool_error_classes` at `crates/afd_db/tests/integration_pool.rs:79` — `the lane's Postgres must hand out one connection within the warm-up attempts: [UZ-INTERNAL-001] the api datastore did not answer within 250ms`. Three facts settle it: the diff carries no Rust, so nothing in it can reach that pool; the same lane returned `349 passed` before the run and `349 passed` on an immediate re-run after it; and the assertion is a 250ms warm-up bound taken while three worktrees, a cargo workspace build and the playbook suite shared one docker Postgres. A tighter bound would be a real finding — it is recorded here rather than repaired, because widening a timing bound to make a lane green is the failure that rule exists to prevent.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE.

## Dead Code Sweep

N/A — no files deleted; the direct-to-backend endpoint configuration is superseded in place, and its removal is the one-line revert path until M181_006 closes.

## Out of Scope

- The Rust daemon's export — M181_004.
- The swap, the soak, and continuity across the boundary — M181_006.
- Any dashboard change: a new panel at this step would be indistinguishable from a regression.

## Product Clarity (authoring record)

1. **Successful user moment** — an operator watches the same dashboards before and after the repoint and cannot tell anything changed.
2. **Preserved user behaviour** — everything; the request path is untouched by construction.
3. **Optimal-way check** — a collector hop beats posting direct to a vendor: the backend becomes configuration applied without redeploying the thing that serves requests, and fan-out to a second destination becomes expressible at all. The ORDERING argument the parent spec used — collectors first, under the incumbent binary — is void, because the incumbent is already the Rust binary and exports nothing.
4. **Rebuild-vs-iterate** — N/A: pure deployment change.
5. **What we build** — collector deployment, endpoint repoint, probe, evidence rows.
6. **What we do NOT build** — daemon changes, dashboards, backend changes.
7. **Fit with existing features** — rides the existing deploy/verify workflow shape, mirroring the `cloudflared-{dev,prod}` sidecar apps already in the tree; the daemon keeps one endpoint, one credential and one failure mode however many backends exist downstream.
8. **Surface order** — N/A — no user surface.
9. **Dashboard restraint** — nothing new to show, by design.
10. **Confused-user next step** — an operator seeing a dark panel reads the runbook row: the revert is one config edit, named there.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one slice — the change is one attributable step and splitting it further would manufacture windows.
- **Alternatives considered:** folding into M181_004 (rejected: mixes Rust and deploy config in one PR, which is the attribution loss §4's own reasoning argues against); deploying with the swap in M181_006 (rejected: the parent spec's optimal-way check).
- **Patch-vs-refactor verdict:** this is a **patch** to the operational layer; no code shape changes.

## Discovery (consult log)

> Indy (2026-09-01): "i wanna see what can be batched parallelized and break to smaller PRs?" … "Yes, 5 specs as drawn" — context: §4's collector-first step of M181_002, split out to run fully parallel; the deploy-approval gate still applies per-edit.

- **Consults** — Architecture / Legacy-Design / gate-flag triage:
  - **Deploy scope (Sep 01, 2026).** Asked before any edit, per the gate this spec declares. Options put: repository diff with Indy running the rollout · collectors deployed but unwired · hold and amend the spec first.
    > Indy (2026-09-01): chose "Repo diff, you run the rollout (Recommended)" — context: `deploy/**` and the two workflow files are approved for edit; the Fly and Grafana Cloud rollout is Indy's, and every rubric row here is graded from the output of that run.
  - **The endpoint is not in `deploy/` (Sep 01, 2026) — Files Changed amended.** Read from source rather than assumed: `GRAFANA_OTLP_ENDPOINT` is a Fly secret staged from the vault by `.github/workflows/deploy-dev-fly.yml:39,62` and `.github/workflows/release.yml:513,539`; neither `deploy/fly/agentsfleetd-dev/fly.toml` nor its production twin mentions it. A spec whose blast radius stops at `deploy/**` cannot repoint anything. Both workflows join the table under the approval above.
  - **Architecture consult — the doc this spec falsifies (Sep 01, 2026).** `docs/architecture/observability.md:38` reads "Direct to Grafana Cloud; **no collector hop**". `dispatch/name_architecture.md` carries no override — the doc wins until reconciled — so the line is reconciled in this diff rather than left to contradict the deployment. The one-directional assert in `probes.sh:225` is unaffected: it forbids claiming a scrape path the deployment lacks, and this change adds no `[[metrics]]` block and no pull endpoint. D2 of the register stays true.
  - **No daemon edit is needed for the repoint, and none is permitted (Sep 01, 2026).** The repoint changes ONE staged string per environment; the collector holds the vendor credential and forwards. Invariant 1 holds by construction rather than by discipline. The Zig reading that produced this — `src/agentsfleetd/observability/otlp/config.zig:60-70` requiring all three knobs or disabling every signal — described a binary that is not the one deployed; see the premise correction below.
  - **RESOLVED — the premise was wrong: the shipped daemon is Rust and exports nothing (Sep 02, 2026).** Indy: "Well i donot use the agentsfleetd zig code." Checked rather than assumed: `Dockerfile:39` copies `dist/agentsfleetd-rs-linux-${TARGETARCH}`; `release.yml:110,308` build it with `cargo build --profile dist --bin agentsfleetd`; the surviving `zig build` steps (`release.yml:162,259`) produce `agentsfleet-runner`. And the Rust daemon has no exporter — `git grep GRAFANA_OTLP -- rustd/` returns nothing and `rustd/Cargo.toml` declares no OTLP dependency, which is exactly M181_004's own Problem statement. So the `GRAFANA_OTLP_*` secrets staged onto both Fly apps today are inert, this spec's original goal (continuity of an incumbent Zig export) was ungradeable as written, and the dependency runs opposite to the drawn one. Re-aimed at the Rust daemon; Dimensions 1.1–1.2 now sit behind M181_004.
    > Indy (2026-09-02): "I think this is aimed at the rust daemon" — context: re-aim approved, and the branch renamed off `under-zig` in the same breath.
  - **Receiver authentication (Sep 02, 2026).** Review found the OTLP receiver listening with no inbound auth. Fly's 6PN spans the organisation, not the app pair, so any workload on it could post series the collector forwards upstream under the real vendor credential — a boundary that had been "hold the vendor key" quietly became "be on the network". Closed with the `basicauth` extension's server half, which costs no daemon change because the daemon already sends that pair on every export and the collector was discarding it unread.
    > Indy (2026-09-02): "i will go with basic auth for an otel collector in fly" — context: options were receiver auth, image pinning, collector scale, and factoring the duplicated deploy step; auth chosen.
  - **Rubric S-row renumber (Sep 01, 2026).** Mechanical, applied under gate-flag triage: the letters are read positionally by `coverage.tsv`, this spec's drawing broke two asserts in `probes.sh`, and the fix is the convention every merged sibling already carries. Detail beneath the rubric.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
