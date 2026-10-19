# M230_001: Test lanes reuse the one shared datastore stack; worktree teardown nukes worktree state

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

**Prototype:** v2.0.0
**Milestone:** M230
**Workstream:** 001
**Date:** Oct 19, 2026
**Status:** PENDING
**Priority:** P2 — developer workflow: container and volume multiplication on a multi-worktree machine, matching the M223 family
**Categories:** API, DOCS, INFRA
**Batch:** B1 — single workstream; no parallel lane
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M223_001 (shared launcher, per-worktree database and Dragonfly namespace machinery this spec extends); coordinate with in-flight M229_001, which edits the same compose and lane files for the Postgres 19 release candidate (RC) 1 bump
**Provenance:** LLM-drafted (GLM 5.3 Fast, 2026-10-19) from Indy's in-session direction
**Canonical architecture:** `docs/architecture/datastore_scaling.md` §Shared local development

---

## Overview

**Goal (testable):** A second worktree running `make test-integration-rustd` against a healthy shared stack starts zero new containers and cannot observe the first worktree's fixtures, and `make down` leaves no worktree database rows or namespace keys behind.
**Problem:** Each worktree that runs the integration or coverage lanes brings up its own Postgres/Dragonfly/QStash/Mailpit compose project (`make/test-infra.mk:161`), so a five-worktree machine runs five stacks plus orphaned stacks after worktrees are deleted; and no teardown drops a worktree's database or namespace anywhere in the dev scripts.
**Solution summary:** Extend M223's shared-stack design from the dev-daemon flow to the test and coverage lanes: `_ensure-test-infra` targets the repository's shared stack instead of bringing up a per-worktree project; the lane's database becomes `test_<worktree-id>` created and migrated per run; Dragonfly connections carry the worktree namespace through the existing `AGENTSFLEET_DEV_NAMESPACE` knob, and the lane reset becomes namespace-scoped instead of cluster-wide. Only the slot-migration/cluster-fault suites keep a disposable stack, because slot topology is cluster-global. `make down` becomes the full worktree teardown: stop the daemon, drop the worktree's databases, sweep its namespace.

## PR Intent & comprehension handshake

- **PR title (eventual):** Lanes reuse the shared datastore stack; teardown nukes worktree state
- **Intent (one sentence):** A machine running lanes in N worktrees has exactly one datastore stack and zero per-worktree residue after teardown, instead of N stacks plus orphans.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `scripts/local-dev-state.sh` — the machinery to mirror: `dev_identity` (worktree Identifier (ID) from path checksum, `local-dev-state.sh:19-39`), `dev_snapshot`, `dev_compose`, `dev_database` (idempotent per-worktree database creation, `:140-142`), `dev_stack_down` (guarded teardown, `:188-194`).
2. `make/test-infra.mk` — the lane surface being converted: `_ensure-test-infra` (`:150-179`), `_reset-test-db` fixed-database reset and cluster-wide flush (`:189-201`), exported lane environment (`:100-143`).
3. `docs/v2/done/M223_001_P2_API_DOCS_INFRA_WORKTREES_SHARE_DEVELOPMENT_DATASTORES.md` — the namespace design and Dimensions this spec extends.
4. `rustd/crates/afd_dragonfly/src/namespace/mod.rs` — the validated namespace boundary and `AGENTSFLEET_DEV_NAMESPACE` (`:6`); the marker-scoped sweep lives in `rustd/crates/afd_dragonfly/tests/integration_namespace/sweep.rs`.
5. `docs/architecture/datastore_scaling.md` §Shared local development — canonical architecture; this spec amends its lane carve-out in the same commit as the implementation.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `make/test-infra.mk` | EDIT | Lane targets the shared stack; per-worktree `test_<worktree-id>` database; namespace-scoped Dragonfly reset; split out the slot-migration/cluster-fault suites onto the disposable stack |
| `make/test-integration-rustd.mk` | EDIT | Integration and coverage suites inherit the shared-stack targeting; per-worktree migrate step |
| `make/bench.mk` | EDIT | Consumes `_ensure-test-infra`; follows the retargeted infra |
| `make/dev.mk` | EDIT | `down` becomes full worktree teardown (daemon, databases, namespace) |
| `scripts/local-dev.sh` | EDIT | Implements worktree teardown: drop `dev_<worktree-id>` and `test_<worktree-id>` databases, sweep the namespace |
| `scripts/local-dev-state.sh` | EDIT | Per-worktree teardown helper; `DEV_STACK` validation widened to accept worktree-named disposable projects for the orphan sweep |
| `scripts/local-dev-test.sh` | EDIT | Two-worktree proof extended: concurrent lanes, zero new containers, teardown residue checks |
| `scripts/make_reset_order_test.sh` | EDIT | Drives the reset; follows the scoped reset |
| `docker-compose.yml` | EDIT | Header narrows to the slot-migration/cluster-fault disposable suite only |
| `.github/workflows/test-integration-rustd.yml` | EDIT | Continuous Integration (CI) lane targeting follows the makefile; **requires Indy's explicit CI-edit approval at EXECUTE** |
| `docs/development.md` | EDIT | Replaces the "retains its own disposable stack" lane paragraph with the shared-stack design and the teardown behaviour |
| `docs/architecture/datastore_scaling.md` | EDIT | §Shared local development: lanes join the shared stack; snapshot image-pinning rule recorded |
| `docs/architecture/testing.md` | EDIT | Lane datastore description follows |
| `.orly/LOCAL.md` | EDIT | Integration-lane bullet reflects the shared stack |
| `rustd/crates/afd_api/tests/{identity_signup_live.rs,integration_tenant_models.rs,integration_tenant_provider.rs}` | EDIT | Consume the per-worktree lane environment; drop `agentsfleetdb` defaults |
| `rustd/crates/afd_fleet/tests/support/fleet_queue.rs` | EDIT | Same lane-environment consumption |
| `rustd/crates/afd_db/tests/{config.rs,config_tls.rs,config_tls_cert_files.rs,integration_pool.rs}` | EDIT | Replace fixed `agentsfleetdb` expectations with the exported lane URL |
| `rustd/crates/afd_bench/src/error/kind.rs` | EDIT | Only if it names the retired fixed database; verify at EXECUTE |
| `scripts/seed-models.mjs` | EDIT | Verify only: dev-flow default, expected unaffected by the lane change |

