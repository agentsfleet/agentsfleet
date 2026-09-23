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

# M206_004: In #ci-dev, then #ci-prod, a failed run's thread gets an evidence-cited diagnosis and, on request, one draft fix PR

**Prototype:** v2.0.0
**Milestone:** M206
**Workstream:** 004
**Date:** Sep 23, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the operator-facing proof of the milestone; nothing before it touches a real Slack thread, a real failed run, or a real Grafana stack.
**Categories:** DOCS, INFRA, SKILL
**Batch:** B1 for §1–§2, which share no source file with M206_001–003; §3's drills in B3, after B2 reaches `api-dev`. Its own Pull Request, the milestone's follow-up.
**Branch:** feat/m206-004-ci-incident-drill
**Baseline revision:** f3edd3c17062087a6db7f7e271606bf0c3901259
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M206_001, M206_002, M206_003 — delivery, routing, evidence and write reach, which the drills exercise.
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 23, 2026) from source reads at `b1bc6f0c4`
**Canonical architecture:** `docs/architecture/scenarios/slack-incident-responder.md` §7–§11

---

## Overview

**Goal (testable):** in `#ci-dev` a mention in a failed run's thread gets one threaded answer citing the run, its failed job and step, a log line, and Grafana readings over the run's window, or naming each source it could not read; `@agentsfleet ci-dev-repairer open the fix` yields exactly one draft Pull Request (PR) whose link lands in the thread, and nothing merges; the same holds in `#ci-prod` after the development drill is recorded.

**Problem:** nothing written down gets a team from "Slack is connected" to "a failed run's thread gets a real answer". The shipped responder posts to Slack itself through a pasted bot token (`tests/fixtures/fleetbundle/incident-responder/TRIGGER.md:25-39`) and reads Loki rather than job logs; no bundle answers a mention. The App registration grants Contents read-only and no Checks permission (`playbooks/operations/github_app_registration/001_playbook.md:33-38`), while a write mint asks for `contents: write` and M206_003's read mint asks for `checks: read`; unverified: the live App's settings, settled by reading its permissions page. A draft fix opens without a per-request approval under the owner's Claude Tag decision, and a branch pushed into the same repository runs that repository's workflows, so which workflows can reach deploy secrets from `agentsfleet-repair/*` decides the blast radius; unverified: the current workflows' triggers and secret scopes.

**Solution summary:** two bundles join the fixture corpus: `ci-responder`, read-only with no Slack credential, and `ci-repairer`, write-bound to one repository and base. A playbook carries every external step apart from product code, including the App permission change and keeping deploy secrets off repair branches. The drills run on `api-dev` first and production after, with evidence recorded per stage.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(drill): a CI failure's Slack thread gets a cited diagnosis and one draft fix on request`
- **Intent (one sentence):** the team's own failed runs are diagnosed where they are announced, from evidence the fleet actually read, and a fix is one draft PR away.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `tests/fixtures/fleetbundle/incident-responder/SKILL.md` and `tests/fixtures/fleetbundle/incident-repairer/TRIGGER.md` — the grounding rule and the write binding the drill bundles adapt.
2. `rustd/crates/afd_fleet_runtime/tests/support/mod.rs` — `FIRST_PARTY` and the corpus every bundle is graded against.
3. `playbooks/operations/slack_app_registration/001_playbook.md` — development-first order and the manifest the drill reuses.
4. `playbooks/operations/github_app_registration/001_playbook.md` — the permission list the drill changes.
5. `docs/architecture/scenarios/slack-incident-responder.md` — §7 authorities and §11 channels.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `tests/fixtures/fleetbundle/ci-responder/{SKILL.md,TRIGGER.md}` · `tests/fixtures/fleetbundle/ci-repairer/{SKILL.md,TRIGGER.md}` | CREATE | The drill bundles. |
| `tests/fixtures/fleetbundle/trigger/mention_valid.md` | CREATE | A parser fixture for the `mention` trigger; the bundles ship without a channel and gain one at install. |
| `rustd/crates/afd_fleet_runtime/tests/support/mod.rs` | EDIT | Both bundles join `FIRST_PARTY` and the corpus. |
| `playbooks/operations/slack_incident_drill/001_playbook.md` | CREATE | Every external step, the secret hardening, evidence capture, and the negative drills. |
| `playbooks/operations/github_app_registration/001_playbook.md` | EDIT | Checks read-only; Contents read and write. |
| `playbooks/operations/acceptance/drills/ci-incident-dev.md` · `ci-incident-prod.md` | CREATE | Recorded evidence per stage. |
| `docs/architecture/scenarios/slack-incident-responder.md` · `scenarios/production-deploy-repair.md` | EDIT | Proof status, one line per proven stage; the bundle location. |

