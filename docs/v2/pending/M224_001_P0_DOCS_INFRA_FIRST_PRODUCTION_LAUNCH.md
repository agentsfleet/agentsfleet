<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M224_001: First production launch — tag `v0.59.0` ships the signed Rust runner release to production, and the four reference bundles settle a thread each

**Prototype:** v2.0.0
**Milestone:** M224
**Workstream:** 001
**Date:** Oct 12, 2026
**Status:** PENDING
**Priority:** P0 — production has never run the Rust runner; `api.agentsfleet.net` does not resolve today
**Categories:** DOCS, INFRA
**Batch:** B1 — one launch; Indy pushes the tag, the agent proves each step before and after
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M213_001 — this spec inherits its R4 and Dimension 1.1 whole, and the three reference-bundle threads of its Dimension 1.3 and R5 (transfer, Discovery)
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 12, 2026) from M213_001's close-out: `.github/workflows/release.yml`, dev run 38176583117, smoke run 38176618397, `GET /v1/admin/fleet-libraries` on dev
**Canonical architecture:** `docs/architecture/runner_execution.md` §Toolbox; `docs/architecture/scenarios/slack-incident-responder.md`; `docs/architecture/scenarios/production-deploy-repair.md`

---

## Overview

**Goal (testable):** `test_release_artifacts_are_static_and_addressed` — the `v0.59.0` release run is green, its GitHub Release carries the static runner, the signed toolbox manifest, its SBOM and the offline bundle, production serves `api.agentsfleet.net`, and four reference-bundle threads settle.
**Problem:** Every fleet a customer would install today has nowhere to run: no release has been cut under the `agentsfleet` name (`gh release list` is empty; the last `release.yml` run is `v0.4.0` on Apr 08, 2026), and the production API hostname does not resolve (smoke run 38176618397: `getaddrinfo ENOTFOUND api.agentsfleet.net`). Three of the four reference bundles have never settled a thread on the Rust runner, because their triggers need a Slack channel, credentials and a library entry dev does not have.
**Solution summary:** Indy pushes `v0.59.0`; `release.yml` builds, signs, publishes and deploys production (Fly API, metal canary, metal fleet). Before the tag, the agent closes the preflight gaps (DNS, credentials gate, library entries); after it, the agent proves the release artifacts and drives the reference-bundle triggers, pausing where a step is Indy's to take.

## PR Intent & comprehension handshake