Historical records (`docs/v2/done/*`, `playbooks/operations/acceptance/baselines/*`, `docs/reviews/*`) carry blast-radius hits as history and are not edited.

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — UFS (named constants for the new environment and database values), NLR (the lane makefiles are touched end-to-end, not surgically), NLG (no compatibility framing: no retained "per-worktree stack mode" alongside the new path), ORP (the `DEV_STACK` widening is orphan cleanup).
- `.orly/dispatch/write_shell.md` — `scripts/local-dev*.sh` edits; Bash 3 compatibility per the file headers.
- `.orly/dispatch/write_rust.md` — test-file edits in `rustd/crates/**`.
- `.orly/dispatch/name_architecture.md` — database and namespace naming grounded in `docs/architecture/datastore_scaling.md` §Shared local development.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| write_shell | yes — `local-dev*.sh`, `make_reset_order_test.sh` | Quoted expansions, array arguments, Bash 3; mirror existing style in those files |
| write_rust | yes — `rustd/crates/**/tests/**` | Test-only edits; preserve error variants |
| write_any | yes | LENGTH caps; MILESTONE-ID in commits; UFS named constants for every new exported value |
| write_documentation | yes — `docs/development.md`, architecture docs | Read `.orly/docs/DOCUMENTATION_RULES.md` before prose |
| CI edit guard | yes — `.github/workflows/**` listed in Files Changed | Explicit Indy approval recorded in Discovery before that edit |
| ZIG / UI / DESIGN TOKEN / SCHEMA | no | No `.zig`, no `ui/`, no `.sql` edits — `teardown.sql` is reused unmodified |

## Prior-Art / Reference Implementations

- **Reference:** `scripts/local-dev-state.sh` (this repository) — the shared-stack snapshot, per-worktree identity, idempotent database creation and guarded teardown this spec mirrors for the lanes; alignment is the point.
- **Reference:** `docs/architecture/datastore_scaling.md` §Shared local development — the isolation table (per-worktree Postgres database, `AGENTSFLEET_DEV_NAMESPACE` prefixing, per-daemon QStash callback ports, distinct Mailpit recipients) already documents every sharing pattern the lanes need.

## Sections (implementation slices)

