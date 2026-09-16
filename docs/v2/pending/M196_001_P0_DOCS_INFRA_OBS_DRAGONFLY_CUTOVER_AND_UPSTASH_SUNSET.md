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

# M196_001: Cut the deployments over to Dragonfly and sunset Upstash

**Prototype:** v2.0.0
**Milestone:** M196
**Workstream:** 001
**Date:** Sep 16, 2026
**Status:** PENDING
**Priority:** P0
**Categories:** DOCS, INFRA, OBS
**Batch:** B2
**Branch:** pending
**Baseline revision:** pending
**Test Baseline:** pending — measured at CHORE(open) against the recorded comparison commit.
**Depends on:** M192_001 (`feat/m192-dragonfly-cluster`). That branch makes the transport cluster-only; this one gives it a cluster to talk to.
**Provenance:** First revision. Scope set by Indy on 2026-09-16 (Discovery); the sequencing constraint below was found by reading `deploy-dev-fly.yml` against `afd_dragonfly`'s boot refusal, not proposed.
**Canonical architecture:** `docs/architecture/datastore_scaling.md`.

---

## Overview

**Goal (testable):** every deployment boots against a Dragonfly cluster, no workflow resolves an Upstash secret, and `DFLYCLUSTER`-shaped readiness passes in dev and prod.
**Problem:** the daemon on `main` after M192 cannot boot against either deployment's datastore. Both point at Upstash, Upstash is not a cluster, and the daemon refuses a non-cluster seed at startup. Nobody can deploy.
**Solution summary:** stand a Dragonfly cluster behind `api-dev.agentsfleet.net` as a container, repoint the dev workflow, rehearse there, repeat for prod against a fresh PostgreSQL and an empty Dragonfly, then delete the Upstash vault items last. The datastore cutover playbook, the architecture pages and the remaining Zig-era prose follow the deployments rather than leading them.

## PR Intent & comprehension handshake

