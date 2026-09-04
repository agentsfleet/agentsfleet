<!-- SPEC AUTHORING RULES — read docs/TEMPLATE.md before editing this file. -->

# M187_001: One fleet, installed to executed to observed, against the Rust daemon

**Prototype:** v2.0.0
**Milestone:** M187
**Workstream:** 001
**Date:** Sep 01, 2026
**Status:** PENDING
**Priority:** P0 — the cutover family proves the Rust daemon SERVES; nothing yet proves a customer's fleet finishes its job on it.
**Categories:** API, CLI, INFRA, UI
**Batch:** B10 — last in the v2 sequence; runs against the binary M181_006 leaves serving.
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
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

**Problem:** the cutover family proves the Rust daemon answers every route, holds its budgets and can be rolled back. None of that is a customer finishing a job. The 41 acceptance journeys under `ui/packages/app/tests/e2e/acceptance/` run against the daemon serving `api-dev`, and the one that reaches a real runner lease — `runner-detail.spec.ts` — is deliberately built to FAIL closed before the model call, because an empty SKILL.md body is the only model-free way to place a failed lease from the outside. So the repository has never asserted that a fleet runs to a real result. M186_001 was written to close exactly that gap and its §1–§5 never ran; its Files Changed still names Zig paths, so running it as written would land connector code into a tree that is being deleted.

**Solution summary:** re-point the existing acceptance corpus at the Rust-served environment and add the leg it has never had — a fleet that executes to a real result and is observed doing so — then fix what that surfaces rather than filing it. Port M186_001's connector proof onto `rustd/crates/afd_connector/**`, keeping its dimensions and discarding its Zig paths. Add one human visual pass with a recorded checklist, because a green Playwright run and a dashboard a person would trust are different claims.

## PR Intent & comprehension handshake

- **PR title (eventual):** test(acceptance): prove a fleet end to end on the Rust daemon
- **Intent (one sentence):** an operator can watch one fleet go from a gallery card to a finished job on the Rust daemon, and the repository asserts that walk on every deploy instead of hoping.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/tests/e2e/acceptance/` — the 41 journeys that exist; `login-install-lifecycle.spec.ts` is the closest walk and `runner-detail.spec.ts` is the only one that reaches a lease.
2. `.github/workflows/deploy-dev-acceptance.yml` — the `qa` / `acceptance-e2e` / `acceptance-cli` / `results` jobs this spec extends, and the gate they report into.
3. `make/acceptance.mk` — `acceptance-e2e` and `cli-acceptance`, the local twins CI runs.
4. `docs/v2/done/M186_001_P0_DOCS_INFRA_LIVE_CONNECTOR_PROOF.md` — the dimensions §3 ports; read its §0 setup-drift list before assuming the environment is clean. It sits in `done/` as a superseded record, not as work — nothing in it is scheduled.
5. `docs/architecture/runner_fleet.md` — the online-heartbeat prerequisite and the execution boundary a lease crosses.
6. `docs/architecture/data_flow.md` — one workspace stream, fleet-tagged frames, reconnect backfill.
7. `docs/v2/pending/M181_006_P0_API_INFRA_OBS_STAGING_SOAK_AND_SWAP.md` §Dead Code Sweep — the rollback window this spec closes, and why the deletion waits for it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/tests/e2e/acceptance/fleet-execution.spec.ts` | CREATE | the leg no journey has: install, activate, trigger, lease, execute to a real result, observe. §1's oracle. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/**` | EDIT | an execution fixture beside the existing install and lifecycle helpers — a bundle whose SKILL.md body is real work, not the empty body `runner-detail` relies on. |
| `.github/workflows/deploy-dev-acceptance.yml` | EDIT | the execution journey joins `acceptance-e2e`; the human-pass evidence joins `results` as a recorded artifact rather than a chat message. |
| `make/acceptance.mk` | EDIT | a local twin for the execution journey, so a developer runs what CI runs. |
| `rustd/crates/afd_connector/**` | EDIT | M186_001's §0 connector repairs, ported: user-authorized connect that restores an existing installation, idempotent disconnect, one workspace/provider writer guard, identity-bound state completion. |
| `rustd/crates/afd_api_tenant/src/handler/connector/**` | EDIT | the routes those repairs surface through. |
| `rustd/crates/**` | EDIT | whatever §1 surfaces — bounded by §2's rule that a fix lands with the test that caught it, never on its own. |
| `playbooks/operations/acceptance/001_playbook.md` | CREATE | the human pass: what a person opens, in what order, what they must see, and where the evidence lands. |
| `docs/architecture/scenarios/github-pr-reviewer.md` | EDIT | the proof punch list this spec finally closes. |
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
| CI/CD edit approval | yes | `.github/workflows/**` edits in §1 need Indy's explicit approval — sought at PLAN, not at commit |
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

### §1 — The fleet sequence completes, and the lane says so every deploy

The first deliverable, and deliberately first: before any connector work, prove the ordinary path. One fleet, from a gallery card to a finished job, against the Rust daemon on the development environment — then keep proving it on every deploy.

