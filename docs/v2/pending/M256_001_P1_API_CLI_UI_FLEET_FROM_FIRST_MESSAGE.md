<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M256_001: A fleet is a chat from its first message, with no setup and no Definition

**Prototype:** v2.0.0

**Milestone:** M256

**Workstream:** 001

**Date:** Nov 24, 2026

**Status:** PENDING

**Priority:** P1, a new person meets a private draft, a setup card and a Save before a fleet does any work

**Categories:** API, CLI, UI

**Batch:** B1, alone; the design is ruled, and nothing is built until Indy approves this report (§1)

**Depends on:** M255_001 (merged as #862), whose Dimension 1.1, the rulings on `docs/architecture/schema_convergence.md`, closed here on Nov 24, 2026

**Provenance:** LLM-drafted (Claude Fable 5.2, Nov 24, 2026) from Indy's brief and rulings and nine read-only source traces at `3fbd9e40e`, re-checked at `d79e88a46`. A codebase reviewer opened every `path:line` below; its corrections are applied (Discovery).

**Canonical architecture:** `docs/architecture/thread_model.md`; `docs/architecture/direction.md`, `fleet_definitions.md` and `user_flow.md` are rewritten by this spec (ARCH-WINS, ARCH-LAND)

---

## Overview

**Goal (testable):** A person's first message creates an active fleet and its first turn in one transaction, the fleet works with nothing set up, and no draft, review, Save, Definition or revision exists in the schema, the wire or either client.

**Terms.** W is `/v1/workspaces/{workspace_id}`, F is `W/fleets/{fleet_id}`, D is `W/drafts/{draft_id}`, L is `/v1/runners/me/runs/{run_id}`. Rust paths that start with a crate name are under `rustd/crates/<layer>/`; app paths are under `ui/packages/app/`.

- A **turn** is one input on a fleet's thread, and a **run** is one try at answering it.
- The **Definition** is today's stored setup: a name, instructions and 14 runtime keys (`af_fleet_runtime/src/config/mod.rs:65-80`), one immutable row per **revision** in `fleet.revisions`.
- A **grant** is a row a person adds to give one fleet one credential: a workspace secret, an integration such as GitHub, or the right to steer other fleets.
- A **trigger** is a row that lets something outside wake a fleet: a webhook, a GitHub App event or a Slack mention. A **schedule** is a cron row. The app lists both as "Wakes on".
- A **durable task** is one accepted objective a fleet works on across several runs, judged done against its completion questions.
- **A person** on a route is a signed-in caller, not an API key (`afd_http/src/auth/person.rs`). **A person's message** is a turn whose actor is `steer:<subject>`, never `steer:api`.

**Problem:** Read from source.

- **Two conversations and two ids.** The setup chat lives on a draft's thread (`afd_setup/src/sql/draft/create.sql:3-18`). Save mints a new fleet id with a new thread (`afd_setup/src/creation/chat_save.rs:133`, `creation/sql/insert.sql:3-16`), so the fleet's chat opens empty.
- **The setup chat does no work.** Its run has one tool, `propose_setup`, no sandbox and no network (`afd_fleet/src/run/draft/payload.rs:122-141`).
- **A ceremony before work.** A draft is private to its creator (`afd_setup/src/sql/draft/lock.sql:8`), is reviewed and saved against a digest (`afd_api_wire/src/fleet/input.rs:48-62`), waits in `installing` for grant cards (`afd_setup/src/creation/activate.rs:77-92`), and expires 30 days after it closes (`afd_setup/src/creation/mod.rs:88`).
- **A document someone must write.** A run is refused without a Definition, a revision and a model (`afd_fleet/src/run/installed.rs:164-193`), and is offered only the tools the Definition lists (`docs/architecture/capabilities.md:23`).

**Solution summary:** A fleet becomes a name, a model, a thread, its memory, its grants, its triggers and its schedules. `POST W/fleets` writes the fleet and its first turn. A run is offered the tool catalog. A person attaches a secret or an integration to a fleet, and a run holds exactly what is attached. The fleet adds its own schedules and triggers by tool, and starts a durable task by tool when a person's message asks for work that must keep going. Standing guidance is the fleet's pinned memory, which every run's prompt carries. Each fleet has a network toggle, private by default. The draft, the Definition and its revisions, the setup run kind, the review, Save, `installing`, `/setup`, the per-fleet budget, the event-gate rules and the grant approval card are deleted.

**Design flow:**

```
TODAY   first message ─► POST W/drafts ─► private draft + its own thread ─► setup run (propose_setup only)
        Save ─► POST W/fleets {draft, review_digest} ─► NEW fleet id + NEW thread, status installing
        grant cards + readiness checks ─► active ─► POST F/messages ─► the first real run, on an empty chat

AFTER   first message ─► POST W/fleets {message, operation_id, model_ref?, name?}
          ONE transaction: fleet.fleets(active) + thread.threads(id = fleet id) + thread.turns #1 queued
        run        the tool catalog, the fleet's memory, the secrets and integrations attached to this fleet
        access     the fleet says "I have no access to Sentry" ─► a person adds it under "Can access" ─► POST F/grants
                   ─► the next run holds it; each secret is sent only to its own host
        wakes on   "check every 15 minutes" ─► cron_add (exists) ─► fleet.schedules
                   "wake on Sentry alerts"  ─► trigger_add ─► fleet.triggers ─► the fleet page shows the URL
        guidance   "always link the evidence" ─► memory_store, pinned (exists) ─► in the system prompt of every later run
        task       "fix the null-cart error until PR 88 is green" ─► the fleet's model calls task_start ─► the daemon binds the task
                   ─► after each group of actions the decision model judges the task's own done-questions
```

**Use case, "watch checkout errors" typed into a new fleet:**

| Step | Rows written | What the runner does |
|---|---|---|
| The person sends it | `fleet.fleets` (`active`), `thread.threads`, `thread.turns` #1 (`queued`, actor `steer:<person>`) | nothing yet; the daemon publishes the wake |
| Run 1 | `thread.runs`, `thread.items`; `memory.entries` when it stores what it learned | works in its sandbox, reads what its runner lets it reach, asks where the errors live, and says it has no access to Sentry |
| The person adds the `sentry` secret under "Can access" | `fleet.grants` (kind `secret`), `fleet.fleets.policy_version` + 1, an `access_changed` activity turn | nothing; the next run's `secrets_map` holds it |
| "check every 15 minutes" | `fleet.schedules` (source `fleet`) | calls `cron_add` |
| The cron fires | `thread.turns` (actor `cron:<schedule>`), `thread.runs` | reads Sentry and reports |
| "wake me on Sentry alerts" | `fleet.triggers` (kind `webhook`, signed with a secret from the workspace vault) | calls `trigger_add`; the page shows the URL to paste into Sentry |
| "fix the null-cart error until the checks on PR 88 pass" | the turn's `plan`, `fleet.durable_task_items`, `fleet.durable_tasks` with its own questions; then one `runner.decision_calls` row per goal check | the model calls `task_start` with the objective, the target and its done-questions; the run carries on as the task's first run |

"Watch" is a schedule or a trigger. A durable task is a bounded objective: it has no timer wake, and its default limits are 7 days and 12 runs (`af_wire/src/durable_task/limits.rs:27-34`).

## Rulings and the defaults that wait for approval

**Ruled by Indy on Nov 24, 2026** (quotes in Discovery).

| # | Ruling |
|---|---|
| Scope | The ceremony and the record are both deleted in M256_001, in one Pull Request. |
| D1, D2 | Nobody writes a setup. `/setup` and the chat behind it are deleted. |
| D3 | A fleet starts with nothing. The payer's balance is the only ceiling on its spend. |
| Access | A person attaches a secret or an integration to a fleet. A run holds only what is attached. |
| Wakes on | The fleet adds its own triggers by tool, as it adds schedules, with no limit beyond what the workspace has connected. |
| Network | Each fleet has an "Allow all" and a "Packages" toggle, private by default. The runner's posture stays as the operator's ceiling. |
| Steering | Any fleet may steer any fleet in its workspace. |
| Gates | The per-fleet rule for events that wait for a person is deleted. |
| Tasks | The fleet's own model decides when a message becomes a task and writes its completion questions. The decision model only judges whether it is done. A later message joins a running task through the fleet's model too. A task follows the fleet's access at each run. No tenant route creates one (D4). |
| Wallet | A payer with no wallet row is refused. |
| Memory | Standing guidance lives in the fleet's memory. Pinned entries are put into every run's prompt, under a short fixed text. Read-only network access is dropped. |
| D5 | The schema folds, fourteen tables to eight, are built as M256_002. |

**What gets looser or different.** Each line is a behaviour change this report asks Indy to accept.

1. Every fleet is offered the 32 published tools and the seven sub-agent tools (`afr_tools/src/catalog/mod.rs:119`), so every run prepares a sandbox. `notify`, `git` and `message` fail until their credential is attached.
2. Any fleet may read and steer any fleet in its workspace, as often as the payer's balance allows. A fleet with no access can drive one that holds access.
3. A fleet may add any webhook, GitHub subscription or Slack channel the workspace has connected. A hostile page a run reads can ask for the same.
4. A fleet set to "Allow all" can post what it has read to any public host. On an `allow_all` runner the sandbox shares the host's network whatever the fleet's toggle says, as today. A stored secret is still sent only to its own `host` field (`af_egress/src/policy/mod.rs:219-241`).
5. No fleet has a budget. A fleet that schedules itself can spend the payer's whole balance.
6. A fleet knows its job only from what it pinned in memory and from the conversation. Today the Definition's instructions are the system prompt (`afr_agent/src/prompt/mod.rs:41-52`) and memory is reached only by a recall (`docs/architecture/memory.md:73-75`). Pinned entries spend prompt space on every run.
7. Read-only is gone. A fleet may write with any credential attached to it, where a Definition could hold it to reads.
8. One model starts a task, where today two must agree (`afr_agent/src/decision/routing.rs:80-106`), and it writes the done-questions the judge then uses. A person may edit them. A later message is linked to a running task by that same model, with no probability bar, where today the link needs 0.95 (`docs/architecture/goal_evaluation.md:383-389`).
9. A fleet that has a task no longer loses its tools on plain questions (`afr_agent/src/loop/mod.rs:111-118`).
10. Approving and denying disappear. With the gate rules and the grant card gone, the only thing a person answers is a run's question. unverified: that nothing else writes an approval gate; the access trace found three writers (`afd_gate/src/gate/park.rs`, `afd_fleet/src/run/commit/question.rs`, the grant request).
11. A removed grant leaves a live run with the secret it was issued. The next run and the next token mint do not have it.

### A1 — A later message reaches an unfinished task through the worker's tool. RULED

Today the decision model's association answer links a person's later message to a task. With the worker deciding, it is shown the fleet's unfinished tasks and calls `task_input` to add the message to one, `task_start` for new work, or neither. The daemon's part stays: it records the input under the task's generation, cancels a wait and queues the continuation (`afd_fleet/src/durable_task/accept/input.rs`). Declined: a decision call on each person's message while a task is unfinished, and a task the person selects.

### A2 — The toggles' defaults and who flips them. RULED: both off; a signed-in person flips them

"Allow all" and "Packages" both start off. A signed-in person with `fleet:write` flips them; an API key cannot. A private fleet's HTTP tools reach the hosts of its attached secrets, GitHub when attached, and the package registries when "Packages" is on. The sandbox follows the runner's posture as today, with those same hosts added on an allow-list runner.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(fleet): a fleet is a chat from its first message; setup, the Definition and revisions are gone
- **Intent (one sentence):** A person starts a fleet by saying what they want and keeps talking to it, and what a fleet may touch is whatever a person attached to it.
- **Authoring handshake:** Indy wants authoring gone cleanly, with no optional setup left behind, in one milestone. ASSUMPTIONS I'M MAKING: 1. Nothing is backward compatible: routes, wire and schema are edited in place, and the dev database is torn down at merge. 2. One Pull Request, built as eight gated commit checkpoints, one per Section. 3. M256_002 builds the fourteen-to-eight folds on the schema this spec leaves.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_gate/src/policy/build/mod.rs` — the one place a run's policy is assembled; §2 changes its inputs from a config to a fleet row and its grants.
2. `rustd/crates/daemon/afd_credential/src/secrets/mod.rs` — `declared` loads named secrets from one workspace's vault; §3 passes it the attached names.
3. `rustd/crates/daemon/afd_api_runner/src/handler/runner/schedule.rs` — a fenced run verb that writes a wake-up row with no Definition; §4's trigger verbs mirror its fields.
4. `rustd/crates/daemon/afd_fleet/src/durable_task/bindings/mod.rs` — the task binding and what it reads from a revision; §5 removes each read.
5. `rustd/crates/daemon/afd_turns/src/sql/admit_one.sql` — how a message becomes a queued turn; §2's first turn is written the same way.

## Files Changed (blast radius)

Counts are from the worktree at `d79e88a46`, production files with tests apart unless a row says otherwise.

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M256_001_P1_API_CLI_UI_FLEET_FROM_FIRST_MESSAGE.md` | EDIT | this spec; moves to `active/`, then `done/` |
| `docs/architecture/*.md` and `docs/architecture/scenarios/*.md` (40 pages name a Definition, draft, revision or setup), `playbooks/operations/slack_incident_drill/001_playbook.md` | EDIT/DELETE | the design as ruled, in a doc-only commit before the code; `fleet_definitions.md` is deleted |
| `rustd/crates/daemon/afd_setup/**` (227 tracked files; 164 production files, 10,975 lines) | DELETE/EDIT | the crate shrinks to the fleet row's create, patch, delete and read (`creation/{mod,read,patch,delete}.rs`), the model list (`catalogue/**`), the compaction store (`context/**`) and the replay receipts (`replay/**`). Everything a draft, a review or an activation owned goes |
| `rustd/crates/protocol/af_fleet_runtime/**` (6,763 production lines) | DELETE/EDIT | the crate shrinks to `name/` and the trigger checks (the webhook verifier, the cron check, the channel id). `definition/**`, `setup/**` and the rest of `config/**` go |
| `rustd/Cargo.toml`, `rustd/Cargo.lock`, the 18 member manifests naming either crate, `.ast-grep/rule-tests/rust-one-agent-loop-test.yml` | EDIT | dependencies dropped as modules go; `cargo shear` stays clean |
| `rustd/crates/daemon/afd_db/schema/*_{setup_drafts,fleet_revisions}.sql`, `20261108120000_schemas.sql`, `20261108120100_roles_and_privileges.sql` | DELETE/EDIT | §8: the two tables, the `setup` schema and its grants |
| `rustd/crates/daemon/afd_db/schema/*_{fleet_fleets,thread_threads,thread_turns,thread_runs,fleet_durable_tasks,runner_checkpoints,fleet_schedules,connector_integration_grants}.sql`, a new `*_fleet_triggers.sql`, `afd_db/src/migration.rs`, `afd_db/src/test_util/fleet/**` | EDIT/CREATE | the Schema table in Interfaces |
| `rustd/crates/daemon/afd_gate/src/**` (4 files hold `FleetConfig`: `policy/build/mod.rs`, `policy/shape.rs`, `gate/first.rs`, `gate/pass.rs`) | EDIT/DELETE | §2: the policy is built from the fleet row and its grants. §8: the rule parks and `policy/grants.rs` go |
| `rustd/crates/daemon/afd_fleet/src/run/**` (crate-wide: 22 files hold `draft_id`, 19 `head_revision_id`, 8 `setup_revision_id`) | EDIT/DELETE | §2, §8: assignment, issue, payload, checkpoint guard, mint scope, the fleet budget's gate and ceilings, the ungranted park; `run/draft/**` (1,188 lines) is deleted |
| `rustd/crates/daemon/afd_fleet/src/durable_task/**` (27 files hold `accepted_revision_id`) | EDIT/DELETE | §5: the task verbs replace the binding's intake; per-task questions and the constant wording; `adoption.rs`, the message and item assessments and their authorization are deleted |
| `rustd/crates/daemon/afd_ingress/src/{binding,app,slack}/**`, `afd_ingress/src/sql/**`, `afd_api_ingress/src/handler/webhook/**` | EDIT | §4: routing reads `fleet.triggers`; the approval webhook goes in §8 |
| `rustd/crates/daemon/afd_cron/src/**` (`recipe/**` is 10 files; 29 production files name a recipe) | DELETE/EDIT | §4: trigger-sourced rows, recipes and overrides go |
| `rustd/crates/daemon/afd_approval/src/**`, `afd_credential/src/**`, `afd_billing/src/{budget.rs,nanos.rs,sql/select_tenant_charges_*.sql}` | EDIT/DELETE | §3: grants are rows a person writes; the request, the card and approve/deny go. §2: the fleet budget goes |
| `rustd/crates/daemon/afd_api_tenant/src/handler/{draft/**,fleet/**,schedule/**,approval/**,turn/stop.rs,grant.rs}`, `afd_api_runner/src/handler/runner/**`, `afd_http/src/{route,handler,services,openapi}/**`, `afd_api/src/lib.rs`, `afd_api/tests/**`, `afd_api_wire/src/**`, `afd_fleet_lifecycle/src/**`, `afd_turns/src/**`, `rustd/crates/foundation/af_core/src/{error_code,problem}/**`, `public/{openapi.json,llms.txt,agentsfleet-manifest.json}` | EDIT/DELETE | the Routes table in Interfaces and its codes; `handler/draft/**` (1,106 lines) is deleted |
| `rustd/crates/protocol/{af_wire,af_observability}/src/**`, `rustd/crates/harness/af_egress/src/**`, `rustd/crates/foundation/af_validate/src/**`, `rustd/crates/runner/{afr_supervisor,afr_agent,afr_tools,main}/**` | EDIT/DELETE | §2 to §5: the payload, the trigger and task verbs and tools; `afr_agent`'s task intake (`decision/{interpretation,routing}.rs` and the intent half of `decision/mod.rs`), the setup tool, its loop and the draft branch go |
| `rustd/crates/daemon/{afd_bench,afd_gateway,afd_runner,main}/**`, `rustd/crates/e2e/af_e2e/tests/**` | EDIT | the other holders of a draft id, a revision id or `installing`, and their tests |
| `ui/packages/app/components/domain/setup/**` (1,414 lines), `ui/packages/app/lib/api/draft*.ts` (29 files, 2,063 lines), `ui/packages/app/lib/api/setup-proxy/**`, `ui/packages/app/lib/{setup-requirements.ts,api/fleets.ts,api/tasks.ts}`, `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/**`, `ui/packages/app/components/{layout,domain/conversation,domain/models,domain/tasks,domain/approvals,domain/schedules}/**`, `ui/packages/app/public/fleet-starters.json`, `ui/packages/app/tests/**` | DELETE/EDIT | §6 |
| `cli/src/commands/chat/**` (1,399 lines), `cli/src/program/tree/*.ts`, `cli/src/commands/{fleet,schedule}/**`, `cli/src/lib/api-paths.ts`, `cli/test/**` | DELETE/EDIT | §7 |
| `.github/workflows/test-unit-rustd.yml` | EDIT, with Indy's approval at build | its miri step names `afd_setup` modules that go |
| `~/Projects/docs` pages and `changelog.mdx` | EDIT, on that repository's own branch after merge | the published quickstart, fleet pages, API reference and changelog |

## Applicable Rules

- **`domains/sql.md`** — SQL-FILE, SQL-UPSERT, SQL-FKSTATE, SQL-LOCKCLOCK, TXN, IDMP
- **`domains/http.md`**, **`domains/http-responses.md`**, **`domains/http-surface.md`** — HTTP-GARDE, HTTP-VALID, HTTP-STATUS, HTTP-STATEEXT, HTTP-OPENAPI
- **`domains/auth.md`** — the scope and the person check of each new and changed route and verb
- **`languages/rust.md`** and **`languages/typescript.md`** — ERR-RS, RS-FROM, RS-MAPERR; the app's and the CLI's layer rules
- **`tests.md`** — TEST-NEG, TEST-RACE, TEST-COUNTER, TEST-REDGREEN, TCF
- **`domains/architecture.md`** and **`domains/logging.md`** — ARCH-WINS, ARCH-LAND, ARCH-PUBLISHED; LOG-EVENT, LOG-RENAME

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | fix the code; no suppression; `cargo shear` after each module deletion |
| `make test-unit-all` and its TypeScript coverage gates | yes | each deleted module takes its tests with it; each new branch gets one |
| `make test-integration-rustd` | yes | every integration Dimension is `#[ignore]`d and runs in this lane |
| `af_e2e/tests/layers.rs` | yes | no crate moves layer; two crates lose dependencies |
| OpenAPI drift and `INVENTORY` counts | yes | regenerate with `--no-banner`; update `route_inventory.rs` and `route_meta_total.rs` |
| `make dry-app` (mocked API) and `make acceptance` (live, Linux, on demand) | not in a green Pull Request | §6's specs run locally; the acceptance specs run on a burst machine before the Pull Request opens |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | the policy build, the grant store and the trigger store each stay one module |

## Prior-Art / Reference Implementations

- **Reference:** `afd_setup/src/creation/system.rs` — a fleet installed with no draft and no digest; §2 reuses its insert statement in one transaction and drops its revision.
- **Reference:** `afd_setup/src/draft/create.rs` — a create whose retry key is a body `operation_id` with a replay receipt; §2 keeps that shape.
- **Reference:** `afd_api_tenant/src/handler/secret/mod.rs` and `afd_api_tenant/src/handler/grant.rs` — a workspace-scoped write and today's grant list and revoke; §3's routes mirror them.
- **Reference:** `afd_api_runner/src/handler/runner/schedule.rs` and `rustd/guffbolt/src/tools/schedule/cron.rs` — a tool family that writes rows through a fenced run verb; §4 mirrors both, with its tools in `afr_tools`.
- **Divergence:** none from the HTTP standard. The CLI keeps its six practices (SPEC-PRIOR): layered command, handler and error; handlers that neither print nor exit; one renderer; structured JSON errors; three test tiers; JSON when stdout is not a terminal.

## Sections (implementation slices)

Build order: §1, a doc-only commit of the design, then §2 to §8, one gated commit each. **The dual-read rule:** until §8 a fleet whose `head_revision_id` is NULL takes the new path, and a fleet with a revision takes today's, so drafts and their tests stay green. Schema edits before §8 only add; §8 drops. The runner protocol's `VERSION` moves once, in §8.

### §1 — Indy approves this report

No code. Every decision above is ruled. Indy reads the report as the dry run of the change and approves it, or sends it back, before the spec leaves `pending/`.

- **Dimension 1.1** — the report is approved → Test `first_message_report_signed_off` (manual): Indy reads the rulings, the eleven behaviour changes, the Sections and the Schema table. Expected: his approval quoted in Discovery. Evidence: this spec, in the Pull Request.

### §2 — A fleet with no revision is created, assigned and run

**Create.** One transaction writes the fleet, its thread and the turn, then the wake and the workspace notice are published. **State authority:** `fleet.fleets` and `uq (workspace_id, name)`. **Guarded transition:** the replay receipt under scope `<workspace>:fleet:create:<person>` is written first. **Failure assumption:** a daemon that dies after the commit leaves a queued turn, which the next daemon's boot gap re-reads (M255_001 D8). `model_ref` is a nullable column on the fleet; NULL means the workspace's default. The name is the client's `name`, else `fleet-` and the last 8 hex digits of its id; a collision retries the create once with a fresh id.

**Run.** The message admit, the candidate list, the assignment and `installed.rs` accept a fleet with no revision. The policy build takes the fleet row and its grants: the whole tool catalog; `secrets_map` and the mintable list from the grants; the context limits from the model; no budget. The wallet check stays and refuses a payer with no wallet row.

**Network.** `fleet.fleets.http_egress` (`restricted` or `public`) and `package_domains` replace the Definition's `network` key and feed `NetworkPolicy` as it does today. `allow` is derived: the `host` of each attached secret, and GitHub's when attached. `read_only` and `read_post_paths` are deleted. The runner's posture and the supervisor's merge of its registry list with the fleet's hosts are unchanged (`afr_supervisor/src/egress/mod.rs`). A person's `PATCH F` flips either toggle (A2).

**Prompt.** The payload carries the fleet's name and no instructions. `afr_agent`'s prompt builder writes a fixed text (what a fleet is, its name, how it remembers, schedules, adds triggers, starts tasks and what to say when it lacks access), then every pinned entry the memory load carried (category `core`), under one heading. Entries of other categories stay recall-only.

**The witness.** `fleet.fleets.policy_version` starts at 0 and rises in the statement that changes the model, a toggle or a grant. A run stores it at issue; the issue insert takes the fleet `FOR SHARE` and writes no run when the version moved; a turn checkpoint is refused when its version differs, as a changed revision is today. A token mint reads the fleet's current grants.

- **Dimension 2.1** — one message, three rows, one transaction → Test `a_first_message_creates_an_active_fleet_with_its_turn` (integration, `afd_db::test_util::statements`): `{message, operation_id}` → 201; one `fleet.fleets` row `active` with a NULL revision, one thread whose id is the fleet's, one `queued` turn with actor `steer:<subject>`; one transaction begins. Red when the turn is written in a second.
- **Dimension 2.2** — a repeat creates nothing → Test `a_repeated_create_answers_the_same_fleet` (integration): 100 callers from a barrier with one `operation_id` → one fleet, one turn, 99 answers with `replayed: true`.
- **Dimension 2.3** — a changed repeat is refused → Test `a_repeated_create_with_other_text_is_refused` (integration): the same key with other text → 409 `UZ-SETUP-005`; after the fleet is deleted → 410.
- **Dimension 2.4** — no model, no rows → Test `a_create_with_no_usable_model_writes_nothing` (integration): no eligible model → 409 `UZ-SETUP-008`, and no fleet, thread or turn row.
- **Dimension 2.5** — a taken name is refused → Test `a_taken_name_refuses_the_create` (integration): an explicit taken `name` → 409 `UZ-AGT-006` and no row.
- **Dimension 2.6** — nothing is private → Test `a_second_member_reads_a_new_fleet_at_once` (integration): another member's `GET F` and `GET F/messages` answer 200 before any run.
- **Dimension 2.7** — a bare fleet's policy → Test `a_new_fleets_run_holds_the_catalog_and_no_secret` (integration): the first run's policy lists the 32 published tools, an empty `secrets_map`, no repository binding, and a restricted network with an empty `allow`.
- **Dimension 2.8** — the wallet is the ceiling → Test `a_run_with_no_balance_is_refused` (integration): balance 0 → the turn is refused `balance_exhausted`; a payer with no wallet row is refused the same way.
- **Dimension 2.9** — a moved version drops the issue → Test `a_grant_removed_during_issue_drops_the_run` (integration): a sibling transaction removes a grant and commits after the assignment and before the run insert, which is seen waiting on it with `pg_blocking_pids` → no run row; the next assignment issues without the secret.
- **Dimension 2.10** — a stale checkpoint is refused → Test `a_checkpoint_from_an_older_version_is_incompatible` (integration): a turn checkpoint written under version N is refused under N+1.
- **Dimension 2.11** — the model is a field → Test `a_model_change_is_one_patch` (integration): `PATCH F {model_ref}` → 200, `policy_version` + 1, the next run's provider is the new model, one `model_changed` turn; an ineligible ref → 409 `UZ-SETUP-008` and no change.
- **Dimension 2.12** — a private fleet is held to its hosts → Test `a_private_fleet_is_refused_at_an_unlisted_host` (integration): a restricted fleet with `sentry` attached → an HTTP tool call to another public host is refused `host_not_allowed`; the secret's own host is reached.
- **Dimension 2.13** — only a person opens the network → Test `an_api_key_cannot_open_a_fleets_network` (integration): an API key sends `PATCH F {http_egress:"public"}` → 403 and no change; a person's patch → 200 and `policy_version` + 1.
- **Dimension 2.14** — pinned memory is in the prompt → Test `a_runs_prompt_carries_the_fleets_name_and_pinned_memory` (unit, `afr_agent`): a payload with two `core` entries and one `daily` entry → the system prompt holds the fixed text, the fleet's name and both pinned entries, and not the third.
- **Dimension 2.15** — a bare fleet's prompt names no job → Test `a_new_fleets_prompt_holds_only_the_fixed_text` (unit): no pinned entry → the prompt holds the fixed text and no memory heading.
- **Dimension 2.16** — the answer arrives in the fleet's chat → Test `a_first_message_is_answered_on_the_fleet_stream` (e2e, `af_e2e`): two daemons, the real bus and Postgres, a native runner; the create, then `GET F/turns/stream` delivers the turn's completion.

### §3 — A person attaches access, and a run holds exactly that

`connector.integration_grants` becomes `fleet.grants`: one row per `(fleet_id, kind, name)`, with `kind` either `secret` or `integration`, a `scope` for GitHub, and the person who added it. A row existing is the grant; `status`, the request and the approval card go. `POST F/grants` and `DELETE F/grants/{id}` need a person with `fleet:write`; each raises `policy_version` and writes an `access_changed` activity turn naming the person. A secret name must exist in the workspace vault, and an integration must be connected, or the add is refused. For `github` the scope is `af_wire`'s existing `RepositoryBinding` (repositories, reach, publication, validation workflows), and the policy build and the token mint read it where they read the Definition's binding today. A run that references a secret it does not hold gets the tool error it gets today, and the fleet says so in its answer.

- **Dimension 3.1** — an attached secret reaches the next run → Test `an_attached_secret_is_in_the_next_runs_policy` (integration): `POST F/grants {kind:"secret", name:"sentry"}` → 201, `policy_version` + 1, one activity turn; the next run's `secrets_map` holds `sentry` and no other workspace secret.
- **Dimension 3.2** — a machine cannot attach → Test `an_api_key_cannot_add_a_grant` (integration): an API key with `fleet:write` → 403 and no row.
- **Dimension 3.3** — a reader cannot attach → Test `a_reader_cannot_add_a_grant` (integration): a `fleet:read` person → 403 and no row.
- **Dimension 3.4** — an unknown name is refused → Test `a_secret_the_workspace_lacks_cannot_be_attached` (integration): a name absent from the vault → 404 and no row; an integration the workspace has not connected → 409 and no row.
- **Dimension 3.5** — a removal takes effect at the next run → Test `a_removed_grant_leaves_the_next_run_without_the_secret` (integration): `DELETE F/grants/{id}` → 204, `policy_version` + 1; the next run holds no such secret, and a token mint for a removed integration is refused.
- **Dimension 3.6** — the GitHub scope bounds the token → Test `a_token_is_minted_only_for_the_attached_repositories` (integration): a grant for `org/app` read → a mint for `org/other`, or for write, is refused.
- **Dimension 3.7** — a secret goes only to its host → Test `an_attached_secret_is_refused_at_another_host` (integration): a run with `sentry` attached, on a fleet set to public, sends `${secrets.sentry.token}` to another host → `credential_host_not_allowed`, and the run continues.
- **Dimension 3.8** — a duplicate is one row → Test `attaching_the_same_secret_twice_is_one_row` (integration): the second `POST` → 200 with the existing row and no version bump.

### §4 — Triggers are rows the fleet adds

`fleet.triggers` holds one row per webhook source, GitHub App subscription or Slack channel, with the fields today's trigger holds. A run adds, lists and removes them with `trigger_add`, `trigger_list` and `trigger_remove` in `afr_tools`, through fenced run verbs carrying the cron verb's `item_id`, `call_number` and `fencing_token`. A person lists and removes them on `GET` and `DELETE F/triggers`. An add is refused only when the workspace has not connected the provider, when the signing secret is not in the workspace vault, or when the webhook source is `approval` or `github`. The three ingress statements read the table, and a GitHub delivery no longer needs a grant on the fleet. Trigger-sourced schedules, recipes and overrides are deleted, so every schedule is an independent row.

- **Dimension 4.1** — a webhook row routes a delivery → Test `a_webhook_trigger_admits_a_signed_delivery` (integration): `trigger_add {kind:"webhook", source:"sentry", verifier, secret_name:"sentry-hook"}` → the row; a signed `POST /v1/webhooks/{fleet}/sentry` → one turn, actor `webhook:sentry`.
- **Dimension 4.2** — a bad signature is refused → Test `an_unsigned_delivery_to_a_trigger_is_refused` (integration): a wrong signature → 401 and no turn; a source with no row → 404.
- **Dimension 4.3** — a missing signing secret is refused → Test `a_webhook_trigger_needs_a_stored_secret` (integration): `secret_name` absent from the workspace vault, or a reserved source → the tool's refusal and no row.
- **Dimension 4.4** — a GitHub row routes an App delivery → Test `a_github_trigger_routes_an_app_delivery` (integration): a subscription for `org/app` → one App delivery for it makes one turn; a delivery for another repository makes none; with GitHub not connected the add is refused.
- **Dimension 4.5** — a mention routes by the row → Test `a_mention_reaches_the_fleet_subscribed_to_its_channel` (integration): one `mention` row for the channel → the mention's turn lands on that fleet; two fleets on one channel route to the named one, as today.
- **Dimension 4.6** — one row per source → Test `a_second_webhook_for_one_source_is_refused` (integration): a duplicate `(fleet, kind, source)` → the tool's refusal and one row.
- **Dimension 4.7** — a person removes one → Test `a_removed_trigger_admits_nothing` (integration): `DELETE F/triggers/{id}` → 204; a signed delivery to its URL → 404.
- **Dimension 4.8** — a schedule needs no recipe → Test `a_schedule_is_edited_by_its_own_fields` (integration): `PATCH F/schedules/{id} {cron}` → 200; an `override` body → 400.

### §5 — The fleet's model starts a task; the decision model judges it

The intent call, the tool-free proposal call and their cross-check (`afr_agent/src/decision/{mod,interpretation,routing}.rs`) are deleted, with the rule that a task fleet's run gets tools only inside a task. Every run is an ordinary run. When the runner is task-capable and the run carries a decision profile, the catalog also offers `task_start` and `task_input`, and the run is shown the fleet's unfinished tasks.

`task_start {title, objective, target, statements}` reaches the daemon through a fenced run verb. **State authority:** `fleet.durable_tasks`. **Guarded transition:** one transaction locks the run's head turn and the fleet, checks, then writes the turn's `plan`, the item and the task, and binds the run, as the binding does today. The checks: the head turn is a person's message; the objective is a span of that message; the target is valid, as `bindings/preparation.rs` judges it; at least one statement, with unique ids and the existing bounds; the task limits and the unfinished-task cap. A refusal is a tool error the model reads. `task_input {durable_task_id, text}` records a person's message as an input of that task under its current generation, as the input receipt does today (A1).

The goal check is unchanged except for its source: it reads the task's own `questions` and a constant next-action wording kept beside `afd_fleet/src/durable_task/decision/compile.rs`. `accepted_revision_id`, its key, the head-equals-accepted check, the authority guard and goal adoption go. `PATCH F/durable_tasks/{id} {questions, expected_generation}`, a person's, edits the questions.

- **Dimension 5.1** — the worker starts a task → Test `a_task_start_binds_a_task_with_its_own_questions` (integration): a run headed by a person's message calls the verb → one task whose `questions` are the call's, bound to the run, and no intent decision call.
- **Dimension 5.2** — plain chat pays nothing → Test `a_plain_message_makes_no_decision_call` (integration): a message answered without `task_start` → `runner.decision_calls` gains no row, and the run held its tools.
- **Dimension 5.3** — a machine turn starts no task → Test `a_task_start_under_a_webhook_turn_is_refused` (integration): the head turn is a webhook's, a cron's or an API key's → the verb refuses with `UZ-DT-004` and no task row exists.
- **Dimension 5.4** — the objective is the person's words → Test `a_task_start_with_an_objective_outside_the_message_is_refused` (integration): an objective that is not a span of the head message → refused and no task row.
- **Dimension 5.5** — a task needs a question → Test `a_task_start_with_no_statement_is_refused` (integration): an empty statement list → refused and no task row.
- **Dimension 5.6** — the cap holds → Test `a_task_start_past_the_unfinished_cap_is_refused` (integration): the configured cap reached → `UZ-DT-005` and no task row.
- **Dimension 5.7** — no judge, no task tools → Test `a_run_without_a_decision_profile_is_offered_no_task_tools` (unit): a runner that is not task-capable, or a run with no decision profile → the policy lacks `task_start` and `task_input`.
- **Dimension 5.8** — a goal check reads the task → Test `a_goal_check_uses_the_tasks_questions` (integration): the goal decision is prepared with the task's questions plus the constant next action, on a schema with no `fleet.revisions`.
- **Dimension 5.9** — a later message joins a task → Test `a_task_input_is_recorded_once` (integration): `task_input` for an unfinished task → one input under its generation; a repeat answers the same receipt; a finished or foreign task → refused.
- **Dimension 5.10** — a person edits the questions → Test `a_question_edit_bumps_the_generation` (integration): the patch → new `questions` and `goal_digest`, generation + 1; a stale generation → 409; an API key → 403.
- **Dimension 5.11** — access added mid-task does not block → Test `a_task_continues_after_a_grant_is_added` (integration): a grant is added while a task waits → its next turn is issued with the grant.

### §6 — The app: one chat, two short lists

`/w/{ws}/fleets/new` renders the fleet composer; Send calls `POST W/fleets` and replaces the URL with the fleet's page. The fleet page shows "Wakes on" (schedules and triggers, with pause and remove, and a webhook's URL) and "Can access" (grants, with add and remove; the add lists the workspace's stored secrets and connected integrations). The model picker and the two network toggles each send one `PATCH F`. The task panel shows and edits a task's questions. The setup components, the draft client, the setup proxy, the `/setup` route, the unfinished-setups list, goal adoption and the approve and deny controls are deleted; the pull-request starter loses its setup wording.

- **Dimension 6.1** — one message, one chat → Test `a first message opens the fleet's chat` (unit, Playwright on the mocked API, `make dry-app`): type on New fleet → the URL is `/fleets/{id}`, the answer renders, the sidebar lists the fleet.
- **Dimension 6.2** — a refused create keeps the text → Test `a refused create returns its text to the composer` (unit): a 409 → the composer holds the message with Resend, and no fleet is listed.
- **Dimension 6.3** — attaching access is one call → Test `adding a secret under can access sends one grant` (unit): pick `sentry` → one `POST F/grants`; a 404 shows the refusal and adds no row.
- **Dimension 6.4** — the lists read rows → Test `the fleet page lists wake-ups and access` (unit): the page renders `GET F/schedules`, `GET F/triggers` and `GET F/grants`, and sends no `/drafts`, `/setup-context` or `/integration-grants` request.
- **Dimension 6.5** — the picker is one call → Test `the model picker sends one patch` (unit): choosing a model sends one `PATCH F {model_ref}` and no draft request.
- **Dimension 6.6** — the toggle is one call → Test `the network toggle sends one patch` (unit): switching "Allow all" on sends one `PATCH F {http_egress:"public"}`; a 403 leaves the switch off.
- **Dimension 6.7** — the panel edits questions → Test `the task panel saves edited questions` (unit): an edit sends one `PATCH F/durable_tasks/{id}`; a 409 reloads the task.
- **Dimension 6.8** — the real path → Test `onboarding chat journey` (e2e, `make acceptance`): a signed-in person types a first message, reads the answer, attaches a secret and sees a later run use it.

### §7 — The CLI: create, chat, and the two lists

`agentsfleet fleet create ["<message>"] [--name] [--model]`. `agentsfleet chat [<id|@name>]` talks to a fleet; with no fleet its first line creates one. `agentsfleet fleet grants list|add|remove`, `fleet triggers list|remove`, `fleet model set <id|@name> <ref>` and `fleet network set <id|@name> --allow-all|--private [--packages|--no-packages]` join `fleet schedule`, which stays. `--definition`, `--review`, `--save`, `--apply`, the draft subcommands, `fleet activate` and `fleet approvals approve|deny` go.

- **Dimension 7.1** — create from the terminal → Test `install fleet cli` (e2e, `make acceptance`): the built binary against a live daemon, `fleet create "watch checkout errors" --json` → `{fleet_id, turn_id}`, exit 0, and the fleet shows in the browser.
- **Dimension 7.2** — chat creates on its first line → Test `chat with no fleet creates one on the first line` (unit, in-process): one `POST W/fleets`, then the fleet stream.
- **Dimension 7.3** — a grant from the terminal → Test `fleet grants add sends one grant` (unit, handler): `fleet grants add @name sentry` → one `POST F/grants`; a 404 prints the JSON error with its code and a suggestion, exit 1.
- **Dimension 7.4** — a removed flag names its replacement → Test `chat --save is refused with the new command` (unit): exit 2, the message names `fleet create`.

### §8 — The ceremony, the record and their engines are deleted

The dual-read rule ends. `setup.drafts`, `fleet.revisions`, the `/drafts` routes, `POST F/revisions`, `GET F/setup-context`, `F/integration-grants`, the old create body, `Owner::Draft`, `SetupWork`, `RunKind::Setup`, `propose_setup`, the Definition and its parser, the review digest, `Start`, `PRIVATE_RETENTION`, `FleetStatus::Installing`, `PatchStatus::Active`, the gate rules, the grant request and its card, approve and deny with the approval webhook and `Producer::GateContinuation`, the task intake's intent call and its tool-less run, the schedule recipes and the fleet budget go, with every column the Schema table drops.

- **Dimension 8.1** — no removed route → Test `no_setup_route_is_mounted` (unit, `route_inventory.rs`): `INVENTORY` holds no `/drafts`, `/revisions`, `/setup-context` or `/integration-grants` path, and each answers 404.
- **Dimension 8.2** — no removed table or column → Test `the_schema_names_no_draft_or_revision` (integration): after migration no `setup` schema and no `fleet.revisions` table exist, `thread.threads.fleet_id` is `NOT NULL` with `id = fleet_id` checked, and no column in `fleet`, `thread` or `runner.checkpoints` is named `draft_id` or ends in `revision_id`.
- **Dimension 8.3** — a fleet is never installing → Test `a_fleet_is_active_or_archived` (unit): `FleetStatus::ALL` has two members, and a stored `installing` fails to parse.
- **Dimension 8.4** — no draft owner on the wire → Test `a_run_payload_names_no_draft_owner` (unit): `{"owner":{"kind":"draft"}}` fails to decode, and the old create body is refused as an unknown field.
- **Dimension 8.5** — a question is still answered → Test `a_runs_question_is_answered_and_resumes` (integration, existing behaviour, a guard): `ask_user` → the run waits; the answer route → a `question_answer` turn resumes it; the approve and deny routes answer 404.

## Interfaces

**Routes.**

```
POST W/fleets          {message, operation_id, model_ref?, name?}; 201 {fleet, accepted: {turn_id, disposition}, replayed: false}
                       a repeat answers 200 with replayed: true. fleet:write
GET F                  Fleet = {id, name, status, etag, model_ref, http_egress, package_domains, policy_version, schedules[], triggers[], grants[],
                       memory_access, created_at, updated_at, events_processed, spent_nanos}
PATCH F                If-Match; {status?: "archived", name?, model_ref?, http_egress?, package_domains?}; the two network fields need a person
POST F/messages        {message, operation_id?}, unchanged
GET, POST F/grants     POST {kind, name, scope?}; 201 Grant, 200 when it exists. A person with fleet:write
DELETE F/grants/{id}   204. A person with fleet:write
GET F/triggers, DELETE F/triggers/{id}                 fleet:read, fleet:write
PATCH F/durable_tasks/{id}   {state, expected_generation} | {questions, expected_generation}, a person's
unchanged              F/schedules, W/secrets, W/models, W/approvals (list) and its answer route
removed                W/drafts and every D route; POST F/revisions; GET F/setup-context; F/integration-grants; the approve and deny
                       routes; POST /v1/webhooks/{fleet_id}/approval; Fleet's definition, setup, start, setup_etag, setup_revision,
                       setup_revision_id, policy_semantics, webhook_urls, pending_approvals, budget_used_nanos (now spent_nanos);
                       FleetSummary's description and triggers; a task's setup_revision and goal_revision_id
```

**Codes.** Kept under their numbers: `UZ-SETUP-005` (a changed replay), `UZ-SETUP-008` (no usable model), `UZ-SETUP-004` (a missing tag), `UZ-AGT-006`, `UZ-AGT-016`. New codes take the next free numbers in `af_core/src/error_code/fleet.rs`, each with its `REGISTRY` row and problem entry: a grant's name is not in the workspace, an integration is not connected, a trigger sits outside the fleet's grants, a trigger already exists. A caller that is not a person answers the existing 403. The rest of the `UZ-SETUP` family and the budget codes are deleted.

**Runner wire.** `RunPayload` loses `setup_revision_id`, `instructions` and the draft owner, and gains `policy_version` and the fleet's `name`. `NetworkPolicy` loses `read_only` and `read_post_paths`. New verbs under L, each with `item_id`, `call_number` and `fencing_token`: `triggers` (add, list, remove), and `durable_tasks` start and input, which replace the binding and the input receipt as the runner calls them. The decision verbs keep the goal assessment and lose the message and item assessments. `VERSION` moves once.

**Schema** (edited in place, pre-production; adds land with their Section, drops in §8).

| Table | Change, with what depends on it |
|---|---|
| `setup.drafts`, `fleet.revisions`, the `setup` schema and its grants | deleted, with `fk_fleets_head_revision` and `fk_revisions_draft` |
| `fleet.fleets` | + `model_ref TEXT`, `http_egress TEXT NOT NULL`, `package_domains BOOLEAN NOT NULL`, `policy_version BIGINT NOT NULL`; − `head_revision_id` and `idx_fleets_head_revision_id`, `setup_stream_ready`, `initial_start` |
| `thread.threads` | − `draft_id`, its key to `setup.drafts`, `idx_threads_draft_id` and the one-owner check; `fleet_id` is `NOT NULL` |
| `thread.turns` | − `operation_id`, `tenant_model_id`, `proposed_revision_id`, `save_requested`, `save_error_code`, their two keys and three indexes |
| `thread.runs` | + `policy_version BIGINT`; − `draft_id`, `idx_runs_draft_id_status_fencing_token`, `ck_runs_one_owner`, `setup_revision_id`; `fleet_id` is `NOT NULL` |
| `fleet.durable_tasks` | − `accepted_revision_id` and `fk_durable_tasks_revision` |
| `runner.checkpoints` | `revision_id`, `fk_checkpoints_revision` and `idx_checkpoints_revision_id` → `policy_version BIGINT NOT NULL` |
| `fleet.schedules` | − `recipe` |
| `connector.integration_grants` → `fleet.grants` | + `kind TEXT NOT NULL`, `scope JSONB`, `created_by TEXT NOT NULL`; `service` → `name`; − `status`, `requested_reason`, `approved_at`, `revoked_at`; unique `(fleet_id, kind, name)` replaces `uq_integration_grants_fleet_id_service` |
| `fleet.triggers` | new: `id UUID` key, `fleet_id` (cascade), `workspace_id`, `kind TEXT`, `source TEXT` (the webhook's slug, `github` or `slack`), `events TEXT[]`, `repositories TEXT[]`, `verifier_scheme`, `verifier_header`, `secret_name`, `channel` (each nullable, set per kind), `created_at`, `updated_at`; unique `(fleet_id, kind, source)`; indexes on `(kind, source)` and `(kind, channel)`; the grants `fleet.schedules` has |

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Replayed create | failure | a retry or a double click | the saved fleet and turn, `replayed: true`; other text → 409; a deleted fleet → 410 | `a_repeated_create_with_other_text_is_refused` |
| No usable model | failure | no eligible model | 409; nothing written | `a_create_with_no_usable_model_writes_nothing` |
| Lost wake or dead daemon | failure | a crash between the commit and the wake | the turn stays queued; the boot gap assigns it | `a_first_message_is_answered_on_the_fleet_stream` |
| Empty wallet or none | capacity | the payer cannot cover a run | the turn is refused `balance_exhausted` | `a_run_with_no_balance_is_refused` |
| Race: removal against issue | failure | a grant is removed while a run is issued | no run is written; the next issue lacks the secret | `a_grant_removed_during_issue_drops_the_run` |
| A machine attaches access | security | an API key calls `POST F/grants` | 403; no row | `an_api_key_cannot_add_a_grant` |
| Secret steered elsewhere | security | a hostile page names another host | `credential_host_not_allowed`; the run continues | `an_attached_secret_is_refused_at_another_host` |
| A token outside its scope | security | a run asks for another repository or for write | the mint is refused | `a_token_is_minted_only_for_the_attached_repositories` |
| A fleet's network opened by a machine | security | an API key patches `http_egress` | 403; no change | `an_api_key_cannot_open_a_fleets_network` |
| A trigger with no signing secret | failure | the run names a secret the workspace lacks | the add is refused; no row | `a_webhook_trigger_needs_a_stored_secret` |
| Forged delivery | security | a wrong signature on a trigger's URL | 401; no turn | `an_unsigned_delivery_to_a_trigger_is_refused` |
| A machine turn starts a task | security | a webhook body or an API key steers the model to `task_start` | the verb refuses `UZ-DT-004`; no task | `a_task_start_under_a_webhook_turn_is_refused` |
| A task with no finish line | failure | the model proposes no statement | `task_start` is refused | `a_task_start_with_no_statement_is_refused` |
| Stale question edit | failure | two people edit one task | 409; the task is unchanged | `a_question_edit_bumps_the_generation` |

## Invariants

1. A thread belongs to one fleet — `thread.threads.fleet_id` is `NOT NULL` with `id = fleet_id` checked in the schema (Dimension 8.2).
2. A run is issued only what was attached when it was issued — the policy build reads `fleet.grants`, and the issue insert re-checks `policy_version` under a share lock (Dimensions 3.1 and 2.9).
3. A secret leaves only for its own host — `af_egress`'s `bound` check, reached through a real run (Dimension 3.7).
4. Only a person adds or removes a grant, opens a fleet's network or edits a task's questions — each route requires the person extractor (Dimensions 3.2, 2.13 and 5.10).
5. A task starts only under a person's message, on that person's words — the task verb re-reads the run's head turn and passes it through `check_human` before it writes (Dimensions 5.3 and 5.4).
6. No code names a draft, a Definition or a revision — the modules are deleted and `Owner` has one variant, so a reference does not compile (Dimension 8.4).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `fleet_created` | product | the create commits, once per fleet | `fleet_id`, `origin` (`cli`, `dashboard`) | no message text or name | `a_first_message_creates_an_active_fleet_with_its_turn` |
| `fleet_access_changed` | product | a grant is added or removed | `fleet_id`, `kind`, `change` | no secret name or value, no repository name | `an_attached_secret_is_in_the_next_runs_policy` |
| `trigger_changed` (log) | operator | a trigger row is added or removed | `fleet_id`, `kind`, `by` (`run` or `person`) | no signing secret, no URL | `a_webhook_trigger_admits_a_signed_delivery` |

The `SetupStep` events, `fleet_setup_finished`, `fleet_setup_cancelled` and `setup_proposal_refused` are removed, and `docs/architecture/product_analytics.md` changes in the same Pull Request.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Indy approved this report, and no default is open (§1) | `git grep -n -e "^### A[1-2] .* DEFAULT$" -- docs/v2` | no output, and Discovery holds his approval | P0 | |
| R2 | A first message creates one active fleet with its turn, once, and is answered on its stream (§2) | `make test-integration-rustd` | exit 0; `a_first_message_creates_an_active_fleet_with_its_turn ... ok`, `a_repeated_create_answers_the_same_fleet ... ok`, `a_first_message_is_answered_on_the_fleet_stream ... ok` | P0 | |
| R3 | A bare fleet runs with the catalog and no secret, and the wallet is its only ceiling (§2) | `make test-integration-rustd` | exit 0; `a_new_fleets_run_holds_the_catalog_and_no_secret ... ok`, `a_run_with_no_balance_is_refused ... ok`; and `make test-unit-all` shows `a_runs_prompt_carries_the_fleets_name_and_pinned_memory ... ok` | P0 | |
| R4 | A run holds exactly what a person attached (§3) | `make test-integration-rustd` | exit 0; `an_attached_secret_is_in_the_next_runs_policy ... ok`, `an_api_key_cannot_add_a_grant ... ok`, `a_removed_grant_leaves_the_next_run_without_the_secret ... ok` | P0 | |
| R5 | Triggers route from rows the fleet added (§4) | `make test-integration-rustd` | exit 0; `a_webhook_trigger_admits_a_signed_delivery ... ok`, `a_github_trigger_routes_an_app_delivery ... ok`, `a_mention_reaches_the_fleet_subscribed_to_its_channel ... ok` | P0 | |
| R6 | The fleet's model starts a task, only under a person's message, and plain chat pays no decision call (§5) | `make test-integration-rustd` | exit 0; `a_task_start_binds_a_task_with_its_own_questions ... ok`, `a_task_start_under_a_webhook_turn_is_refused ... ok`, `a_plain_message_makes_no_decision_call ... ok`, `a_goal_check_uses_the_tasks_questions ... ok` | P0 | |
| R7 | The app is one chat with two lists (§6) | `make dry-app` | exit 0; `a first message opens the fleet's chat` passed | P0 | |
| R8 | The CLI creates a fleet and attaches access (§7) | `make test-unit-all` | exit 0; `chat with no fleet creates one on the first line` and `fleet grants add sends one grant` pass | P1 | |
| R9 | The live path works for a signed-in person (§6, §7) | `make acceptance SUITE=all` | exit 0; `onboarding chat journey` and `install fleet cli` passed | P1 | |
| R10 | No draft, Definition, revision, review, Save, grant card or `installing` remains (§8) | `make test-unit-all` | exit 0; `no_setup_route_is_mounted ... ok`, `a_fleet_is_active_or_archived ... ok`; every Dead Code Sweep row prints nothing | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the deleted modules | `git ls-files rustd/crates/daemon/afd_setup/src/draft rustd/crates/daemon/afd_setup/src/setup_run rustd/crates/daemon/afd_setup/src/review rustd/crates/protocol/af_fleet_runtime/src/definition rustd/crates/protocol/af_fleet_runtime/src/setup rustd/crates/daemon/afd_fleet/src/run/draft rustd/crates/daemon/afd_cron/src/recipe` | no output |
| drafts and revisions | `git grep -n -e "setup\.drafts" -e "fleet\.revisions" -e draft_id -e head_revision_id -e setup_revision_id -e accepted_revision_id -e proposed_revision_id -e policy_semantics -e initial_start -- rustd ui/packages cli public` | no output |
| the Definition and its engines | `git grep -n -w -e FleetConfig -e "Definition::parse" -e DecisionPolicy -e GatePolicy -e propose_setup -e SetupWork -e "Owner::Draft" -e "RunKind::Setup" -e same_durable_task_authority -- rustd` | no output |
| review, Save and `installing` | `git grep -n -e review_digest -e draft_candidate_id -e SaveSource -e save_requested -e PRIVATE_RETENTION -e "FleetStatus::Installing" -e "PatchStatus::Active" -e setup_stream_ready -- rustd ui/packages cli public` | no output |
| the grant card, approve and deny | `git grep -n -e request_for_setup -e first_ungranted -e integration_grants -e "integration-grants" -e GateContinuation -e gate_continuation -- rustd ui/packages cli public` | no output |
| the task intake | `git grep -n -e task_intent_unavailable -e allow_actions -e "DecisionAssessment::Message" -e "DecisionAssessment::Item" -e read_post_paths -e INSTALLED_INSTRUCTIONS -- rustd` | no output |
| recipes and the fleet budget | `git grep -n -e "Source::Trigger" -e daily_nanos -e monthly_nanos -e budget_used_nanos -- rustd ui/packages cli public` | no output |
| the setup clients | `git ls-files ui/packages/app/components/domain/setup "ui/packages/app/lib/api/draft*" ui/packages/app/lib/api/setup-proxy ui/packages/app/lib/setup-requirements.ts cli/src/commands/chat/setup-session.ts cli/src/commands/chat/setup-recovery.ts` | no output |

## Out of Scope

- The fourteen-to-eight schema folds: M256_002, on the schema this spec leaves.
- A tenant route that creates a durable task (D4), and a timer wake for a waiting task.
- Renaming the two shrunken crates: a follow-up; `afd_setup` no longer holds a setup.
- Billing on the call path, and runner checkpoints' storage beyond the version column: Indy's separate tracks.
- Moving today's drafts, Definitions and revisions: the dev database is torn down at merge.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a person types one sentence on New fleet and reads the fleet's answer in that fleet's chat, with nothing to set up.
2. **Preserved user behaviour** — messages, stops, schedules, memory, a run's question, durable tasks, archive and delete work as they do today. Webhook URLs and GitHub and Slack routing keep working from rows.
3. **Optimal-way check** — one call that writes the fleet and its turn, and no document behind it. Every decision is ruled; the gap is Indy's approval of this report.
4. **Rebuild-vs-iterate** — rebuild how a fleet comes to exist and what a run's policy is read from; iterate on the run loop, schedules and tasks.
5. **What we build** — a create that takes a message; a policy read from the fleet and its grants; grants a person adds; a network toggle per fleet; trigger rows with their tools; a task the fleet's model starts, with its own questions; two lists on the fleet page; the CLI's create, chat and lists.
6. **What we do NOT build** — a setup of any kind, a settings page, a starter Definition, a fleet budget, an access request from a run, a task route, a slug derived from the message.
7. **Fit with existing features** — it builds on the cron tools, memory, the secret vault and the question a run can ask. It must not destabilize the secret's host binding, which becomes the one rule on where a credential goes.
8. **Surface order** — API and runner first, then the app and the CLI, then the teardown, so each checkpoint is green.
9. **Dashboard restraint** — "Wakes on" and "Can access" show rows that exist. "Can access" offers only what the workspace already stores or has connected.
10. **Confused-user next step** — a fleet that lacks access says so in its answer and "Can access" is beside the chat; a refused create names its code and the page that fixes it; `agentsfleet fleet grants list` and `fleet triggers list` print what a fleet has.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** eight Sections as gated commits: approval of the defaults, a fleet with no revision runs, access, triggers, tasks, app, CLI, teardown. The dual-read rule keeps today's path alive until the teardown.
- **Alternatives considered:** deleting the ceremony and keeping the record as what the chat writes to (the author's recommendation, declined by Indy for "Everything in M256_001"); access asked by the run at first use behind an approval card (declined: nothing today stops a run mid-call and resumes it); every fleet using every workspace secret (declined for attaching, which is nearly as small); triggers added only by a person, or bounded by the fleet's grants (both declined by Indy); deleting the runner's posture (declined here: a tenant's toggle would put a sandbox on the runner machine's own network); a Tasks toggle, a `/task` message, or a decision call on every message (declined by Indy for the fleet's own model deciding); a version on each task (dropped: a new fleet's first task would block on its first grant).
- **Patch-vs-refactor verdict:** a **refactor** because two tables, a run kind and the source of every run's policy are removed, two crates lose most of their code, and the approval machinery loses all but the question.

## Discovery (consult log)

- > Owner (Indy, Nov 24, 2026): "Today a user goes draft → setup → fleet → durable tasks. I want to start a fleet and keep chatting. The setup draft is confusing." and "No private state: no \"private until approved\" draft phase." and "No setup approval step. Approvals happen on GitHub and in other tools, not in a setup card." and "No expiry. Archive already exists; the user decides." — context: the goal and the brief's three rulings.
- > Owner (Indy, Nov 24, 2026): "Ideally i wanna get rid of authoring drafts in a clean way?  no stale or unused optional ones just for the sake of it?" — context: D1 and D2. "Everything in M256_001", chosen after the sizing was put to him, having first said "let us not change untill we know what we are geting in to" — context: the scope.
- > Owner (Indy, Nov 24, 2026): "Yes budget - approved payer's balance" — context: D3. On the network: "The runner's egress only", with "why are we debating on this? this is already covered?" — it was: a secret is bound to its own host in `af_egress`, which the author had missed.
- > Owner (Indy, Nov 24, 2026): "Delete the rule (Recommended)" — context: the event gates. "Questions per task", with "i think most things we do before must move to here in the fleet chat. The fleet setup was a conversation chat nothing big deal to it. How did we ask for decision? the same could move here?" — context: tasks.
- > Owner (Indy, Nov 24, 2026): "i think it must be easy to associate secrets to fleets? only if its added then we use them?", then "A person attaches it (Recommended)" — context: access. He had first allowed "every feleet may use every workspace secret if this is simple to do so".
- > Owner (Indy, Nov 24, 2026): "The fleet adds them by tool", with "how did this work before? wakeups?" — context: triggers. Before, the setup chat's model wrote them into the Definition and a person's Save made them live.
- > Owner (Indy, Nov 24, 2026): "the md file save in openclaw is our database here" and "where it gets stored as memory" — context: standing guidance is the fleet's memory.
- > Owner (Indy, Nov 24, 2026): "D4 Agreed" — context: a task is asked for in a message. "Tables merge more 14 down to 8" — context: D5, repeated after the Fable review's three objections were put to him; every verdict cell of `schema_convergence.md` reads `RULED`, which closes M255_001 Dimension 1.1.
- > Owner (Indy, Nov 24, 2026): "Any fleet may" — context: steering, against the author's recommendation that a person allow it. "No limit" — context: the triggers a fleet may add, against the recommendation to bound them by the fleet's grants.
- > Owner (Indy, Nov 24, 2026): "i think the runner poster must be nuked and this must on a fleet based, the summary has the toggle. So remove from runner and add it to per fleet based, by defaut fleets are private.", then, after the host-network risk was put to him, "Fleet toggle; runner stays the ceiling (Recommended)" — context: the network. It replaces his earlier "The runner's egress only".
- > Owner (Indy, Nov 24, 2026): "Refuse the run (Recommended)" — context: a payer with no wallet row. "Follow the fleet's access (Recommended)" — context: a task and the fleet's grants.
- > Owner (Indy, Nov 24, 2026): "who decided the durable task before, since now its manual?", "so a decision call for every message, is that how we do today?", "decision can be on messages too. i want to understand how decision worked in drafts", then "The fleet's own model decides, like setup did", with "how does the model call decide for a decision model elaborate ascii pictorial" — context: task intake. The author had recommended a Tasks toggle and said this option deserved its own spec; it is §5 here, since no other switch survives the Definition.
- > Owner (Indy, Nov 24, 2026): "for a plain message like `hello`, `how are you`, `what is your name` what happens" and "i wanna keep it chat based unlike you had before where you just save when i just types 1 message, in here it wont be the case since there is no safe keep it open like openclaw like" — context: nothing about a fleet's job is saved on its first message; it grows by chat. "Pinned memory in every prompt (Recommended)" — context: the prompt. "Drop it" — context: read-only network access.
- > Owner (Indy, Nov 24, 2026): "Both off; a person flips (Recommended)" — context: A2. "The fleet's own model, by tool", with "what is the current as is behavior before we intend to change" — context: A1, against the author's recommendation to keep the decision model's 0.95 bar while a task runs.
- **Owner decisions** — every decision in this spec: RULED. Pending: Indy's approval of the report (Dimension 1.1); a question that times out is not an answer.
- **Consults** — nine read-only traces by Explore agents: at `3fbd9e40e` the setup draft, the fleet chat, durable tasks, and the app and CLI; at `d79e88a46` the trigger readers, the access readers, the revision and task readers, the setup-only client code, and OpenClaw's source at `630b7e1d`. OpenClaw is one chat that keeps going after a machine-level form; its agent writes its own instruction files and schedules.
- **Model reviews, round 1** — EGM review of the first draft, four Claude Fable 5.2 Goldfish reviewers: comprehension 5 Blocker, 10 Major, 10 Nit; zoom-out 2, 7, 4; design 3, 5, 4; codebase-details 2, 9, 7. That draft kept an authored setup; Indy's rulings replaced its design.
- **Model reviews, round 2** — EGM review of the no-Definition rewrite, three Claude Fable 5.2 reviewers: design 4 Blocker, 7 Major, 4 Nit; implementation-readiness 5, 8, 3; codebase-details 1, 9, 5. **Fixed by Indy's rulings:** the run that asks for access mid-call had no resume and invalidated its own checkpoint (design B1, readiness B2, B3); access is attached by a person. **Fixed by rewriting:** a task blocked on its own first grant (design B2; a task follows the fleet's access); no first slice landed green (design B3, readiness B4; the dual-read rule and add-then-drop schema); a hostile page could open a trigger (design B4; first bounded, then overruled); the crate moves made a Cargo cycle and broke `af_wire`'s rule (readiness B1; both crates shrink in place); a sweep token matched a type that stays (readiness B5, codebase B1; dropped); the HTTP tools ignored the runner's posture (design M4; a per-fleet toggle under the runner's ceiling); steering was a confused deputy (design M5; first a grant, then overruled); the decision call taxed machine turns (design M7; no intent call at all); the schema table lacked dependents and types, error codes, tiers and missing Dimensions (readiness M1 to M8); wrong layers, counts and a missing wire field (codebase M1 to M9). **Left as stated:** the model writes a task's finish line and a person may edit it (design M6), on Indy's ruling "Questions per task". **Overruled by Indy after this round:** bounding a trigger by the fleet's grants (design B4) and steering as a grant (design M5); both risks are named under "What gets looser".
- **Open blockers** — Indy's approval of the report. Whether anything besides the gate rules, the question and the grant request writes an approval gate; `git grep -n INSERT_GATE` at PLAN settles it before §8 deletes approve and deny. How `task_input` and the unfinished-task list reach a run's prompt without the deleted intake; reading `afd_fleet/src/durable_task/input_receipt/` and `afr_agent/src/decision/mod.rs` at PLAN settles it. §2's prompt and network paragraphs and all of §5 were written after the round-2 review; round 3 below reviews them.
- **Discovery candidates, recorded and not scope** (Indy's list, Nov 24, 2026) — billing on the call path, as a per-call usage frame or a platform model gateway; a delivery claim lifetime derived from the retry budget; sandbox drain as a kernel event; a metrics sampler audit; the mailinator teardown sweep. From the M255 close-out: the docs pages `fleets/library.mdx` and `api-reference/error-codes.mdx` still describe plugins, whose routes are gone; the docs Makefile's 10-second `openapi.json` download cap times out on slow links; `cargo shear` warns about unlinked `af_fleet_runtime/src/setup/tests/{schema,settle}.rs`, which this spec deletes; three files over 350 lines (`afr_supervisor`'s `downlink/terminal_tests.rs` and `renew/tests.rs`, the daemon's `config/settings.rs`); renewal slack after one silent round is about 1.5 s, with clock skew and a doubled reconnect pause not budgeted.
- **Deferrals and transfers** — out: the fourteen-to-eight folds to M256_002, on Indy's words "`setup.drafts` (rows 1 and 4) is decided by this milestone, and the folds after it follow." In: M255_001 Dimension 1.1, closed here. Each further one needs an owner quote, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
