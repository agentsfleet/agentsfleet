<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M257_001: The prime fleets pass on Haiku 5.5: a task ends when its finish lines hold, and no turn waits behind a dropped hold

**Prototype:** v2.0.0

**Milestone:** M257

**Workstream:** 001

**Date:** Nov 26, 2026

**Status:** DONE

**Priority:** P1, a finished task blocks as unverified, a chat turn waited 195 seconds on dev, and the default model's first message fails with a 400

**Categories:** API, CLI, DOCS, INFRA, OBS

**Batch:** B1, alone; this branch, `fix/task-done-by-finish-lines`, one Pull Request

**Depends on:** none

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 26, 2026) from Indy's rulings of Nov 26, 2026, the dev incident report on the 195-second wait (read at `d044cd1`, its files unchanged at `576d0b462`), the Gemini review, the catalogue patch and this branch's diff

**Canonical architecture:** `docs/architecture/durable_tasks.md` §Goal evaluation; `docs/architecture/runner_fleet.md` §Capacity and sandbox holds and §Placement; `docs/architecture/models.md`

---

## Overview

**Goal (testable):** On Claude Haiku 5.5, `stubborn-mule`'s task ends `done` once its one statement holds, whatever the next action says.

**Terms.** Rust paths that start with a crate name are under `rustd/crates/<layer>/`. Line references are at `576d0b462` unless a line names this branch. A **finish line** is one of a task's completion statements; the decision model scores each as a Noul between 0 and 1.

**Problem:** Read from source and from dev.

- **A finished task blocks.** A completion needed the next action `DONE` at the profile's `done` thresholds, and every statement at `statement_min_bps` (`afr_agent/src/decision/goal.rs:52`; `afd_fleet/src/durable_task/decision/authorize.rs:90`). A dev check held its statement at 0.97 while `DONE` had probability 0.43 and confidence 0.24. The run stopped with no report, and the task blocked as `completion_unverified`.
- **A turn waited 195 seconds on dev (Nov 25, 2026).** The census counted another fleet's hold the runner had already evicted (`afd_fleet/src/run/sql/run/insert_run_with_turn.sql:14-22`; `afd_runner/src/sql/holds/clear_dropped_holds.sql:5`). The release published no wake (`afd_fleet/src/run/affinity.rs:187-197`). Ordinary order has no age key (`select_ledger_candidates.sql:57-60`), so 66 passes ran and none picked the turn.
- **The lost line names no guard.** `run_assignment_lost` says "the pass outlived its assignment" (`afd_fleet/src/run/pull/step.rs:53`), which is wrong for every refusal but one, and logs no census.
- **Capacity is a guess.** The local bootstrap assigns `worker_count: 4` (`deploy/local/bootstrap.ts:300`), and no runner reports its CPUs, memory or free disk (`af_wire/src/runner/capability.rs:25`).
- **Haiku 5.5 is refused.** A dev turn on Nov 26, 2026 at 06:32 UTC failed in 140 ms: "the model provider refused the turn with status 400 (invalid_request_error)". Each request feature sent to Anthropic alone was accepted.
- **The cause.** The runner's whole request, replayed, drew `tools.32.custom.input_schema.type: Field required`: `browser`, `browser_open` and `screenshot` take untyped arguments, so their schema is `{}` (`guffbolt/src/tools/browser.rs`). MiniMax and Fireworks accept that schema, so every Claude model was refused and no other model was.
- **The catalogue is stale.** Its rows were verified on Oct 18, 2026. Both Gemini rows reserve 943,718 output tokens against Google's 65,536 per request, so Argon compacts after about 52,000 input tokens (Gemini review §6.1).

**Solution summary:**

- **Finish lines decide done.** Every statement at `statement_min_bps` completes the task, whatever the next action says. The next action only chooses `CONTINUE`, `WAIT` or blocked. The `done` threshold is deleted.
- **Three scheduling fixes.** The census counts live runs only; a lost assignment re-rings the fleet's wake; the head's age breaks the last tie in place of `random()`. The lost line names its guard and the census.
- **The runner reports its host.** The heartbeat carries CPUs, memory and free disk, and `agentsfleet platform runners list` shows them; dev's `worker_count` is set from them.
- **Haiku 5.5 is the default** on dev, in the local bootstrap and in CI acceptance, and the runner's Messages request is fixed so Anthropic accepts it.
- **The catalogue is current:** 21 models, Gemini output at 65,536, and guffbolt-ai's Gemini list redone.
- **Every crate is current.**

**Design flow:**

```
goal check ─► every statement ≥ statement_min_bps? ─yes─► Complete (native evidence still applies)
                         │no
                         ▼
             next_action at its own threshold: CONTINUE ─► continue · WAIT ─► wait · anything else ─► no report, blocked

pass ─► Runs::issue ─► census: live runs < worker_count ─no─► lost: log reason + census ─► let_go ─► publish the fleet's wake
                                │yes                                                          │
                                ▼                                                             ▼
                              run issued                         next pass: held · hint · locality · oldest head
```

## Rulings

**Ruled by Indy on Nov 26, 2026** (quotes in Discovery).

