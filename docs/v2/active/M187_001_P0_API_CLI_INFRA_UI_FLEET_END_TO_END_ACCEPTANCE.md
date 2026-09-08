<!-- SPEC AUTHORING RULES — read docs/TEMPLATE.md before editing this file. -->

# M187_001: One fleet, installed to executed to observed, against the Rust daemon

**Prototype:** v2.0.0
**Milestone:** M187
**Workstream:** 001
**Date:** Sep 01, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — the cutover family proves the Rust daemon SERVES; nothing yet proves a customer's fleet finishes its job on it.
**Categories:** API, CLI, INFRA, UI
**Batch:** B10 — last in the v2 sequence; runs against the binary M181_006 leaves serving.
**Branch:** `feat/m187-fleet-e2e-acceptance`
**Test Baseline:** unit=5801 (cargo 2459 + app 2637 + cli 146 + design-system 559, `make test-unit-all`, Sep 07, 2026) integration=408 (`make test-integration-rustd`, 408 passed / 0 failed at `7e73e7f8d`)
**Depends on:** M181_006 (the production swap — this spec grades the daemon that swap leaves serving, so it cannot start before it); M181_002 (the route surface every journey below walks); M135_002 (an online runner whose heartbeat advances, without which nothing executes); M186_001 (the live connector proof this spec ports and supersedes — see Decomposition)
**Provenance:** human-directed — Indy, Sep 01, 2026: the end-to-end verification born as M136_001 and renumbered M186_001 must be PORTED to Rust rather than landed as Zig; the fleet sequence is proven first and its defects fixed after; a human eyeball pass rides beside the automated lane. The deletion this spec originally carried moved to M181_006 §4 on Sep 04, 2026 — see the scope change above.
**Canonical architecture:** `docs/architecture/scenarios/github-pr-reviewer.md` §Remaining proof punch list

---

## Overview

> **SCOPE CHANGE (Sep 04, 2026) — the deletion left this spec.** §5 deleted the
> Zig daemon and gated it behind §1–§4 on the stated ground that the tree was
> M181_006's rollback. It is not: the cutover was accelerated, `Dockerfile:39`
> already ships the Rust binary, and rollback is the previous image digest. The
> gate was protecting a property that no longer existed, so it moved to
> M181_006 §4 along with the deletion, and this spec keeps the live fleet
> verification it is named for.
>
> > Indy (2026-09-04): "so move the sunset to here in your spec, delete from the
> > M187_001 spec" — context: M181_006's premise rewrite voided the rollback
> > justification, and a blast-radius grep put the real cost at 92 files rather
> > than one directory.
>
> **What went with it:** §5's four dimensions, rubric rows R7–R11, Invariant 1's
> gate, the Dead Code Sweep, and the `src/agentsfleetd/**` DELETE row. What did
> NOT change: the runner stays Zig (Indy, Sep 02, 2026), so `src/runner/**`,
> `src/build/**` and `build_runner.zig` were never this spec's business and are
> not M181_006's either beyond being graded as survivors.

**Goal (testable):** one fleet completes install → activate → trigger → lease → execute → observe against the Rust daemon on the development environment, graded by the `deploy-dev / acceptance` lane and countersigned by a recorded human visual pass, with every defect that walk surfaces fixed inside this milestone.

**Problem:** the cutover family proves the Rust daemon answers every route, holds its budgets and can be rolled back. None of that is a customer finishing a job. The 44 acceptance journeys under `ui/packages/app/tests/e2e/acceptance/` run against the daemon serving `api-dev`, and the one that reaches a real runner lease — `runner-detail.spec.ts` — is deliberately built to FAIL closed before the model call, because an empty SKILL.md body is the only model-free way to place a failed lease from the outside. So the repository has never asserted that a fleet runs to a real result. M186_001 was written to close exactly that gap and its §1–§5 never ran; its Files Changed still names Zig paths, so running it as written would land connector code into a tree that is being deleted.

**Solution summary:** re-point the existing acceptance corpus at the Rust-served environment and add the leg it has never had — a fleet that executes to a real result and is observed doing so — then fix what that surfaces rather than filing it. Port M186_001's connector proof onto `rustd/crates/afd_connector/**`, keeping its dimensions and discarding its Zig paths. Add one human visual pass with a recorded checklist, because a green Playwright run and a dashboard a person would trust are different claims.

## PR Intent & comprehension handshake

- **PR title (eventual):** test(acceptance): prove a fleet end to end on the Rust daemon
- **Intent (one sentence):** an operator can watch one fleet go from a gallery card to a finished job on the Rust daemon, and the repository asserts that walk on every deploy instead of hoping.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

**Filled Sep 07, 2026, late — and the lateness is itself a finding.** EXECUTE ran
across three sessions with this section carrying its template text, so the
restatement below is written against work already committed rather than ahead of
it. It is recorded honestly rather than backdated.

**Restatement.** Prove the boring path first: one fleet, installed the way a
customer installs it, doing real work and reporting a real answer, on the Rust
daemon rather than the Zig one it replaced. Then keep proving it automatically,
and have a person confirm once that the screens an operator actually looks at
tell the truth. What §1 breaks, §2 fixes with the assertion that caught it —
the point is not a green run, it is that a red run would have been red for a
reason someone can read.

`ASSUMPTIONS I'M MAKING:`

1. "End to end" means through the dashboard an operator uses, not through the
   API underneath it. A journey that posts JSON and asserts JSON proves the
   daemon and not the product.
2. The human pass (§4) grades what automation structurally cannot: whether the
   screens are trustworthy. It is not a slower duplicate of §1.
3. A defect §1 surfaces belongs to §2 whether or not it is convenient. Both
   defects this milestone actually surfaced — the missing `approval-signing`
   row and the `%2F` relay — came from §4.2's walk rather than §1's, and both
   were cutover regressions in the connector flow, so they were folded in under
   §2's rule rather than filed away.
4. §4.2 cannot be graded before merge. `deploy-dev.yml` triggers on
   `push: main`, so no feature-branch build ever reaches dev, and a verdict
   "for the build the PR ships" cannot exist until that build IS main. The
   dimension is amended below rather than pretended satisfied.

## Implementing agent — read these first

