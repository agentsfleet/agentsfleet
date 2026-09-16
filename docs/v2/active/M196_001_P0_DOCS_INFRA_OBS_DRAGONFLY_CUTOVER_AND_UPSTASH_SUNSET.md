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

# M196_001: Stand Dragonfly up on Fly for dev, wire prod, and sunset Upstash

**Prototype:** v2.0.0
**Milestone:** M196
**Workstream:** 001
**Date:** Sep 17, 2026
**Status:** IN_PROGRESS
**Priority:** P0
**Categories:** DOCS, INFRA, OBS
**Batch:** B2
**Branch:** `feat/m196-dragonfly-fly-dev`
**Baseline revision:** `b1c9ef58f1ac5f87488904e9f3e77cdeb60770d6` (`main`, merge of #689)
**Test Baseline:** 2702 cargo + 8399 bun unit tests passing on this branch (`make test-unit-all`, exit 0). Integration counts are due before the Pull Request.
**Depends on:** M192_001, merged as #689 (`b1c9ef58f`). That branch made the transport cluster-only; this one gives it a cluster to talk to.
**Provenance:** Second revision. The first was a two-environment cutover; Indy narrowed it on 2026-09-16 to a dev acceptance with prod wired but ungraded, and struck the data migration that was never going to happen (Discovery). The sequencing constraint below was found by reading `deploy-dev-fly.yml` against `afd_dragonfly`'s boot refusal, not proposed.
**Canonical architecture:** `docs/architecture/datastore_scaling.md`.

---

## Overview

**Goal (testable):** a Dragonfly cluster runs as a Fly container in `iad`, `agentsfleetd-dev` boots against it over Fly's private network, a fleet starts, and `make acceptance-e2e` + `make cli-acceptance` pass on the merge-to-main deploy lane. Prod is wired the same way in the same change and proves itself the next time `release.yml` runs.
**Problem:** the daemon on `main` after M192 cannot boot against either deployment's datastore. Both point at Upstash, Upstash is not a cluster, and the daemon refuses a non-cluster seed at startup. Nobody can deploy.
**Solution summary:** stand a four-process Dragonfly cluster as a Fly app beside `agentsfleetd-dev`, reachable on `dragonfly-dev.internal` over 6PN, mirroring the `otelcol-dev` shape the deploy graph already knows how to bring up. Repoint `deploy-dev-fly.yml` at it and let the merge-to-main lane's own acceptance gates grade the result. Build the prod app and repoint `release.yml` in the same change, ungraded here. The cutover playbook, the architecture pages and the remaining Zig-era prose follow.

**Nothing is migrated.** No Upstash key, queue entry or stream is copied anywhere. Dev starts empty and prod starts empty; the Upstash instances keep running, untouched, until their vault items are deleted in §3. "Cutover" here means repointing a connection string, never moving data.

## PR Intent & comprehension handshake

- **PR title (eventual):** run Dragonfly on Fly for dev, wire prod, and retire Upstash
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
| `deploy/fly/dragonfly-dev/fly.toml` | CREATE | The dev cluster's Fly app: `iad`, `performance-2x`/4GB, a `/data` volume, no public service, health check on the cluster's own readiness. |
| `deploy/fly/dragonfly-dev/Dockerfile` | CREATE | The pinned Dragonfly image with `scripts/dragonfly-cluster.sh` as its entrypoint, and `DRAGONFLY_ANNOUNCE_IP` resolved from `FLY_PRIVATE_IP` at boot. |
| `deploy/fly/dragonfly-prod/fly.toml` | CREATE | The same app in the prod organisation. Wired, not rehearsed here. |
| `deploy/fly/dragonfly-prod/Dockerfile` | CREATE | The same image and entrypoint. |
| `scripts/dragonfly-cluster.sh` | EDIT | `HOST=127.0.0.1` (line 59) becomes `${DRAGONFLY_ANNOUNCE_IP:-127.0.0.1}`. See §1 — a node announcing loopback is unreachable from a different Fly app. |
| `scripts/dragonfly_cluster_test.sh` | EDIT | Cover the new default and the override. |
| `.github/workflows/deploy-dev-fly.yml` | EDIT | Bring `dragonfly-dev` up before the daemon, and resolve `DRAGONFLY_URL` from the cluster's vault item instead of `upstash-dev`. |
| `.github/workflows/release.yml` | EDIT | The same for prod, in this change, graded by its own next run. |
| `playbooks/operations/datastore_cutover/001_playbook.md` | CREATE | The runbook: stand up, verify, repoint, drain, roll back. |
| `playbooks/operations/datastore_cutover/probes.sh` | CREATE | The probes each step is tagged with, refusing a rubric row that has none. |
| `playbooks/operations/datastore_cutover/probes_test.sh` | CREATE | Self-tests, discovered by `SCRIPT_SELF_TESTS` and run by `make lint-all`. |
| `playbooks/operations/cutover/001_playbook.md` | EDIT | Its "the same Postgres, the same Redis and the same ledger" rollback premise is false once the store moves; it is scoped to the binary swap it describes. |
| `docs/architecture/data_flow.md` | EDIT | Names Upstash as the queue. |
| `docs/architecture/high_level.md` | EDIT | Names Upstash in the deployment picture. |
| `docs/architecture/scaling.md` | EDIT | Names Upstash in the scaling story. |
| `docs/architecture/README.md` | EDIT | Names Upstash in the index. |
| `rustd/crates/**/*.rs` | EDIT | Comment prose only: the dead `.zig` citations M192 did not reach. No identifier moves. |
| `src/runner/engine/runtime/policy_http_request.zig` | EDIT | Restores `pub` on `tool_description`. Folded in rather than opened as a second Pull Request — see §7; the runner build is red on `main` and every deploy lane begins with it. |

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

Execution order is §1 → §2 → §3. §1 is the graded one: dev must boot and pass its acceptance gates. §2 wires prod in the same change without grading it, and §3 sweeps every Upstash reference out of the tree while leaving the vault items alive. §4–§6 may land in any order.

### §0: The constraint this spec exists to clear

Dependencies: none. M192 made the transport cluster-only. `afd_dragonfly::error::ErrorKind::NotACluster` refuses boot when `INFO cluster` does not answer `cluster_enabled:1`, and the refusal is classified permanent so no retry clears it. Both deployments resolve their seed from an Upstash item (`deploy-dev-fly.yml`, `release.yml`), and Upstash is not a cluster. **The first deploy of the M192 daemon therefore fails boot in dev and in prod.** This is not a migration that can be scheduled at leisure; it is the unblocking of a branch that is otherwise unshippable.

- **Dimension 0.1**: the boot refusal is reproduced against a non-cluster endpoint before anything is repointed, so the failure this spec prevents is observed rather than assumed → Test `test_a_non_cluster_seed_refuses_boot_with_its_own_class`, with `test_an_evicting_primary_refuses_boot` for the other permanent class. NOT DONE.

### §1: Dev on Fly, beside the daemon

Dependencies: §0. A four-process Dragonfly cluster runs as its own Fly app, `dragonfly-dev`, in `iad` — the same region and organisation as `agentsfleetd-dev`. Containers are the dev-only answer by decision (Discovery), not a pattern prod inherits in spirit even though §2 builds the same app. The daemon reaches it at `dragonfly-dev.internal:7001` over Fly's private network, which is in-region and WireGuard-encrypted, so the seed is plaintext and `DRAGONFLY_TLS_CA_CERT_FILE` stays unset. That is a deliberate saving, not an omission: a TLS handshake against an RSA-2048 leaf costs ~230 ms, a lane opens hundreds of connections, and `scripts/dragonfly-cluster.sh` already records what that does to a connect budget.

**One machine, four processes, not four machines.** `scripts/dragonfly-cluster.sh` runs two primaries and two replicas in one container and bootstraps `DFLYCLUSTER CONFIG` on every start, because Dragonfly forgets its layout on restart. Four Fly machines would each need to announce their own 6PN address, which Fly reassigns, so every restart would need the layout pushed again from outside. One machine keeps the rig the integration lane already proves.

**The announce address is the one real code change.** Line 59 of that script pins `HOST=127.0.0.1`, which is correct for compose — the daemon service shares the cluster's network namespace (`docker-compose.yml:146`) — and wrong on Fly, where the daemon is a different app. A `MOVED` reply naming `127.0.0.1` would send the client to itself. `HOST` becomes `${DRAGONFLY_ANNOUNCE_IP:-127.0.0.1}`; the Fly Dockerfile's entrypoint exports it from `FLY_PRIVATE_IP`. The default preserves every existing caller unchanged.

The dev workflow stops resolving `op://ZMB_CD_DEV/upstash-dev/api-url` and resolves `op://ZMB_CD_DEV/dragonfly-dev/api-url`. The vault and the Fly token item are confirmed present (`op item list --vault ZMB_CD_DEV` shows `fly-api-token` and `upstash-dev`). The knob names are already what M192 renamed them to; this section changes what they point at, never what they are called. The bring-up hop goes ahead of the daemon deploy in `deploy-dev-fly.yml`, mirroring the collector hop already there: `ensure_fly_app.sh --create-only`, stage secrets, then `ensure_fly_app.sh` with a desired count, which refuses to report success it cannot prove from a passing health check.

- **Dimension 1.1**: the dev cluster answers `CLUSTER SHARDS` with online primaries and replicas, and the daemon's own preflight passes against it rather than against a local rig → Test `test_dev_cluster_satisfies_the_daemons_preflight`. NOT DONE.
- **Dimension 1.2**: the dev workflow resolves no Upstash path and the deployed process reads its seed from `DRAGONFLY_URL` → Test `test_dev_workflow_resolves_no_upstash_path`. NOT DONE.
- **Dimension 1.3**: a node started with `DRAGONFLY_ANNOUNCE_IP` set announces that address, and one started without it still announces `127.0.0.1` → Test `test_a_node_announces_its_configured_address`. NOT DONE.
- **Dimension 1.4**: the merge-to-main deploy lane goes green end to end — build, Fly, metal, verify, then `qa`, `acceptance-e2e` and `acceptance-cli` — with the daemon leasing against the cluster → Test `test_the_dev_deploy_lane_passes_its_acceptance_gates`, with `test_a_failed_dev_deploy_leaves_the_previous_release_serving` for the failed-deploy arm. NOT DONE.

### §2: Prod, wired but not rehearsed

Dependencies: §1 written; NOT §1 green. `deploy/fly/dragonfly-prod/` and the `release.yml` repoint land in this same change, because a prod deployment left resolving an Upstash seed is a deployment that cannot boot the M192 daemon at all, and leaving that to a later milestone leaves prod unshippable rather than merely unrehearsed.

What this section does NOT do is grade itself. There is no prod rehearsal, no change window and no manual cutover run inside this spec; prod proves itself the next time `release.yml` runs, and the acceptance surface for this milestone is dev. `agentsfleetd-prod` and `otelcol-prod` have never deployed (`flyctl apps list` reports both `pending`), so there is no live prod service this can take down.

PostgreSQL is untouched here. The fresh-start decision M192 recorded stands, and so does its consequence: nothing is imported, so there is no import tool, no receipt, and no boot gate for one — a gate whose only fresh-boot exit is a genesis receipt is machinery for a migration that will not happen.

- **Dimension 2.1**: the prod workflow resolves no Upstash path and names the prod cluster's vault item → Test `test_prod_workflow_resolves_no_upstash_path`. NOT DONE.
- **Dimension 2.2**: `deploy/fly/dragonfly-prod/` declares the prod organisation's app, its region and its volume, and differs from the dev app only in those values → Test `test_the_prod_cluster_app_matches_the_dev_shape`. NOT DONE.

### §3: Sunset Upstash

Dependencies: §1 green. 226 references across more than twenty files. Workflow references, fixtures, scripts and docs go in this change.

**The vault items are NOT deleted in this milestone.** `upstash-dev` and `upstash-prod` stay in `ZMB_CD_DEV` and `ZMB_CD_PROD` until prod has booted once against its own cluster, because deleting a credential is the one step with no rollback: a repointed workflow reverts in a commit, and a deleted secret cannot be un-deleted. The deletion step is written into the playbook, behind the same approval gate credential rotation uses, and is run by hand later — not by this Pull Request.

- **Dimension 3.1**: no workflow, script, fixture or test resolves an Upstash path → Test `test_no_upstash_reference_survives_the_sweep`. NOT DONE.
- **Dimension 3.2**: the playbook's vault-deletion step refuses without the approval gate, and refuses while any deployment has yet to record a green boot against its cluster → Test `test_the_vault_deletion_refuses_without_approval`. NOT DONE.

### §4: The cutover playbook

Dependencies: §1. A new `playbooks/operations/datastore_cutover/` carrying the runbook, its `probes.sh`, and self-tests. The existing `playbooks/operations/cutover/001_playbook.md` is corrected rather than extended: it describes the Zig-to-Rust binary swap (`M181_006`) and says the rollback is boring because the swap keeps "the same Postgres, the same Redis and the same ledger". That is true of a binary swap and false of a store cutover, and leaving it unqualified invites someone to roll back this one the same way.

- **Dimension 4.1**: every step is a command carrying a probe tag, and the probe runner refuses a rubric row that has none → Test `test_every_cutover_step_carries_a_probe`. NOT DONE.
- **Dimension 4.2**: the binary-swap playbook states which cutover it covers and that a store change is not it → Test `test_the_binary_swap_playbook_scopes_its_rollback_claim`. NOT DONE.

### §5: Architecture pages

Dependencies: §2. `data_flow.md`, `high_level.md`, `scaling.md` and `README.md` all name Upstash as the queue. `datastore_scaling.md` is canonical and already describes Dragonfly, so the four are reconciled TO it rather than rewritten independently.

- **Dimension 5.1**: no architecture page names Upstash **as the datastore**; QStash references survive untouched, because the cron trigger is a different Upstash product and is not being retired → Test `test_architecture_pages_name_the_deployed_datastore`. NOT DONE.
- **Dimension 5.2**: the ten `redis_*` measurement names and their Rust constants read `dragonfly_*`, and no `redis::` path, `redis://` scheme or `REDIS_*` environment name moves with them → Test `test_measurement_names_match_the_deployed_datastore`. NOT DONE.

### §6: The rest of the Zig-era prose

Dependencies: none. 687 comment lines in `rustd/**/*.rs` cite a `.zig` file that no longer exists; M192 cleared 55 of them and stopped there deliberately. Identifiers do not move: `redis::` driver paths, the `redis://` URL schemes and the `REDIS_*`-shaped names inside redis-rs are another crate's contract. The 18 `.zig` files that still exist are a live cross-language contract with the runner and every citation of those stays.

- **Dimension 6.1**: no Rust comment cites a `.zig` file absent from the tree, and every citation of a file still present survives → Test `test_no_rust_comment_cites_a_deleted_zig_file`. NOT DONE.

### §7: Unblock the runner build

Dependencies: none, and everything depends on it. `1bbce85c0 chore(zig): narrow 71 pub symbols nothing outside their file reads` removed `pub` from `tool_description` in `src/runner/engine/runtime/policy_http_request.zig:66`. The symbol is read by `tools_mod.ToolVTable(@This())` through comptime reflection from inside the `nullclaw` package (`zig-pkg/nullclaw-2026.5.29-*/src/tools/root.zig:270`), so neither a consumer grep nor `zlint`'s `unused-decls` can see the use. `zig build --build-file build_runner.zig -Dtarget=x86_64-linux-musl` fails, which takes `compile-runner` down, which takes the whole dev deploy lane down before it reaches anything this milestone changes.

Folded here because the PR budget is one per milestone and §1's acceptance surface IS the deploy lane this fix unblocks: without it R9 cannot go green, so it is not an adjacent cleanup but the first step of the graded path.

- **Dimension 7.1**: the runner compiles for both Linux targets, and the three `Tool` interface symbols stay `pub` because their reader is comptime, not a grep → Test `test_the_tool_interface_symbols_stay_public`. NOT DONE.

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

- **No data moves.** Nothing in this milestone reads an Upstash key and writes it into Dragonfly. Both clusters start empty. Enforced by R10 and by the absence of any import step from the playbook.
- **Deletion is not in this Pull Request.** `upstash-dev` and `upstash-prod` survive it. The playbook carries the deletion step behind the credential-rotation approval gate, and the probe runner refuses it until both deployments have recorded a green boot.
- **Dev is the graded environment.** Prod is wired in the same change and proves itself on its own next release run; no rubric row grades a prod deploy.
- **Identifiers do not move in §6.** Enforced by a grep in the rubric: `redis::` paths and URL schemes must survive the prose sweep unchanged.
- **No schema change.** Enforced by the absence of `schema/` from Files Changed and by the SCHEMA GUARD.

## Metrics & Observability

The daemon's own datastore metrics keep the SHAPE M192 gave them. What this spec adds is deployment-time evidence rather than a runtime signal: each cutover step records its probe result into the playbook's coverage file, and the Grafana runner-offline threshold keeps deriving from the pinned lease clock (`afd_core::timing`, pinned to the Zig mirror by `cross_runtime_timing`). No product or operator signal changes shape, and no new event name is minted.

**Ten measurement names said `redis`, and this spec renames them.** M192 renamed every Rust identifier whose name disagreed with its value and deliberately left the string literals, because a measurement name is read by Grafana queries outside this repository and a repository-only rename breaks the dashboard silently — the M181 `LEASE_TTL_MS` failure exactly.

**That reasoning assumed a live dashboard, and there is not one.** `flyctl apps list` reports `agentsfleetd-prod` and `otelcol-prod` as never deployed, so no production series exists to orphan. Indy called it on 2026-09-16 (Discovery): rename now, repoint the dashboard by hand, and take the breakage while breakage is free. Waiting for a change window to protect a dashboard nobody is reading is ceremony.

The names, each renamed with the Rust constant that carries it so name and value still agree: `redis_connect_started`, `redis_connect_completed`, `redis_connect_failed`, `redis_connections_opened`, `redis_dedicated_connected`, `redis_subscribers`, `redis_bytes_total`, `redis_bytes_per_fleet`, `redis_calls_per_steer` and `idle_redis_calls_per_poll` all take a `dragonfly_` prefix (`idle_dragonfly_calls_per_poll` for the last).

What does NOT move is the `redis::` driver paths, the `redis://` URL schemes or the `REDIS_*` environment variable names. Those are another crate's surface and a deployment contract respectively, and §6's prose sweep leaves them alone.

**One more name lived outside this repository entirely, and is already closed.** `docs/VERIFY_TIERS.md` is an `orly`-managed file: its integration row read "Live Postgres and Redis via docker compose", and the text ships from the `@agentsfleet/orly` pack, not from here. M192 corrected the word on its branch and `orly doctor` refused the edit — a managed file changed after orly wrote it — so the word was put back rather than ship a red governance gate, and the correction went to the pack (`agentsfleet/orly#43`) with the release that carries it (`#44`, `0.10.10`). M192 then bumped its pin and ran `orly update`, which restored the right word with `orly doctor` green. Recorded here because it is the same shape as the measurement names above — the repository is not the only reader, so a repository-only edit is not the fix — and because it is the one instance of that shape this milestone does NOT inherit.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 0.1 | integration | `test_a_non_cluster_seed_refuses_boot_with_its_own_class` | A non-cluster endpoint refuses boot with the permanent class. |
| 1.1 | manual | `test_dev_cluster_satisfies_the_daemons_preflight` | Preflight passes against the deployed dev cluster; output recorded in Session Notes. |
| 1.2 | unit | `test_dev_workflow_resolves_no_upstash_path` | The dev workflow names no `upstash` vault path. |
| 1.3 | unit | `test_a_node_announces_its_configured_address` | `DRAGONFLY_ANNOUNCE_IP` is announced when set; `127.0.0.1` when unset. |
| 1.4 | acceptance | `test_the_dev_deploy_lane_passes_its_acceptance_gates` | The merge-to-main lane's `qa`, `acceptance-e2e` and `acceptance-cli` jobs all pass. |
| 2.1 | unit | `test_prod_workflow_resolves_no_upstash_path` | The release workflow names no `upstash` vault path. |
| 2.2 | unit | `test_the_prod_cluster_app_matches_the_dev_shape` | The prod app declares its own org, region and volume and differs in nothing else. |
| 3.1 | unit | `test_no_upstash_reference_survives_the_sweep` | Zero `upstash` matches outside archived specs. |
| 3.2 | unit | `test_the_vault_deletion_refuses_without_approval` | The deletion step exits non-zero with no approval. |
| 4.1 | unit | `test_every_cutover_step_carries_a_probe` | Each step has a probe tag; an untagged row fails the runner. |
| 4.2 | unit | `test_the_binary_swap_playbook_scopes_its_rollback_claim` | The binary-swap playbook names its scope and excludes a store change. |
| 5.1 | unit | `test_architecture_pages_name_the_deployed_datastore` | No page names Upstash as the datastore; QStash references survive. |
| 5.2 | unit | `test_measurement_names_match_the_deployed_datastore` | Ten measurements read `dragonfly_*`; driver paths and env names unchanged. |
| 6.1 | unit | `test_no_rust_comment_cites_a_deleted_zig_file` | Every cited `.zig` path resolves in the tree. |
| 7.1 | unit | `test_the_tool_interface_symbols_stay_public` | `tool_name`, `tool_description` and `tool_params` are all `pub`. |
| 0.1 | integration | `test_an_evicting_primary_refuses_boot` | A primary with eviction enabled refuses boot. |
| 1.4 | manual | `test_a_failed_dev_deploy_leaves_the_previous_release_serving` | A failed deploy leaves the prior release serving; output recorded. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | No workflow resolves an Upstash secret (§1, §2, §3) | `git grep -rin upstash -- .github/ deploy/ playbooks/ scripts/` | 0 matches | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R3 | Driver identifiers survive the prose sweep (§6) | `git grep -c 'redis::' -- 'rustd/crates/**/*.rs'` | at least 15 matches | P0 | |
| R4 | No Rust comment cites a deleted `.zig` file (§6) | `bash playbooks/operations/datastore_cutover/probes.sh zig-citations` | exit 0 | P1 | |
| R5 | Every cutover step carries a probe (§4) | `bash playbooks/operations/datastore_cutover/probes_test.sh` | exit 0 | P0 | |
| R6 | The dev daemon boots against the cluster and a fleet starts (§1) | `flyctl logs --app agentsfleetd-dev \| grep -i 'preflight'` after the merge-to-main lane runs | preflight passes, no `NotACluster` | P0 | |
| R7 | Dashboard acceptance green against dev (§1) | `make acceptance-e2e` | exit 0 | P0 | |
| R8 | CLI acceptance green against dev (§1) | `make cli-acceptance` | exit 0 | P0 | |
| R9 | The merge-to-main deploy lane is green (§1) | `gh run list --workflow deploy-dev.yml --branch main --limit 1` | conclusion `success` | P0 | |
| R10 | No Upstash data was copied anywhere (§0) | `git grep -rniE 'migrat|import|dump|restore' -- playbooks/operations/datastore_cutover/` | no step that moves data | P0 | |
| R11 | The cluster announce default is unchanged for compose (§1) | `bash scripts/dragonfly_cluster_test.sh` | exit 0 | P0 | |
| R12 | The runner cross-compiles for both Linux targets (§7) | `zig build --build-file build_runner.zig -Doptimize=ReleaseSafe -Dtarget=x86_64-linux-musl` then `-Dtarget=aarch64-linux-musl` | exit 0 both | P0 | |
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
- A rehearsed prod cutover, a prod change window, or any rubric row that grades a prod deploy. Prod is wired here and tested on merge.
- Deleting the Upstash vault items. Written into the playbook, run by hand later.
- Repointing the Grafana dashboard queries at the renamed measurements. The rename lands here; the dashboard is Indy's by hand, and breakage is free until production exists.

## Product Clarity (authoring record)

1. **Successful user moment** — a deploy that boots. Today, after M192, it does not.
2. **Preserved user behaviour** — all of it. No endpoint, command or wire shape moves.
3. **Optimal-way check** — the optimal way is the one that keeps a rollback until the last possible step, which is why credential deletion is last and everything else is a revertible commit.
4. **Rebuild vs iterate** — iterate. The compose rig already bootstraps a four-node cluster; the dev container is that, hosted.
5. **What we build** — a dev cluster, two repointed workflows, a cutover playbook, and the doc reconciliation that follows them.
6. **What we do NOT build** — an import tool, a reverse migration, a Swarm trial, or a second queue service.
7. **Fit with existing features** — it completes M192, which is otherwise undeployable.
8. **Surface order** — dev, then prod, then deletion. Nothing user-visible changes order.
9. **Dashboard restraint** — no NEW dashboard. The runner-offline threshold keeps deriving from the pinned lease clock. The existing panels break once, when the ten renamed measurements stop matching their queries, and are repointed by hand; that is a migration of what exists, not a new surface.
10. **Confused-user next step** — an operator whose deploy refuses boot reads the permanent class and the preflight message, which already names which check failed.

## Decomposition & alternatives (patch vs refactor)

A patch is the right size. The alternative considered and rejected: repoint prod and dev together in one change, which removes the rehearsal that makes the prod step safe and buys nothing but one fewer deploy. Also rejected: keeping Upstash behind a compatibility shim so the daemon accepts a non-cluster seed — that reintroduces the standalone transport M192 deleted before it was written, and a second transport is two semantics forever.

## Discovery (consult log)

- **Scope set (Indy, 2026-09-16):** "When this PR gets merged, Indy must have sunset the redis from upstash and move to containers for development only that talks to api-dev.agentsfleet.net of dragonflydb running as a container" · "The necessary playbooks and so must be update to date" · "the architechture must reference dragonflydb" · "I assume the local is cutover fully to dragonflydb" · "the crate names, variables, imports can stay. Any comment that must have dragonflydb must point to dragonfly as opposed to redis". Taken as written: containers are dev-only, §6 moves prose and not identifiers.
- **Sequencing constraint (found, not proposed, 2026-09-16):** `deploy-dev-fly.yml` and `release.yml` both resolve an Upstash item, and `afd_dragonfly::error::ErrorKind::NotACluster` refuses boot on a non-cluster seed. The first deploy of the M192 daemon fails in both environments, so this spec is the unblocking of that branch rather than a follow-up to it.
- **Inherited from M192, not re-decided:** the fresh start (Discovery 2026-09-14 — PostgreSQL rebuilt, Dragonfly empty, every pre-cutover event discarded, no import tool) and the `DRAGONFLY_*` knob names (Discovery 2026-09-16).
- **Scope narrowed (Indy, 2026-09-16, second revision):** "the Dragonfly swarm is something we avoid doing now" · "The focus is on development" · "when we say cutover there is no need to migrate any upstash redis data to dragonflydb in dev fly.io" · "the acceptance of the M196_001 is to deploy dragonflydb in fly.io as container and have that connect with agentsfleetd-rs and ensure the fleet can be started (acceptance-e2e, acceptance-cli passes)" · "The playbooks must be updated as well" · "The CI upon merge to master must pass". Then, on prod: "i think wire production we will test upon merge." Taken as written — §2 wires prod and grades nothing, §3 stops short of deleting the vault items, and the rubric gains R6-R9 because the first revision's acceptance surface did not mention either acceptance lane.
- **Container sizing (Indy, 2026-09-16):** "also use a performant container for dragonfly" and "since its run in the same region, the agentsfleetd-rs in fly.io can connect to it via private, so there will be no or less latency". Hence `performance-2x`/4GB against `otelcol-dev`'s `shared-cpu-1x`/512mb, and a plaintext 6PN seed rather than TLS.
- **Vault confirmed (Indy, 2026-09-16):** `ZMB_CD_DEV` is the dev vault and `fly-api-token` the Fly credential item. `flyctl auth whoami` returns `nkishore@megam.io`; `flyctl apps list` shows `agentsfleetd-prod` and `otelcol-prod` still `pending`, which is why §2 can wire prod without risking a live service.
- **One Pull Request (Indy, 2026-09-16):** "I only need 1 PR / not a gazilion PRs". The runner build fix (§7) folds in rather than opening its own.
- **Measurement rename, reversed (Indy, 2026-09-16):** "we agreed to rename ... redis_* to dragonfly_*. So do it. Indy will deal with grafana dashboard breakage ... the breakage of dashboard is fine since we are not in production yet." The change-window argument protected a dashboard reading a production series that does not exist. Renamed in §5.
- **QStash stays (Indy, 2026-09-16):** "yes QStash will stay, only Upstash Redis is moved to dragonfly in fly.io for dev". Dimension 5.1's first wording would have swept the cron trigger out of the architecture pages with the datastore; it is scoped to datastore references now.
- **scaling.md figures (Indy, 2026-09-16):** rewrite the math for self-hosted AND mark what is not re-measured. Both done: the plan cap, the per-request bill and the TLS dial cost are gone and say so; the latency and volume tables carry an explicit unverified note naming what they were measured against.
- **Consults / Metrics review / Skill-chain outcomes / Deferrals:** recorded per Section.