- **PR title (eventual):** chore(m224): record the first production launch and its bundle proofs
- **Intent (one sentence):** A fleet owner can install a reference bundle against production and see its thread settle, because the Rust runner release is live there.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `.github/workflows/release.yml` — the whole launch: credentials gate, tag/`VERSION` agreement, static builds, toolbox signing, npm `next`, the GitHub Release, the three production deploy jobs.
2. `docs/v2/active/M213_001_P0_API_DOCS_INFRA_RUST_RUNNER_CUTOVER_ZIG_RETIRED.md` — Discovery holds the toolbox key, the 262,848,512-byte image ceiling and the dev proofs this launch repeats on production (it moves to `docs/v2/done/` when it closes).
3. `docs/architecture/scenarios/slack-incident-responder.md` — the mention and repair-line threads, and what each must show.
4. `playbooks/deploy/prod/001_playbook.md` — production diagnosis rules (no shell in the API image).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M224_001_P0_DOCS_INFRA_FIRST_PRODUCTION_LAUNCH.md` | EDIT | the spec itself: Dimensions, Graded column and Discovery evidence |
| `playbooks/operations/acceptance/baselines/` | CREATE | the launch's baseline evidence file |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — NLG (no compatibility framing for the retired Zig release), NDC.
- `.orly/dispatch/lifecycle.md` — release and deploy edits need Indy's per-item approval; `gh release create` and `git push --tags` are his.
- `.orly/dispatch/write_documentation.md` — any changelog entry the launch earns in `~/Projects/docs`.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SPEC TEMPLATE | yes — this file | sections complete, no template residue |
| Releases / CI edits (Hard Safety) | yes — the tag push | Indy pushes the tag; any workflow fix found by preflight is asked by name before it is edited |
| File & Function Length (≤350/≤50/≤70) | no | docs only |

## Prior-Art / Reference Implementations

- **Reference:** dev run [38176583117](https://github.com/agentsfleet/agentsfleet/actions/runs/38176583117) — the same build, sign and metal deploy, green on `875fe40c9`; production differs by the tag trigger and the prod vault.

## Sections (implementation slices)

### §1 — Preflight: production can receive a release

The tag must not be the first thing that finds a missing hostname or credential. **Implementation default:** run `ENV=prod STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh` and resolve `api.agentsfleet.net` before asking Indy to tag, because `release.yml` runs that gate first and fails the whole launch on it.

- **Dimension 1.1** — The production credential gate passes and `api.agentsfleet.net` resolves to the production Fly app → Test `test_prod_preflight_passes`

### §2 — The release (inherited from M213_001 R4 and Dimension 1.1)

- **Dimension 2.1** — The binary is static; the toolbox is content-addressed, its manifest verifies against the release public key, and the SBOM and offline bundle ship with it on the GitHub Release → Test `test_release_artifacts_are_static_and_addressed`
- **Dimension 2.2** — Production's runner registers with Landlock, seccomp, `cpu io memory pids`, bubblewrap and egress enforcement true and `degraded: false` → Test `test_prod_runner_registers_with_capabilities`

### §3 — The reference bundles settle (inherited from M213_001 Dimension 1.3 and R5)

Each needs a trigger only Indy or his accounts can send; the agent prepares, Indy acts, the agent records. **Implementation default:** prove on dev first, then production, so a bundle bug is found where it costs nothing.

- **Dimension 3.1** — `ci-responder`, bound to a Slack test channel Indy names, answers a mention in that thread → Test `test_mention_thread_settles`
- **Dimension 3.2** — `ci-repairer`, addressed in that channel with a repair line, opens one draft Pull Request on `agentsfleet/linkwarden` → Test `test_repair_line_opens_one_draft_pr`
- **Dimension 3.3** — `incident-repairer` has a platform library entry and its `elastic`, `grafana` and `github` credentials, and a failed `workflow_run` on `agentsfleet/agentsfleet` settles its thread → Test `test_workflow_run_thread_settles`

## Interfaces

```
git tag v0.59.0 && git push origin v0.59.0          # Indy
GET /v1/fleets/runners/{runner_id}                  # achievable{landlock,seccomp,cgroup_controllers,bubblewrap,egress_enforcement}, degraded
GET /v1/workspaces/{ws}/fleets/{fleet}/events/{id}  # status=processed, failure_label=null
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Tag and `VERSION` disagree | tag pushed off a stale commit | `verify-tag` fails before any build; delete nothing, re-tag from the right commit (Indy) |
| Production credential missing | vault item absent | `check-credentials` fails first and names the item; nothing deploys |
| Toolbox over its ceiling | an image change grew it | the build refuses before signing; no release is created |
| Runner refuses boot | a controller or capability missing on a production host | `deploy.sh` calls the unit unhealthy and the canary stops the fleet rollout |
| Bundle thread fails | bundle, credential or egress fault | the event records `failure_label`; fixed on dev before the production retry |

## Invariants