1. `ui/packages/app/tests/e2e/acceptance/` — the 44 journeys that exist; `login-install-lifecycle.spec.ts` is the closest walk and `runner-detail.spec.ts` is the only one that reaches a lease.
2. `.github/workflows/deploy-dev-acceptance.yml` — the `qa` / `acceptance-e2e` / `acceptance-cli` / `results` jobs this spec extends, and the gate they report into.
3. `make/acceptance.mk` — `acceptance-e2e` and `cli-acceptance`, the local twins CI runs.
4. `docs/v2/done/M186_001_P0_DOCS_INFRA_LIVE_CONNECTOR_PROOF.md` — the dimensions §3 ports; read its §0 setup-drift list before assuming the environment is clean. It sits in `done/` as a superseded record, not as work — nothing in it is scheduled.
5. `docs/architecture/runner_fleet.md` — the online-heartbeat prerequisite and the execution boundary a lease crosses.
6. `docs/architecture/data_flow.md` — one workspace stream, fleet-tagged frames, reconnect backfill.
7. `docs/v2/done/M181_006_P0_API_INFRA_OBS_STAGING_SOAK_AND_SWAP.md` §4 — the Zig daemon deletion that already landed there; this spec grades the tree it left behind and touches no Zig.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/tests/e2e/acceptance/fleet-execution.spec.ts` | CREATE | the leg no journey has: install, activate, trigger, lease, execute to a real result, observe. §1's oracle. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/**` | EDIT | an execution fixture beside the existing install and lifecycle helpers — a bundle whose SKILL.md body is real work, not the empty body `runner-detail` relies on. |
| `.github/workflows/deploy-dev-acceptance.yml` | NONE | the execution journey joins `acceptance-e2e` by the directory glob it already runs; the human verdict is a committed file, not a job output (Indy, Sep 07, 2026 — see Discovery). |
| `make/acceptance.mk` | EDIT | a local twin for the execution journey, so a developer runs what CI runs. |
| `rustd/crates/afd_connector/**` | EDIT | M186_001's §0 connector repairs, ported: user-authorized connect that restores an existing installation, idempotent disconnect, one workspace/provider writer guard, identity-bound state completion. |
| `rustd/crates/afd_api_tenant/src/handler/connector/**` | EDIT | the routes those repairs surface through. |
| `rustd/crates/**` | EDIT | whatever §1 surfaces — bounded by §2's rule that a fix lands with the test that caught it, never on its own. |
| `playbooks/operations/acceptance/001_playbook.md` | CREATE | the human pass: what a person opens, in what order, what they must see, and where the evidence lands. |
| `playbooks/operations/acceptance/verdicts/<build-sha>.md` | CREATE | the recorded human verdict for the graded build: reviewer, build, defects raised. |
| `docs/architecture/scenarios/github-pr-reviewer.md` | EDIT | the proof punch list this spec finally closes. |
| `scripts/model-library-allowlist.json`; `scripts/check_model_allowlist.py` | EDIT | folded in by Indy on Sep 07, 2026 (see Discovery): one priced Fireworks spelling, GLM 5.3 and 5.3 Flash, GPT 6 Astra with the 5.6 line at the page's current rates, Opus 5, Fable 5.1 and Sonnet 5's now-standard price — every rate read from the provider page the allowlist cites. |
| `ui/packages/app/tests/e2e/acceptance/{install-ui,seed}.ts`; `login-install-lifecycle`, `workspace-fleet-lifecycle`, `runner-detail`, `template-onboarding`, `signup-lifecycle`, `operator-journey`, `platform-library-onboarding` specs | EDIT | §2's first defect: every dashboard install onboards a STABLE template so identical bytes converge on one library row, and the platform-library journey walks "Load more" the way a person does. |
| `docs/v2/done/M186_001_P0_DOCS_INFRA_LIVE_CONNECTOR_PROOF.md` | DONE (Sep 03, 2026) | superseded — its dimensions moved here and the file records where they went; closed to `done/` on main ahead of this spec's CHORE(open), so no edit rides this milestone's diff. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — TCF (an acceptance journey that cannot fail is theatre — every new assertion is made red before it is trusted), ECL (a provider or network outage mid-journey is an environment condition, not a product defect, and the lane must say which it saw), TST-NAM (journey names carry no milestone), UFS (fixture identifiers and journey selectors are named constants).
- `dispatch/write_ts_adhere_bun.md` — the journeys and fixtures are TypeScript; TS FILE SHAPE DECISION at PLAN.
- `dispatch/write_rust.md` — the connector port is Rust; preserved error variants, deterministic concurrency tests.
- `dispatch/write_auth.md` → the product's `docs/AUTH.md` — §3 touches provider authorization and token minting.
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — the playbook and architecture edits are published prose.
- `dispatch/verify.md` — every done-claim is a rubric row; a package-scoped runner never satisfies one.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| CI/CD edit approval | no | sought at PLAN; Indy chose to leave CI alone, and the journey needs no workflow edit to be required |
| UI / DESIGN TOKEN | yes | journeys assert on design-system selectors, never on arbitrary class strings |
| LENGTH / UFS | yes | journeys and fixtures under the caps; selectors and fixture ids as named constants |
| SCHEMA GUARD | no | no schema change |
| ERROR REGISTRY | yes | the connector port keeps its `UZ-` codes; a ported handler answering a new code declares it |
| MILESTONE-ID | yes | none in source; the playbook is docs (exempt) |
| ZIG GATE | no | no Zig is written or deleted here |

## Prior-Art / Reference Implementations

- **Reference:** `ui/packages/app/tests/e2e/acceptance/login-install-lifecycle.spec.ts` — the install → observe → bill → halt walk this spec extends through execution. Its auth prefix, teardown and workspace helpers are reused rather than re-invented.
- **Reference:** `ui/packages/app/tests/e2e/acceptance/runner-detail.spec.ts` — the only journey that reaches a real lease. Read its comment on WHY it fails closed before the model call: that constraint is exactly what §1 must lift, and lifting it safely is the design question.
- **Reference:** M186_001 §0 — fourteen dimensions of live-setup drift, already diagnosed. §3 ports them; it does not rediscover them.

## Sections (implementation slices)

### §1 — The fleet sequence completes, and the lane says so every deploy — DONE

The first deliverable, and deliberately first: before any connector work, prove the ordinary path. One fleet, from a gallery card to a finished job, against the Rust daemon on the development environment — then keep proving it on every deploy.

**Why the existing corpus is not already this.** The 44 journeys cover auth, install, navigation, billing and lifecycle, and they are real. What none of them asserts is a fleet producing a RESULT: the closest, `runner-detail.spec.ts`, seeds a bundle with an empty SKILL.md body precisely so the lease fails closed before the model call, because that was the only model-free way to place a failed lease from outside. That made a triage journey possible and left the success path unproven.