| # | Ruling |
|---|---|
| Done | Finish lines decide done; the next action chooses only `CONTINUE`, `WAIT` or blocked. The `done` threshold is removed. |
| Scheduling | The three fixes: the census counts live runs only, a lost assignment re-rings the wake, and an age term joins ordinary ordering. The census is logged on `run_assignment_lost`. |
| Capacity | The runner reports host CPU, memory and free disk on its heartbeat; `platform runners list` shows them; dev's `worker_count` is set from them. |
| Model | Claude Haiku 5.5 is the default model, and the 400 is fixed in this PR. |
| Crates | Every crate is updated and checked in this PR. |
| One PR | Everything here goes in one Pull Request. |

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(task): finish lines decide done; turns are not held behind dropped holds; Haiku 5.5 by default
- **Intent (one sentence):** The prime fleets pass on the default model, because a task ends when its finish lines hold and no turn waits behind a stale hold.
- **Authoring handshake:** Indy wants the fixes this branch found shipped together, with Haiku 5.5 as the default. ASSUMPTIONS I'M MAKING:
  1. One Pull Request, one commit per Section.
  2. Nothing is backward compatible: a stored profile that carries `done` fails the read, and the playbook replaces it.
  3. An open marker is filled when the code lands.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/runner/afr_agent/src/decision/goal.rs` — the runner's reading of a goal check, which §1 changes.
2. `rustd/crates/daemon/afd_fleet/src/durable_task/decision/authorize.rs` — the daemon's authorization of the same report, which must agree with the runner.
3. `rustd/crates/daemon/afd_fleet/src/run/sql/run/insert_run_with_turn.sql` and `select_ledger_candidates.sql` — the census and the candidate order §2 changes.
4. `rustd/crates/daemon/afd_fleet/src/run/unsent/mod.rs` — `release_unsent` re-rings a wake after a release (`:59`); §2's lost path mirrors it.
5. `deploy/local/bootstrap.ts` — the local platform model, its key and the runner's `worker_count`.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/done/M257_001_P1_API_CLI_DOCS_INFRA_OBS_PRIME_FLEETS_PASS_ON_HAIKU.md` | CREATE | this spec; Status reads DONE at close |
| `docs/v2/pending/M258_001_P1_API_CLI_DOCS_INFRA_OBS_FLEETS_RUN_IN_OUR_FIRECRACKER_MICROVMS.md`, `docs/v2/pending/M258_003_P1_API_CLI_DOCS_INFRA_OBS_UI_RUNS_FOLLOW_ENGINE_POLICY_BUBBLEWRAP_RETIRED.md`, `docs/v2/pending/M259_001_P1_API_CLI_DOCS_INFRA_OBS_UI_ANY_MODEL_SEARCHES_READS_AND_BROWSES.md`, `docs/v2/pending/M259_002_P1_API_CLI_DOCS_INFRA_UI_TRUTHFUL_PROMPTS_AND_IMAGES_IN_MESSAGES.md` | CREATE | the next two milestones' specs, pending: M258 the compute layer (M258_002, checkpoints and snapshots, follows), M259 the web and the harness. They replace `M254_001`, the Lightpanda browser spec, which is removed |
| `rustd/crates/protocol/af_wire/src/decision/config.rs`, `public/openapi.json` | EDIT | §1: `GoalDecisionPolicy` loses `done` |
| `rustd/crates/runner/afr_agent/src/decision/goal.rs`, `afd_fleet/src/durable_task/decision/authorize.rs` | EDIT | §1: statements decide done, by the same rule on both sides |
| `rustd/crates/runner/afr_agent/src/loop/durable_task_tests/behavior/{mod,goal_results}.rs`, `afd_fleet/tests/integration_durable_tasks/decisions/goal_evidence.rs` | EDIT/CREATE | §1's tests |
| the `done` fixtures: `afd_admin/src/test_util.rs`, `afd_admin/tests/{decision_model/fixture,integration_decision_models}.rs`, `afd_api_operator/src/handler/admin/models/input/tests.rs`, `afd_fleet/src/durable_task/decision/prepare_tests.rs`, `afr_agent/src/fixture/mod.rs`, `afr_supervisor/src/{run_loop/tests/recovery/fixture,verbs/tests/decision/mod}.rs`, `runner/main/examples/kernel_lane/supervisor_recovery/wire.rs`, `scripts/model-catalogue/decision.integration.test.ts` | EDIT | §1: every profile they build drops `done` |
| `cli/src/commands/platform/decision-schema.ts`, `cli/test/platform-decision.unit.test.ts`, `deploy/local/bootstrap.ts`, `deploy/local/bootstrap.test.ts` | EDIT | §1: the CLI refuses a profile with `done`; the bootstrap writes none |
| `tests/fixtures/fleets/{stubborn-mule,careful-squirrel}.json`, `ui/packages/app/tests/e2e/acceptance/prime-stubborn-mule.spec.ts` | EDIT | §1: the fixtures' wording |
| `docs/architecture/durable_tasks.md`, `playbooks/operations/model_catalogue/001_playbook.md` | EDIT | §1: the goal rules; replacing a stored decision profile |
| `afd_fleet/src/run/sql/run/{insert_run_with_turn,select_ledger_candidates}.sql`, `afd_fleet/src/run/{issue.rs,pull/step.rs,affinity.rs}`, `afd_fleet/tests/{integration_run_issue_capacity.rs,integration_ledger_queue/ordering.rs}` | EDIT | §2 |
| `docs/architecture/runner_fleet.md` | EDIT | §2: the census (`:151`) and the candidate order (`:163`) |
| `af_wire/src/runner/**`, `afr_supervisor/src/**` (the heartbeat), `afd_runner/src/**`, `afd_db/schema/20261108123100_runner_runners.sql`, `afd_api_operator/src/handler/operator/runners.rs`, `cli/src/commands/platform/{runners,runner-schema}.ts`, `cli/test/**` | EDIT | §3; the runner protocol `VERSION` moves once |
| `deploy/local/{bootstrap.ts,bootstrap.test.ts,bootstrap.sh,acceptance.sh}`, `docs/development.md` | EDIT | §3's local `worker_count`; §4's local model and `ANTHROPIC_API_KEY` |
| `.github/workflows/acceptance-on-demand.yml`, `.orly/orly.json` (`burst`) | EDIT, with Indy's approval at build | §4: `ANTHROPIC_API_KEY` from `op://…/agentsfleet-anthropic-api-token/credential` in place of the Fireworks key |
| `rustd/guffbolt/src/tools/schema.rs`, `rustd/crates/runner/afr_agent/tests/providers/main.rs` | EDIT | §4: every tool's schema names the object type, and its test |
| `rustd/crates/protocol/af_observability/src/semconv/model-library-allowlist.json`, `rustd/guffbolt-ai/{catalog/gemini.json,README.md,src/providers/mod.rs}`, `scripts/model-catalogue/{catalogue,curated}.test.ts`, `scripts/model-catalogue/seed.integration.test.ts` | EDIT | §5 |
| `rustd/Cargo.toml`, `rustd/Cargo.lock`, member manifests | EDIT | §6 |
| `rustd/crates/protocol/af_wire/src/protocol/mod.rs`, `af_wire/tests/protocol.rs`, `docs/architecture/runner_protocol.md` | EDIT | the runner protocol moves to `agentsfleet.runner.v6` |
| `afd_fleet/src/run/issue.rs` | DELETE | split into `afd_fleet/src/run/issue/{mod,refused,refused_tests}.rs` (CREATE), under the length caps |
| `afd_fleet/src/run/{store.rs,deliver/issue.rs,pull/held.rs}` | EDIT | §2: the census read on refusal, and the re-ring |
| `afd_fleet/tests/{integration_run_issue,integration_run_issue_stop,integration_run_issue_refusals}.rs`, `afd_fleet/tests/integration_held_sandbox/expiry_race.rs`, `afd_fleet/tests/integration_ledger_queue/ordering.rs`, `afd_runner/tests/capacity/**` | EDIT | §2's tests and the refusal assertions |
| `afd_runner/src/heartbeat/host.rs`, `afr_supervisor/src/downlink/policy/host_tests.rs`, `afd_api/tests/integration_operator_runner_host.rs` | CREATE | §3: fact conversion and its tests |
| `afd_api_wire/src/operator.rs`, `afd_api/src/openapi/stability/mod.rs`, `afd_api/tests/integration_operator_runner.rs` | EDIT | §3: the operator view's `host`, marked beta |
| `af_wire/tests/durable_task_validation.rs`, `guffbolt-ai/src/models/tests.rs` | EDIT | Dimensions 1.7 and 5.3 |
| `docs/architecture/{run_flow,postgres_schemas}.md` | EDIT | §2 and §3 |
| `docs/v2/pending/M254_001_P2_INFRA_BROWSER_TOOLS_THAT_WORK_ON_LIGHTPANDA.md` | DELETE | replaced by the M259 specs |
| `docs/v2/pending/M258_002_P1_API_DOCS_INFRA_OBS_SAVES_PAUSE_ONLY_FOR_WHAT_CHANGED.md` | CREATE | the compute layer's checkpoints and snapshots |
| `docs/v2/done/M253_004_P1_API_DOCS_INFRA_GUFFBOLT_AI_PROVIDERS_FOR_GUFFBOLT.md` | EDIT | one dated note on where the browser work went |
| `~/Projects/docs`: `changelog.mdx`, `runners.mdx` | EDIT, own branch | the operator-visible changes: runner host facts and the decision profile without `done` |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR. **`domains/sql.md`** — SQL-FILE, SQL-LOCKCLOCK, TXN.
- **`domains/http-surface.md`** — HTTP-OPENAPI, HTTP-BREAK for the profile and the runner list. **`domains/logging.md`** — LOG-EVENT, LOG-FIELDS, LOG-MSG for `run_assignment_lost`.
- **`tests.md`** — TEST-NEG, TEST-REDGREEN, TEST-RACE, TCF. **`code.md`** — NLG, ORP, GRD.
- **`domains/architecture.md`** — ARCH-WINS, ARCH-LAND. **`languages/typescript.md`** — TS-SCHEMA.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | fix the code; `cargo shear` stays clean after §6 |
| `make test-unit-all` | yes | §1 and §2's unit tests, the CLI and bootstrap tests, the catalogue tests |
| `make test-integration-rustd` | yes | §1 and §2's live-Postgres tests |
| OpenAPI drift | yes | regenerate with `--no-banner` |
| `make check-version` | yes | the runner protocol `VERSION` moves once, for §3 |
| `make acceptance` (live, Linux, on demand) | not in a green PR | the prime suite on Haiku 5.5, on a burst machine, before close |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | `goal_results.rs` takes the goal-result tests out of `behavior/mod.rs` |