1. A release never deploys a binary whose toolbox manifest does not verify against the built-in public key — enforced by the runner's admission at boot (`afr_sandbox::toolbox::admit`) and `deploy.sh`'s bundle check.
2. The tag matches `VERSION` — enforced by `release.yml`'s `verify-tag` job.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | manual | `test_prod_preflight_passes` | the preflight gate exits 0; `dig +short api.agentsfleet.net` prints an address |
| 2.1 | manual | `test_release_artifacts_are_static_and_addressed` | release run green; zero `NEEDED` for the binary; toolbox digest matches and length ≤262,848,512 bytes; Release lists the four artifacts |
| 2.2 | manual | `test_prod_runner_registers_with_capabilities` | the runner read shows the five capabilities true and `degraded: false`; output pasted |
| 3.1 | manual | `test_mention_thread_settles` | one Slack mention → one `processed` event; thread link pasted |
| 3.2 | manual | `test_repair_line_opens_one_draft_pr` | one repair line → one draft Pull Request link, one `processed` event |
| 3.3 | manual | `test_workflow_run_thread_settles` | one failed `workflow_run` → one `processed` event on `incident-repairer` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Production can receive a release (§1) | `dig +short api.agentsfleet.net` | non-empty | P0 | |
| R2 | The release ships the static binary, the signed toolbox, its SBOM and the offline bundle (§2; M213_001 R4) | manual — the release run URL and its verify-step output in Session Notes | green run URL | P0 | |
| R3 | Four reference-bundle threads settle (§3; M213_001 R5) | manual — four thread links in Session Notes, the `github-pr-reviewer` one inherited from M213_001's Discovery | four links | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.orly/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `.orly/dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `.orly/docs/TEMPLATE.md`.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- An arm64 release, dm-verity and Firecracker — their own specs, as M213_001 recorded.
- Posting the `github-pr-reviewer` review: egress refuses the write today (`docs/architecture/scenarios/github-pr-reviewer.md` §4); M225_001 records the token cost that refusal causes.
- Revoking a compromised toolbox — deferred by Indy in M213_001.

## Product Clarity (authoring record)

1. **Successful user moment** — A fleet owner mentions `@agentsfleet` in a Slack thread on production and the answer lands in that thread.
2. **Preserved user behaviour** — Dev keeps serving; the CLI's `latest` npm tag is untouched (the release publishes to `next`).
3. **Optimal-way check** — Tag-driven release is the shortest path; the gap is that preflight is manual, accepted because this is the first launch.
4. **Rebuild-vs-iterate** — Iterate: the pipeline already proved itself on dev.
5. **What we build** — No code; a preflight, a tag, four proofs.
6. **What we do NOT build** — Release automation beyond `release.yml`; see Out of Scope.
7. **Fit with existing features** — Must not destabilise the dev lane, which keeps its own deploy.
8. **Surface order** — N/A — no new user surface; the launch makes existing ones reachable.
9. **Dashboard restraint** — N/A — no dashboard change.
10. **Confused-user next step** — `agentsfleet doctor` against production names an unreachable API.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** preflight, release, bundle proofs, in that order, because each needs the one before.
- **Alternatives considered:** keeping these rows in M213_001 and parking it (rejected by Indy: "Move to a launch spec (Recommended)").
- **Patch-vs-refactor verdict:** this is a **patch** because it changes no code; it runs the pipeline M213_001 built.

## Discovery (consult log)

- **Consults** — Transfer from M213_001 (Oct 12, 2026). Inherited whole: M213_001 R4 and Dimension 1.1 → this spec's R2 and Dimension 2.1; M213_001 R5 and the three unproved bundles of its Dimension 1.3 → this spec's R3 and Dimensions 3.1–3.3. The fourth bundle, `github-pr-reviewer`, settled on dev in M213_001 (event `1791777420543-1356`) and carries over as a link, not a Dimension.
  > Indy (2026-10-12): "release tag - defer/park i will push a production push later" — context: M213_001 R4, the first production launch.
  > Indy (2026-10-12): "Move to a launch spec (Recommended)" — context: where M213_001's P0 release row goes.
  > Indy (2026-10-12): "Only the two GitHub ones" — context: which bundle triggers M213_001 runs; the Slack pair moves here.
  Dimension 3.3 (`incident-repairer`) was one of "the two GitHub ones"; it could not run on dev for the blockers below, and its transfer here waits on Indy's word in M213_001's close-out.
  Known blockers found while closing M213_001: `api.agentsfleet.net` does not resolve (smoke run 38176618397, `ENOTFOUND`); the dev platform library holds only `github-pr-reviewer`; no `incident-repairer` source repository exists; the dev workspace holds no secrets.
- **Metrics review** — no analytics or funnel playbook update required: no product signal changes.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