- **PR title (eventual):** cut dev and prod over to Dragonfly and retire Upstash
- **Intent (one sentence):** make the M192 daemon deployable, because today it is not.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_dragonfly/src/preflight.rs` — what boot actually demands: `cluster_enabled`, sharded pub/sub, and a primary that retains rather than evicts. The cutover is not done until this file is satisfied against a real endpoint.
2. `docs/architecture/datastore_scaling.md` — the canonical target sharding and the evidence rules a claim here has to meet.
3. `playbooks/operations/cutover/001_playbook.md` — the SHAPE to mirror (register, probe tags, drain order, rollback rule) and the document §4 corrects: it is the Zig-to-Rust binary swap and its "no store change" premise is false for this one.
4. `.github/workflows/deploy-dev-fly.yml` — where the dev secret is resolved and handed to the daemon.
5. `docs/v2/done/M192_001_P0_API_INFRA_OBS_DRAGONFLY_SCALE_REDIS_PARITY.md` — the Discovery entries that already decided the fresh start and the knob names; this spec inherits them rather than re-deciding.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `.github/workflows/deploy-dev-fly.yml` | EDIT | The dev seed stops resolving an Upstash item and resolves the cluster's. |
| `.github/workflows/release.yml` | EDIT | The same for prod, after dev is green. |
| `playbooks/operations/datastore_cutover/001_playbook.md` | CREATE | The runbook: stand up, verify, repoint, drain, roll back. |
| `playbooks/operations/datastore_cutover/probes.sh` | CREATE | The probes each step is tagged with, refusing a rubric row that has none. |
| `playbooks/operations/datastore_cutover/probes_test.sh` | CREATE | Self-tests, discovered by `SCRIPT_SELF_TESTS` and run by `make lint-all`. |
| `playbooks/operations/cutover/001_playbook.md` | EDIT | Its "the same Postgres, the same Redis and the same ledger" rollback premise is false once the store moves; it is scoped to the binary swap it describes. |
| `docs/architecture/data_flow.md` | EDIT | Names Upstash as the queue. |
| `docs/architecture/high_level.md` | EDIT | Names Upstash in the deployment picture. |
| `docs/architecture/scaling.md` | EDIT | Names Upstash in the scaling story. |
| `docs/architecture/README.md` | EDIT | Names Upstash in the index. |
| `rustd/crates/**/*.rs` | EDIT | Comment prose only: the dead `.zig` citations M192 did not reach. No identifier moves. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — ORP (every removal lists its references), NLR (touch-it-fix-it on the files this opens), NLG (no legacy framing: the retired daemon is not a live constraint), NDC (no dead code at write time).
- `dispatch/write_shell.md` — `probes.sh` and its self-tests: quoted expansions, array arguments, temp-file cleanup, no untrusted `eval`.
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — the architecture pages are published docs; DOC-31's `verified` and `product_version` fields apply to every page this edits.
- `dispatch/name_architecture.md` — `docs/architecture/datastore_scaling.md` wins over this spec until reconciled.

## Applicable Gates

- **LENGTH** — `001_playbook.md` and `probes.sh` are new files under the caps; split by concern rather than trimming prose.
- **LOGGING** — no new runtime logging surface; satisfied by scope.
- **UFS** — `probes.sh` names each endpoint and each vault path once.
- **SCHEMA GUARD** — does not fire: no `schema/` edit. PostgreSQL is rebuilt from the existing migrations, not migrated.
- **DOC READ GATE** — fires on the architecture pages and on `probes.sh`; one proof line per triggered document per turn.
- **GREPTILE GATE** — end-of-turn read over the diff.

## Prior-Art / Reference Implementations

- `playbooks/operations/cutover/001_playbook.md` — owners block, probe tags, drain order, "why the rollback is boring" framing. The shape is right; only its premise about the store is wrong.
- `playbooks/operations/credential_rotation/001_playbook.md` — the vault-gate pattern for a step that touches 1Password, including the approval gate this spec's §3 needs.
- `docker-compose.yml` and `scripts/dragonfly-cluster.sh` — a four-node cluster that already bootstraps `DFLYCLUSTER CONFIG` and health-checks `CLUSTER SHARDS`. The dev container is this, hosted.

## Sections (implementation slices)

Execution order is §1 → §2 → §3, and it is not negotiable: dev rehearses, prod follows a green rehearsal, and the credential is deleted only once nothing needs it. §4–§6 may land in any order once §1 is green.

### §0: The constraint this spec exists to clear

Dependencies: none. M192 made the transport cluster-only. `afd_dragonfly::error::ErrorKind::NotACluster` refuses boot when `INFO cluster` does not answer `cluster_enabled:1`, and the refusal is classified permanent so no retry clears it. Both deployments resolve their seed from an Upstash item (`deploy-dev-fly.yml`, `release.yml`), and Upstash is not a cluster. **The first deploy of the M192 daemon therefore fails boot in dev and in prod.** This is not a migration that can be scheduled at leisure; it is the unblocking of a branch that is otherwise unshippable.

- **Dimension 0.1**: the boot refusal is reproduced against a non-cluster endpoint before anything is repointed, so the failure this spec prevents is observed rather than assumed → Test `test_a_non_cluster_seed_refuses_boot_with_its_own_class`, with `test_an_evicting_primary_refuses_boot` for the other permanent class. NOT DONE.

### §1: Dev on a container, behind `api-dev.agentsfleet.net`

Dependencies: §0. A Dragonfly cluster runs as a container for the dev environment — containers are the dev-only answer by decision (Discovery), not a pattern prod inherits. The dev workflow stops resolving `op://$VAULT_DEV/upstash-dev/api-url` and resolves the cluster's seed into `DRAGONFLY_URL`. The knob names are already what M192 renamed them to; this section changes what they point at, never what they are called.

- **Dimension 1.1**: the dev cluster answers `CLUSTER SHARDS` with online primaries and replicas, and the daemon's own preflight passes against it rather than against a local rig → Test `test_dev_cluster_satisfies_the_daemons_preflight`. NOT DONE.
- **Dimension 1.2**: the dev workflow resolves no Upstash path and the deployed process reads its seed from `DRAGONFLY_URL` → Test `test_dev_workflow_resolves_no_upstash_path`. NOT DONE.
- **Dimension 1.3**: a dev deploy boots, serves a request, and leases work end to end against the cluster → Test `test_dev_deploy_boots_and_leases_against_the_cluster`, with `test_a_failed_dev_deploy_leaves_the_previous_release_serving` for the failed-deploy arm. NOT DONE.

### §2: Prod, fresh

Dependencies: §1 green. `release.yml` repoints the same way. The cutover is a fresh start already decided in M192's Discovery (2026-09-14): PostgreSQL is rebuilt and Dragonfly provisioned empty, every pre-cutover event is discarded, and nothing is imported. There is therefore no import tool, no receipt, and no boot gate for one — a gate whose only fresh-boot exit is a genesis receipt is machinery for a migration that will not happen.