## Prior-Art / Reference Implementations

- **Reference:** `afd_fleet/src/run/unsent/mod.rs:56-59`, which publishes the fleet's wake after it releases an unsent run; §2's lost path does the same after `let_go`.
- **Reference:** the census statement `afd_runner/src/capacity/sql/census.sql`, read under the already-locked host row; §2's log reads it the same way.
- **Divergence:** none from the HTTP standard. The CLI keeps its six practices (SPEC-PRIOR).

## Sections (implementation slices)

Build order: §1 to §6, one commit each. §4's acceptance run needs §1's fixtures and §2's fixes to prove the prime fleets.

### §1 — Finish lines decide done

The runner and the daemon read a goal check by one rule. A task whose every statement scores a finite Noul at or above `statement_min_bps` completes, whatever `next_action` says. A task bound to a pull request still needs its native checks: while they are pending the task waits, and a red check blocks it.

Otherwise the next action decides: `CONTINUE` or `WAIT` at its own probability and confidence thresholds. `DONE` without its statements, `BLOCKED` or anything else reports nothing, and the task blocks as `completion_unverified`.

`GoalDecisionPolicy.done` is deleted; the type still denies unknown fields, so a stored profile that carries `done` fails the read.

This Pull Request also edits schema slot `20261108123100` in place, so `agentsfleetd migrate` refuses an existing database. Dev's database is torn down before merge, and the decision model is seeded again after the reset. The playbook's "Replace the decision model" serves a later change that edits no applied slot.