- **Dimension 1.1** — DONE — a fleet installed from the gallery reaches the active state on the Rust daemon, through the same dashboard walk an operator uses → Test `a gallery install reaches active without a confirm step`
- **Dimension 1.2** — DONE — an online runner leases that fleet's delivery, and the lease is observable as the operator's own view of it → Test `the delivery is leased by an online runner`
- **Dimension 1.3** — DONE — the lease executes to a REAL result rather than failing closed: the fleet's work completes, and the result is readable from the fleet's thread → Test `the lease finishes and its result reaches the thread`
- **Dimension 1.4** — DONE — the workspace stream carries that activity exactly once, to the acting fleet's tile and no other → Test `activity routes to one tile over one workspace stream`
- **Dimension 1.5** — DONE — the whole walk runs unattended inside the `acceptance-e2e` job of `deploy-dev / acceptance`, which is already required, and a red journey is a red job → Test `the execution journey is a required acceptance job` (the Playwright acceptance project globs the directory, pinned by a unit test over the config)
- **Dimension 1.6** — DONE — the journey distinguishes a PRODUCT failure from an environment one and names which it saw, so a provider outage never reads as a regression (RULE ECL) → Test `an unreachable dependency is reported as environment, not defect`

### §2 — What §1 surfaces gets fixed here, with the test that caught it — DONE

The half that is normally lost. A first honest end-to-end walk finds defects; the rule is that each lands as a fix plus the assertion that would have caught it, in one commit, inside this milestone. A defect filed and deferred is scope leaving the spec.

- **Dimension 2.1** — DONE — every defect §1 surfaces is either fixed with a regression test in the same commit, or carries an Indy-acked verbatim deferral quote in Discovery. No third category → Test `each Discovery defect row resolves to a commit or a quote`
- **Dimension 2.2** — DONE — the fix commits do not weaken the journey that caught them: no assertion is relaxed, no wait is lengthened past its documented budget, no step is skipped → Test `no acceptance assertion is weakened by a fix commit`

### §3 — The live connector proof, ported to Rust — DONE

M186_001's dimensions, re-planned onto the Rust tree. The proof is unchanged; the implementation it grades is.

**Its Files Changed cannot be carried over.** M186_001 names `src/agentsfleetd/http/handlers/connectors/{binding_tx,disconnect,sql}.zig` as CREATE rows. Landing those would write Zig files into a tree M181_006 §4 deletes. Every row is re-read against `rustd/crates/afd_connector/**` at CHORE(open) — which M186_001's own reactivation clause already promised and never got to run.

- **Dimension 3.1** — DONE (the live refusal this milestone existed to find, fixed) — `Connect` authorizes the GitHub user and restores the unique accessible existing installation to the selected workspace → Test `connect restores an existing installation`
- **Dimension 3.2** — DONE — `Disconnect` removes the vault handle and reverse-routing row, is safe to retry, and does not uninstall the external App → Test `disconnect is idempotent and leaves the external app alone`
- **Dimension 3.3** — DONE — every provider callback commits its routing row and its sealed grant in ONE transaction, and a disconnect removes both or neither; no advisory lock, because the transaction is the guard (Indy, Sep 07, 2026 — see Discovery) → Test `a connect that cannot seal its grant leaves no routing row`
- **Dimension 3.4** — DONE — a provider return completes only for the identity that started its signed state → Test `completion rejects a different identity without consuming the state`
- **Dimension 3.5** — DONE — one signed GitHub delivery creates exactly one fleet event and one fleet-authored review; the replay of that exact delivery creates neither again → Test `a replayed delivery adds no second event or review`
- **Dimension 3.6** — DONE — the fleet receives no material from a provider its trigger does not declare → Test `an undeclared connector is never injected`

### §4 — A person watches it work, and the evidence is a file — IN_PROGRESS (4.1 playbook DONE, 4.2's mechanism DONE: verdict file format + `01_verdict_check.sh`) · 4.2's verdict and 4.3's sign-off are UNMET, and **M193_001** carries the walk that produces them

A green Playwright run and a dashboard an operator would trust are different claims, and only the first is automated. This section is the second, made repeatable: a written walk, a recorded verdict, and evidence that lands in the lane's artifacts rather than in a chat message.

- **Dimension 4.1** — DONE — the playbook states the walk as ordered steps with an explicit "you must see" per step, so two people running it reach the same verdict → Test `the acceptance playbook carries a see-this assertion per step`
- **Dimension 4.2** — the human verdict is a committed file under `playbooks/operations/acceptance/verdicts/`, naming the reviewer, the build sha and any defect raised; CHORE(close) grades its presence for the build the PR ships → Test `the verdict file names the reviewer and the build` (a shell check in the playbook, run by hand). **Amended Sep 07, 2026:** "the build the PR ships" cannot exist before merge. `deploy-dev.yml` triggers on `push: branches: [main]`, so no feature-branch build ever reaches dev, and the two defects this milestone found — the missing `approval-signing` row and the `%2F` relay — both live on the path the walk has to cross. The verdict is therefore recorded against the MERGE COMMIT, as the first post-merge action, and CHORE(close) grades that the walk is scheduled rather than that it is done. The skeleton at `verdicts/7e73e7f8d.md` stays in the tree refusing until then.
- **Dimension 4.3** — the walk is signed off against the Rust daemon by a person, with screenshots attached to the milestone's PR → graded by Indy's explicit go in Discovery, not by a command

## Interfaces

```text
POST /v1/workspaces/{workspace_id}/fleets                       install a fleet
POST /v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages   trigger one delivery (the steer)
GET  /v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages   the thread, with response_text
GET  /v1/workspaces/{workspace_id}/events/stream                the one workspace SSE stream
GET  /v1/workspaces/{workspace_id}/events                       its durable backfill
GET  /v1/fleets/runners                                         runner liveness
POST /v1/webhooks/{fleet_id}/github                             the signed delivery §3.5 replays
```

No new endpoint. Every route above ships before this spec starts; what changes is that a journey walks all of them in one sequence.

## Failure Modes

| Failure | Trigger | Handling |
|---|---|---|
| The execution leg is flaky | a real model call in a required gate makes the lane non-deterministic | the fixture bundle does deterministic work with no model dependence; if that proves impossible the journey is quarantined to a scheduled lane and §1.5 is re-scoped in Discovery, never left flaky-and-required |
| A provider outage reads as a regression | GitHub or the model endpoint is down mid-run | RULE ECL — Dimension 1.6's classifier names environment, and the gate reports it as such |
| A fix in §2 papers over §1 | a defect is "fixed" by relaxing the assertion that caught it | Dimension 2.2 asserts the journey's assertions are not weakened; the diff is reviewed against the journey as it stood |

