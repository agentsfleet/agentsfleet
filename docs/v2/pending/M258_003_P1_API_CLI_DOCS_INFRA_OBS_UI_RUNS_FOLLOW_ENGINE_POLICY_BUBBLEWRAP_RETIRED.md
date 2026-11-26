<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M258_003: A tenant's runs land on the engine its policy names, Fly.io Sprites included, and bubblewrap is gone

**Prototype:** v2.0.0

**Milestone:** M258

**Workstream:** 003

**Date:** Nov 26, 2026

**Status:** PENDING

**Priority:** P1, a customer cannot choose where its tool calls run, full hosts make work wait, and a shared-kernel engine still ships

**Categories:** API, CLI, DOCS, INFRA, OBS, UI

**Batch:** B3, after M258_001 and M258_002 merge

**Depends on:** M258_001, for its seam, `afr_engine` crate, conformance suite, Firecracker engine and reported memory bound. M258_002, for checkpoint and snapshot on the chunk map, which a run moving between our runners and the snapshot trials rely on

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 26, 2026) from Indy's rulings of Nov 26, 2026 and the research in `~/.local/state/agentsfleet-m254/` (`vm-engines/sprites-cloud.md`, `vm-engines/design.md`, `harness/engine-policy-placement.md`, `harness/skeptic-review.md`, `space/trait-comparison.md`). Each repository citation was re-read at `679002666`.

**Canonical architecture:** `docs/architecture/runner_fleet.md` §Placement and §Limits, `runner_execution.md` §Sandbox engines and `billing.md`; this spec rewrites each for the engine policy

---

## Overview

**Goal (testable):** A turn reaches only runners its tenant's policy permits, bursts to Sprites only when no local worker is free, and nothing names bubblewrap.

**The compute layer.** M258 is the compute layer, ruled the first milestone so bubblewrap is sunset first. M258_001 is the engine seam, the suite, our Firecracker and the dev host. M258_002 rebuilds checkpoint and snapshot on a live chunk map, and this spec is M258_003; the packing is O1.

**Terms.**

- A Sprite is a Fly.io microVM, a small virtual machine (VM), driven over HTTP and WebSocket at `api.sprites.dev`.
- A runner's **substrate** is `local`, our Firecracker on the runner's own host, or `sprites`. The **engine policy** is `local_only`, `local_then_sprites` or `sprites_only`.
- Rust paths that start with a crate name are under `rustd/crates/<layer>/`. Line references are at `679002666`; "working tree" marks a file another branch is editing.

**Problem.** Read from source.

- **Nothing places a run by engine.** The scan has no engine predicate (`afd_fleet/src/run/sql/run/select_ledger_candidates.sql:10-33`, working tree), and no tenant setting exists. A run row names no engine (`afd_db/schema/20261108123200_thread_runs.sql:18-20`), and labels "carry no security meaning" (`docs/architecture/runner_fleet.md:88`).
- **One rate.** Every run-time second costs `RUN_NANOS_PER_SEC` (`afd_billing/src/nanos.rs:25`), whatever ran it.
- **No engine without KVM.** A release runner off Linux refuses at boot (`runner/main/src/host.rs:102-117`). CI builds the runner for Linux only (`.github/workflows/build-candidate.yml:28-33`).
- **bubblewrap still ships.** M258_001 leaves it compiled beside Firecracker. The three tiers still mean bubblewrap (`runner_fleet.md:120`), and the report still carries five Linux booleans.

**Solution summary.** A Sprites engine joins behind M258_001's seam and passes the same suite. The daemon reads each tenant's engine policy, or a platform default, and places turns by substrate in the three statements that already enforce ownership. Burst is a placement rule onto Sprites runners, never a fallback inside a runner.

Each run records its engine and root and is billed at its engine's rate. Production switches to Firecracker, then bubblewrap, its probe fields and the Linux-named tiers are deleted.

**Design flow:**

```text
one KVM host                                          the daemon's placement, per turn
 ├─ agentsfleet-runner (unit 1) --engine firecracker      policy(tenant) × substrate(runner tier)
 │     tier firecracker · sandboxes_max 13                 local_only          local: yes   sprites: no
 └─ agentsfleet-runner (unit 2) --engine sprites           local_then_sprites  local: yes   sprites: holder, or no
       tier sprites · worker_count = burst concurrency                                       live local worker free
       ─► api.sprites.dev: create, policy, executor        sprites_only        local: no    sprites: yes
tenant ─► PUT /v1/tenants/me/engine ─► runner.tenant_placements (absent = [runner.placement] default)
issue  ─► thread.runs.engine, thread.runs.root ─► usage line priced at the engine's rate
```

One engine per runner process, and no fallback inside a runner, stay true (`runner/main/src/engine.rs:17`; `afr_sandbox/src/boot/mod.rs:70`). A host with KVM bursts by running a second runner process whose engine is Sprites.

**What is unverified.**

- Nothing has been created at Fly. Every Sprites fact is from Fly's pages read on Nov 26, 2026, and timings are vendor-stated.
- §1 settles five things: what an enforced `allow *` blocks, policy rollback, a restricted token's reach, Fly's tools, and Rust generation from OpenAPI 3.1.
- Fly's Terms of Service (ToS) §1.4(d) bars use "for the benefit of a third party" (https://fly.io/legal/terms-of-service.md); O2 is open. Whether production hosts have `/dev/kvm` is unchecked (A2).

**What changes for a fleet.** Each line is a behaviour Indy accepts by approving this spec.

