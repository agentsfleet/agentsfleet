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

# M181_005: Collectors deployed under the Zig daemon, dashboards unbroken

**Prototype:** v2.0.0
**Milestone:** M181
**Workstream:** 005
**Date:** Sep 01, 2026
**Status:** PENDING
**Priority:** P0 — on the cutover's critical path: continuity across the swap is graded through these collectors
**Categories:** INFRA | OBS
**Batch:** B6 — fully parallel: touches deploy configuration only and serves the export path that exists today
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
**Depends on:** M181_001 (the probe runner this spec's evidence rows ride); nothing in the Rust tree — the Zig daemon's endpoint is already configuration
**Provenance:** LLM-drafted (Claude Opus 5, Sep 01, 2026) — §4's collector-first step of M181_002, split out on Indy's parallelization call
**Canonical architecture:** `docs/architecture/observability.md` §The three signal paths

---

## Overview

**Goal (testable):** the collectors serve the Zig daemon's existing export on staging and production with every dashboard panel continuous — before any binary changes anywhere.

**Problem:** the cutover wants to grade signal continuity across a binary swap. If the collectors and the binary change together, a swap-day anomaly is unattributable: it could be the new infrastructure or the new daemon. The Zig daemon's export endpoint is already configuration, so the collector layer can land first, under the incumbent binary, as its own separately attributable change.

**Solution summary:** stand the collectors up in front of the Zig daemon's export; repoint the daemon's endpoint configuration at them; let the collectors' own configuration fan out to the existing backend so no dashboard notices; record the evidence in the cutover runbook's register so M181_006 inherits a proven telemetry path.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(deploy): collectors in front of the Zig daemon's export
- **Intent (one sentence):** the telemetry path the cutover will rely on is proven under the incumbent binary, so infrastructure change and binary change stay separately attributable.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/observability.md` §The three signal paths — what the daemon exports and where it goes today.
2. `deploy/**` — the existing staged deploy/verify shape this rides; no new workflow shapes.
3. `playbooks/operations/cutover/001_playbook.md` — the runbook whose register records this step's evidence; M181_006 reads it.
4. `src/agentsfleetd/cmd/serve*.zig` knob surface — the vendor-named endpoint configuration being repointed; config only, no Zig edits.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `deploy/**` | EDIT | collector deployment and configuration; the daemon's export endpoint repointed at it |
| `playbooks/operations/cutover/001_playbook.md` | EDIT | the evidence row: collectors serving under Zig, dashboards verified continuous |
| `playbooks/operations/cutover/probes.sh` | EDIT | an executable probe for the collector path, tagged to this spec's rubric row |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (endpoints and collector names as named values in config, not repeated literals), NDC (no collector features configured ahead of a consumer), TIM (batch/queue limits in collector config are named numbers), ECL (a backend outage during the change is an environment condition the probe distinguishes from a path defect).
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — the runbook rows are published prose.
- `dispatch/write_shell.md` — the probe additions: quoted expansions, no untrusted `eval`.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| CI/CD edit approval | yes | `deploy/**` changes need Indy's explicit approval per the hard-safety list — sought at PLAN, before any edit |
| LOGGING | no | no daemon source touched |
| LENGTH / UFS | yes | config files under the cap; shared endpoints named once |
| MILESTONE-ID | yes | none in config; playbook is docs (exempt) |
| SCHEMA GUARD | no | no schema change |

## Prior-Art / Reference Implementations

- **Reference:** `.github/workflows/deploy-dev*.yml` + `deploy/` — the staged deploy, verify and acceptance shape; this change reuses its verification pattern rather than inventing one.
- **Reference:** the declared-divergence register M181_001 seeded in the runbook — the evidence format this spec's rows append to.

## Sections (implementation slices)

### §1 — Collectors serving the incumbent export

The collectors deploy first, under the Zig daemon. Standing them up in front of the incumbent binary proves the telemetry path with dashboards intact and nothing else changing. The daemon's endpoint is repointed by configuration; the collectors fan out to the existing backend, chosen in collector configuration with no daemon redeploy.

- **Dimension 1.1** — the collectors serve the Zig daemon's export on staging with every dashboard panel continuous → Test `test_collector_path_under_zig`
- **Dimension 1.2** — the same, on production, as a change window with a stated revert (point the endpoint back) → Test `test_collector_path_production_probe`
- **Dimension 1.3** — the runbook's register records the evidence, and the probe runner covers this spec's rubric row — an uncovered row is a red run → Test `test_runbook_probes` (the existing row-coverage assert, extended by the new tagged probe)

## Interfaces

```
Zig daemon export endpoint      configuration only — repointed at the collectors
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

1. The Zig daemon's binary and flags are untouched — the diff contains no `src/**` path; enforced by rubric R2's Files-Changed check.
2. Rollback of this change is one configuration edit, stated in the runbook row before the change is made.
3. Every rubric row here is probe-tagged or manifest-declared — the probe runner's existing row-coverage assert.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| existing families, now through the collectors | ops | unchanged | unchanged | unchanged — the collectors add no attributes | `test_collector_path_under_zig` |

No product-analytics changes; no new panels — continuity is the deliverable.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | e2e (staging) | `test_collector_path_under_zig` | per-signal series present through the collectors; no renamed series; panel set unchanged |
| 1.2 | e2e (production window) | `test_collector_path_production_probe` | same assertion, production; revert path stated before execution |
| 1.3 | unit | `test_runbook_probes` | row-coverage: this spec's rows are tagged or manifest-declared |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Collectors serve the Zig export, dashboards continuous (§1) | `bash playbooks/operations/cutover/probes.sh` | exit 0, collector rows green | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table; no `src/**` or `rustd/**` path present | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** S1–S3 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.unit`, `verify.lint`); S4 is the template's hygiene gate. The integration lane is omitted: this diff carries no Rust and the lane grades none of it.

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
3. **Optimal-way check** — collectors under the incumbent binary first beats collectors-with-the-swap: one ambiguous change becomes two attributable ones. That reasoning is inherited from the parent spec and is the whole point of the split.
4. **Rebuild-vs-iterate** — N/A: pure deployment change.
5. **What we build** — collector deployment, endpoint repoint, probe, evidence rows.
6. **What we do NOT build** — daemon changes, dashboards, backend changes.
7. **Fit with existing features** — rides the existing deploy/verify workflow shape; the Zig release path stays intact as the rollback.
8. **Surface order** — N/A — no user surface.
9. **Dashboard restraint** — nothing new to show, by design.
10. **Confused-user next step** — an operator seeing a dark panel reads the runbook row: the revert is one config edit, named there.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one slice — the change is one attributable step and splitting it further would manufacture windows.
- **Alternatives considered:** folding into M181_004 (rejected: mixes Rust and deploy config in one PR, which is the attribution loss §4's own reasoning argues against); deploying with the swap in M181_006 (rejected: the parent spec's optimal-way check).
- **Patch-vs-refactor verdict:** this is a **patch** to the operational layer; no code shape changes.

## Discovery (consult log)

> Indy (2026-09-01): "i wanna see what can be batched parallelized and break to smaller PRs?" … "Yes, 5 specs as drawn" — context: §4's collector-first step of M181_002, split out to run fully parallel; the deploy-approval gate still applies per-edit.

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