## Invariants

1. Every acceptance assertion added here is made red before it is trusted (RULE TCF) — a journey that passes against a broken daemon is worse than none, because it reads like evidence.
2. A defect §1 surfaces leaves this milestone as a fix with a test, or as an Indy-acked verbatim quote. There is no third disposition.
3. The human pass produces a file, not a recollection — Dimension 4.2 fails the gate when the verdict artifact is missing or names a different build.
4. No Zig is written in this milestone, and none is deleted either — §3 ports onto the Rust tree, and the sunset is M181_006 §4's.

## Metrics & Observability

The acceptance lane is the operator-facing signal and it already reports through the `results` job; this spec adds one journey to what that job runs, and one committed file per human sign-off. No new product analytics event: the journeys observe surfaces that already emit, and a journey that needed a new event to be observable would be asserting on instrumentation rather than behaviour.

| Signal | Where | Proof |
|---|---|---|
| Execution journey verdict | `deploy-dev / acceptance` → `acceptance-e2e` | Dimension 1.5 |
| Human visual verdict + reviewer + build | `playbooks/operations/acceptance/verdicts/` | Dimension 4.2 |
| Environment-vs-defect classification | journey output | Dimension 1.6 |

## Test Specification (tiered)

| Tier | Scope | Runner |
|---|---|---|
| e2e (required) | §1's execution journey, §3.5's delivery and replay | `make acceptance-e2e` → `deploy-dev / acceptance` |
| e2e (existing) | the 44 journeys, re-pointed at the Rust-served environment | `make acceptance-e2e` |
| cli | the CLI leg of the install and lifecycle walk | `make cli-acceptance` |
| integration | §3's connector writers, guards and identity binding | `make test-integration-rustd` |
| unit | §3's ported pure logic; §2's regression tests where the defect is unit-shaped | `make test-unit-all` |
| human | §4's playbook walk | `playbooks/operations/acceptance/001_playbook.md` |

`/orly-write-unit-test` runs once per Section over that Section's diff and again at the boundary. `/orly-write-integration-test` runs at the boundary for §3, which crosses a module boundary with real input and output.

## Acceptance Rubric (single scoring surface)

| # | Outcome | Verify command | Expected | Priority | Graded |
|---|---|---|---|---|---|
| R1 | A fleet executes end to end (§1) | `make acceptance-e2e` | exit 0, the execution journey among the passing specs | P0 | ✅ run 34188812140 on `9d5bfdf93` — `fleet-execution.spec.ts` ran at [37/94] and is absent from the failure list. §1's walk is green on dev on the build carrying the Accept fix |
| R2 | The journey is a required gate (§1.5) | `cd ui/packages/app && bun run test -- playwright.acceptance` (the config pin) and the `acceptance-e2e` run for HEAD | the pin passes; the run lists `fleet-execution.spec.ts` | P0 | ✅ run 34188812140 listed it at [37/94], so the glob pin holds and the journey is required. `fleet-webhook-delivery.spec.ts` — R2's blocker last run — PASSED at [43/94], which settles its defect row as test-side: it never reached `classifyUnleased`, so no product defect stood behind it. The job's one remaining red, `login-install-lifecycle.spec.ts:45`, is diagnosed and fixed in this branch |
| R3 | The CLI leg holds (§1) | `make cli-acceptance` | exit 0 | P0 | ✅ `acceptance / acceptance-cli` success on run 34147331454, the deploy of `6ebe1e2cc` |
| R4 | Connector proof green on the Rust tree (§3) | `make test-integration-rustd` | exit 0 | P0 | ✅ `make test-integration-rustd` exit 0 — **409 passed / 0 failed** on this tree (408 at `7e73e7f8d`, +1 the relay test) |
| R5 | Every §1 defect resolved or quoted (§2) | inspect Discovery's defect table | no row without a commit or a verbatim quote | P0 | ✅ both defect rows resolve to a commit in this branch; no row carries a deferral |
| R6 | Human verdict recorded (§4) | `ls playbooks/operations/acceptance/verdicts/` | a file for the graded build naming the reviewer | P0 | ❌ no verdict file exists for any build. The walk has never been completed by a person. **M193_001** is the spec that will run it, but a pending spec is not evidence, so this row stays ❌ and this spec stays in `active/` — the ship gate admits only ✅ for a P0, and it has no override. Two earlier gradings here (`SCHEDULED`, then `HANDED OFF`) were invented categories that let the spec reach `done/` without the work; both were wrong and greptile caught the second. The walk was attempted live on `9d5bfdf93` and got further than any before it — GitHub accepted the redirect with no consent screen, the exchange succeeded — then stopped on the `User-Agent` defect this branch fixes. A verdict cannot be recorded against a build whose connect refuses, and the fix reaches dev only on merge. The verdict is owed against this PR's merge commit, as the first post-merge action. Superseded detail from the previous grading: no verdict file for `6ebe1e2cc`. The skeleton this spec's §4.2 amendment named at `verdicts/7e73e7f8d.md` was never committed — `git ls-tree -r 7e73e7f8d` under that directory returns `README.md` alone — so the check refuses for a missing file rather than a refusing one. The walk itself got three legs further on Sep 8: connect now reaches GitHub, GitHub accepts the redirect, and the exchange defect above stops it |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | ✅ `make harness-verify` exit 0 (Sep 07, this tree) |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ exit 0 — 5829 = cargo 2476 + app 2647 + cli 146 + design-system 560 |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | ✅ exit 0 — red twice first: `rustfmt --check` on `integration_connector_github.rs:256` and an unfulfilled `#[expect(clippy::expect_used)]` at `replacement.rs:12`, both leftovers of the six-file split, both repaired |
| S4 | Version sync | `make check-version` | exit 0 | P0 | ✅ exit 0, all versions 0.29.0 |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `gitleaks detect` — no leaks found, 5252 commits scanned |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ plus one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE.

## Dead Code Sweep

N/A — no files deleted. This spec ADDS an acceptance journey, ports connector
proof onto the Rust tree, and writes a playbook; nothing it touches becomes
dead.

The sweep this spec used to carry is M181_006 §4's as of Sep 04, 2026. That is
not a deferral — it is a different milestone's scope, gated on nothing this spec
produces, and it may well land before §1 here even starts.

## Out of Scope

- New product features. Every route this spec walks ships before it starts.
- Performance and soak budgets — M181_006 owns those; this spec asks whether the work COMPLETES, not how fast.
- The OpenAPI coverage gate (M181_003), the export (M181_004), the collectors (M181_005).
- Rewriting the 44 existing journeys. They are re-pointed and fixed where they break; a redesign is separate scope.