1. Under `local_then_sprites`, a run may start on a Sprite while our hosts are full. It starts fresh, on Fly's root and its tool versions, not our toolbox.
2. Under `local_then_sprites`, work goes to Sprites when no local runner is live. Today it waits in Postgres (`runner_fleet.md:229`, working tree).
3. A task run whose checkpoint names our toolbox never runs on a Sprite, because its restore would be refused.
4. On a Sprite a fleet can install packages with `apt`, which our toolbox purged (`scripts/toolbox/manifest.txt:27`). D14 decides whether that stays.
5. A Sprites run is billed at its own rate (D10).

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(fleet): an engine policy places runs on Firecracker or Fly.io Sprites; bubblewrap retired
- **Intent (one sentence):** A customer picks where its fleets' tool calls run, our full hosts overflow to Sprites, and the shared-kernel engine is gone.
- **Authoring handshake:** Indy wants Sprites "as either burst or sole one. So customers can pick", with our platform on "our hardware + burst as sprites". The engine choice is a configurable policy, bubblewrap retires, our toolbox stays on our hosts, and a host with KVM can burst. ASSUMPTIONS I'M MAKING:
  1. O1: the compute layer packs into three Workstreams. This one is Sprites, their snapshots, the policy, burst, production and bubblewrap's deletion.
  2. O2: all three values are built. Production offers tenants no Sprites value and sets no Sprites default until Fly answers §1.4(d) in writing, because burst runs tenant code on Sprites too.
  3. O3: overflow bursts to Sprites, and a Firecracker runner's free worker counts against its reported memory bound.
  4. O4 is M258_001's. O5: the compute layer is M258, and web plus harness is M259.
  5. The policy is per tenant, as model selection is (`afd_db/schema/20261108121500_model_tenant_models.sql:3-5`).
  6. Nothing is backward compatible. The wire, tiers and schema change in place, with one runner protocol `VERSION` move.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_fleet/src/run/sql/run/select_ledger_candidates.sql` — the scan whose ownership predicate the engine predicate sits beside.
2. `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/provider/mod.rs` — the tenant setting to mirror: a platform default, a tenant's row, and `DELETE` to reset.
3. `docs/v2/pending/M251_001_P1_API_CLI_DOCS_INFRA_OBS_UI_TENANT_RUNNERS_CONNECT_ROTATE_PROXY.md` — its §4 plans `runner.tenant_placements`, the row this policy shares.
4. https://api.sprites.dev/openapi.json — the API, version 0.1.70, the client is generated from.
5. https://docs.fly.io/sprites/concepts/networking.md — the policy semantics, among them "`{ "rules": [] }` means no enforcement".

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M258_003_P1_API_CLI_DOCS_INFRA_OBS_UI_RUNS_FOLLOW_ENGINE_POLICY_BUBBLEWRAP_RETIRED.md` | CREATE | this spec; moves to `active/`, then `done/` |
| `docs/v2/reviews/m258-sprites-spike.md` | CREATE | §1's record; helpers stay outside the repository |
| `rustd/crates/runner/afr_engine_sprites/**` | CREATE | §2; a lower-layer `af_` crate instead if D9 has the daemon relay Sprite traffic |
| `rustd/crates/runner/main/**` | EDIT | `EngineChoice::Sprites`; a macOS build hosting it; `Bubblewrap` deleted; the kernel lane defaults to Firecracker |
| `rustd/crates/runner/afr_engine_bubblewrap/**` | DELETE | §8 |
| `rustd/crates/runner/{afr_engine,afr_supervisor,afr_cgroup,afr_guest}/**`, `afr_tools/src/generic/env/mod.rs` | EDIT | §8: the bubblewrap-only paths and names go |
| `rustd/crates/protocol/af_wire/src/runner/{mod.rs,capability.rs}`, `af_wire/src/report/mod.rs`, `af_wire/src/protocol/mod.rs` | EDIT | tiers `firecracker`, `sprites`, `dev_none`; the report's `root`; the Linux booleans and `LandlockDeny` go; `VERSION` moves once |
| `rustd/crates/daemon/afd_runner/src/{placement/**,reconcile.rs,spelling.rs,capacity/**}` | CREATE/EDIT | §3: the rule; the evidence table reads engines; the refusal reason `engine_policy` |
| `rustd/crates/daemon/afd_fleet/src/run/sql/run/{select_ledger_candidates,assign_turn_head,insert_run_with_turn}.sql`, `afd_fleet/src/run/issue.rs` | EDIT | §3, §5 |
| `rustd/crates/daemon/afd_gateway/src/{registry,scheduler}/**` | EDIT | §5: local connections first; a recheck when the last local worker fills |
| `rustd/crates/daemon/afd_db/schema/<UTC>_runner_tenant_placements.sql` | CREATE | §3, unless M251 lands it first; then EDIT |
| `rustd/crates/daemon/afd_db/schema/{20261108123200_thread_runs,20261108124600_billing_usage_lines}.sql`, `afd_db/src/migration.rs` | EDIT | `engine` and `root` on the run; `engine` on the usage line |
| `rustd/crates/daemon/main/src/config/settings.rs`, `deploy/fly/agentsfleetd-{dev,prod}/agentsfleetd.toml` | EDIT | `[runner.placement] engine_policy` and `tenant_choices`, per O2 |
| `rustd/crates/daemon/afd_billing/src/{nanos.rs,price/rate.rs}`, `afd_fleet/src/run/renew/batch/columns.rs`, `afd_fleet/src/run/sql/report_statement.rs`, `afd_setup/src/catalogue/project/mod.rs`, `afd_api_wire/src/workspace_model.rs` | EDIT | §4: a run-time rate per engine |
| `rustd/crates/daemon/afd_http/src/route/tenant.rs`, `afd_api_tenant/src/handler/tenant/engine/**`, `afd_api_wire/src/tenant_engine.rs`, `afd_api/tests/{route_inventory,route_meta_total}.rs`, `public/openapi.json` | CREATE/EDIT | §4: the tenant route |
| `rustd/crates/foundation/af_core/src/{error_code,problem}/**` | EDIT | one code: a value this deployment does not offer, with its `REGISTRY` row and problem entry |
| `cli/src/program/tree/engine.command.ts`, `cli/src/commands/engine/**`, `cli/src/lib/api-paths.ts`, `cli/src/commands/platform/{runners.ts,runner-vocabulary.ts,runner-schema.ts}`, `cli/test/**` | CREATE/EDIT | §4, §7 |
| `ui/packages/app/app/(dashboard)/settings/engine/**`, `ui/packages/app/lib/api/tenant_engine.ts`, `ui/packages/app/tests/**` | CREATE/EDIT | §4 |
| `deploy/baremetal/agentsfleet-runner-sprites.service`, `deploy/baremetal/agentsfleet-runner.service`, `playbooks/lib/runner/{prepare.sh,common.sh}`, `playbooks/lib/egress_host_deps.sh` | CREATE/EDIT | §5's second unit; §8 drops the bubblewrap install |
| `deploy/local/{bootstrap.ts,linux.sh,linux-runner.Dockerfile}`, `Dockerfile`, `scripts/{build-acceptance.ts,test-runner-startup.sh}`, `make/test-unit.mk`, `playbooks/operations/acceptance/measure_toolbox_first_use.sh` | EDIT | §8: no bubblewrap tier, package or engine argument |
| `.github/workflows/{build-candidate.yml,release.yml,acceptance-on-demand.yml,test-integration-rustd.yml}`, `.github/actions/build-toolbox/install-tools.sh` | EDIT, with Indy's approval at build | a macOS runner build; production's engine flag; bubblewrap names go |
| `docs/architecture/{runner_fleet,runner_execution,billing,testing,product_analytics}.md`, `docs/AUTH.md`, `docs/development.md`, `SECURITY.md`, `playbooks/PRIME.md`, `docs/ux.html` | EDIT | the policy, burst, two engines, the route's scopes, the new event; bubblewrap leaves |
| `~/Projects/docs` engine, self-hosting and billing pages, `changelog.mdx` | EDIT, on that repository's own branch after merge | a public route, command and behaviour |