### §1 — Lanes target the shared stack

The integration and coverage lanes reuse the repository's one shared stack through the same state machinery as the dev launcher: `_ensure-test-infra` verifies shared-stack health instead of running `docker compose up` from the worktree, the lane's `TEST_DATABASE_URL` selects a per-worktree `test_<worktree-id>` database created idempotently and migrated before suites run, and every suite datastore connection carries the worktree namespace via `AGENTSFLEET_DEV_NAMESPACE`. **Implementation default:** namespace value `test_<worktree-id>` reusing the existing knob, because a second environment name for the same boundary would be an alias.

- **Dimension 1.1** — A second worktree running the integration lane against a healthy shared stack starts zero new containers → Test `lanes_create_no_new_containers`
- **Dimension 1.2** — The lane creates and migrates its `test_<worktree-id>` database idempotently on every run → Test `lane_database_created_and_migrated_idempotently`
- **Dimension 1.3** — Two worktrees running the same suites concurrently against one shared stack observe only their own fixture rows, keys, streams and mail → Test `lanes_isolate_concurrent_worktrees`
- **Dimension 1.4** — Existing namespace isolation proofs stay green → Test `namespaces_isolate_values_sessions_and_scans` (regression)

### §2 — Reset scoped to the worktree

`_reset-test-db` applies `teardown.sql` to the worktree's own database and replaces the cluster-wide Dragonfly flush with a namespace-scoped sweep using the existing sweep machinery, so one worktree's reset leaves every other worktree's fixtures intact. `KEEP_TEST_STATE=1` keeps its skip-reset meaning.

- **Dimension 2.1** — Resetting worktree A leaves worktree B's rows, keys, streams and consumer groups intact → Test `namespace_reset_leaves_sibling_state_intact`
- **Dimension 2.2** — `KEEP_TEST_STATE=1` skips the reset → Test `keep_test_state_skips_reset` (regression)

### §3 — Slot-migration and cluster-fault suites stay disposable

Suites that move cluster slots keep today's per-worktree disposable compose project, because slot topology is cluster-global and cannot be namespaced. The integration gate routes them by explicit selection, and `docker-compose.yml` remains their definition.

- **Dimension 3.1** — The slot-migration suite runs green on its disposable stack while another worktree runs the shared-stack lane → Test `slot_migration_suite_on_disposable_stack`

### §4 — Worktree teardown nukes worktree state

`make down` in a worktree stops its daemon, deregisters it, drops its `dev_<worktree-id>` and `test_<worktree-id>` databases, and sweeps its namespace keys. `make up` afterwards recreates and migrates, which it already does on every start.

- **Dimension 4.1** — After `make down`, `pg_database` holds no row for the worktree and its namespace holds zero keys → Test `teardown_leaves_no_worktree_residue`
- **Dimension 4.2** — `make up` after `make down` reaches a healthy daemon → Test `up_after_down_rebuilds` (regression)

### §5 — Orphan sweep and documentation

`make down DEV_STACK=<project>` accepts worktree-named disposable projects in addition to `agentsfleet-dev-*`, so a worktree deleted without teardown can be swept with the documented command instead of raw docker. The lane carve-out lines in the docs and architecture pages are rewritten to the one-stack design in the same commits.

- **Dimension 5.1** — `make down DEV_STACK=<worktree-named project>` removes that project's containers and volumes → Test `orphan_disposable_stack_sweep`
- **Dimension 5.2** — Documentation states the one-stack design with no disposable-lane carve-out → Test `docs_currency_check` (manual): grep rows in the Acceptance Rubric

## Interfaces