**Why the existing corpus is not already this.** The 41 journeys cover auth, install, navigation, billing and lifecycle, and they are real. What none of them asserts is a fleet producing a RESULT: the closest, `runner-detail.spec.ts`, seeds a bundle with an empty SKILL.md body precisely so the lease fails closed before the model call, because that was the only model-free way to place a failed lease from outside. That made a triage journey possible and left the success path unproven.

- **Dimension 1.1** — a fleet installed from the gallery reaches the active state on the Rust daemon, through the same dashboard walk an operator uses → Test `a gallery install reaches active without a confirm step`
- **Dimension 1.2** — an online runner leases that fleet's delivery, and the lease is observable as the operator's own view of it → Test `the delivery is leased by an online runner`
- **Dimension 1.3** — the lease executes to a REAL result rather than failing closed: the fleet's work completes, and the result is readable from the fleet's thread → Test `the lease finishes and its result reaches the thread`
- **Dimension 1.4** — the workspace stream carries that activity exactly once, to the acting fleet's tile and no other → Test `activity routes to one tile over one workspace stream`
- **Dimension 1.5** — the whole walk runs unattended in `deploy-dev / acceptance` and fails the gate when any leg breaks → Test `the execution journey is a required acceptance job`
- **Dimension 1.6** — the journey distinguishes a PRODUCT failure from an environment one and names which it saw, so a provider outage never reads as a regression (RULE ECL) → Test `an unreachable dependency is reported as environment, not defect`

### §2 — What §1 surfaces gets fixed here, with the test that caught it

The half that is normally lost. A first honest end-to-end walk finds defects; the rule is that each lands as a fix plus the assertion that would have caught it, in one commit, inside this milestone. A defect filed and deferred is scope leaving the spec.

- **Dimension 2.1** — every defect §1 surfaces is either fixed with a regression test in the same commit, or carries an Indy-acked verbatim deferral quote in Discovery. No third category → Test `each Discovery defect row resolves to a commit or a quote`
- **Dimension 2.2** — the fix commits do not weaken the journey that caught them: no assertion is relaxed, no wait is lengthened past its documented budget, no step is skipped → Test `no acceptance assertion is weakened by a fix commit`

### §3 — The live connector proof, ported to Rust

M186_001's dimensions, re-planned onto the Rust tree. The proof is unchanged; the implementation it grades is.

**Its Files Changed cannot be carried over.** M186_001 names `src/agentsfleetd/http/handlers/connectors/{binding_tx,disconnect,sql}.zig` as CREATE rows. Landing those would write Zig files into a tree M181_006 §4 deletes. Every row is re-read against `rustd/crates/afd_connector/**` at CHORE(open) — which M186_001's own reactivation clause already promised and never got to run.

- **Dimension 3.1** — `Connect` authorizes the GitHub user and restores the unique accessible existing installation to the selected workspace → Test `connect restores an existing installation`
- **Dimension 3.2** — `Disconnect` removes the vault handle and reverse-routing row, is safe to retry, and does not uninstall the external App → Test `disconnect is idempotent and leaves the external app alone`
- **Dimension 3.3** — every provider callback and disconnect commits its rows under one workspace/provider writer guard → Test `connector writers wait on the shared workspace provider lock`
- **Dimension 3.4** — a provider return completes only for the identity that started its signed state → Test `completion rejects a different identity without consuming the state`
- **Dimension 3.5** — one signed GitHub delivery creates exactly one fleet event and one fleet-authored review; the replay of that exact delivery creates neither again → Test `a replayed delivery adds no second event or review`
- **Dimension 3.6** — the fleet receives no material from a provider its trigger does not declare → Test `an undeclared connector is never injected`

### §4 — A person watches it work, and the evidence is a file

A green Playwright run and a dashboard an operator would trust are different claims, and only the first is automated. This section is the second, made repeatable: a written walk, a recorded verdict, and evidence that lands in the lane's artifacts rather than in a chat message.

- **Dimension 4.1** — the playbook states the walk as ordered steps with an explicit "you must see" per step, so two people running it reach the same verdict → Test `the acceptance playbook carries a see-this assertion per step`
- **Dimension 4.2** — the human verdict is recorded as an artifact the `results` job reads, with the reviewer, the build, and any defect raised → Test `the results job fails when the human verdict artifact is absent or stale`
- **Dimension 4.3** — the walk is signed off against the Rust daemon by a person, with screenshots attached to the milestone's PR → graded by Indy's explicit go in Discovery, not by a command

## Interfaces

```text
POST /v1/workspaces/{workspace_id}/fleets            install a fleet
POST /v1/fleets/{fleet_id}/steer                     trigger one delivery
GET  /v1/workspaces/{workspace_id}/events            the one workspace stream
GET  /v1/fleets/runners                              runner liveness
POST /v1/webhooks/{fleet_id}/github                  the signed delivery §3.5 replays
```