**Blast radius of the deletion** (SPEC-BLAST). `git grep -l -w -i bubblewrap` at `679002666` finds 348 hits in 131 files; M258_001 moves the 45 files in `afr_sandbox` and `main`.

- **Production files outside them:** those in the table above, plus `afd_runner/src/{reconcile.rs,view/decode.rs}`, `af_wire/src/runner/capability.rs`, `afr_supervisor/src/capability/mod.rs` and `afr_tools/src/generic/env/mod.rs`.
- **Test files:** `cli/test/platform-output.unit.test.ts`, `afd_api/tests/runner_plane/durable_task_routes/fixture.rs`, `afd_bench/src/lane/runs/seed/mod.rs`, `afd_fleet/tests/{integration_runner_admin,support/fleet_requests,verdict_matrix}.rs`, `afd_runner/src/heartbeat/tests.rs`, three `afd_runner/tests/integration_runner_plane/` files, four `af_e2e/tests/` files, `afr_executor/tests/lifecycle.rs` and four `afr_supervisor` test files.
- **Records that keep their words (DOC-S7):** 30 files under `docs/v2/`, `docs/reviews/space-sandbox/**` and three acceptance baselines.
- **Tier tokens:** `landlock_full`, `container_nested`, `LandlockFull` and `ContainerNested` hit 79 lines in 54 files outside `docs/v2/`, each rewritten in §7.

## Applicable Rules