## Product Clarity (authoring record)

1. **Successful user moment** — an operator installs a fleet from the gallery, watches it pick up work and finish, and reads the result. Today no test asserts that moment exists.
2. **Preserved user behaviour** — every existing journey keeps passing; the install, lifecycle and billing walks are unchanged.
3. **Optimal-way check** — the optimal proof is the walk a customer takes, run unattended on every deploy. A synthetic harness that stubbed the runner would prove the harness.
4. **Rebuild vs iterate** — iterate. 44 journeys, their fixtures, auth and teardown already exist; this adds one journey and re-points the rest.
5. **What we build** — the execution journey, the connector port, and the human playbook.
6. **What we do NOT build** — new endpoints, new dashboards, new analytics events, a second acceptance framework.
7. **Fit with existing features** — rides `deploy-dev / acceptance` as it stands; the new journey is one more spec in a suite that already reports into `results`.
8. **Surface order** — dashboard first (it is the operator's own view), CLI second, API assertions only where the UI cannot observe the fact.
9. **Dashboard restraint** — no new surface. The journey observes what an operator already sees.
10. **Confused-user next step** — a failed run names which leg broke and whether it was product or environment (Dimension 1.6), so the reader's next move is obvious from the failure line alone.

## Decomposition & alternatives (patch vs refactor)

**M186_001 is superseded, not duplicated.** Its §1–§5 never ran and its Files Changed names Zig paths under the tree M181_006 §4 deletes, so running it as written would land code with a guaranteed zero lifespan. Its dimensions move here: §0's connector repairs become §3, and its live Slack and GitHub proof becomes §3.5–§3.6. M186_001 is edited to record where its scope went rather than deleted, because it carries Indy's dated quotes and the incident history that produced them, and rewriting those would falsify the record.

**Both alternatives about the deletion are now moot, and the reason is worth keeping.** This spec once rejected folding into M181_006, and rejected deleting at cutover, on one argument: that spec's rollback WAS the Zig binary, so a milestone could not both depend on it and delete it. The argument was sound and its premise was already false — the cutover had been accelerated and rollback was the previous image digest. Both rejections rested on the same dissolved fact, which is why the deletion could move in one step rather than being re-argued.

## Discovery (consult log)

| Date | Consult | Outcome |
|---|---|---|
| Sep 01, 2026 | Indy — scope | "read and port to the M186 work, first will be to test the sequence of a fleet we have end to end fully and the look at the fixes." §1 is the fleet sequence; §2 is the fixes; §3 is the port. |
| Sep 01, 2026 | Indy — human pass | "it could have human part to eyeball manually as well and then lets add the tests as needed to verify it in acceptance* job or so." §4 is the eyeball; §1.5 is the acceptance job. |
| Sep 01, 2026 | Indy — deletion | "all is good with this spec the agentsfleet zig related and its files must be deleted." Was §5, gated behind §1–§4 by Invariant 1. **The decision stands; its home moved to M181_006 §4 on Sep 04, 2026** — the gate's stated reason (the tree is the rollback) had dissolved. |
| Sep 02, 2026 | Agent — blast-radius grep (`dispatch/write_spec.md` §Authoring discipline) | The teardown grep §5 never ran. `git grep -rn -w 'agentsfleet-runner'` returns live hits in `release.yml`, `deploy-dev-build.yml`, `deploy-dev-metal.yml`, `deploy/baremetal/agentsfleet-runner.service`, `build.zig:185`, `build_runner.zig`, `README.md:43`, `SECURITY.md:19,23,24,27` and `AGENTS.md:12`. The runner is Zig-only with no Rust counterpart; §5's runner rows are BLOCKED pending the row below. |
| Sep 02, 2026 | Indy — runner disposition | "the src/runner will be on zig no action needed there." The runner is not part of the cutover. The deletion (then §5, now M181_006 §4) scoped to `src/agentsfleetd/**`; the runner-survives assertions went with it. |

| Sep 07, 2026 | Agent — execution path | No model-free execution exists from outside: `src/runner/child_exec.zig:145` fails closed on an empty body, and every non-empty body reaches the provider through `src/runner/engine/runner.zig:184`; the stub is a comptime flag. §1.3 makes one real model call and asserts outcome, never text; §1.6 classifies provider failure as environment. |
| Sep 07, 2026 | Indy — CI edit | "I think leave CI alone, since the current acceptance-e2e fails, if it passes take it as a go." No workflow edit; the journey joins `acceptance-e2e` by glob and that job's result is the gate. |
| Sep 07, 2026 | Indy — writer guard | "Why doyou need the advisory lock" — it is not needed: `grant.rs:185-214` already commits both rows in one transaction. 3.3 amended to assert that property. |
| Sep 07, 2026 | Indy — human verdict | "I donot understand this? the acceptance-e2e can be assumed for red or green" — the job result is the automated gate; the human verdict is a committed file, no job reads it. 4.2 amended. |
| Sep 07, 2026 | Agent — Interfaces | The steer route is `POST …/fleets/{id}/messages` (`afd_api_tenant/src/lib.rs:110`) and the stream is `…/events/stream` (`stream.rs:166`); the Interfaces block is corrected. |

| Sep 07, 2026 | Indy — GitHub fact-find | "I thought the github was done in the milestones @M175...M189x to see if we had fixed the github issue. I recall doing it. Fact find" — it was: `1414cc8d1` (Jul 11) shipped `github/ownership.zig`; M180_001's `afd_connector` (Aug 29, `16bcbb1f7`) left the GitHub arm "not reachable today"; M181_006 §4 deleted the Zig. §3.1 is the port of that lost logic. Divergence: the zero-installation branch answers `UZ-CONN-008` instead of redirecting to GitHub's install page, because the Rust tree carries no App slug. |
| Sep 07, 2026 | Indy — catalogue | "Also I find two FIREWORKS and FIREWORKS-AI? keep one, ensure the model library seed has the GLM 5.3 Flash GLM 5.3, OpenAI has GPT 6 Astra and so on" — folded into this spec and PR rather than a second tree. `fireworks` keeps the rows; `fireworks-ai` carries `duplicate_spelling`. The dev catalogue still holds the `fireworks-ai` rows the seed script leaves alone by design: removing them is a catalogue-playbook apply plus a delete, Indy's. Together's GLM 5.3 rows are NOT added — its page shows no cached rate and nothing here invents one. |

| Sep 07, 2026 | Indy — drop the rate audit | "could you delete the audits/cross-tier-rates.sh as well?" Done in both repositories: `~/Projects/orly` branch `chore/drop-cross-tier-rates` (commit `eec8074`) removes the script and its `registry.json` entry, and this branch removes the materialised copy plus its `.oracle/orly.json` row. The script had NO caller — its header claimed `make harness-verify`, which lists eight gates and never it. What it guarded, `RUN_NANOS_PER_SEC`, is pinned instead by `rustd/crates/afd_billing/tests/cross_runtime_rates.rs` — a test beside the constant it defends, which reads both TypeScript mirrors and runs in `make test-unit-all` rather than in a gate nothing invoked. `dispatch/write_changelog.md` and `docs/CHANGELOG_VOICE.md` still mention it; both are orly-managed and the orly source already dropped the mention, so `orly update` clears them once the pack is published. Closed specs under `docs/v2/done/` keep their references as history. |
| Sep 07, 2026 | Agent — runner-offline alert is broken | Investigating Indy's "56.7 years" Grafana panel found a live defect beyond the display. `agentsfleet_runner_last_seen_seconds` publishes a Unix epoch timestamp (`afd_observability/src/runner.rs:253`), while every consumer expects an AGE in seconds: `docs/architecture/runner_fleet.md:693` says "render-time delta", the panel is titled "Runner seconds since last seen", and the critical alert is `agentsfleet_runner_last_seen_seconds > 90` (`grafana/assets/alerts.json:42`). A timestamp is always greater than 90, so the "Runner heartbeat overdue" alert can never have worked. NOT this milestone's scope; recorded here so it is not lost, and raised to Indy for its own spec. |

| Sep 07, 2026 | Agent — every dev connector answers 503, and it is not the app bags | The blocker that held §4.2/4.3 for two sessions. `state_secret` (`afd_api_tenant/src/handler/connector/mod.rs:100`) resolves the deployment's HMAC key BEFORE any provider bag is read, and refuses the whole connect when it is absent — for every provider, with no log line. The key is a vault row named `approval-signing` (`afd_http/src/services/ingress.rs:176`), and the dev admin workspace held `fireworks github-app jira-app linear-app qstash slack slack-app zoho-app` and not that. Zig read the same secret from the ENVIRONMENT (`config/env_resolve.zig:37` → `cmd/serve_secrets.zig:34`, zero hits for a vault row of that name in the whole Zig tree); the Rust cutover moved it into the vault and neither the deploy nor the sync script followed. `grep -rn APPROVAL_SIGNING_SECRET rustd/` returns nothing, so the Fly variable `deploy-dev-fly.yml:44,100` still sets is read by no one. Everything the previous session ruled out was correctly ruled out — the answer was a fifth `unconfigured()` producer nobody had counted. |
| Sep 07, 2026 | Agent — why the catalogue was not the contradiction it looked like | `provisioned()` (`afd_connector/src/app.rs:134`) lists `<provider>-app` NAMES and never reads `approval-signing`, so `configured: true` and a 503 are consistent. Both `SELECT_SECRET_PROJECTIONS` (`sql.rs:108`) and `SELECT_SECRET_ENVELOPE` (`sql.rs:147`) filter the same table on the same predicate, which is what makes the catalogue decisive about presence — the read the previous session filed as "not evidence" was the one that settled it. |
| Sep 07, 2026 | Agent — dev has had no debug logging since the cutover | The clue both prior sessions reasoned from ("the daemon never logs `secret_opened`") was a measurement artifact. `deploy-dev-fly.yml:94` sets `LOG_LEVEL="debug"`; the daemon reads `AGENTSFLEET_LOG_LEVEL` (`agentsfleetd/src/logs.rs:44`) and falls back to `LevelFilter::INFO` (`logs.rs:50`). `flyctl secrets list -a agentsfleetd-dev` shows `LOG_LEVEL` and no `AGENTSFLEET_LOG_LEVEL`. So `secret_opened` (`afd_vault/src/load.rs:79`) and `connector_connect_initiated` (`connect.rs:206`) — both `tracing::debug!` — are unreachable on dev whatever happens. `docs/architecture/observability.md:156` documents the correct name, so the workflow is the stale half. Same cutover pattern as the row above. **Fixed here on Indy's call** ("i think just send the PR with the log level debug"): `deploy-dev-fly.yml` now sets `AGENTSFLEET_LOG_LEVEL="debug"` and no longer sets the dead spelling, pinned by `test_dev_deploy_sets_the_daemons_own_log_level`, which fails on either half. Prod never set it, so prod stays at INFO by design and needs no change. The stale `LOG_LEVEL` secret still sits on the Fly app until someone unsets it; it is read by nothing and harmless. |
| Sep 07, 2026 | Indy — seed the row, and make the pipeline do it | "Seed the vault row now", then "seed the vault and keep the playbooks and the pipeline updated" and "so the pipeline seeds the secrete". Done: the dev row was created live (HTTP 201) and connect went 503 → 200 for github, zoho and slack against `api-dev` with no restart, because `state_secret` reads per request. `platform_secret_sync.sh` gains an `approval-signing` case, both deploy families gain a post-deploy seeding step, and `credentials_test.sh` gains a gate that fails when either family loses it — proven red by removing the dev step. |
| Sep 07, 2026 | Agent — the seed exposed a second defect in the same flow | With connect answering 200, Indy clicked Connect and GitHub refused: "The `redirect_uri` is not associated with this application." The minted URI was `…/api%252Fconnectors/github/callback`. `RELAY_PATH` was one string `"api/connectors"` handed to `Url::path_segments_mut().extend()`, which percent-encodes each item as a single segment, so the separator became data. Zig built the same route from a format string (`callback.zig:46`), which is why it never had it. The existing test could not catch it: it compares `relay_uri` against `relay_url` and both come out of `relay`, so it agreed with itself. Fixed in `callback.rs` with `the_relay_spells_the_route_the_dashboard_mounts`, proven red without the fix. |
| Sep 07, 2026 | Agent — the cutover weakened a recorded security invariant | `M157_001` justified an invariant with "The secret is boot-resolved daemon config, NOT a workspace secret, so a fleet holding `secret_read` cannot reach it". It is now a workspace vault row, so that sentence is false as written. There is no value-read API (`handler/secret.rs` exposes POST, GET-list, PUT, DELETE only), so the exposure is the WRITE path: `PUT /v1/workspaces/{admin}/secrets/approval-signing` would let a holder of `secret:write` on the admin workspace replace the platform-wide HMAC key and forge approval-webhook signatures and install states. Surfaced to Indy, who chose to seed now and fix properly later; NOT this milestone's scope and it wants its own spec. |

| Sep 08, 2026 | Indy — why `/v1/connectors/` is called legacy | "I donot understand the fix for callback url? why is v1/connectors/ legacy?" It is not a legacy ROUTE: one path carries two routes with two guards (`handler/connector/callback.rs:1-7`) and the `POST` is the live completion endpoint. What is legacy is pointing a provider REGISTRATION at the `GET`, because a provider's redirect carries no session of ours — the daemon would have to write a connection for whoever the state claimed. The dashboard is the target because that is where the person's bearer already is, which is what §3.4's identity binding needs. Zig's own commit `3b8154da0` made that move on Aug 16, and dev deployed it the same day — `e8d040592`, Aug 16 10:32Z, `fly=success` — so the same click would have failed in Zig from that hour. |
| Sep 08, 2026 | Indy — "how come we missed the GitHub settings?" | Nobody missed it, and an earlier reading here was wrong twice. It took the red `deploy-dev` runs of Aug 10–21 for deploys that never happened: every one shipped Fly, and only the acceptance jobs after it were red. And it implied the registration move was agreed and shrugged off, which the record does not support — PR #607's body states it ("Existing provider registrations must move to `https://<APP_HOST>/api/connectors/{provider}/callback`"), the playbook was updated the same day (`Updated: Aug 16, 2026`), and M186_001's Files Changed scheduled it. What made the break survive 23 days is that an EXISTING connector row needs no re-consent: the connection made before the flip kept working, so the only action that could reveal the mismatch was a fresh connect or a reconnect. Indy, Sep 08: "i can vouch that the connected GH had an issue, disconnect and connect ... since i wanted to test again. and never tested that milestone." The reconnect was the armed trap, and the port started first. |
| Sep 08, 2026 | Indy — was the reconnect path ever tested? | Built and tested, never exercised. Zig's `github/callback_integration_test.zig` carries eight integration tests including `GitHub callback restores an existing installation after internal state loss` — the reconnect case exactly — beside four `ownership.zig` unit tests over zero/one/several installations and malformed discovery responses. Every one runs against `FakeGitHub` on `127.0.0.1` (`callback_integration_test.zig:90`). The live pass was M136_001 → M186_001 and never ran, so the one property a fake cannot hold — that the vendor answers in the format the daemon parses — was never checked in either tree. That is the seam the `Accept` defect sat in, and the argument for §4 existing at all. |
| Sep 08, 2026 | Agent — a fixture more forgiving than the vendor certifies the bug | 409 integration tests exercised the exchange and every one of them passed while every live GitHub connect refused. `fake_provider.rs` answered JSON whatever the request asked for, so the daemon's missing `Accept` header could not be seen from inside the suite. The fixture now answers the way GitHub does; with the pre-fix daemon it turns 5 connector-callback tests red on the exact `UZ-CONN-006` dev returned. The lesson is the fixture's, not the daemon's: a vendor stub that is easier to satisfy than the vendor is a test that certifies the defect. |

| Sep 08, 2026 | Agent — the third header, one call after the second | The live walk on `9d5bfdf93` reached GitHub, took the redirect with no consent screen, and the exchange SUCCEEDED — then `UZ-CONN-006`, `req_4fb822da2280`, 403. The 403 is not the exchange: `github/probe.rs` sends no `User-Agent`, and `api.github.com` refuses an unnamed request with "Request forbidden by administrative rules" BEFORE reading `Authorization`. Measured: no header → 403, header → 401 "Requires authentication". `afd_library/src/github.rs:13` has always sent `agentsfleetd`; the connector's probe is the half that did not follow. What hid it is that `opens()` reads 401/403/404 as "this token does not open that installation", so a malformed request surfaced as every person lacking access to every installation. Third instance of the `Accept` class, and the fixture was forgiving again — `fake_provider` now enforces the rule GitHub enforces. |
| Sep 08, 2026 | Agent — the wrong leg was named | `probe.rs:85` raised `UZ-CONN-006`, whose sentence says the provider "refused the token exchange", for a failure of the installation LISTING. The exchange had succeeded, so the code sent the diagnosis to a client secret that was working. `UZ-CONN-009` now names the listing. The registry gate earned its keep on the first attempt: a code declared but absent from `REGISTRY` is invisible to every check in `tests/error_code.rs`, exactly as that file's comment warns. |
| Sep 08, 2026 | Agent — `login-install-lifecycle.spec.ts:45` was never a product defect | Two wrong readings before the artifact settled it. Not two anchors in one tile (`FleetTile` renders one; the visible "Manage fleet →" is a `<span>`), and not a duplicated row (`schema/890`'s `ON CONFLICT (fleet_id)` requires a UNIQUE constraint, so the counters join cannot fan out). `error-context.md:27` records `unexpected value "hidden"` and the a11y snapshot at failure time lists ONE link. During a router transition React keeps the outgoing tree mounted and hidden while the incoming one renders, so the document briefly holds two anchors for one fleet; Playwright throws a strict-mode violation the instant a locator resolves to more than one element and does not retry past it. `expectRowState` now reads the visible wall — one line, four specs. Same family as the `awaitLease` row below: a test asserting on a state it misread. |
| Sep 08, 2026 | Indy — the Zig graph | "is the agentsfleetd zig completely cleaned up... How about the build.zig and build.zig.zon? do they only have the necessary ones". They did not. Seven of eight dependency pins had no consumer — `b.dependency()` is called once in the whole graph, for nullclaw. `make bench-incident` had been unrunnable since `2f0021d1b` (`-Dwith-bench-tools` deleted with the daemon graph). `tests/bench/` imported a `bench_app` module that no longer exists. The manifest named `agentsfleetd`, a binary the graph no longer builds, and listed an empty `config` directory. All swept. |
| Sep 08, 2026 | Indy — the incident bench | "who will use that? bench-incident? i donot understand the need for this bench?" Nobody used it. `bench.yml` is `workflow_dispatch` only AND disabled; M157_001's rubric row R6 that was supposed to consume it has an EMPTY graded cell in a spec marked DONE; it had been broken since the cutover unnoticed. Deleted on Indy's call. A gate nobody runs is not a gate, and the last non-runner Zig went with it. |
| Sep 08, 2026 | Agent — a green lane that reads red | I reported `zig build test-lib` as failing. It was not: exit 0, 159/159. `src/lib/logging/mod_test.zig:258` wrote `startup probe: ok` to real stderr, and `zig build` echoes a test binary's stderr under the label `failed command:`. Proven by deletion — without that line the lane prints nothing. The call now formats an empty string, keeping the branch and dropping the noise. Recorded because the misreading was mine and the output invited it. |
| Sep 08, 2026 | Indy — closing with an ungraded row | "You said you will create a new spec with the pending items in this spec, if so why are you complaining on R6? Or when you moved this spec to done, what are you thoughts?" Fair, and the close was inconsistent: R6 was marked SCHEDULED, a third category invented to let the spec move to `done/` when the ship gate admits only ✅ or a return to EXECUTE — then flagged in the PR body as owed, which is complaining about a hole this author dug. Resolved by creating **M193_001**, which was promised earlier in the session and not delivered. R6 is now a handoff to a named owner rather than an open row. |
| Sep 08, 2026 | greptile — P1 on #670, and it is right | "M187's ship gate requires every P0 rubric row to be graded ✅, but R6 is marked HANDED OFF while M193 remains pending and contains no completed verdict evidence." Correct, and it is the SECOND time this row was dressed up rather than graded: first `SCHEDULED`, then `HANDED OFF`, each a third category invented so the spec could reach `done/`. The gate reads `any ❌ → return to EXECUTE` and gives P0 no override. R6 is now ❌ and this spec is parked in `active/` with `Status: IN_PROGRESS`. M193_001 still owns the walk; owning work is not the same as having done it. |

**Defect table (§2)** — populated during §1; every row resolves to a commit or a verbatim quote before CHORE(close).

| Defect | Surfaced by | Resolution |
|---|---|---|
| The published platform entry never shows on the regular workspace's gallery first page (`deploy-dev` run 34084609124, `platform-library-onboarding.spec.ts:229`) | §1.1's gallery walk, and the CI run Indy pointed at | Not a product defect: the fixture workspace held 103 leftover tenant templates from per-run unique onboards and the platform entry sat last under the recorded M143 order. Fix: stable templates in every fixture (`installViaUI` callers, `runner-detail`, `template-onboarding`) plus `loadWholeGallery` in the platform journey; pinned by `test_every_dashboard_install_names_a_constant_template`. 28/28 on dev. |
| The wall tile's overlay link carries no text, so a `toContainText` on the link reads empty (`fleet-execution.spec.ts` first run) | §1.4 | Test-side: the tile locator anchors on the `[data-kind]` card that holds the link. Not a product defect. |
| Every connector on dev answers 503 `UZ-CONN-001` at connect, github and zoho and slack alike (`req_10adc2703aab`, `req_caddaca807fc`, `req_51c0f8091687`) | §4.2's walk, which could not start | Environment, then process. The admin workspace held no `approval-signing` row because the Rust daemon reads from the vault what the Zig daemon read from the environment. Row seeded live (201); connect verified 200 for all three with no restart. Both deploy families now seed it, `platform_secret_sync.sh` knows how, and `credentials_test.sh` fails when a family stops — proven red. |
| The minted `redirect_uri` carries `%2F` where the path needs a separator, so every provider refuses the consent screen (`/api%2Fconnectors/github/callback`) | §4.2's walk, once the row above unblocked connect | Product defect, fixed here. `RELAY_PATH` is now two segments rather than one slash-bearing string (`afd_connector/src/callback.rs`), pinned by `the_relay_spells_the_route_the_dashboard_mounts`, which asserts the literal route and that no relay carries percent-encoding. Proven red without the fix. **Verified live on `6ebe1e2cc`, Sep 8:** the minted URI is now `https://app-dev.agentsfleet.net/api/connectors/github/callback` and GitHub accepts it. |
| The dev GitHub App's registered callback still named the API host, so GitHub refused every consent screen with `Invalid Redirect URI` | §4.2's walk, once the relay fix reached dev | Configuration — and the unfinished half of a Zig commit, not a cutover regression. `3b8154da0` (Aug 16) moved the minted redirect to the dashboard and demoted the API path to a relay; M186_001's Files Changed carried the matching row ("Move every provider registration to the authenticated dashboard callback URL") and never ran. Indy re-registered the dashboard URL on Sep 8. Measured rather than assumed: an authorize call carrying NO `redirect_uri` redirects to whatever is registered, which is how the stale entry named itself. |
| The token exchange asks for no JSON, so GitHub answers form-encoded and the daemon cannot read a grant it was issued (`UZ-CONN-006`, `req_bc84d6211732`) | §4.2's walk, once the registration matched | Product defect, fixed here. `exchange.rs` sends `Accept: application/json` — the header Zig sent at `connectors/oauth2.zig:112-115` and the port dropped — and `fake_provider.rs` now answers form-encoded unless a caller asks for JSON. Proven red: 5 connector-callback failures carrying the same `UZ-CONN-006` and 502 dev returned. Green after: 16 connector tests, then the 409-test lane at exit 0. Reaches dev only on merge. |
| `awaitLease` reads an absent event row as a lease, so `fleet-webhook-delivery` failed on a bare length mismatch instead of its own classifier (`deploy-dev` run 34147331454) | §1.5's lane, on the journey's first run against dev | Test defect, fixed here. Its docstring claimed the ingress writes the row before answering 202; `afd_ingress::deliver`'s module note says the opposite — nothing reaches Postgres at ingress and the row appears at LEASE. An empty listing now reads as the waiting state, so a delivery nobody takes reaches `classifyUnleased` and names environment or product (RULE ECL). Which of the two dev hit is what the next lane run will say; the current run could not tell anyone. |
| The GitHub installation probe sends no `User-Agent`, so `api.github.com` refuses every listing with 403 and the daemon reports it as a failed token exchange (`UZ-CONN-006`, `req_4fb822da2280`) | §4.2's walk, once the Accept fix reached dev | Product defect, fixed here. `github/probe.rs` names the daemon on both vendor calls through one extracted builder; `UZ-CONN-009` splits the listing from the exchange. Proven red twice — the unit test on the daemon's own builder (`left: None, right: Some("agentsfleetd")`) and the live vendor contract against real `api.github.com` (`left: 403, right: 403`). `fake_provider` now enforces the vendor's rule so the class fails in the 409-test lane rather than on a person. |
| `expectRowState` counts anchors without asking whether they are visible, so `login-install-lifecycle.spec.ts:45` fails whenever it lands inside a router transition (`deploy-dev` run 34188812140) | §1.5's lane | Test defect, fixed here. React keeps the outgoing tree mounted and hidden while the incoming one renders; the locator matched both copies and Playwright throws on a multi-match without retrying. Filtering to the visible tree fixes it for the four specs sharing the helper. The page was correct every run — evidenced by the a11y snapshot and screenshot in the failure artifact. |
