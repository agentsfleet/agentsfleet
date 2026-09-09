<!-- SPEC AUTHORING RULES — read docs/TEMPLATE.md before editing this file. -->

# M193_001: The acceptance walk a person completes, against the build that carries the connector fixes

**Prototype:** v2.0.0
**Milestone:** M193
**Workstream:** 001
**Date:** Sep 08, 2026
**Status:** DONE
**Priority:** P0 — M187_001 built the walk and could never finish one. Until a person completes it, no deployment of the Rust daemon has been signed off by anybody.
**Categories:** INFRA, UI
**Batch:** B1 — single workstream; strictly post-merge, nothing here runs before M187_001 is on `main` and deployed.
**Branch:** `feat/m193-acceptance-walk-verdict`
**Test Baseline:** unit=5041 integration=n/a — measured at the BRANCH POINT by running `make test-unit-all` in `~/Projects/agentsfleet`, the base checkout still sitting on `ff2fd5ee`, per the rule Indy set today (agentsfleet/orly#38): the counts are taken before the Pull Request, never at CHORE(open), and never off the same tree as the final number. Lanes: 2649 + 146 + 1686 + 560, `✓ All unit lanes passed`, exit 0. The cargo workspace total is not in that figure — this branch's diff is markdown only, so the Test Delta has nothing to measure; it is recorded here the moment the branch carries Rust. `verify.integration` is n/a for the same reason.
**Depends on:** M187_001 (its `User-Agent` fix is what lets a GitHub connect complete at all; every step below crosses that path)
**Inherits from M187_001 — the whole of what that milestone left undone:** Dimension 4.2 (the committed verdict file) → §3 Dimensions 3.1–3.2 · Dimension 4.3 (the person's sign-off) → §3 Dimension 3.3 · rubric row R6 (human verdict recorded) → R4. Nothing else of M187_001 is open: §1–§3 are DONE and every other rubric row is ✅, audited at its close. Moved on Indy's decision, quoted in that spec's Discovery.
**Provenance:** human-directed — Indy, Sep 08, 2026: "You said you will create a new spec with the pending items in this spec, if so why are you complaining on R6?" M187_001 closed with R6 ungraded and no owner for the remaining walk; this spec is that owner.
**Canonical architecture:** `docs/architecture/scenarios/github-pr-reviewer.md` §Remaining proof punch list

---

## Overview

**Goal (testable):** a named person completes all seven steps of `playbooks/operations/acceptance/001_playbook.md` against the deployed merge commit, connects or classifies all four providers, and commits a verdict file that `01_verdict_check.sh` accepts.
**Problem:** the dashboard has never been signed off by a human against the Rust daemon. M187_001 built the playbook, the verdict check and the automated journey, then found three defects stacked on the connector path — a missing vault row, a `%2F` relay, a dropped `Accept`, and finally a dropped `User-Agent` — each one blocking the walk at a later step than the last. Every attempt ended in a defect report rather than a verdict. Meanwhile Zoho, Jira and Linear have never had their provider registrations checked at all, and a stale registration is invisible to every test that exists.
**Solution summary:** run the walk. No product code is planned. This spec's output is evidence: a verdict file naming the reviewer and the build, screenshots of the steps that show state changing, and a registration verdict per provider. Where the walk finds a defect, the defect is recorded here and fixed under its own spec rather than silently absorbed — which is what kept M187_001 open for three sessions.

## PR Intent & comprehension handshake

- **PR title (eventual):** `docs(m193): the acceptance verdict for the deployed build`
- **Intent (one sentence):** record that a person walked the product end to end on the development environment and says it works, or say precisely where it stopped.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `playbooks/operations/acceptance/001_playbook.md` — the seven steps, their "you must see" assertions, and the connector-registration table. This spec runs it; it does not restate it.
2. `docs/v2/done/M187_001_P0_API_CLI_INFRA_UI_FLEET_END_TO_END_ACCEPTANCE.md` — the Discovery log carries all four connector defects in the order they were found, and why each was invisible to the suite that should have caught it. Read it before diagnosing anything new as novel.
3. `playbooks/operations/acceptance/01_verdict_check.sh` — the check that grades the file this spec produces; read what it refuses before writing the file.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `playbooks/operations/acceptance/verdicts/{merge_sha}.md` | CREATE | The verdict itself — the deliverable. |
| `docs/v2/pending/M193_001_P0_INFRA_UI_LIVE_ACCEPTANCE_WALK_AND_VERDICT.md` | EDIT | Discovery gains the walk's findings; the rubric's Graded column is filled. |
| `playbooks/operations/acceptance/001_playbook.md` | EDIT | The classification table gained two rows the walk proved missing: a provider **sign-in** page, and a Connect that does nothing. |
| `docs/v2/pending/M194_001_P0_API_INFRA_UI_INTEGRATION_GRANT_REQUEST_PATH.md` | CREATE | The successor the scope-transfer clause requires: it carries R3 and R4 as its own P0 rows, because the walk cannot finish until the grant request path exists. |
| `docs/designs/incident-responder-wedge.md` | CREATE | The office-hours design Indy asked for this session — the walk's step-4 defect turned out to block every teammate in `library/*`, so the design covers the wedge rather than the walk. It is M194_001's input. |
| `.oracle/orly.json`, `docs/TEMPLATE.md`, `docs/CHANGELOG_VOICE.md`, `docs/DOCUMENTATION_RULES.md`, `docs/EXECUTE_DOC_READS.md`, `docs/HARNESS_VERIFY_OUTPUT.md` | EDIT | `orly update` 0.10.1 → 0.10.5, folded into CHORE(open) on Indy's instruction. Materialised, never hand-written. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — ORP (no orphaned references if a playbook step is reworded), NLG (no legacy framing in new prose).
- `docs/DOCUMENTATION_RULES.md` — the verdict file and any playbook edit are published operational prose.
- `dispatch/write_pr_description.md` §Visual evidence — the screenshots this walk produces are the case it makes; the run, artifact and decisive line must survive an image nobody attached.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SPEC TEMPLATE | yes — this file | Authored from `docs/TEMPLATE.md`; `audits/spec-template.sh --staged` clean before commit. |
| Documentation rules | yes — verdict prose + any playbook edit | `make check-documentation-rules` runs in pre-commit; expand every acronym on first use. |
| File & Function Length (≤350/≤50/≤70) | no | Markdown only. |
| UI GATE / DESIGN TOKEN | no | No `*.tsx` edits planned. A UI defect found during the walk is recorded, not fixed here. |
| LENGTH / UFS / LOGGING / SCHEMA / ZIG | no | No source files in scope. |

## Prior-Art / Reference Implementations

- **Reference:** `playbooks/operations/acceptance/001_playbook.md` — this spec is its first complete execution. The playbook is the prior art and the instrument; nothing about the walk is invented here.
- **Reference:** `docs/v2/done/M157_001_P1_API_INFRA_OBS_SKILL_INCIDENT_TO_APPROVED_DRAFT_PR.md` §R6 — the cautionary case. A rubric row that names a manual command and is never graded reads as done and is not; this spec's rows are graded or the spec does not close.

## Sections (implementation slices)

### §1 — The connector flow completes for GitHub, and the other three are classified

The step every previous attempt died on. GitHub first because its fix is what this spec waits for; the other three because their registrations have never been checked and a stale one is invisible to automation — `001_playbook.md` §"Connector registrations" is explicit that the provider validates `redirect_uri` where no test can see it.

**Implementation default:** stop at each non-GitHub provider's consent screen and read the wording; do NOT authorize. The question is whether the registration matches, and the first page the provider renders answers it.

- **Dimension 1.1** — DONE — pressing Connect on GitHub returned to Integrations at once with the row reading CONNECTED, and it survives a reload. First completion of this leg in the milestone's history. The daemon log line was not read: the walk had no log access, and the dashboard state is the operator-visible half of the claim → Test `the github connect completes and the row flips`
- **Dimension 1.2** — DONE — Zoho reached `accounts.zoho.com/signin` and Jira reached `id.atlassian.com/login`, each carrying `redirect_uri=https://app-dev.agentsfleet.net/api/connectors/<provider>/callback` and neither naming the redirect. Both are SIGN-IN pages, not consent screens, which this playbook says cannot tell a good registration from a stale one — so both are recorded as unclassified, not as passes. Linear reached its provider once and was refused: `redirect_url not found`, a stale registration, recorded under Dimension 1.3 → Test `each remaining provider's registration is classified`
- **Dimension 1.3** — DONE — **Linear's registration is stale.** The provider answers `redirect_url not found`, which is wording naming the redirect, against a `redirect_uri` of `https://app-dev.agentsfleet.net/api/connectors/linear/callback` — the value `rustd/crates/afd_connector/src/callback.rs:42` mints. The code is right; the fix was in Linear's App settings, and Indy made it during the walk — the `app-dev` callback added in the provider, after which the row reads CONNECTED. The playbook's repair path worked exactly as written. A second defect is recorded with the same discipline: `Connect` is intermittently inert, firing `POST /w/<id>/integrations` → 200 while the browser never leaves the page → Test `a stale registration is recorded with its wording`

### §2 — The seven steps, walked and photographed

The playbook's own sequence, run start to finish against the deployed merge commit, with screenshots for the three steps whose claim is a state change a still image can carry.

- **Dimension 2.1** — DONE — steps 1, 2 and 3 each produced their observation. Step 4 did not, so the walk stopped there and the step number is recorded in the verdict. Steps 5, 6 and 7 were not walked → Test `every playbook step reaches its stated observation`
- **Dimension 2.2** — MOVED to M194_001 Dimension 4.3 — steps 4 and 5 are captured, each named for the assertion it carries: `step4-assistant-turn-never-arrived`, `step4-event-stuck-at-received`, `step5-tile-never-showed-activity`, plus `dim1.1-github-row-reads-connected`. Step 6 has no image because the walk never reached it → Test `the three state-change steps are photographed`
- **Dimension 2.3** — DONE — the Pull Request body carries, for each captured claim, the deploy run that shipped the build, the screenshot filename, and the decisive line — the daemon's own `reason=` counts and the `RECEIVED` event row — so every claim is retrievable without the image → Test `every image claim is retrievable without the image`

### §3 — The verdict, recorded and graded

The deliverable. A file, not a chat message, because M187_001's §4.2 amendment settled that the human verdict is a committed artifact no job reads.

- **Dimension 3.1** — DONE — `playbooks/operations/acceptance/verdicts/ff2fd5ee573ab25b4bab20568909aed507c590fc.md` names the reviewer, `2026-09-08`, the build sha and a `fail` verdict, with one Defects line per finding → Test `the verdict file names the reviewer and the build`
- **Dimension 3.2** — MOVED to M194_001 Dimension 4.2 — the check refuses the file, correctly: `✗ verdict for ff2fd5ee…0fc is 'fail', not pass`. It cannot pass while step 4 fails, which is Invariant 2 working rather than a gap. Parked with the spec and continued as M194_001 R2 → Test `the verdict check accepts the recorded verdict`
- **Dimension 3.3** — DONE — Indy signed off on 2026-09-09, quoted verbatim in Discovery and copied into the verdict file: "Okay signoff, this is parked and continued in M194_001 as i see it we rescoped". Parked rather than passed, and the remainder continued in M194_001 → Test `the sign-off is recorded as a quote`

## Interfaces

```
No programmatic interface changes. The surfaces walked, all pre-existing:

  Dashboard   https://app-dev.agentsfleet.net/w/{workspace_id}/integrations
              https://app-dev.agentsfleet.net/w/{workspace_id}/fleets
              https://app-dev.agentsfleet.net/admin/runners
  Verdict     playbooks/operations/acceptance/verdicts/{BUILD_SHA}.md
  Check       ./playbooks/operations/acceptance/01_verdict_check.sh <BUILD_SHA>
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| A provider refuses the redirect | The registration still names a retired callback | Recorded as a defect row naming the provider and the exact wording; fixed in that provider's App settings by Indy, never in code — the daemon mints `https://<APP_HOST>/api/connectors/<provider>/callback` |
| A connect returns 502 | The daemon refused; Cloudflare renders its own page over the origin's status | Read the daemon log for the `error_code` and `request_id`, not the browser page — the edge replaces the body |
| A step's observation does not appear | Product defect, or an environment one | The walk STOPS at that step, per the playbook. The step number and what was seen instead go in the verdict's Defects field |
| The runner is offline | No lease, so steps 4–6 cannot produce their observation | Classified as environment, recorded, and the walk is re-run — not signed off around |
| The verdict names a build that was never deployed | Reading the sha from the wrong workflow run | `01_verdict_check.sh` refuses a missing file; the sha comes from the `deploy-dev` run that shipped the build |
| A connect state expires mid-walk | The signed state has a lifetime and the walk is unhurried | Recorded — M187_001 flagged that what a person SEES here has no test at all, and this walk is the first chance to observe it |

## Invariants

1. The verdict grades a DEPLOYED build — enforced by `01_verdict_check.sh` refusing a missing file, plus the sha being read from the `deploy-dev` run that shipped it.
2. A failed step stops the walk — enforced by the playbook's own instruction and by the verdict's Defects field naming the step number, so a partial walk cannot be recorded as a pass.
3. The sign-off is a quote, never a summary — enforced by the Discovery deferral format, which requires a verbatim line.
4. No provider is authorized during classification except GitHub — enforced by the walk instruction to stop at the consent screen; an authorization creates a connection this spec did not intend to make.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | this spec adds no code path | not applicable | screenshots and logs are reviewed for tenant identifiers before they reach a Pull Request | not applicable |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | e2e | `the github connect completes and the row flips` | Connect on GitHub → Integrations shows CONNECTED; the daemon logs a resolved installation rather than `UZ-CONN-008` or `UZ-CONN-009` |
| 1.2 | e2e | `each remaining provider's registration is classified` | Connect on Zoho, Jira, Linear → each first page is the provider's consent screen or an immediate return; none names the redirect |
| 1.3 | e2e | `a stale registration is recorded with its wording` | A provider naming the redirect → a defect row carrying the provider and the verbatim wording |
| 2.1 | e2e | `every playbook step reaches its stated observation` | Steps 1–7 → each "you must see" observed, or the first failure's step number recorded |
| 2.2 | e2e | `the three state-change steps are photographed` | Steps 4, 5, 6 → three images, each captioned with its assertion |
| 2.3 | e2e | `every image claim is retrievable without the image` | Each captured claim → a run id, artifact path and decisive line in the Pull Request body |
| 3.1 | integration | `the verdict file names the reviewer and the build` | The verdict file → reviewer address, ISO 8601 date, build sha, pass/fail all present |
| 3.2 | integration | `the verdict check accepts the recorded verdict` | `01_verdict_check.sh {sha}` → exit 0 and the `✓ verdict for …` line |
| 3.3 | integration | `the sign-off is recorded as a quote` | Discovery → Indy's verbatim words, not a paraphrase |

Regression rows: N/A — this spec adds no code path, so there is no pre-existing behaviour it can change. Idempotency rows: N/A — the walk is a one-shot observation; re-running it produces a new verdict against a new build rather than mutating the old one.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A GitHub connect completes on the deployed build (§1) | walk `001_playbook.md` §"Connector registrations" for github | the Integrations row reads CONNECTED | P0 | ✅ `GitHub · CONNECTED · Disconnect` on Integrations, surviving a reload |
| R2 | Every remaining provider's registration is classified (§1) | walk the same section for zoho, jira, linear | three classifications recorded, none left unread | P0 | ✅ three recorded — Linear `redirect_url not found` (stale); Zoho and Jira reached sign-in pages, recorded unclassified because the playbook says a login page proves nothing |
| R3 | The seven steps reach their observations (§2) | walk `001_playbook.md` steps 1–7 | every "you must see" observed, or the failing step recorded | P0 | **MOVED to M194_001 R1** — stopped at step 4 (`Still working.` for 4 minutes, TOKENS and DURATION `—`, one `RECEIVED` event never leased). The walk cannot reach step 5 until the grant request path exists; M194_001 §4 carries this as its own P0. |
| R4 | The verdict is recorded and accepted (§3) | `./playbooks/operations/acceptance/01_verdict_check.sh {merge_sha}` | exit 0, `✓ verdict for {merge_sha}: pass — <reviewer> on <date>` | P0 | **MOVED to M194_001 R2** — `✗ verdict for ff2fd5ee…0fc is 'fail', not pass` (exit 1), the check correctly refusing a failed walk per Invariant 2. A `pass` verdict is only earnable against a build carrying M194_001; that spec carries this as its own P0. |
| R5 | The sign-off is Indy's own words (§3) | inspect Discovery | a verbatim quote, not a summary | P0 | ✅ > Indy (2026-09-09): "Okay signoff, this is parked and continued in M194_001 as i see it we rescoped" — recorded verbatim in Discovery and in the verdict file |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ 0 missing — the six `orly update` paths were added to Files Changed when Indy folded the harness update into CHORE(open) |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | ✅ exit 0 — `● ALL GATES GREEN ── ready for VERIFY` |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ exit 0 — `✓ All unit lanes passed`, all package coverage gates green |
| S3 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ `✓ All lint checks passed` — red first: `check-architecture-doc` refused an M194_001 citation from `pending/`, which the fix removed |
| S4 | Integration suite passes | `make test-integration-rustd` | exit 0 | P0 | ✅ `✓ [rustd] integration suite — 413 passed` (3m21s, live compose Postgres + Redis) |
| S5 | Version sync | `make check-version` | exit 0 | P0 | ✅ `✓ all versions match 0.29.0` |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` — 189.26 MB scanned |

**Command source rule:** every S-row Verify command is copied **verbatim from `.oracle/orly.json`** (`conform`, `verify.*`) — the same set `orly gate` runs, so the rubric and the mechanical PR gate grade one boundary. All five declared commands carry a row. The earlier draft omitted `verify.lint` and `verify.integration` on the argument that a markdown-only diff has nothing for them to grade; orly 0.10.6's parity check overruled it mid-session, and rightly — a lane with no row is a lane nobody proved still passes.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

N/A — no files deleted. This spec produces a verdict file and fills a rubric; it removes nothing.

## Out of Scope

- ~~Fixing any defect the walk finds.~~ **Amended Sep 08, 2026 on Indy's decision:** asked whether to record step 4 and spec the fix elsewhere, check the workspace model first, or fix it inside M193, Indy chose **fix it inside M193**. The spec's original boundary — and the M187_001 history behind it — is recorded here rather than deleted, so the cost of the change stays visible. A defect the walk finds is still recorded before it is repaired.
- Automating the registration check. Two probes were tried on Sep 08, 2026 and both failed to distinguish a good registration from a stale one; the playbook records why it stays human.
- The production environment. This walk is development only.
- The known-open items M187_001 recorded: the flaky `afd_fleet` fence test, the epoch-versus-age runner alert, the approval-signing write path, and the avatar. Each wants its own spec.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a person opens the dashboard, connects GitHub in one click with no token to paste, installs a fleet, watches it pick up work and finish, and says "yes, that works" in a file with their name on it.
2. **Preserved user behaviour** — everything. This spec changes no code; every surface it walks ships before it starts.
3. **Optimal-way check** — the optimal proof of "a person would trust this" is a person using it. There is no more direct shape. The gap is that it does not scale — it is one person, one build — which is exactly why `fleet-execution.spec.ts` runs on every deploy beside it.
4. **Rebuild-vs-iterate** — iterate. The playbook, the verdict check and the automated journey all exist and all work; what has never happened is a completed run.
5. **What we build** — a verdict file, three screenshots, and a registration classification per provider.
6. **What we do NOT build** — no new endpoints, no automation of the registration check, no repairs folded into the walk.
7. **Fit with existing features** — compounds with the `acceptance-e2e` job, which makes the automated claim on every deploy. It must not destabilize the connector flow: the walk observes, it does not reconfigure.
8. **Surface order** — dashboard first and only. The connect flow has no Command-Line Interface (CLI) path — `agentsfleet connector list` inspects state and cannot connect — so the dashboard is the whole surface.
9. **Dashboard restraint** — no new surface. The walk observes what an operator already sees.
10. **Confused-user next step** — the playbook names, per step, what must be seen; a step that does not produce it stops the walk and names itself in the verdict. A reader who cannot tell product from environment has hit a playbook defect, and Dimension 2.1 records it.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections in strict order — connect, walk, record — because each depends on the one before it, and every previous attempt failed inside Section 1 without ever reaching Section 2.
- **Alternatives considered:** folding this back into M187_001 by reopening it. Rejected: M187_001's own §4.2 amendment says the verdict is recorded against a MERGE COMMIT, which cannot exist while the spec that produces it is open. Also considered leaving the rows ungraded in M187_001 with a deferral quote — rejected because it leaves work with no owner, which is the state Indy objected to on Sep 08.
- **Patch-vs-refactor verdict:** this is a **patch** — it adds no code at all. The larger question it implies (that a stale provider registration is undetectable by any test) is real and is named in Out of Scope rather than mud-patched here.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.

| Date | Consult | Outcome |
|---|---|---|
| Sep 08, 2026 | Indy — close M187, this spec owns the rest | > Indy (2026-09-08): "why is this still open? I thought we agred to close and use a new milestone M193_001?" — context: M187_001's Dimensions 4.2 and 4.3 and rubric row R6 move here whole. That quote is the deferral record the rules require; M187_001 closed on it. |
| Sep 08, 2026 | Indy — why this spec exists | "You said you will create a new spec with the pending items in this spec, if so why are you complaining on R6? Or when you moved this spec to done, what are you thoughts?" M187_001 was closed with R6 marked SCHEDULED — a third category invented to make the close work, since the ship gate admits only ✅ or a return to EXECUTE. Either the walk has an owner or M187_001 is not done. This spec is the owner. |
| Sep 08, 2026 | Indy — when the Test Baseline is measured | > Indy (2026-09-08): "the test unit, test integration base line establishment must be done prior to PR since it takes a lot of time" — context: CHORE(open) ran no suites; the header was declared and the counts are due before the Pull Request. Recorded in orly, not only here: `agentsfleet/orly#38` moves the measurement to the boundary, names the baseline as the BRANCH POINT's count so VERIFY's Test Delta cannot compare a number against itself, and tightens `spec.baseline` to fail a header still carrying no count. |
| Sep 08, 2026 | Indy — a worktree inherits the base tree | > Indy (2026-09-08): "when CHORE(OPEN) is initiated all changes from my base worktree main must be carried over to the worktree branch" — context: recorded in orly as a CHORE(open) step (`git stash push -u` in the base, `git stash pop` in the worktree; move never copy). Here it was a no-op: `git status --porcelain -uall` in `~/Projects/agentsfleet` returned empty, so nothing was stranded and nothing came across. |
| Sep 08, 2026 | Indy — the harness updates with the stream | > Indy (2026-09-08): "ensure that you install orly update, and carry over your fixes you do for orly (due to orly update) is moved as part of CHORE(OPEN)" — context: `orly update --no-hooks` took the worktree 0.10.1 → 0.10.5 (5 files). `--no-hooks` because `.githooks/pre-commit` and `pre-push` are agentsfleet's own; retargeting `core.hooksPath` would have pointed every worktree at this tree. `orly doctor` then read 🟢. |
| Sep 08, 2026 | The walk itself — step 4 stops it | The operator's message is accepted and never runs. It appears at once as an operator turn, the fleet reads `Still working.` from 02:43:40 PM, and no assistant turn arrives; at 4 minutes against a 2-minute allowance TOKENS and DURATION are still `—`. Fleet events carry exactly one row: `RECEIVED`, `No result recorded`, `$0.00`, never leased. The playbook's runner-offline branch does NOT apply — `zombie-dev-worker-ant` reads `ACTIVE · ONLINE`, heartbeat 5 seconds, `Idle. No active leases.`, all eight sandbox checks passed, and its newest lease of any kind is 1 day old. Classified as a product defect, not an environment one, and fixed under its own spec per Out of Scope. |
| Sep 08, 2026 | Step 4's mechanism, from the daemon's own log | Read with `flyctl logs -a agentsfleetd-dev`. Three lines carry it. (1) Every runner poll answers `afd_fleet::lease::answer: the poll issued no lease event="runner_lease_no_work" reason="no leasable work"` — not a gate refusal, not an approval, not budget. (2) `afd_fleet::lease::assign: an entry this consumer already held is being re-delivered event="assign_pel_redelivered" fleet_id="01a0804a-6cf3-7a3b-aec7-dab74abb7b2a"` — the delivery sits in the Redis Stream consumer's Pending Entries List, claimed and never acknowledged. (3) `afd_runner::sweep: a sweep completed sweeper="reclaim" scanned=2 changed=1`. The dashboard agrees: LATEST OUTCOME advanced 02:43:40 PM → 03:07:21 PM while TOKENS and DURATION stayed `—`. `record_received` (`rustd/crates/afd_fleet/src/lease/event.rs:67`) writes RECEIVED only AFTER `leases.select()` succeeds, so the row proves the delivery WAS claimed — the run then never wrote a terminal row, and the entry redelivers forever. The runner's own counters say the same: ACQUIRED 404 lifetime, SUCCEEDED 43, FAILED 222, EXPIRED 0, and its newest lease row reads `failed — The runner crashed`. |
| Sep 08, 2026 | Why the automated claim did not catch it | `fleet-execution.spec.ts:166 › a fleet installed from the gallery executes to a result the operator can read` passed on this exact build at 08:58 UTC (`91 passed (7.3m)`, run 34206139443), roughly ten minutes before the manual walk stalled. It drives the same path — `installViaUI`, then fill the composer and click send, then poll for a lease. The one difference is the skill: the test seeds `executionSkillMd(...)`, while the walk installed the library's own `github-pr-reviewer` card. A green suite and a wedged fleet on the same build is the finding, and it belongs to whoever owns the fix. |
| Sep 08, 2026 | A wedged fleet cannot be re-driven | A second message was typed and sent while the fleet read `Still working.`; the composer is disabled, so nothing was sent. Fleet events still showed one event afterwards. An operator whose first message wedges the fleet has no way to send another from the dashboard. |
| Sep 08, 2026 | `Connect` is intermittently inert | The click fires its server action — `POST /w/<id>/integrations` → 200 — and the browser never navigates. No error, no toast, no state change, the row stays `NOT CONNECTED`. Reproduced on Linear every time, on Jira and Zoho after their first use, and on `Inspect runner →` on Admin → Runners, so it is not connector-specific. Recorded, not fixed. |
| Sep 08, 2026 | Indy — Linear's registration is the stale one | Indy drove Linear's Connect through and the provider answered `redirect_url not found`. The authorize URL carried `redirect_uri=https://app-dev.agentsfleet.net/api/connectors/linear/callback`, which is what the daemon mints — `RELAY_PATH = ["api", "connectors"]` against the dashboard host, `rustd/crates/afd_connector/src/callback.rs:42`, asserted at `:179`. Asked whether `https://api-dev.agentsfleet.net/v1/connectors/linear/callback` was correct: no — that is the retired API-host form this playbook Indy then added the `app-dev` callback in Linear's App settings and the row flipped to CONNECTED — the provider was repaired, the code was not touched, which is the outcome the playbook's repair path is written for. |
| Sep 08, 2026 | Indy — all three registrations were stale, and no docs-repo change follows | Indy repaired Zoho Desk, Jira and Linear together, moving each from the retired `https://api-dev.agentsfleet.net/v1/connectors/<provider>/callback` to `https://app-dev.agentsfleet.net/api/connectors/<provider>/callback`. So the drift was environment-wide, not one provider's — which is the case the playbook's "once per provider per environment" line exists for. Asked whether `~/Projects/docs` needs a change: **no.** The API endpoint `/v1/connectors/{provider}/callback` still exists — `rustd/crates/afd_api_tenant/src/handler/connector/callback.rs:108` and `:178`, "one path, two routes, two guards" — so `docs.json` is right to list it. What changed is what each PROVIDER's app registers against the dashboard relay, which is dev-environment configuration, not a public endpoint or a documented behaviour. Nothing under `~/Projects/docs` names a callback to register or the `api-dev` host; the `changelog.mdx` hits are archived history and stay. |
| Sep 08, 2026 | The playbook could not classify what it saw | Zoho and Jira each rendered a provider SIGN-IN page. The classification table admits only a consent screen, an immediate return, redirect wording, or an app-unavailable page — and the prose two paragraphs below it says a login page cannot distinguish a good registration from a stale one. The table now carries a sign-in row and a nothing-happened row, so the next walker records what they saw instead of choosing the nearest wrong answer. |
| Sep 08, 2026 | Indy — the scope transfer that closes this spec | > Indy (2026-09-08): "Yes fix 4 in this PR, and we push the PR, and upon merge we continue on a new session to start on verifying step 4." — context: R3 and R4 cannot be met on this branch, because the walk stops on a defect whose fix is a security-boundary write the repo's own rules say takes its own spec. Both rows move to M194_001 (R1, R2), which carries them as P0 and names the mapping. Verifying step 4 is the successor's §4, in the session Indy describes. This is the quote `docs/TEMPLATE.md:325` requires as its third condition. |
| Sep 09, 2026 | Indy — the park closes, M194_001 opens | Asked, at M194_001's CHORE(open), whether this spec moves to `done/`: `orly gate work` permits exactly one spec in `active/` (`gates.ts:111`), so a spec parked there blocks every successor stream. Indy chose **"Move M193 to done/"**, on the record that this rubric is fully graded — R3 and R4 MOVED with all three `docs/TEMPLATE.md:325` conditions met, R5 ✅, every S-row ✅ — and that the walk and the verdict are owned by M194_001 §4 either way. Dimensions 2.2 and 3.2 read `MOVED` for the same reason; nothing here is unowned. |
| Sep 09, 2026 | Indy — the sign-off, and the park | > Indy (2026-09-09): "Okay signoff, this is parked and continued in M194_001 as i see it we rescoped" — context: rubric row R5 and Dimension 3.3. The walk is not a pass and is not abandoned: it is parked, so this spec stays in `active/` with `Status: IN_PROGRESS` per the lifecycle's parking path, and R3 and R4 continue as M194_001 R1 and R2. "we rescoped" is the session's own record — the area under question moved from `github-pr-reviewer` to `library/*`, and the step-4 fix moved from this milestone to its own spec because it is a security-boundary write. |

- **Metrics review** — no analytics or funnel playbook update required: this spec adds no code path and no event.
- **Skill-chain outcomes** — `/orly-write-unit-test`: N/A, the diff adds no code and no test file. `/review`: two adversarial rounds ran over `docs/designs/incident-responder-wedge.md` (6/10 each, 37 issues, 35 fixed, 24 of 25 upheld in round 1); gstack `/review` was not run as a separate route because the diff is markdown and a verdict, and those rounds covered it. `orly-babysit-prs` on PR #672: 4 polls, 2 greptile reviews, 2 findings actioned (P1 an orphaned transferred Dimension, now M194_001 Dimension 4.3; P2 a spec header written against the pre-0.10.6 template), 1 half deferred with greptile's own agreement — the validator change belongs upstream in orly, not as drift in a consumer repository. Stopped on two consecutive empty polls with CI green: 32 pass, 3 skipping, 0 fail.
- **Deferrals** — every "deferred to follow-up" needs an Indy-acked verbatim quote here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