```
# Lane environment exported by make/test-infra.mk (shared-stack form)
TEST_DATABASE_URL=postgres://…@127.0.0.1:${AGENTSFLEET_PG_HOST_PORT}/test_<worktree-id>
TEST_DRAGONFLY_URL=redis://:agentsfleet@127.0.0.1:${AGENTSFLEET_DRAGONFLY_BASE_PORT}
TEST_DRAGONFLY_TLS_URL=rediss://:agentsfleet@127.0.0.1:${AGENTSFLEET_DRAGONFLY_LAST_PORT}
AGENTSFLEET_DEV_NAMESPACE=test_<worktree-id>   # existing knob, namespace/mod.rs:6

# Teardown surface (make/dev.mk)
make down                          # daemon stop + deregister + DROP worktree databases + namespace sweep
make down DEV_STACK=<project>      # orphan sweep; now accepts worktree-named disposable projects
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Shared Mailpit cross-reads | Two worktrees' invite suites share one inbox | Suites match messages by the recipient they invited, the pattern `datastore_scaling.md` already documents; the concurrency proof fails loudly on any cross-read |
| QStash schedule collision | Shared emulator with its fixed identity | Schedules key on the destination URL, which embeds each worktree's daemon port per the architecture table; collision covered by the concurrency proof |
| Slot topology mutated on the shared cluster | A slot-migration suite routed to the shared stack | Gate routes slot suites to the disposable stack by explicit selection; the lane smoke asserts the routing |
| Concurrent database creation race | Two worktrees create `test_<worktree-id>` simultaneously | Idempotent create-if-not-exists, the `dev_database` precedent (`local-dev-state.sh:141`), with duplicate-key tolerated as success |
| Snapshot image skew | The shared snapshot is pinned by whichever worktree provisioned it | The lane runs against the snapshot's datastore versions and prints them; the pinning rule is recorded in `datastore_scaling.md`; no silent mismatch |

## Invariants

1. One shared stack per machine serves dev daemons and the shared-stack lane; a lane run creates zero containers — asserted by the lane smoke in `scripts/local-dev-test.sh`.
2. No shared-stack lane query observes another worktree's database rows or namespace keys — enforced by per-worktree database names plus the `afd_dragonfly` namespace boundary; proven by the concurrency proof.
3. `make down` leaves zero worktree residue — the teardown itself verifies database absence and namespace emptiness, and fails loudly otherwise.
4. Cluster-global mutations (slot layout) occur only on disposable stacks — enforced by the gate's explicit suite routing.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable | not applicable | no product/operator signal changes | none | none | none |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | e2e | `lanes_create_no_new_containers` | With the shared stack healthy, a second worktree's lane run leaves `docker ps` container count unchanged |
| 1.2 | integration | `lane_database_created_and_migrated_idempotently` | Two consecutive lane starts in one worktree produce one `test_<worktree-id>` database, migrated, second start a no-op |
| 1.3 | e2e | `lanes_isolate_concurrent_worktrees` | Two worktrees run the same suite concurrently on one shared stack; each observes only its own fixtures, keys, streams and mail |
| 1.4 | integration | `namespaces_isolate_values_sessions_and_scans` | M223 namespace proofs remain green unchanged |
| 2.1 | integration | `namespace_reset_leaves_sibling_state_intact` | Reset in worktree A; worktree B's rows, keys, streams and consumer groups are byte-for-byte intact |
| 2.2 | integration | `keep_test_state_skips_reset` | `KEEP_TEST_STATE=1` preserves fixtures across runs |
| 3.1 | integration | `slot_migration_suite_on_disposable_stack` | Slot-migration suite green on its own project while a shared-stack lane runs elsewhere |
| 4.1 | integration | `teardown_leaves_no_worktree_residue` | After `make down`: zero `pg_database` rows for the worktree; zero keys under its namespace prefix |
| 4.2 | integration | `up_after_down_rebuilds` | `make up` after full teardown recreates the database, migrates and serves |
| 5.1 | integration | `orphan_disposable_stack_sweep` | `make down DEV_STACK=<worktree-named project>` removes its containers and volumes; refuses when live roots remain |
| 5.2 | manual | `docs_currency_check` | Procedure: run the R3 grep commands; evidence pasted into Pull Request Session Notes; required person: Indy |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Lanes reuse the shared stack and teardown leaves no residue (§1, §4) | `bash scripts/local-dev-test.sh --live <sibling-checkout>` | exit 0 | P0 | |
| R2 | Reset stays scoped to one worktree (§2) | `make test-integration-rustd 2>&1 \| tee /tmp/m230-lane.log; grep -c 'namespace_reset_leaves_sibling_state_intact' /tmp/m230-lane.log` | exit 0 and count ≥ 1 | P0 | |
| R3 | Documentation carries the one-stack design (§5) | `grep -c 'retains its own disposable stack' docs/development.md docs/architecture/datastore_scaling.md` | 0 matches | P1 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3a | Lint passes | `make lint-all` | exit 0 | P0 | |
| S3b | Integration suite passes | `make test-integration-rustd` | exit 0 | P0 | |
| S3c | Version sync passes | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.orly/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `.orly/dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

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

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted | N/A |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| fixed `agentsfleetdb` lane references | `git grep -rn -w 'agentsfleetdb' make/ \| wc -l` | 0 matches |
| cluster-wide `reset` in the shared-stack lane path | `git grep -n 'TEST_DRAGONFLY_CONTROL) reset' make/test-infra.mk \| grep -v disposable \| wc -l` | 0 matches |