- **Dimension 2.1**: the prod workflow resolves no Upstash path and boots against the cluster → Test `test_prod_workflow_resolves_no_upstash_path`. NOT DONE.
- **Dimension 2.2**: a rebuilt PostgreSQL and an empty Dragonfly accept new work and settle it exactly once, with no pre-cutover row consulted → Test `test_a_fresh_pair_accepts_and_settles_without_prior_state`. NOT DONE.

### §3: Sunset Upstash

Dependencies: §1 and §2 both green. 226 references across more than twenty files. The vault items `upstash-dev` and `upstash-prod` are deleted LAST, because deleting a credential is the one step in this spec with no rollback: a repointed workflow can be reverted in a commit, and a deleted secret cannot be un-deleted. Everything else — workflow references, fixtures, docs — goes first, so the deletion removes something already unused.

- **Dimension 3.1**: no workflow, script, fixture or test resolves an Upstash path → Test `test_no_upstash_reference_survives_the_sweep`. NOT DONE.
- **Dimension 3.2**: the vault deletion runs behind the same approval gate credential rotation uses, and refuses without it → Test `test_the_vault_deletion_refuses_without_approval`. NOT DONE.

### §4: The cutover playbook

Dependencies: §1. A new `playbooks/operations/datastore_cutover/` carrying the runbook, its `probes.sh`, and self-tests. The existing `playbooks/operations/cutover/001_playbook.md` is corrected rather than extended: it describes the Zig-to-Rust binary swap (`M181_006`) and says the rollback is boring because the swap keeps "the same Postgres, the same Redis and the same ledger". That is true of a binary swap and false of a store cutover, and leaving it unqualified invites someone to roll back this one the same way.

- **Dimension 4.1**: every step is a command carrying a probe tag, and the probe runner refuses a rubric row that has none → Test `test_every_cutover_step_carries_a_probe`. NOT DONE.
- **Dimension 4.2**: the binary-swap playbook states which cutover it covers and that a store change is not it → Test `test_the_binary_swap_playbook_scopes_its_rollback_claim`. NOT DONE.

### §5: Architecture pages

Dependencies: §2. `data_flow.md`, `high_level.md`, `scaling.md` and `README.md` all name Upstash as the queue. `datastore_scaling.md` is canonical and already describes Dragonfly, so the four are reconciled TO it rather than rewritten independently.

- **Dimension 5.1**: no architecture page names Upstash, and each carries a current `verified` and `product_version` per DOC-31 → Test `test_architecture_pages_name_the_deployed_datastore`. NOT DONE.

### §6: The rest of the Zig-era prose

Dependencies: none. 687 comment lines in `rustd/**/*.rs` cite a `.zig` file that no longer exists; M192 cleared 55 of them and stopped there deliberately. Identifiers do not move: `redis::` driver paths, the `redis://` URL schemes and the `REDIS_*`-shaped names inside redis-rs are another crate's contract. The 18 `.zig` files that still exist are a live cross-language contract with the runner and every citation of those stays.

- **Dimension 6.1**: no Rust comment cites a `.zig` file absent from the tree, and every citation of a file still present survives → Test `test_no_rust_comment_cites_a_deleted_zig_file`. NOT DONE.

## Interfaces

No public endpoint, command, flag or wire shape changes. `DRAGONFLY_URL`, `DRAGONFLY_TLS_CA_CERT_FILE`, `DRAGONFLY_CONNECT_TIMEOUT_MS` and `DRAGONFLY_REQUEST_TIMEOUT_MS` are already the names M192 shipped; this spec changes the values they resolve to and the vault items behind them. The 1Password item names `upstash-dev` and `upstash-prod` are the only identifiers that move, and they are deployment configuration rather than a product surface.

## Failure Modes

| Failure | Behaviour | Negative test |
|---|---|---|
| A seed that is not a cluster | Boot refuses with the permanent class; no retry clears it | `test_a_non_cluster_seed_refuses_boot_with_its_own_class` |
| A cluster that evicts | Boot refuses rather than accepting silent data loss | `test_an_evicting_primary_refuses_boot` |
| Upstash deleted while a workflow still resolves it | The sweep fails before the deletion step is reachable | `test_no_upstash_reference_survives_the_sweep` |
| Vault deletion attempted without approval | Refused, and nothing is deleted | `test_the_vault_deletion_refuses_without_approval` |
| Dev cluster unreachable mid-rehearsal | The deploy fails closed and the previous release keeps serving | `test_a_failed_dev_deploy_leaves_the_previous_release_serving` |

## Invariants