`stubborn-mule`'s follow-up asks for a task that stores and recalls `count-to-three` with one statement. `careful-squirrel`'s first message opens "Acknowledge this instruction in one line." and asks for nothing until a later message names a secret.

- **Dimension 1.1** — DONE — held statements complete the task → Test `statements_that_hold_complete_the_task_whatever_the_next_action_says` (unit, `afr_agent`): the dev checks, `CONTINUE` at statement 0.03, `CONTINUE` at 0.26, then `DONE` at probability 0.43, confidence 0.24, statement 0.97 → `Complete` after three action groups; `CONTINUE` at 1.0 with statement 0.97 → `Complete`.
- **Dimension 1.2** — DONE — untrusted results report nothing → Test `untrusted_goal_results_authorize_no_report` (unit, `afr_agent`): a stale record, an unknown record, a refused outcome, `WAIT` below its thresholds, an unknown choice, and `DONE` with a statement at 0.89 → no report, and the run stops with recovery.
- **Dimension 1.3** — DONE — a wait and a completion keep their proof → Test `complete_and_wait_reports_retain_native_goal_proof_and_private_history` (unit, `afr_agent`): `DONE` with statements at 1.0 → `Complete`; `WAIT` with statements at 0.0 → `Waiting`; both carry the native goal proof.
- **Dimension 1.4** — DONE — the daemon agrees → Test `a_completion_is_prepared_by_its_statements_alone` (integration, `afd_fleet`): a next action of `BLOCKED` 0.49 and `DONE` 0.38 with every statement at 1.0 → the completion prepares; a statement below its floor → `UZ-DT-007`.
- **Dimension 1.5** — DONE — no tool writes `done` → Test `refuses malformed decision definitions before HTTP` (unit, CLI, `cli/test/platform-decision.unit.test.ts`): a goal carrying `done` → refused before any request.
- **Dimension 1.6** — DONE — the bootstrap writes no `done` → Test `the decision model is created once and its key saved once in the platform workspace` (unit, `deploy/local/bootstrap.test.ts`): the created profile's goal is exactly `questions_revision`, `statement_min_bps`, `continue_action` and `wait`.
- **Dimension 1.7** — DONE — a stored `done` fails loud → Test `a_goal_policy_that_still_names_done_is_refused_by_name` (unit, `af_wire/tests/durable_task_validation.rs`): a profile with `goal.done` → the decode names the unknown field.
- **Dimension 1.8** — DONE — the live path → Test `prime-stubborn-mule.spec.ts` (e2e, `make acceptance`, fixture `stubborn-mule`): the task stores and recalls `count-to-three` and ends `done`.

### §2 — No turn waits behind a dropped hold

**Census.** `insert_run_with_turn.sql` drops the other fleets' live holds from the census (`:17-21`), so admission equals the runner's credits, which count runs only (`afr_supervisor/src/downlink/running.rs:73`). The runner already keeps one worker free of holds (`afr_supervisor/src/holds/keeper.rs:159-169`).

**State authority:** the host row, locked `FOR UPDATE`. This reverses `the_fleet_issuer_refuses_a_third_worker_and_transfers_its_own_hold_once` (`afd_fleet/tests/integration_run_issue_capacity.rs:127`) and `runner_fleet.md:151`.

**Re-ring.** On `assignment_lost`, after `let_go`, the daemon publishes the fleet's wake, as `release_unsent` does, so the lost turn returns as a hint.

**Age.** The ordinary order's last key becomes the head's age, oldest first, in place of `random()` (`select_ledger_candidates.sql:60`). Held, hint and locality keys stay ahead, because a refused pass never tries the next candidate. Age-first would let one refused head block every pass.

**Log.** `Runs::issue` returns a typed reason when it writes no run: `census_full`, `fenced`, `scope`, `host_unproven`, `host_closed`, `choice_stale`, `placement` or `task`. On a census refusal it reads the census in the same transaction through `afd_runner::capacity::lock`. `run_assignment_lost` logs `reason`, `occupied`, `held` and `worker_count`, and the "outlived" text goes.

- **Dimension 2.1** — DONE — a dropped hold refuses nothing → Test `a_hold_the_runner_evicted_does_not_refuse_its_free_worker` (integration, `afd_fleet/tests/integration_run_issue_capacity.rs`): `worker_count` 2; fleet A has one live run; fleet B's thread holds this runner with `updated_at = now()`; the heartbeat reconcile runs with no holds → `Runs::issue` for fleet C returns `Some`. On the parent commit, `None`.
- **Dimension 2.2** — DONE — live runs still fill the host → Test `the_fleet_issuer_refuses_a_third_worker_and_transfers_its_own_hold_once` (integration, `afd_fleet/tests/integration_run_issue_capacity.rs`): two live runs on `worker_count` 2 → a third fleet is refused `census_full` with the census it read; a fleet's own hold still transfers once.
- **Dimension 2.3** — DONE — a lost turn comes back first → Test `a_turn_whose_assignment_was_lost_is_offered_before_newer_sticky_work` (integration, `afd_fleet/tests/integration_ledger_queue/ordering.rs`): A's turn is refused while another run fills the host, then released; capacity frees; B, which last ran here, queues a newer turn → the next ordinary selection returns A's. On the parent commit, B's.
- **Dimension 2.4** — DONE — age breaks the tie → Test `fleets_that_tie_are_offered_oldest_turn_first` (integration, `afd_fleet/tests/integration_ledger_queue/ordering.rs`): five fleets queued in order with no hold, no hint and no locality → the runner is offered them in that order. With the random tie-break restored it fails.
- **Dimension 2.5** — DONE — the lost line names its guard → Test `a_lost_assignment_names_the_guard_that_refused_it` (unit, beside `afd_fleet/src/run/pull/step.rs`, `af_core::test_util::trace::Capture`): a census refusal → `run_assignment_lost` with `reason = "census_full"`, `occupied`, `held` and `worker_count`, and no "outlived" text.
- **Dimension 2.6** — DONE — another guard is told apart → Test `a_lost_assignment_names_the_guard_that_refused_it` (unit, the same test): a refusal that read no census → `reason = "host_unproven"`, no census fields and no ring. Both insert refusals, `census_full` and `fenced`, carry the census, because both read it.