## Out of Scope

- Rust target-directory and sccache disk hygiene (coverage artifact cleanup, target seeding) — separate follow-up; this session measured the problem but it is a different surface.
- CI remote cache topology and the warm Linux worker.
- Production and deployed datastore configuration; `teardown.sql` is reused unmodified.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy opens a fifth worktree, runs `make test-integration-rustd`, and `docker ps` still shows exactly the four shared containers when it finishes green.
2. **Preserved user behaviour** — Single-worktree lane runs keep today's semantics: reset-per-run, `KEEP_TEST_STATE=1` opt-in, identical suite outcomes.
3. **Optimal-way check** — Reusing M223's snapshot, identity and namespace machinery is the direct path; the gap was targeting, not missing primitives.
4. **Rebuild-vs-iterate** — Iterate; the isolation primitives shipped with M223, only the lanes' targeting changes.
5. **What we build** — Lane retargeting in `make/test-infra.mk` and `make/test-integration-rustd.mk`, per-worktree test databases, namespace-scoped reset, teardown nuke in the local-dev scripts, widened orphan sweep, doc corrections.
6. **What we do NOT build** — No new orchestration tooling, no per-lane compose projects beyond the slot suite, no CI cache work.
7. **Fit with existing features** — Compounds with M223's shared dev stack; must not destabilize the dev-daemon flow or the in-flight M229 schema work on the same files.
8. **Surface order** — N/A — no user surface; developer tooling only.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — N/A — no user surface; `make status` remains the developer's visibility command.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five Sections along the lane lifecycle — target, reset, isolate the slot suite, teardown, sweep+docs — because each lands independently behind the same design.
- **Alternatives considered:** keep per-worktree stacks (rejected: violates the one-stack design and multiplies containers and volumes, the exact sprawl observed); serialize all lanes on one shared fixed-name database (rejected: destroys concurrent worktree parallelism and reintroduces the cross-destruction failure recorded at `make/test-infra.mk:155-158`).
- **Patch-vs-refactor verdict:** this is a **patch** — retargeting existing machinery; no new abstraction is introduced.

## Discovery (consult log)

- **Consults** — name_architecture: `docs/architecture/datastore_scaling.md` §Shared local development answers the naming and sharing questions (per-worktree database, `AGENTSFLEET_DEV_NAMESPACE` prefix, per-daemon QStash callback ports, distinct Mailpit recipients). Its line "Its schema resets and cluster fault tests must never use the shared development project" is the carve-out this spec supersedes; the doc edit lands in the same commit as the implementation (landing rule (b)). Divergence evidence gathered in-session: `make/test-infra.mk:161` (bare `docker compose up` per worktree), `:192` (fixed `agentsfleetdb`), `:200` (cluster-wide flush), `local-dev-state.sh:159` (`DEV_STACK` validation limited to `agentsfleet-dev-*`), `make down` today stops only the daemon (no `DROP DATABASE` or namespace flush anywhere in `scripts/local-dev*.sh`), and the orphaned `754-app` stack from a deleted worktree as the teardown-gap proof.
- **Metrics review** — no analytics or funnel playbook update required; internal-only workflow change.
- **Skill-chain outcomes** — pending (`/orly-write-unit-test`, `/review`, `orly-babysit-prs` at their lifecycle stages).
- **Deferrals** — none yet. Owner direction on record: "i didnt want multiple stacks per work tree, i wanted a shared stack across mulitple trees… a tear down must nuke the database of that worktree and namespace from dragonfly that is the design" — Indy, session direction, Oct 19, 2026.
- **Required human decisions** — CI-edit approval for `.github/workflows/test-integration-rustd.yml` (operating-model CI/CD guard); rebase coordination with in-flight M229_001 on the shared compose and lane files.