- **Deletion is last.** No step deletes a vault item before both deployments are green. Enforced by the probe runner, which refuses the deletion step unless the dev and prod probes have recorded passes.
- **Prod follows a green dev rehearsal.** Enforced by the playbook's gate script, not by reading order.
- **Identifiers do not move in §6.** Enforced by a grep in the rubric: `redis::` paths and URL schemes must survive the prose sweep unchanged.
- **No schema change.** Enforced by the absence of `schema/` from Files Changed and by the SCHEMA GUARD.

## Metrics & Observability

The daemon's own datastore metrics keep the SHAPE M192 gave them. What this spec adds is deployment-time evidence rather than a runtime signal: each cutover step records its probe result into the playbook's coverage file, and the Grafana runner-offline threshold keeps deriving from the pinned lease clock (`afd_core::timing`, pinned to the Zig mirror by `cross_runtime_timing`). No product or operator signal changes shape, and no new event name is minted.

**Ten measurement names still say `redis`, and this spec owns them.** M192 renamed every Rust identifier whose name disagreed with its value and deliberately left the string literals, because a measurement name is read by Grafana queries that live outside this repository and a rename inside the repository alone silently breaks the dashboard that reads it. That is the M181 `LEASE_TTL_MS` failure exactly, and the reason it is a cutover task rather than a rename commit: the dashboard and the emitter have to move together, in a change window, with the old name still serving until the new one is proven.

The names, verified present as string literals in `rustd/` at the time of writing: `redis_connect_started`, `redis_connect_completed`, `redis_connect_failed`, `redis_connections_opened`, `redis_dedicated_connected`, `redis_subscribers`, `redis_bytes_total`, `redis_bytes_per_fleet`, `redis_calls_per_steer`, and `idle_redis_calls_per_poll`. Their Rust constants keep the same spelling as their values, so name and value agree and M192's own rename rule left them alone correctly.

The rename lands with the dashboard migration in one step, is graded by the same probe-and-record discipline as every other cutover step, and is reverted by the same rollback. Until it runs, the names are load-bearing and MUST NOT be changed by a repository-only sweep.

**One more name lives outside this repository entirely.** `docs/VERIFY_TIERS.md` is an `orly`-managed file: its integration row reads "Live Postgres and Redis via docker compose", and the text ships from the `@agentsfleet/orly` pack, not from here. M192 corrected the word on its branch and `orly doctor` refused the edit — a managed file changed after orly wrote it — so the word was put back rather than ship a red governance gate. The correction belongs in the pack, released, and picked up by `orly update`. Same shape as the measurement names above: the repository is not the only reader, so a repository-only edit is not the fix.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 0.1 | integration | `test_a_non_cluster_seed_refuses_boot_with_its_own_class` | A non-cluster endpoint refuses boot with the permanent class. |
| 1.1 | manual | `test_dev_cluster_satisfies_the_daemons_preflight` | Preflight passes against the deployed dev cluster; output recorded in Session Notes. |
| 1.2 | unit | `test_dev_workflow_resolves_no_upstash_path` | The dev workflow names no `upstash` vault path. |
| 1.3 | manual | `test_dev_deploy_boots_and_leases_against_the_cluster` | A dev deploy boots, serves, and leases; output recorded in Session Notes. |
| 2.1 | unit | `test_prod_workflow_resolves_no_upstash_path` | The release workflow names no `upstash` vault path. |
| 2.2 | integration | `test_a_fresh_pair_accepts_and_settles_without_prior_state` | An empty pair accepts work and settles it exactly once. |
| 3.1 | unit | `test_no_upstash_reference_survives_the_sweep` | Zero `upstash` matches outside archived specs. |
| 3.2 | unit | `test_the_vault_deletion_refuses_without_approval` | The deletion step exits non-zero with no approval. |
| 4.1 | unit | `test_every_cutover_step_carries_a_probe` | Each step has a probe tag; an untagged row fails the runner. |
| 4.2 | unit | `test_the_binary_swap_playbook_scopes_its_rollback_claim` | The binary-swap playbook names its scope and excludes a store change. |
| 5.1 | unit | `test_architecture_pages_name_the_deployed_datastore` | No page names Upstash; each carries `verified` and `product_version`. |
| 6.1 | unit | `test_no_rust_comment_cites_a_deleted_zig_file` | Every cited `.zig` path resolves in the tree. |
| 0.1 | integration | `test_an_evicting_primary_refuses_boot` | A primary with eviction enabled refuses boot. |
| 1.3 | manual | `test_a_failed_dev_deploy_leaves_the_previous_release_serving` | A failed deploy leaves the prior release serving; output recorded. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | No workflow resolves an Upstash secret (§1, §2, §3) | `git grep -rin upstash -- .github/ deploy/ playbooks/ scripts/` | 0 matches | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R3 | Driver identifiers survive the prose sweep (§6) | `git grep -c 'redis::' -- 'rustd/crates/**/*.rs'` | at least 15 matches | P0 | |
| R4 | No Rust comment cites a deleted `.zig` file (§6) | `bash playbooks/operations/datastore_cutover/probes.sh zig-citations` | exit 0 | P1 | |
| R5 | Every cutover step carries a probe (§4) | `bash playbooks/operations/datastore_cutover/probes_test.sh` | exit 0 | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Lint green | `make lint-all` | exit 0 | P0 | |
| S3 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S4 | Integration tier green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S8 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