No new endpoint. Every route above ships before this spec starts; what changes is that a journey walks all of them in one sequence.

## Failure Modes

| Failure | Trigger | Handling |
|---|---|---|
| The execution leg is flaky | a real model call in a required gate makes the lane non-deterministic | the fixture bundle does deterministic work with no model dependence; if that proves impossible the journey is quarantined to a scheduled lane and §1.5 is re-scoped in Discovery, never left flaky-and-required |
| A provider outage reads as a regression | GitHub or the model endpoint is down mid-run | RULE ECL — Dimension 1.6's classifier names environment, and the gate reports it as such |
| A fix in §2 papers over §1 | a defect is "fixed" by relaxing the assertion that caught it | Dimension 2.2 asserts the journey's assertions are not weakened; the diff is reviewed against the journey as it stood |

## Invariants

2. Every acceptance assertion added here is made red before it is trusted (RULE TCF) — a journey that passes against a broken daemon is worse than none, because it reads like evidence.
3. A defect §1 surfaces leaves this milestone as a fix with a test, or as an Indy-acked verbatim quote. There is no third disposition.
4. The human pass produces a file, not a recollection — Dimension 4.2 fails the gate when the verdict artifact is missing or names a different build.
4. No Zig is written in this milestone, and none is deleted either — §3 ports onto the Rust tree, and the sunset is M181_006 §4's.

## Metrics & Observability

The acceptance lane is the operator-facing signal and it already reports through the `results` job; this spec adds two rows to what that job carries — whether the execution journey passed, and whether a human signed the build off. No new product analytics event: the journeys observe surfaces that already emit, and a journey that needed a new event to be observable would be asserting on instrumentation rather than behaviour.

| Signal | Where | Proof |
|---|---|---|
| Execution journey verdict | `deploy-dev / acceptance` → `results` | Dimension 1.5 |
| Human visual verdict + reviewer + build | `results` artifact | Dimension 4.2 |
| Environment-vs-defect classification | journey output | Dimension 1.6 |

## Test Specification (tiered)

| Tier | Scope | Runner |
|---|---|---|
| e2e (required) | §1's execution journey, §3.5's delivery and replay | `make acceptance-e2e` → `deploy-dev / acceptance` |
| e2e (existing) | the 41 journeys, re-pointed at the Rust-served environment | `make acceptance-e2e` |
| cli | the CLI leg of the install and lifecycle walk | `make cli-acceptance` |
| integration | §3's connector writers, guards and identity binding | `make test-integration-rustd` |
| unit | §3's ported pure logic; §2's regression tests where the defect is unit-shaped | `make test-unit-all` |
| human | §4's playbook walk | `playbooks/operations/acceptance/001_playbook.md` |

`/orly-write-unit-test` runs once per Section over that Section's diff and again at the boundary. `/orly-write-integration-test` runs at the boundary for §3, which crosses a module boundary with real input and output.

## Acceptance Rubric (single scoring surface)

| # | Outcome | Verify command | Expected | Priority | Graded |
|---|---|---|---|---|---|
| R1 | A fleet executes end to end (§1) | `make acceptance-e2e` | exit 0, the execution journey among the passing specs | P0 | |
| R2 | The journey is a required gate (§1.5) | `gh workflow view "deploy-dev / acceptance" --yaml \| grep -c fleet-execution` | at least 1 | P0 | |
| R3 | The CLI leg holds (§1) | `make cli-acceptance` | exit 0 | P0 | |
| R4 | Connector proof green on the Rust tree (§3) | `make test-integration-rustd` | exit 0 | P0 | |
| R5 | Every §1 defect resolved or quoted (§2) | inspect Discovery's defect table | no row without a commit or a verbatim quote | P0 | |
| R6 | Human verdict recorded (§4) | the `results` job artifact for the graded build | present, names the reviewer and the build | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |

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
- Rewriting the 41 existing journeys. They are re-pointed and fixed where they break; a redesign is separate scope.

## Product Clarity (authoring record)

1. **Successful user moment** — an operator installs a fleet from the gallery, watches it pick up work and finish, and reads the result. Today no test asserts that moment exists.
2. **Preserved user behaviour** — every existing journey keeps passing; the install, lifecycle and billing walks are unchanged.
3. **Optimal-way check** — the optimal proof is the walk a customer takes, run unattended on every deploy. A synthetic harness that stubbed the runner would prove the harness.
4. **Rebuild vs iterate** — iterate. 41 journeys, their fixtures, auth and teardown already exist; this adds one journey and re-points the rest.
5. **What we build** — the execution journey, the connector port, the human playbook, and the deletion.
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
| Sep 02, 2026 | Indy — runner disposition | "the src/runner will be on zig no action needed there." The runner is not part of the cutover. §5 scopes to `src/agentsfleetd/**`; R9 and Dimension 5.4 assert the runner build survives it. |

**Defect table (§2)** — populated during §1; every row resolves to a commit or a verbatim quote before CHORE(close).

| Defect | Surfaced by | Resolution |
|---|---|---|