### §3 — The runner reports its host

The heartbeat request gains `host`: CPUs, memory in bytes and free disk in bytes on the file system that holds the runner's sandboxes. **Implementation default:** read through `rustix`, already a workspace crate, and `std::thread::available_parallelism`, so nothing is hand-written.

The daemon keeps the latest report on the runner row, and the admin runner list and detail return it. `agentsfleet platform runners list` shows CPUS, MEMORY and FREE DISK. A fact the runner cannot read is absent and shows as unknown.

Dev's `worker_count` is then set from the facts. The local bootstrap's `worker_count: 4` (`deploy/local/bootstrap.ts:299`) is not set from facts in this Pull Request: the bootstrap registers the runner before its first heartbeat. Dropping that promise awaits Indy's ruling (asked Nov 26, 2026, unanswered). The count names its forcing limit: the sandbox's default memory limit, 2 GiB a worker (`runner_execution.md` §Limits: 64 workers reach 128 GiB), and `MAX_WORKERS`, 64.

- **Dimension 3.1** — DONE — the runner reports → Test `a_beat_carries_the_hosts_processors_memory_and_free_disk` (unit, `afr_supervisor/src/downlink/policy/host_tests.rs`): a heartbeat carries `host` with its processors, memory and free disk.
- **Dimension 3.2** — DONE — an unreadable fact is absent → Test `a_beat_carries_the_hosts_processors_memory_and_free_disk` (unit, the same test): a sandbox root the runner cannot read → no `free_disk_bytes`, a `runner_host_fact_unread` line, and the heartbeat still goes. Memory is absent off Linux.
- **Dimension 3.3** — DONE — the daemon keeps and serves it → Test `a_beats_host_facts_reach_the_operator_runner_view` (integration, `afd_api/tests/integration_operator_runner_host.rs`): a heartbeat's facts land on the runner row, and the operator's runner view returns them.
- **Dimension 3.4** — DONE — the CLI shows it → Test `renders each runner's processors, memory and free disk` (unit, `cli/test/platform-output.unit.test.ts`): `platform runners list` prints CPUS, MEMORY and FREE DISK, and "Not reported" for a runner with no facts; `platform runners show` prints the same three.
- **Dimension 3.5** — DEFERRED to after deploy (Indy's quote in Discovery) — dev's capacity follows its host → Test `dev_runner_capacity_set_from_its_facts` (manual): an operator reads `platform runners list` on dev and runs `platform runners update` with the count the facts support. Evidence: both outputs in the Pull Request Session Notes.

### §4 — Haiku 5.5 is the default, and Anthropic accepts its request

The runner's Messages request for `claude-haiku-5-5` is fixed so Anthropic answers 200. Anthropic requires every function tool's `input_schema` to name its type.

A tool taking untyped arguments generates `{}`, so `Schema::of` now states `"type": "object"` when the generated schema names no type (`guffbolt/src/tools/schema.rs`). Every wire sends a call's arguments as one JSON object, so the type is true of every tool.

The default moves in three places.

- **Dev:** an operator's `PUT /v1/admin/platform-keys` names `anthropic` and `claude-haiku-5-5`, an admin write, never a deployment (`docs/architecture/models.md:74`).
- **Local:** the bootstrap's provider and model become `anthropic` and `claude-haiku-5-5`, keyed by `ANTHROPIC_API_KEY`. Today they are Fireworks's MiniMax M3.1 Flash and `FIREWORKS_API_KEY` (`deploy/local/bootstrap.ts:11-12,147`).
- **CI:** acceptance resolves `op://${{ vars.VAULT_DEV }}/agentsfleet-anthropic-api-token/credential` in place of the Fireworks item (`.github/workflows/acceptance-on-demand.yml:111`; `.orly/orly.json:221`).

- **Dimension 4.1** — DONE — the request is accepted → Test `every_published_tool_is_offered_to_messages_as_an_object_schema` (integration, `afr_agent` providers suite, a fake Messages provider on a real socket): a run offering every published tool → every function tool's `input_schema` has `"type": "object"`. Without the fix it names `browser`, `browser_open` and `screenshot`.
- **Dimension 4.2** — DONE — local setup uses Haiku → Test `initial setup seeds Claude Haiku 5.5 with the machine's Anthropic key, and a missing key refuses before any write` (unit, `deploy/local/bootstrap.test.ts`): initial setup creates `anthropic/claude-haiku-5-5` and saves its key from `ANTHROPIC_API_KEY`; without the variable it refuses, naming it.
- **Dimension 4.3** — DEFERRED to after deploy (Indy's quote in Discovery) — dev's default is Haiku → Test `dev_default_model_is_haiku` (manual): an operator writes the platform key and model on dev, then sends one message to a new fleet. Evidence: the write's answer and the answered turn's model in the Session Notes.
- **Dimension 4.4** — DEFERRED to after deploy (Indy's quote in Discovery) — the prime fleets pass on it → Test `prime-morning-rooster.spec.ts`, `prime-careful-squirrel.spec.ts` and `prime-stubborn-mule.spec.ts` (e2e, `make acceptance SUITE=all`): each passes with Haiku 5.5 as the platform model.

### §5 — The catalogue is current

The allowlist moves to the rows verified on Nov 26, 2026. Fable 5.2, GPT-6.2 Sol, GPT-6.2 Astra and Qwen4 Max replace their predecessors, and a Mistral promotion ends. Gemini 4 Carbon joins as a native row, for 21 models: 4 Messages, 4 Responses, 11 Chat and 2 Gemini.

Both Gemini Argon rows, native and routed, and the new Carbon row cap output at 65,536, Google's per-request limit. guffbolt-ai's Gemini list adds `gemini-4-neon`, raises Argon and Carbon to 65,536, and drops the agent, Live, image and shut-down models.

- **Dimension 5.1** — DONE — 21 curated rows → Test `membership and safe diagnostic names come from the allowlist` (unit, `scripts/model-catalogue/curated.test.ts`): the document holds 21 models.
- **Dimension 5.2** — DONE — Gemini output is capped → Test `every Gemini 4 row states Google's 65,536-token output limit, not its context window` (unit, `scripts/model-catalogue/curated.test.ts`; against the old allowlist it reads 943,718): every row whose model is Gemini 4 has `max_output_tokens` 65,536.
- **Dimension 5.3** — DONE — guffbolt-ai's list is Google's → Test `the_gemini_list_serves_neon_and_caps_gemini_4_output` (unit, `guffbolt-ai/src/models/tests.rs`): `gemini-4-neon` resolves; `gemini-2.5-computer-use-preview-10-2025` and `gemini-3.1-flash-live-preview` do not; Argon's output is 65,536.
- **Dimension 5.4** — DONE — dev reads the new rows → Test `dev_catalogue_applied` (manual): the playbook's diff and apply on dev, then `agentsfleet platform models update` for the Argon rows, because the apply keeps saved inference (`scripts/model-catalogue/sql/upsert_models.sql:17`). Evidence: the diff, the apply and the update in the Session Notes.

### §6 — Every crate is current

Every workspace dependency moves to its newest release that the lanes pass on. Compatible releases come from `cargo update`; a major release and each exact pin is checked against its changelog. The pins are `aws-sigv4`, `aws-credential-types`, `image`, `rig-core`, `tokio-quiche`, `async-nats` and `libc` (`rustd/Cargo.toml:507-508,619,635,759,762,1011`), and the patched `datagram-socket` (`:1369-1370`).

- **Dimension 6.1** — DONE — every pin is checked → Test `crate_pins_checked` (manual): the table below lists each pin and breaking release, its newest version and the decision, with the reason a pin stays.
- **Dimension 6.2** — DONE — the new lockfile passes → Test `every_published_tool_is_offered_to_messages_as_an_object_schema` (integration, `afr_agent` providers suite): rig-core 0.47.1 encodes a run offering every published tool, and the S rows pass on the final tree.

Read from the crates.io index on Nov 26, 2026, over all 162 workspace dependencies:

| Crate | Ours | Newest | Decision |
|---|---|---|---|
| `rig-core` | `=0.47.0` | 0.47.1 | Moves to `=0.47.1`. Its only `rig-core` entry is the model catalog refresh (Gemini 4 Neon, GPT-6.2); every other entry in the release is another rig crate (release notes, `v0.47.1`). |
| `libc` | `=0.2.189` | 0.2.191 | Stays: `quiche` 0.30.1 and `tokio-quiche` 0.23.0, both newest, require `libc` below 0.2.190. |
| `parking_lot` | 0.12.5 | 0.13.0 | Stays: its `parking_lot_core` 0.10 needs `libc` 0.2.191. |
| `gix` | 0.90.0 | 0.91.0 | Stays: its `gix-hashtable` 0.19 takes `parking_lot` 0.13. |
| `aws-sigv4`, `aws-credential-types`, `image`, `tokio-quiche`, `async-nats` | exact pins | the same | Stay: each pin is the newest release. |
| `datagram-socket` (patched) | 0.8.1 | 0.8.1 | The patch stays. |
| 24 others | compatible | newer | `cargo update`. |

## Interfaces

**Decision profile.** `GoalDecisionPolicy` is `{questions_revision, statement_min_bps, continue_action, wait}`; `done` is removed, and an unknown field is refused. `statement_min_bps` decides done.

**Runner wire.** `HeartbeatRequest` gains a required `host: {cpu_count?, memory_bytes?, free_disk_bytes?}`. `VERSION` moves once, to `agentsfleet.runner.v6`.

**Admin route.** `GET /v1/admin/runners` and the runner detail gain `host`, the last facts that passed their bounds, marked beta.

**Log.** `run_assignment_lost` carries `reason` and, for `census_full`, `occupied`, `held` and `worker_count`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Untrusted goal result | security | a stale, unknown or refused record claims completion | no report; the run stops with recovery | `untrusted_goal_results_authorize_no_report` |
| Statement short | failure | one statement under the floor while `DONE` is claimed | refused `UZ-DT-007` | `a_completion_is_prepared_by_its_statements_alone` |
| Stored `done` | failure | a profile written before this change | the read fails loud; the playbook replaces the profile | the Dimension 1.7 test |
| Stale hold | capacity | the daemon counts a hold the runner dropped | the census no longer counts holds | `a_hold_the_runner_evicted_does_not_refuse_its_free_worker` |
| Lost turn starves | capacity | a refused turn sorts last with no pass owed | its wake is re-rung; it returns as a hint | `a_turn_whose_assignment_was_lost_is_offered_before_newer_sticky_work` |
| Loss loop | capacity | the census and the runner's credits disagree until the next heartbeat | only a refusal another pass can clear rings (`census_full`, `fenced`, `choice_stale`); the rest stop | the Dimension 2.6 test, and `stale_native_grants_cannot_survive_rotation_or_supersession_at_the_host_lock` (e2e), which failed while every refusal rang |
| Unreadable host fact | failure | the runner cannot read disk or memory | that fact is absent; the heartbeat goes | the Dimension 3.2 test |
| Missing key | failure | no `ANTHROPIC_API_KEY` for initial local setup | the bootstrap refuses, naming it | the Dimension 4.2 test |

## Invariants

1. A completion is reported only when every statement holds — `statements_hold` decides in `afr_agent/src/decision/goal.rs` and `authorize.rs` alike (Dimensions 1.1 and 1.4).
2. No profile carries a `done` threshold — the field is gone and `deny_unknown_fields` refuses it (Dimensions 1.5 and 1.7).
3. Admission matches the runner's credits — the census counts live runs only (Dimension 2.1).
4. A lost assignment leaves a wake behind — the lost path publishes it after `let_go` (Dimension 2.3).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `run_assignment_lost` (log, changed) | operator | `Runs::issue` writes no run | `runner_id`, `fleet_id`, `agentsfleet_turn_id`, `fencing_token`, `reason`, `occupied`, `held`, `worker_count` | no message text | `a_lost_assignment_names_the_guard_that_refused_it` |
| runner `host` facts | operator | each heartbeat | `cpu_count`, `memory_bytes`, `free_disk_bytes` | no path, no hostname beyond the runner's `host_id` | the Dimension 3.3 test |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Held statements complete a task, and untrusted results report nothing (§1) | `make test-unit-all` | exit 0; `statements_that_hold_complete_the_task_whatever_the_next_action_says ... ok`, `untrusted_goal_results_authorize_no_report ... ok` | P0 | |
| R2 | The daemon authorizes a completion by its statements alone (§1) | `make test-integration-rustd` | exit 0; `a_completion_is_prepared_by_its_statements_alone ... ok` | P0 | |
| R3 | A dropped hold refuses nothing, and a lost turn comes back first (§2) | `make test-integration-rustd` | exit 0; `a_hold_the_runner_evicted_does_not_refuse_its_free_worker ... ok`, `a_turn_whose_assignment_was_lost_is_offered_before_newer_sticky_work ... ok` | P0 | |
| R4 | The lost line names its guard and the census (§2) | `make test-unit-all` | exit 0; `a_lost_assignment_names_the_guard_that_refused_it ... ok` | P1 | |
| R5 | The runner's host facts reach `platform runners list` (§3) | `make test-unit-all` | exit 0; `a_beat_carries_the_hosts_processors_memory_and_free_disk ... ok`, `renders each runner's processors, memory and free disk` passed | P1 | |
| R6 | The prime fleets pass on Haiku 5.5 (§1, §4) | `make acceptance SUITE=all` | exit 0; `prime-stubborn-mule.spec.ts`, `prime-careful-squirrel.spec.ts` and `prime-morning-rooster.spec.ts` passed | P0 | |
| R7 | The catalogue holds 21 current rows (§5) | `make test-unit-all` | exit 0; `membership and safe diagnostic names come from the allowlist` passed | P1 | |
| R8 | No `done` threshold is left (§1) | `git grep -n -e "goal\.done" -e "policy\.done" -e "done: threshold" -e "pub done: DecisionThreshold" -- rustd cli/src deploy scripts` | 0 matches | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the `done` threshold | `git grep -n -e "goal\.done" -e "policy\.done" -e "pub done: DecisionThreshold" -- rustd` | no output |
| the wrong lost text | `git grep -n -e "the pass outlived its assignment; no run was written" -- rustd` | no output |
| the random tie | `git grep -n -w random -- rustd/crates/daemon/afd_fleet/src/run/sql/run/select_ledger_candidates.sql` | no output |

**Every hit of the `done` threshold at `576d0b462`, for SPEC-BLAST.** Production: `af_wire/src/decision/config.rs:45`, `afr_agent/src/decision/goal.rs:52`, `afd_fleet/src/durable_task/decision/authorize.rs:90`, `cli/src/commands/platform/decision-schema.ts:9`, `deploy/local/bootstrap.ts:190`, `public/openapi.json:24204,24211`. Tests and fixtures: `afd_admin/src/test_util.rs:102`, `afd_admin/tests/decision_model/fixture.rs:76`, `afd_admin/tests/integration_decision_models.rs:243`, `afd_api_operator/src/handler/admin/models/input/tests.rs:226`, `afd_fleet/src/durable_task/decision/prepare_tests.rs:47`, `afr_agent/src/fixture/mod.rs:93`, `afr_supervisor/src/run_loop/tests/recovery/fixture.rs:32`, `afr_supervisor/src/verbs/tests/decision/mod.rs:44`, `runner/main/examples/kernel_lane/supervisor_recovery/wire.rs:25`, `cli/test/platform-decision.unit.test.ts:8`, `scripts/model-catalogue/decision.integration.test.ts:84`. The next action's `"DONE"` option stays: it is the model's answer, which no longer completes a task by itself.

## Out of Scope

- The compute layer (M258) and the web, browser, prompts and images (M259): their own specs, pending in this Pull Request.
- The five other catalogue rows that reserve 943,718 output tokens (four Fireworks rows and `meta/muse-wonder`): they need their providers' limits read first.
- Gemini's `store` and `retryDelay` fixes, and a real stop reason in place of `cut` (Gemini review §6).
- Sequencing holds on the wire so the reconcile can trust the runner's immediate beat.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a person starts a task on a new fleet, and it ends `done` once its one finish line holds, on the default Haiku 5.5.
2. **Preserved user behaviour** — waits, continuations, blocks, chat turns, holds and the runner list keep working.
3. **Optimal-way check** — one rule for done on both sides, a census of runs, one wake and one ordering key. The gap was a judge that could veto finished work.
4. **Rebuild-vs-iterate** — iterate: each fix is local to the statement, the query or the payload it names.
5. **What we build** — the statement rule; the census and ordering fixes with their log; the host facts; the Haiku default and its fix; the catalogue rows; the crate updates.
6. **What we do NOT build** — a `done` threshold kept as an option, which Indy ruled out. Age-first ordering, since one refused head would block every pass. Automatic `worker_count` changes: an operator sets it.
7. **Fit with existing features** — it compounds with durable tasks, holds and the model catalogue. It must not destabilize a fleet's own hold transfer, which `integration_run_issue_capacity.rs` keeps.
8. **Surface order** — runner and daemon first, then the CLI's list, then the defaults.
9. **Dashboard restraint** — no UI change; the runner facts show in the CLI only.
10. **Confused-user next step** — a blocked task's card names `completion_unverified`; an operator reads `run_assignment_lost`'s `reason`; a refused profile names its field, and the playbook replaces it.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one Section per ruling, ordered so the acceptance run proves the earlier ones.
- **Alternatives considered:**
  - The runner keeping holds to `workers - busy`: declined, as a hold would reserve a worker for up to 600 seconds.
  - Sequencing holds on the wire: declined, as the census fix removes the race.
  - Age-first ordering: declined, as a refused pass never tries the next candidate.
  - Keeping `done` as a stricter optional gate: declined by Indy's ruling.
- **Patch-vs-refactor verdict:** a **patch** because each change edits one rule, statement or default in place.

## Discovery (consult log)

- > Owner (Indy, Nov 26, 2026): "Finish lines decide done" — context: every completion statement at `statement_min_bps` completes the task; the next action only chooses `CONTINUE`, `WAIT` or blocked; the unused `done` threshold is removed.
- > Owner (Indy, Nov 26, 2026): "I agree on those 3" — context: the census counts live runs only, a lost assignment re-rings the wake, and an age term joins ordinary ordering; logging the census on `run_assignment_lost` was added with them.
- > Owner (Indy, Nov 26, 2026): "4 is very less availability" — context: the runner reports host CPU, memory and free disk on its heartbeat, `platform runners list` shows them, and dev's `worker_count` is set from them.
- > Owner (Indy, Nov 26, 2026): "make haiku as the default model" — context: Claude Haiku 5.5, blocked by Anthropic's 400 on the runner's Messages request; the fix is in this Pull Request.
- > Owner (Indy, Nov 26, 2026): "update and check all the crates and push in this PR" and "everything you do must go in 1 PR" — context: §6, and the M258 and M259 specs travel in this Pull Request.
- > Owner (Indy, Nov 26, 2026), recorded in the M259 specs: "Essentially add missing features for guffbolt agent harness", "max 2 in 1 milestone for the guffbolt tool updates", "I need the final scope to fix", "Hybrid", "Does parallels offer search too", "I think i wanna take the call to use like amp for websearch and avoid hybrid?", "Allow it now", "look for the updated lightpanda", "i am thinking to move to the firecracker spec as well", "Why do you need screenshot", "In web and the cli, since if its api then allow both", "i think though this is billable, it must be convered to the platform runtime cost?" and "when will you have this usecase? i am leaning towards a and b".
- **Consults** — the dev incident report (Nov 26, 2026) traced fleet `…17ab`'s second message. It found 17 `run_assignment_lost` events, 15 of them at 3 live runs, and 66 assign passes in the wait. The census had counted holds the runner evicted.
- **The report's tests and gaps** — it names the three regression tests §2 carries. It left `worker_count` 4 unverified; `deploy/local/bootstrap.ts:300` assigns 4 locally, and dev's row is gone.
- **Evidence for §1** — the dev goal check in `statements_that_hold_complete_the_task_whatever_the_next_action_says` (this branch, `goal_results.rs`) and the muddled next action in `goal_evidence.rs` (`BLOCKED` 0.49, `DONE` 0.38) are the shapes dev produced.
- **Evidence for §4** — the dev turn `01a2168e-8d7f-7295-9cdc-bb19572547ed` (Nov 26, 2026, 06:32:50 UTC): status `fleet_error`, `wall_ms` 140, "the model provider refused the turn with status 400 (invalid_request_error)".
- **Corrections** — the local default model today is Fireworks's MiniMax M3.1 Flash (`deploy/local/bootstrap.ts:11-12`), not a Claude model; CI acceptance supplies only `FIREWORKS_API_KEY` (`acceptance-on-demand.yml:111`). The catalogue patch caps the Gemini rows only; five other rows keep 943,718 (Out of Scope).
- **Model reviews** — none.
- **Open blockers** — none.
- **Deferrals and transfers** — Dimensions 3.5, 4.3 and 4.4 are proven after this Pull Request deploys, because each needs the deployed runner or dev's live data.
  - > Owner (Indy, Nov 26, 2026): "Okay yes i approve that" — context: a pull-request task whose completion statements hold while its checks are pending waits for them rather than being blocked.
  - > Owner (Indy, Nov 26, 2026): "Close now, prove after deploy" — context: his answer when asked whether to close this spec now and prove the three post-deploy Dimensions after deploy.