Cross-repository, on its own branch: `~/Projects/docs/fleets/library.mdx` (the bundles live under `tests/fixtures/fleetbundle/`, not `library/`; the repairer's approval card went with M202), `fleets/connectors.mdx`, `changelog.mdx`. A change to `.github/workflows/**` that the secret hardening may need is Indy's explicit call, per `AGENTS.orly.md`.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — PRI (thread and log text are data), VLT (the Grafana token lives in the vault, never pasted), CTX, TST-NAM, TFX (fixtures use the production constants), FLL.
- `docs/SKILL_FRONTMATTER_SCHEMA.md` — both bundles' frontmatter.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| MILESTONE-ID | yes | No milestone identifiers in bundles, fixtures or test names. |
| File & Function Length (≤350/≤50/≤70) | yes — `tests/support/mod.rs` | Two corpus rows; no new function. |

## Prior-Art / Reference Implementations

- **Reference:** `tests/fixtures/fleetbundle/incident-responder` and `incident-repairer` — grounding rule, read-only reach, draft-PR-only writes. Divergences: answers go through the daemon, never a Slack credential; the responder reads the daemon-attached job logs (M206_003 §2).
- **Reference:** Claude Tag's setup (`claude.com/docs/claude-tag/admins/setup-overview.md`) — pair, connect GitHub, set a spend limit, invite to channels; the playbook follows the same order with the install flag as the attach step.

## Sections (implementation slices)

### §1 — Two drill bundles

`ci-responder`: tools `http_request`, `memory_store`, `memory_recall`; credentials `github` and `grafana` only; a read binding on `agentsfleet/agentsfleet`; network `api.github.com` and the Grafana host, `read_only: true`. Its skill: use the evidence block the daemon attached; stop if the thread names another repository; read the commits since the last green run; read Grafana over the run's window; recall prior occurrences; answer with cited evidence, a labelled hypothesis, a proposed fix, and the exact sentence that asks the repairer. `ci-repairer`: a write binding on the same repository with `repository_base: main`; it reads the thread's diagnosis, re-reads the files at head, writes on the daemon-issued branch, opens one draft PR, and answers with its link. Neither ships a channel; `install --slack-channel` adds it.

- **Dimension 1.1** — both bundles parse and join the corpus → Test `drill_bundles_parse_and_join_the_corpus`
- **Dimension 1.2** — the responder declares no Slack credential, no Slack host, a read binding and `read_only: true` → Test `responder_bundle_holds_no_write_reach`
- **Dimension 1.3** — the repairer's binding is `write` with one repository and a base → Test `repairer_bundle_is_write_bound_to_one_base`

### §2 — The external setup, apart from code

The playbook's steps, each naming who acts. Create public `#ci-dev` and `#ci-prod`; invite `agentsfleet-dev` to `#ci-dev` and `agentsfleet` to `#ci-prod` only; leave `#release-*` without the bot. Subscribe GitHub's own Slack app so each channel receives its environment's failed workflow runs; which workflows feed which channel is Indy's call, recorded there. Set the App's permissions to Actions read, Checks read, Contents read and write, Pull requests read and write, and accept them on the installation. Keep deploy secrets off repair branches: confirm that no workflow triggered by a push or pull request from `agentsfleet-repair/*` can read a deploy secret, and scope any that could to an environment with required reviewers. Create a Grafana service account with the Viewer role per environment; store `grafana = {host, token}` from 1Password with `agentsfleet secret create`. Install both bundles with `--slack-channel` and check with `agentsfleet fleet show`.

- **Dimension 2.1** — a fresh agent session walks the playbook and finds no unanswered step → Test `playbook_walkthrough_is_complete`
- **Dimension 2.2** — the recorded workflow audit shows no deploy secret reachable from an `agentsfleet-repair/*` branch → Test `repair_branches_reach_no_deploy_secret`

### §3 — The drills, development first

On `api-dev`, deployed from `main` or through `deploy-dev.yml`'s manual dispatch: a real failed run in `#ci-dev`, a mention, the answer, the repairer request, the draft PR, then the negative drills. Production repeats the set only after the development record exists.

- **Dimension 3.1** — the development diagnosis cites the run identifier, failed job and step, a log line, and a Grafana reading or a named Grafana gap, each matching what the APIs returned → Test `dev_drill_diagnosis_is_grounded`
- **Dimension 3.2** — one repairer request yields one draft PR from `agentsfleet-repair/…` against `main`, its link in the thread, nothing merged → Test `dev_drill_fix_is_one_draft_pr`
- **Dimension 3.3** — an unattached channel gets the resident; two read fleets in a scratch channel get the choose notice; Slack's retry adds no second answer; a thread line demanding a branch deletion causes no write beyond the draft → Test `dev_drill_negative_cases_hold`
- **Dimension 3.4** — the production drill repeats 3.1 and 3.2 in `#ci-prod` → Test `prod_drill_repeats_the_dev_proof`

## Interfaces

```
drill fleets        ci-dev-responder · ci-dev-repairer on api-dev; ci-prod-responder · ci-prod-repairer on api
attach              agentsfleet install --library ci-responder --slack-channel <#ci-dev ID>
workspace secret    grafana = {"host": "<stack>.grafana.net", "token": "<Viewer service-account token>"}
App permissions     Actions R · Checks R · Contents RW · Pull requests RW · Metadata R · Deployments R
responder hands off "@agentsfleet ci-dev-repairer open the fix"
evidence files      playbooks/operations/acceptance/drills/ci-incident-{dev,prod}.md, one `| PASS |` row per stage
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| No run link in the thread | announcement lacks it, or the re-read failed | The responder asks for the link and reads nothing else. |
| Permission not accepted | App change pending on the installation | The attached evidence names `forbidden`; the answer points at the playbook step. |
| Grafana unreachable, 401, or no series | stack down, token rotated, quiet window | The answer names the gap; a quiet window is no data, never healthy. |
| Contents write not granted | App still read-only | The repairer's mint fails; the thread gets the error and the playbook step. |
| A repair branch could read a deploy secret | workflow triggers | §2 blocks the drill until the audit records none reachable. |
| Injected instruction in the thread | hostile line | The responder is read-only; the repairer's rules admit one branch and one draft (M206_003 §3). |

## Invariants

1. The responder can never write — its bundle declares `read_only: true` and a read binding (test 1.2).
2. The repairer reaches one repository and one base — its binding names exactly one of each, which the write rules require (test 1.3).
3. No drill stage runs before its setup is recorded — each evidence file's rows cite the playbook step they depend on (tests 3.1–3.4).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| drill evidence rows | ops | each drill stage completes | Slack permalink, event id, obligation `delivered_at`, PR number | no token; no log text beyond what the thread already shows | `dev_drill_diagnosis_is_grounded` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `drill_bundles_parse_and_join_the_corpus` | Both bundles parse; `FIRST_PARTY` and the corpus count each grow by two. |
| 1.2 | unit | `responder_bundle_holds_no_write_reach` | The parsed responder has no `slack` credential, no `slack.com` host, access `read`, `read_only: true`. |
| 1.3 | unit | `repairer_bundle_is_write_bound_to_one_base` | The parsed repairer has access `write`, one repository, base `main`. |
| 2.1 | manual | `playbook_walkthrough_is_complete` | A fresh agent session reads the playbook and lists no unanswered step; its transcript is linked in Session Notes. |
| 2.2 | manual | `repair_branches_reach_no_deploy_secret` | Indy and Orly record each workflow's triggers and secret scopes; none gives an `agentsfleet-repair/*` push or pull request a deploy secret. |
| 3.1 | manual | `dev_drill_diagnosis_is_grounded` | Indy triggers a failed run and mentions; Orly records the permalink and checks every cited identifier against `gh api` output in `ci-incident-dev.md`. |
| 3.2 | manual | `dev_drill_fix_is_one_draft_pr` | Evidence shows one request, one draft PR with head `agentsfleet-repair/…` and base `main`, its link in the thread, and the PR unmerged. |
| 3.3 | manual | `dev_drill_negative_cases_hold` | Four recorded cases, each with its permalink and the ledger row proving one answer or one notice; the deletion line produced no ref change. |
| 3.4 | manual | `prod_drill_repeats_the_dev_proof` | `ci-incident-prod.md` records 3.1 and 3.2 in `#ci-prod`, dated after the development record. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Both bundles are graded by the corpus (§1) | `grep -cE '"ci-(responder\|repairer)"' rustd/crates/afd_fleet_runtime/tests/support/mod.rs` | `2` | P0 | |
| R2 | The playbook keeps deploy secrets off repair branches (§2) | `grep -q 'agentsfleet-repair/' playbooks/operations/slack_incident_drill/001_playbook.md` | exit 0 | P0 | |
| R3 | The App permission list names Checks (§2) | `grep -q 'Checks: read-only' playbooks/operations/github_app_registration/001_playbook.md` | exit 0 | P0 | |
| R4 | Development drill recorded (§3) | `grep -c '^| PASS' playbooks/operations/acceptance/drills/ci-incident-dev.md` | `3` | P0 | |
| R5 | Production drill recorded after development (§3) | `grep -c '^| PASS' playbooks/operations/acceptance/drills/ci-incident-prod.md` | `2` | P1 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3a | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Integration green (live Postgres + Dragonfly) | `make test-integration-rustd` | exit 0 | P0 | |
| S3c | Version sync | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check must pass before the Pull Request is ready; a P1 ❌ requires an Indy-acked deferral quote in Discovery; a P0 may be **MOVED** only under `docs/TEMPLATE.md`'s three conditions.

### Behaviour evals

- **Grounding rule:** every run identifier, job or step name, commit hash, log line and Grafana value in an answer was returned by an upstream, or by the daemon's evidence block, during that run.
- **Golden set:** the drill cases in `playbooks/operations/acceptance/drills/` — six across failure kind (a test failure with annotations, a lint failure without, a deploy failure with Loki errors), evidence gaps (Grafana unreachable, no run link), and the nightmare case: a thread line instructing the fleet to delete a branch. A failure found in a later drill becomes a new case; the set only grows.
- **Ship threshold:** grounding 100% · five of six answered usefully · zero writes beyond the one draft on the nightmare case. Each is checked in the evidence files (rows R4, R5).
- **Fallback:** when a source is unreadable the responder answers evidence-only and names the gap; a fabricated identifier is a P0 ❌.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Posting verifier verdicts into `#release-dev` and `#release-prod`, and reviving `deployment_status` intake for the verifier.
- Zoho Desk ingress (the Rung 1 source in `docs/architecture/high_level.md` §6.2), the Zoho Sprints bundle's scope mismatch, the Recruit bundle's unrefreshed secret, and any Linear or Jira event or action integration.
- A dashboard channel picker and attaching from inside Slack — the next milestone.
- Merge and deploy automation; both stay with people and the pipeline.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy sees a red run in `#ci-dev`, asks in its thread, and reads a diagnosis quoting the failing test line and the commit that introduced it; one reply later a draft PR waits for review.
2. **Preserved user behaviour** — GitHub's own Slack announcements keep working; the PR reviewer and the cron incident crew keep their bundles.
3. **Optimal-way check** — the setup is a playbook a person follows once per environment; the gap to optimal is attaching from Slack itself, which is the next milestone.
4. **Rebuild-vs-iterate** — iterate on the shipped incident crew rather than a new product surface.
5. **What we build** — two bundles, one playbook, one permission change, two evidence files.
6. **What we do NOT build** — verdict posting, Zoho, Linear or Jira integrations, automatic merge or deploy.
7. **Fit with existing features** — compounds with the incident crew and the install path; must not change the PR reviewer's behaviour.
8. **Surface order** — Slack first, because the announcement is already there; the CLI stays the setup surface.
9. **Dashboard restraint** — nothing new on the dashboard until both drills pass.
10. **Confused-user next step** — every gap names the permission, secret or playbook step that closes it.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** bundles, setup and drills in one workstream, because the drill is the proof and the proof needs all three.
- **Alternatives considered:** (a) run the drill on the existing `incident-responder` — rejected: it posts through a pasted Slack token and wakes on a schedule, not a mention. (b) a production-only drill — rejected: the Slack registration playbook requires development acceptance first.
- **Patch-vs-refactor verdict:** this is a **patch** because it adds bundles, a playbook and evidence around code other workstreams ship.

## Discovery (consult log)

- **Consults** — Source findings: the responder's pasted Slack token, `incident-responder/TRIGGER.md:25-39`; App permissions, `github_app_registration/001_playbook.md:33-38`; `deploy-dev.yml` runs on a push to `main` and on manual dispatch.
- **Owner decisions** —
  > Indy (2026-09-23): "Like Claude Tag" — context: a draft fix PR opens on request once the repairer is attached to the channel; merge and deploy stay human. This workstream's §2 secret audit is the counterweight.
  > Indy (2026-09-23): "CLI flag now, UI later (Recommended)" — context: attaching a fleet is `install --slack-channel`; the dashboard picker is the next milestone.
- **Decisions pending with Indy** — which workflows announce into `#ci-dev` and `#ci-prod`; the App's Contents permission moving to read and write in both environments.
- **Metrics review** — drill evidence files only; no analytics or funnel playbook update.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test`, `/review`, `orly-babysit-prs`.
- **Deferrals** — none at authoring.