## Dead Code Sweep

The `upstash-dev` and `upstash-prod` vault items, every workflow reference to them, and the CLI test fixtures naming them go with §3; the dead `.zig` citations go with §6. Each removal lists its references (RULE ORP) in the commit that removes it. The grep that proves it: `git grep -rin upstash` and the `.zig` citation probe.

## Out of Scope

- Dragonfly Cloud Swarm. `AGENTS.md` records it as the later move once its control plane is worth its bill; four self-hosted processes in one region are the target here.
- Any schema change, reverse migration, or import tool. The cutover is a fresh start by a decision already recorded.
- Renaming the `DRAGONFLY_*` knobs. M192 shipped them.
- Prod running on containers. Containers are the dev answer only.

## Product Clarity (authoring record)

1. **Successful user moment** — a deploy that boots. Today, after M192, it does not.
2. **Preserved user behaviour** — all of it. No endpoint, command or wire shape moves.
3. **Optimal-way check** — the optimal way is the one that keeps a rollback until the last possible step, which is why credential deletion is last and everything else is a revertible commit.
4. **Rebuild vs iterate** — iterate. The compose rig already bootstraps a four-node cluster; the dev container is that, hosted.
5. **What we build** — a dev cluster, two repointed workflows, a cutover playbook, and the doc reconciliation that follows them.
6. **What we do NOT build** — an import tool, a reverse migration, a Swarm trial, or a second queue service.
7. **Fit with existing features** — it completes M192, which is otherwise undeployable.
8. **Surface order** — dev, then prod, then deletion. Nothing user-visible changes order.
9. **Dashboard restraint** — no NEW dashboard. The runner-offline threshold keeps deriving from the pinned lease clock. The existing panels do change once, when the ten `redis_*` measurement names are renamed with their queries in the same cutover step; that is a migration of what exists, not a new surface.
10. **Confused-user next step** — an operator whose deploy refuses boot reads the permanent class and the preflight message, which already names which check failed.

## Decomposition & alternatives (patch vs refactor)

A patch is the right size. The alternative considered and rejected: repoint prod and dev together in one change, which removes the rehearsal that makes the prod step safe and buys nothing but one fewer deploy. Also rejected: keeping Upstash behind a compatibility shim so the daemon accepts a non-cluster seed — that reintroduces the standalone transport M192 deleted before it was written, and a second transport is two semantics forever.

## Discovery (consult log)

- **Scope set (Indy, 2026-09-16):** "When this PR gets merged, Indy must have sunset the redis from upstash and move to containers for development only that talks to api-dev.agentsfleet.net of dragonflydb running as a container" · "The necessary playbooks and so must be update to date" · "the architechture must reference dragonflydb" · "I assume the local is cutover fully to dragonflydb" · "the crate names, variables, imports can stay. Any comment that must have dragonflydb must point to dragonfly as opposed to redis". Taken as written: containers are dev-only, §6 moves prose and not identifiers.
- **Sequencing constraint (found, not proposed, 2026-09-16):** `deploy-dev-fly.yml` and `release.yml` both resolve an Upstash item, and `afd_dragonfly::error::ErrorKind::NotACluster` refuses boot on a non-cluster seed. The first deploy of the M192 daemon fails in both environments, so this spec is the unblocking of that branch rather than a follow-up to it.
- **Inherited from M192, not re-decided:** the fresh start (Discovery 2026-09-14 — PostgreSQL rebuilt, Dragonfly empty, every pre-cutover event discarded, no import tool) and the `DRAGONFLY_*` knob names (Discovery 2026-09-16).
- **Consults / Metrics review / Skill-chain outcomes / Deferrals:** recorded per Section.