- **`domains/sql.md`** — SQL-FILE, SCH, STS for spellings bound from constants, SQL-ONEFACT, TXN, SQL-LOCKCLOCK, SQL-UPSERT.
- **`domains/http.md`**, **`http-responses.md`**, **`http-surface.md`** — HTTP-GARDE, HTTP-VALID, HTTP-STATUS, HTTP-NEWCODE, HTTP-REGISTRY, HTTP-OPENAPI, HTTP-ENUM.
- **`domains/auth.md`** — AUTH-GUARD for the route's scopes, AUTH-SECRET for the Sprites token.
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-ENV for the token, RS-COMBOS for a macOS build with the Sprites feature alone, RS-RACE.
- **`languages/typescript.md`** — TS-SCHEMA, TS-ERRSTYLE, TS-ACTIONS. **`code.md`** — NLG, ORP, CODE-ERRREG, ECL, OWN.
- **`tests.md`** — TEST-NEG, TEST-ACKLOSS for a create whose reply is lost, TEST-INJECT, TEST-MOCK for the fake API, TEST-SWITCH for the paid lane, TEST-SKIP.
- **`domains/architecture.md`** — ARCH-WINS, ARCH-LAND, ARCH-PUBLISHED. **`domains/logging.md`** — LOG-EVENT, LOG-SECRET.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all`, `make test-unit-all`, `make test-integration-rustd` | yes | the fake Sprites API keeps every Sprites test in these lanes |
| `af_e2e/tests/layers.rs` | yes | only `agentsfleet_runner` and `af_e2e` name `afr_engine_sprites` |
| The paid real-API lane | never in a gate | run by hand, as acceptance is (`.orly/LOCAL.md`, "no gate runs it, because it costs money") |
| OpenAPI drift and `INVENTORY` counts | yes | regenerate with `--no-banner`; update `route_inventory.rs` and `route_meta_total.rs` |
| `make dry-app` and `make acceptance` | not in a green Pull Request | the engine page's spec and acceptance run before the Pull Request opens |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | the Sprites engine splits into client, policy, transport, holds and sweep |

## Prior-Art / Reference Implementations

- **Reference:** model selection, tenant-scoped with a platform default (`afd_db/schema/20261108121500_model_tenant_models.sql:3-5`; `afd_http/src/route/tenant.rs:123,155-158`; `cli/src/program/tree/models.command.ts`). The engine setting mirrors its route, CLI group and app page.
- **Reference:** the placement report (`harness/engine-policy-placement.md` §5) for the rule, its storage and the burst predicate. M251 §4 plans `runner.tenant_placements`.
- **Reference:** SPACE (`Sandboxed Platform for Agentic Code Execution`) at `~/Projects/oss/rs/SPACE` `0aa39485e`. The default must be in the allowed set and an unknown name fails startup (`docs/execution-environments.md:850-853`). An init failure "is an error, not a fallback" (`crates/l40_api/api/src/controllers/substrate/AGENTS.md:54`); SPACE has nothing for Sprites, so the engine is ours.
- **Divergence:** none from the HTTP standard. The CLI keeps its six practices: layered command, handler and error; handlers that neither print nor exit; one renderer. Structured JSON errors; three test tiers; JSON when stdout is not a terminal.

## Sections (implementation slices)

Build order: §1, a spike, commits only its record. §2 adds the engine, §3 the policy and placement, §4 its surfaces and billing, §5 burst and §6 the macOS runner. §7 switches production, and §8 deletes bubblewrap once no runner row reports it.

### §1 — Spike against the live Sprites API

A dashboard-made token, restricted by name prefix and count, drives the live API from the dev runner host. Helpers stay outside the repository, and the record lands in `docs/v2/reviews/m258-sprites-spike.md`. A failed check is recorded and blocks §2 until Indy rules.

- **Dimension 1.1** — the policy holds before the first command → Test `sprite_policy_holds_before_the_first_command` (manual). Create, set `[allow h1, deny *]`, read it back, then exec: `h1` connects, while an unlisted name, a raw IP, `10.0.0.5`, `169.254.169.254`, `169.254.0.1`, `100.64.0.1` and `::ffff:10.0.0.1` fail. The record repeats each under an enforced `[allow *]` and under `[]`, which Fly says enforces nothing.
- **Dimension 1.2** — the policy cannot be rolled back from inside → Test `a_sprite_cannot_roll_its_policy_back` (manual). Checkpoint under `[allow *]`, apply `[deny *]`, then restore that checkpoint through `/.sprite/api.sock` from inside; a probe of `h1` fails. The record says whether the privileges policy can close the socket.
- **Dimension 1.3** — Fly's root carries the toolbox's tools → Test `a_fresh_sprite_has_the_toolbox_tools` (manual). On a fresh Sprite, `command -v` finds `git`, `python3`, `node`, `rg`, `jq`, `curl`, `uv` and `uvx`, and `ca-certificates` and `coreutils` are present. The record lists versions and `/etc/os-release`, and shows our executor answering through `/proxy` on loopback.
- **Dimension 1.4** — a restricted token reaches only its prefix → Test `a_prefix_token_reaches_only_its_prefix` (manual). With the token restricted to `afr-<runner>-`, create, exec, proxy and delete outside the prefix are refused. The record names the generator that turned the OpenAPI 3.1 document into Rust; progenitor reads 3.0.x only.

### §2 — The Sprites engine

`afr_engine_sprites` is a client generated from the pinned OpenAPI document, as §1 named, with `tokio-tungstenite` for WebSocket channels. It has no Linux-only dependency, so it builds on macOS. `prepare` names the Sprite by the runner's prefix and the sandbox's `Uuid7`.

`prepare` creates the Sprite, sets its network policy, reads it back and sets the memory limit. It uploads and starts our executor, checks the tool list, then dials the executor through `/proxy`.

**State authority:** the Sprite's name at Fly. **Guarded transition:** a create answered `409` adopts the named Sprite. **Failure assumption:** a create whose answer is lost answers `Ambiguous` and is never repeated; the sweep lists by prefix and deletes.

**Policy and holds.** No network compiles to `[deny *]`, a listed plan to its `allow` rules then `deny *`, and the public internet to an enforced `[allow *]`, never `[]`. Holds are `Stop` only: sessions killed, the link closed, and Fly pauses the Sprite after about 30 seconds (*vendor-stated*). `quiesce` answers `Witness::Guest`, and `kill` answers `Unsettled` unless the Sprite is deleted.

**Snapshots on Fly's checkpoints.** M258_001's `Snapshots` interface is served by Fly: `take` posts a checkpoint, `restore` restores it, and `forget` deletes it. A restore replaces the writable overlay and ends every session (https://docs.fly.io/sprites/concepts/checkpoints.md), so no process of the old environment survives. A Fly checkpoint belongs to one Sprite, and no public call restores it onto another, so a Sprite snapshot restores only on its own Sprite.

Policies ride in checkpoints (https://fly.io/sprites/checkpoint-restore.md). After every restore the engine applies the run's compiled policy again and reads it back before the executor answers. §1.2 proves code inside cannot roll it back.

**The root is Fly's, stated.** The engine never mounts, unpacks or uploads our toolbox; a Sprite's create takes only `name` and `url_settings`. Its `environment()` names `Vendor { ubuntu, 26.04 }`, as the Sprite reports it. A Sprite missing a tool from §1.3's list is deleted and the run refused `sandbox_unavailable`, naming the tool; the engine installs nothing.

**Resources and credentials.** `Granted` reports memory as set, 8 virtual CPUs and 100 GB fixed by Fly, and no process or I/O cap. The engine's token comes from the runner's environment through an injected reader, held as an `af_secrets::Secret`. It is never written into a Sprite or onto the wire, and its custody is D9.

- **Dimension 2.1** — the engine passes the suite against a fake → Test `the_sprites_engine_passes_the_suite_against_the_fake_api` (unit, `make test-unit-all`). Every must-match trial passes over a scripted fake API, the six snapshot trials (M258_002 adds two) included. Its may-differ list names the vendor root and the fixed processors and disk.
- **Dimension 2.2** — a lost reply makes one Sprite → Test `a_create_answered_409_adopts_the_named_sprite` (unit). The fake commits the create and drops the reply. The retry with the same id answers `409`, and one Sprite is adopted.
- **Dimension 2.3** — an unknown create is swept, never retried → Test `a_create_timeout_is_ambiguous_and_swept` (unit). The fake times out, so the engine answers `Ambiguous` and sends no second create. `sweep` deletes every Sprite under the prefix that the keep set does not name.
- **Dimension 2.4** — no plan compiles to "no enforcement" → Test `no_plan_compiles_to_an_empty_rule_list` (unit). No network gives `[deny *]`, listed gives `[allow h1, allow h2, deny *]`, and public gives `[allow *]`. No input yields `[]`.
- **Dimension 2.5** — no command before the policy reads back → Test `no_command_reaches_a_sprite_before_its_policy_reads_back` (unit, fake). A read-back that differs from the compiled rules deletes the Sprite and refuses the run. The fake saw no exec.
- **Dimension 2.6** — a restore cannot bring back an old policy → Test `a_restore_reapplies_the_policy_before_the_executor_answers` (unit, fake). The fake's restore brings back `[allow *]` from the checkpoint, and the engine applies `[deny *]` and reads it back before the first exec. A read-back that differs deletes the Sprite and refuses the run.
- **Dimension 2.7** — the real API agrees → Test `the_sprites_engine_passes_the_suite_against_the_real_api` (integration, the paid lane, by hand). Every must-match trial passes against `api.sprites.dev` with a restricted token. Counts go in the Session Notes.

### §3 — The engine policy, stored and enforced in placement

`runner.tenant_placements` holds one row per tenant with `engine_policy`, its spelling validated in the application; it shares the row M251 plans for `placement_type`. A missing row reads `[runner.placement] engine_policy` from the daemon's file, `local_only` when the file is silent. `[runner.placement] tenant_choices` lists the values a tenant may pick, so O2's gate is a file edit.

`SandboxTier` gains `firecracker` and `sprites`. A runner's substrate is `sprites` for the `sprites` tier and `local` otherwise.

`afd_runner::placement` holds one rule: `permits(policy, substrate, a_local_worker_is_free, this_runner_holds_the_fleet)`. The scan, the claim and the issue apply it beside ownership (`select_ledger_candidates.sql:12`, working tree), with spellings bound from constants.

**State authority:** the tenant's row, read under the host lock at issue. **Guarded transition:** the issue re-checks policy against substrate and refuses with reason `engine_policy` (`afd_runner/src/capacity/mod.rs:305-310`, working tree). The issue copies `engine` and `root` from the runner row's report onto `thread.runs`, as it copies `posture`, `provider` and `model`.

- **Dimension 3.1** — the rule answers every cell → Test `the_engine_rule_answers_every_cell` (unit). 3 policies × 2 substrates × free or full × holder or not give 24 verdicts. `sprites_only` refuses every local cell and `local_only` every Sprites cell.
- **Dimension 3.2** — a policy keeps work off the wrong engine → Test `a_sprites_only_fleet_is_never_offered_to_a_local_runner` (integration). Scan, claim and issue offer a `sprites_only` fleet's turn to no local runner. They offer a `local_only` fleet's to no Sprites runner.
- **Dimension 3.3** — a hole in the scan is caught at issue → Test `a_forced_hole_is_refused_at_issue_with_engine_policy` (integration). A forced claim across the rule logs `runner_placement_refused` with reason `engine_policy`. No run row is written.
- **Dimension 3.4** — a run records its engine and root → Test `a_run_records_the_engine_and_root_it_ran_on` (integration). A run on a Sprites runner records `engine` `sprites` and its vendor root. A later tier edit leaves both unchanged.
- **Dimension 3.5** — a changed policy refuses the old holder → Test `a_policy_changed_under_a_hold_refuses_the_holder` (integration). A fleet is held on a Sprites runner, then its tenant picks `local_only`. The next turn goes local and the holder is refused.

### §4 — The setting's surfaces, and billing by engine

`GET`, `PUT` and `DELETE /v1/tenants/me/engine` read, write and reset the policy. A read returns the effective policy, whether it is the default, the default and the values offered. A value outside `tenant_choices` answers `409` with the new code.

`agentsfleet engine current|use <policy>|reset` mirrors `agentsfleet models`. The app's account settings gain an engine page beside billing, members and security. It says that Sprites runs bill at their own rate.

A run's `engine` picks its run-time rate wherever `RUN_NANOS_PER_SEC` is bound today, and `billing.usage_lines` gains `engine`. Firecracker keeps today's rate, and the Sprites rate is D10. Fly's usage API is a metric read after a run, never a charge: its minute-grained records may be `partial` and close at month end.

- **Dimension 4.1** — the route reads, writes, resets and refuses → Test `the_engine_setting_route_reads_writes_and_resets` (integration). No row reads the default with `is_default`; `PUT local_then_sprites` reads back; `DELETE` restores the default. An unknown value answers `400`, a value not offered `409` with the code, and the row is unchanged after each.
- **Dimension 4.2** — a Sprites run pays its own rate → Test `a_sprites_run_is_charged_at_the_sprites_rate` (integration). A Sprites run's line names `sprites` at the Sprites rate, and a Firecracker run's is unchanged. A retried renewal charges nothing.
- **Dimension 4.3** — the CLI refuses before it asks → Test `engine use refuses an unknown policy without a request` (unit, handler). `agentsfleet engine use cloud` exits 4, names the three values and sends no request. `engine current --json` prints `{policy, is_default, default, offered}`.
- **Dimension 4.4** — the app keeps the choice it could not save → Test `a refused engine choice keeps the previous one` (unit, vitest). A `409` leaves the earlier choice selected and shows the refusal's sentence. A `200` follows one `PUT`.

### §5 — Burst: a second runner process on the same host

A host with KVM bursts. One unit runs `--engine firecracker` and a second runs `--engine sprites`, each with its own registration, `host_id`, storage home and token.

A Sprites runner takes a `local_then_sprites` fleet only as its holder, or when no live local runner able to serve it has a free worker. A free worker is a live-run count below the lesser of `worker_count` and the reported `sandboxes_max`. A Firecracker host full at 13 therefore counts as full.

Local connections are scanned before Sprites ones (`afd_gateway/src/registry/mod.rs:101-108`). An issue that fills a host's last local worker rings a recheck, so two turns arriving together are not split by scan order. No queue timer exists, because the scheduler runs none (`afd_gateway/src/scheduler/mod.rs:1`).

A turn whose head checkpoint names a root the Sprites runner does not report is not offered to it.

**Bounds.** The Sprites runner's `worker_count` bounds concurrent Sprites, and Fly's plan bounds running Sprites and creates (10 a minute on pay-as-you-go, *vendor-stated*). The payer's wallet bounds money. A monthly spending limit on burst is a number Indy names (D11).

- **Dimension 5.1** — a full host bursts, a free one does not → Test `a_full_local_host_bursts_to_the_sprites_runner` (integration). A Firecracker runner has `worker_count` 64, `sandboxes_max` 13 and 13 runs live, so the Sprites runner takes the next turn. At 12 live it takes none.
- **Dimension 5.2** — two turns, one free worker, no wait → Test `two_turns_with_one_free_local_worker_run_at_once` (integration). One turn runs locally and the other on the Sprites runner. Neither waits for the first to end.
- **Dimension 5.3** — one host, two runner processes → Test `one_host_runs_a_firecracker_and_a_sprites_runner` (e2e, `af_e2e`). Two runner processes on one machine serve one daemon, the Sprites one on the fake API. A burst fleet's concurrent turns land on both, and the local engine never creates a Sprite.
- **Dimension 5.4** — a Sprites outage stops burst → Test `a_sprites_outage_degrades_the_runner_within_one_beat` (integration). The fake API fails, so the Sprites runner's report fails its API check and the runner reads degraded with zero workers. The turn waits for a local worker.
- **Dimension 5.5** — a task checkpoint stays on its root → Test `a_task_run_with_a_toolbox_checkpoint_is_not_offered_to_a_sprites_runner` (integration). A task's checkpoint names our toolbox, and every local worker is busy. The Sprites runner is not offered it.

### §6 — A runner without KVM: the macOS build

The Sprites engine needs no Linux, so `--engine sprites` boots with no delegated cgroup, toolbox or KVM. CI builds a macOS runner with the Sprites feature alone. Indy's M2 Mac runs it natively, in place of the Linux VM it needs today (`runner_fleet.md:120`).

- **Dimension 6.1** — the Sprites engine boots anywhere, and only with a token → Test `the_sprites_engine_boots_without_a_cgroup_and_refuses_without_a_token` (unit, `main`). With a fake API it boots on the test host with no cgroup root. With no token it refuses naming the variable, and no engine starts.
- **Dimension 6.2** — a fixture fleet answers from a Mac → Test `a_mac_runner_answers_a_fixture_fleet` (manual). Indy's Mac runs the macOS build against the dev daemon with a restricted token, and `morning-rooster`'s first message is answered. Evidence goes in the Session Notes.

### §7 — Production runs on Firecracker, and tiers name engines

Production runner hosts restart with `--engine firecracker` after the dev host has run Firecracker for the bake period Indy names (D12). Each host needs `/dev/kvm` (A2). The tiers become the engines, `firecracker`, `sprites` and `dev_none`, and the reconcile demands that the report's `engine` match its tier.

- **Dimension 7.1** — production reports Firecracker → Test `production_runners_report_firecracker` (manual). Aiwa restarts each host after D12, and every row shows `engine: firecracker`, `kvm: usable` and a self-test `all_ok`. The release's acceptance passes; evidence goes in the Session Notes.
- **Dimension 7.2** — a tier names an engine → Test `a_tier_names_its_engine` (unit, `af_wire` and `afd_runner`). `firecracker`, `sprites` and `dev_none` round-trip, and `landlock_full` fails to decode. A `sprites` tier whose report names `firecracker` reads degraded with its own reason.

### §8 — bubblewrap deleted

Once no runner row reports `bubblewrap`, `afr_engine_bubblewrap` and `EngineChoice::Bubblewrap` go. So do the report's `landlock`, `seccomp`, `cgroup_controllers`, `bubblewrap` and `egress_enforcement` fields, `FailureClass::LandlockDeny` and the bubblewrap install. `afr_guest`'s hardening stays, because the Firecracker guest's inner environment runs it.

- **Dimension 8.1** — no engine named bubblewrap → Test `bubblewrap_is_not_an_engine` (unit, `main`). `--engine bubblewrap` answers `InvalidValue`. `EngineChoice` holds `firecracker` and `sprites`, plus `unsandboxed` in debug builds.
- **Dimension 8.2** — the report names no Linux mechanism → Test `a_report_naming_a_linux_mechanism_is_refused` (unit, `af_wire`). A report carrying `landlock` is refused as an unknown field. An engine report round-trips.
- **Dimension 8.3** — the lane runs Firecracker by default → Test `make test-runner-kernel` (integration, a KVM host). With no `ENGINE`, the suite runs on Firecracker. Every trial passes.

## Interfaces

```text
GET    /v1/tenants/me/engine   200 {policy, is_default, default, offered[]}            scopes as /v1/tenants/me/provider
PUT    /v1/tenants/me/engine   {policy: "local_only"|"local_then_sprites"|"sprites_only"}; 200 as GET;
                               400 unknown value; 409 a value this deployment does not offer
DELETE /v1/tenants/me/engine   200 as GET, the default restored
CLI    agentsfleet engine current | use <policy> | reset;  agentsfleet-runner run --engine firecracker|sprites
Wire   SandboxTier: firecracker | sprites | dev_none;  CapabilityReport: engine, isolation, kvm, sandboxes_max, root
Daemon [runner.placement] engine_policy = "local_only";  tenant_choices = ["local_only"]   (production, until O2)
Schema runner.tenant_placements (tenant_id key, engine_policy); thread.runs + engine, root; billing.usage_lines + engine
```

The new code is numbered at PLAN from the next free in `af_core/src/error_code/`. Its `REGISTRY` row and problem entry land in the same commit.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Create reply lost | failure | Fly commits; the answer is dropped | the retry's `409` adopts; one Sprite | `a_create_answered_409_adopts_the_named_sprite` |
| Create times out | failure | no answer at all | `Ambiguous`; never repeated; swept by prefix | `a_create_timeout_is_ambiguous_and_swept` |
| Policy not in force | security | read-back differs | Sprite deleted; run refused; no exec sent | `no_command_reaches_a_sprite_before_its_policy_reads_back` |
| Policy rolled back by a restore | security | a checkpoint carries a looser policy | the compiled policy is applied again and read back before the executor answers | `a_restore_reapplies_the_policy_before_the_executor_answers` |
| Unenforced policy | security | a plan compiled to `[]` | cannot compile | `no_plan_compiles_to_an_empty_rule_list` |
| Fly down | dependency | the API fails | the Sprites runner degrades; no burst | `a_sprites_outage_degrades_the_runner_within_one_beat` |
| Wrong engine | security | a scan hole | refused at issue, `engine_policy` | `a_forced_hole_is_refused_at_issue_with_engine_policy` |
| Value not offered | auth | a tenant picks Sprites before O2 | `409`; row unchanged | `the_engine_setting_route_reads_writes_and_resets` |
| Missed burst | race | two turns, one free worker | local first; a recheck rings | `two_turns_with_one_free_local_worker_run_at_once` |
| Restore across roots | failure | a toolbox checkpoint meets a Sprite | not offered to the Sprites runner | `a_task_run_with_a_toolbox_checkpoint_is_not_offered_to_a_sprites_runner` |
| Tool missing on a Sprite | dependency | Fly's image lacks a listed tool | Sprite deleted; `sandbox_unavailable` naming it | `the_sprites_engine_passes_the_suite_against_the_fake_api` |
| Missing token | failure | the variable is unset | boot refuses naming it | `the_sprites_engine_boots_without_a_cgroup_and_refuses_without_a_token` |

## Invariants

1. A tenant's turn reaches only a runner its policy permits — the scan, the claim and the issue apply `permits` beside ownership (Dimensions 3.2, 3.3).
2. No Sprite receives a command before its policy reads back as compiled — `prepare` and every restore read it back before the executor answers (Dimensions 2.5, 2.6).
3. No plan compiles to an unenforced Sprite policy — the compiler's output type has no empty variant (Dimension 2.4).
4. One engine per runner process — `EngineChoice` maps one choice to one backend, and burst is a second process (Dimensions 5.3, 6.1).
5. A run's engine and root are what ran it — the issue copies both from the locked runner row (Dimension 3.4).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `run_burst_placed` (log) | operator | a Sprites runner takes a `local_then_sprites` turn | `fleet_id`, `runner_id`, `reason` (`full` or `no_live_local`) | no tenant name | `a_full_local_host_bursts_to_the_sprites_runner` |
| `engine_policy_changed` | product | a tenant writes or resets its policy | `tenant_id`, `policy`, `is_default` | no person's email | `the_engine_setting_route_reads_writes_and_resets` |
| `sprite_swept` (log) | operator | a sweep deletes Sprites | `runner_id`, `deleted`, `left` | no token | `a_create_timeout_is_ambiguous_and_swept` |

`docs/architecture/product_analytics.md` gains `engine_policy_changed` in the same Pull Request.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every open owner decision is ruled (Discovery) | `git grep -n -E "^  - \*\*[A-Z][0-9]+\*\* .\[\?\]" -- "docs/v2/*/M258_003_*"` | no output | P0 | |
| R2 | The spike's record is committed (§1) | `git ls-files docs/v2/reviews/m258-sprites-spike.md` | one line | P0 | |
| R3 | The Sprites engine is safe against a fake API, its snapshots included (§2) | `make test-unit-all` | exit 0; `no_plan_compiles_to_an_empty_rule_list`, `a_restore_reapplies_the_policy_before_the_executor_answers`, `the_sprites_engine_passes_the_suite_against_the_fake_api` pass | P0 | |
| R4 | Placement obeys the policy and records engine and root (§3) | `make test-integration-rustd` | exit 0; `a_sprites_only_fleet_is_never_offered_to_a_local_runner ... ok`, `a_run_records_the_engine_and_root_it_ran_on ... ok` | P0 | |
| R5 | The setting and billing work (§4) | `make test-integration-rustd` | exit 0; `the_engine_setting_route_reads_writes_and_resets ... ok`, `a_sprites_run_is_charged_at_the_sprites_rate ... ok` | P0 | |
| R6 | A KVM host bursts through a second process (§5) | `make test-integration-rustd` | exit 0; `one_host_runs_a_firecracker_and_a_sprites_runner ... ok` | P0 | |
| R7 | The app keeps a refused choice (§4) | `make dry-app` | exit 0 | P1 | |
| R8 | bubblewrap is gone (§8) | `git grep -c -w -i bubblewrap -- rustd cli deploy make scripts playbooks/lib Dockerfile .github` | no output | P0 | |
| R9 | Firecracker is the lane's default (§8) | `make test-runner-kernel` | exit 0; zero failed trials | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the bubblewrap engine | `git ls-files rustd/crates/runner/afr_engine_bubblewrap` | no output |
| the Linux-named tiers and fields | `git grep -n -w -e landlock_full -e container_nested -e LandlockFull -e ContainerNested -e LandlockDeny -e egress_enforcement -- rustd cli ui/packages deploy public` | no output |

`WAITS_FOR_FIRECRACKER` (`afr_tools/src/generic/env/mod.rs:48-49`) names Firecracker as the browser's missing piece; M259's browser work owns it.

## Out of Scope

- The browser, web search, and the harness's tools and prompts: M259.
- Sprite checkpoints as holds, and a pool of pre-created Sprites: not planned; holds stay `Stop`.
- Sprites in a chosen region (Fly offers none), a tenant's own Fly account on a tenant runner, and a timer-based burst threshold: not planned.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a customer picks "Sprites only" on the engine page, and its next run's row reads `engine: sprites`.
2. **Preserved user behaviour** — a tenant that picks nothing runs where it runs today, on our hosts. Every tool, hold and network policy keeps its verdicts.
3. **Optimal-way check** — one setting, read by the three statements that already decide ownership. The gap is Fly's written answer (O2).
4. **Rebuild-vs-iterate** — iterate on placement, billing and the tenant settings, and add one engine behind the seam.
5. **What we build** — the Sprites engine, the policy row and rule, the route, CLI and page, and a rate per engine. Also the second runner unit, the macOS build, production's switch and bubblewrap's deletion.
6. **What we do NOT build** — a per-run engine choice, a queue-age timer, an invented spend cap, a Sprite pool or a Mac-local VM engine.
7. **Fit with existing features** — compounds with tenant runners and M251's placement row. It must not destabilize tenant ownership, which the rule sits beside.
8. **Surface order** — API, then CLI, then the app, because both clients read the one route.
9. **Dashboard restraint** — the page offers only `offered` values. Sprites values appear in production only after O2.
10. **Confused-user next step** — a refused value names the values offered. `agentsfleet engine current` prints the effective policy and the default.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** spike, engine, policy, surfaces, burst, Mac, production, deletion. Each Section builds on the one before.
- **Alternatives considered:**
  - One runner process routing each run to two engines. Refused: the daemon would not know which engine ran a run, and a fallback inside a runner breaks `engine.rs:17`.
  - Labels for the policy, which fail because an untagged fleet matches every runner.
  - A queue-age burst threshold, an invented number with no timer to fire it.
  - Fly's metered usage as the charge: minute records, `partial`, final at month end.
  - Mounting our toolbox in a Sprite, which Indy ruled out: "out toolbox sticks to our host only".
- **Patch-vs-refactor verdict:** a **refactor** because placement, billing, the wire's tiers and report, and the engine set all change, and an engine crate is deleted.

## Discovery (consult log)

- > Owner (Indy, Nov 26, 2026): "I also need the sprites firecracker in fly as either burst or sole one. So customers can pick. In our case we pick 1. our hardware + burst as sprites 2. an user can pick sprintes fully So the policy must be configurable on the engine choice. I think our code is abstracted that way. 3. Is headless chromium the simplest or anyothers equivalent to chromium 4. I want to replace the engine first 5. Then the web search with parallel + other providers like opencode and the browse decision 6. so 2 milestones, in the same milestone i need the tools of guffbolt to be improved with the system prompts and such proposed like opencode" — context: the policy's three values and the platform's own choice.
- > Owner (Indy, Nov 26, 2026): "bubblewrap must be retired if we do 3rd party sprites based firecraker or any sandbox or firecracker engine our own." and "CAn you move our spec to 258_001, 258_002 then?" — context: §8 and the numbering.
- > Owner (Indy, Nov 26, 2026): "Also while building our own I think should we refer and use ~/Projects/oss/rs/SPACE ? from perplexity which build a firecracker gRPC layer?" and "OUR ABSTRACTION OF THE ENGINE MUST BE IMPROVED BY LOOKING A THE ~/PROJECTS/OSS/RS/SPACE? MAY BE? ON TEH TRAITS AND SUCH" — context: M258_001's seam, which this engine implements.
- > Owner (Indy, Nov 26, 2026): "So i can't burst to fly sprites if i had my own host with kvm running firecracker ?" and "Okay where will the engine abstraction live and all these changes i want a better crate management now, since the runner should use another engine crate or so is my thinking" — context: §5's second process on a KVM host, and `afr_engine_sprites` as its own crate.
- > Owner (Indy, Nov 26, 2026): "does afr_engine_sprites take the erofs type of toolbox or?" and "afr_engine_sprites - root = Fly's ubuntu 26.04 - can you switch to Debian 13. if sprites offers theirs example fly debian 13 then why mount our toolbox inside, out toolbox sticks to our host only?" — context: §2 uses Fly's root as it is. A Sprite takes no image or distribution choice, and Fly's images are "for internal use by Sprites" (`superfly/sprites-images` README).
- > Owner (Indy, Nov 26, 2026): "also read this sprite design https://fly.io/blog/design-and-implementation/ so you know if you prefer to redesign anything", then "i am thinking we must do that" and "even in our checkpoint and restore to make them faster" — context: applied to our own engine in M258_001 and M258_002; a Sprite already works that way, and its checkpoints serve §2's snapshots.
- > Owner (Indy, Nov 26, 2026): "I prefer the milestone where ever we do the firecracker and sprites engine abstractions move (to me this is a compute layer) must  fix the checkpoint and snapshot, this must be the first milestone so we sunset bubblewrap and move on to firecracker and sprites" — context: ruled; M258 is the compute layer and lands first. Its packing into three Workstreams is O1.
- **Open owner decisions.** Each stays open until Indy rules; each recommendation is Aiwa's.
  - **O1** `[?]` the compute layer's packing into three Workstreams, the coordinator's proposal after Indy's ruling. Recommend it, so Sprites build on a seam and a snapshot design already proven on our hosts.
  - **O2** `[?]` a customer's "Sprites only", and burst of tenant runs, against Fly ToS §1.4(d). Recommend building all three values and asking Fly in writing. Until it answers, production offers `local_only` alone, and dev, whose tenants are ours, runs `local_then_sprites`; this is not legal advice.
  - **O3** `[?]` no overcommit, disk-only holds, and overflow to Sprites. Recommend as assumed; §5 counts the memory bound as full.
  - **O4** `[?]` the Firecracker client copy, which is M258_001's. Recommend as M258_001 records.
  - **O5** `[?]` M258 then M259. Recommend as assumed.
  - **D9** `[?]` the Sprites token's custody. The API mints none; restricted tokens (name prefix, count, labels, expiry) are made in the dashboard. Options: (a) the daemon relays every stream, (b) a restricted token per platform runner host, or (c) a public Sprite URL behind a per-run secret.
  - **D9, recommendation:** (b), with prefix the runner's, count its `worker_count`, an expiry, and never on a tenant host, if §1.4 passes. (a) puts every byte through the daemon and moves the client to a lower-layer `af_` crate; (c) opens a port.
  - **D10** `[?]` the Sprites run-time rate. Fly charges $0.0385 per CPU-hour and $0.021875 per GB-hour (*vendor-stated*); Indy names ours.
  - **D11** `[?]` a monthly spending limit on burst, a number only Indy names. Recommend none in the first cut, with the Sprites runner's `worker_count` as the bound.
  - **D12** `[?]` the bake period before production switches. Recommend that Indy names it; inventing one breaks the no-invented-limits rule.
  - **D13** `[?]` the production host kernel and SMT, re-ruling M258_001's D6 and D7. Recommend Firecracker's validated 6.18, and SMT off on hosts that run tenants' fleets, per Firecracker's production guide.
  - **D14** `[?]` `apt` on a Sprite: allow it as a stated property of that engine, or remove the package tools after create. Recommend allowing it on the may-differ list. The tenant has root in its own VM, removing `apt` stops no download-and-run, and the network policy is the boundary.
  - **A2** `[?]` whether each production runner host has `/dev/kvm`. One `test -c /dev/kvm` on each host in `PROD_RUNNER_HOSTS` settles it.
- **Consults** — the research reports named in Provenance. Skeptic corrections applied: an empty Sprite rule list enforces nothing, so deny-all and allow-all compile to explicit rules. The API mints no credential, and burst is placement, which the coexistence report deferred and Indy's ruling brings in.
- **Model reviews** — none yet. The Elephant-Goldfish Model (EGM) review runs at `orly-spec` before the docs Pull Request.
- **Open blockers** — O1 to O5, D9 to D14 and A2; §1's four checks.
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
