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

**Batch:** B1, alone; the design is ruled and Indy approved this report on Nov 24, 2026 (§1)

**Depends on:** M255_001 (merged as #862), whose Dimension 1.1, the rulings on `docs/architecture/schema_convergence.md`, closed here on Nov 24, 2026

**Provenance:** LLM-drafted (Claude Fable 5.2, Nov 24, 2026) from Indy's brief and rulings and nine read-only source traces at `3fbd9e40e`, re-checked at `d79e88a46`. Three rounds of zero-context review opened the cited source; their corrections are applied (Discovery).

**Canonical architecture:** `docs/architecture/thread_model.md`; `docs/architecture/direction.md`, `fleet_definitions.md` and `user_flow.md` are rewritten by this spec (ARCH-WINS, ARCH-LAND)

---

## Overview

**Goal (testable):** A person's first message creates an active fleet and its first turn in one transaction, the fleet works with nothing set up, and no draft, review, Save, Definition or revision exists in the schema, the wire or either client.

**Terms.** W is `/v1/workspaces/{workspace_id}`, F is `W/fleets/{fleet_id}`, L is `/v1/runners/me/runs/{run_id}`. Rust paths that start with a crate name are under `rustd/crates/<layer>/`; app paths are under `ui/packages/app/`.

- A **turn** is one input on a fleet's thread, and a **run** is one try at answering it. The **Definition** is today's stored setup: a name, instructions and 14 runtime keys (`af_fleet_runtime/src/config/mod.rs:65-80`), one immutable row per **revision** in `fleet.revisions`.
- A **grant** is a row a person adds to give one fleet one credential: a workspace secret or an integration such as GitHub.
- A **trigger** is a row that lets something outside wake a fleet: a webhook, a GitHub App event or a Slack mention. A **schedule** is a cron row. The app lists both as "Wakes on".
- A **durable task** is one accepted, bounded objective a fleet works on across several runs, judged done against its completion statements. It has no timer wake, and its default limits are 7 days and 12 runs (`af_wire/src/durable_task/limits.rs:27-34`); "watch" is a schedule or a trigger.
- **A person** on a route is a signed-in caller, not an API key (`afd_http/src/auth/person.rs`). **A person's message** is a turn that passes `check_human` (`afd_fleet/src/durable_task/accept/source.rs:77-92`): a signed-in person's, or an allowed Slack person's mention.

**Problem:** Read from source.

- **Two conversations and two ids.** The setup chat lives on a draft's thread (`afd_setup/src/sql/draft/create.sql:3-18`). Save mints a new fleet id with a new thread (`afd_setup/src/creation/chat_save.rs:133`, `creation/sql/insert.sql:3-16`), so the fleet's chat opens empty.
- **The setup chat does no work.** Its run has one tool, `propose_setup`, no sandbox and no network (`afd_fleet/src/run/draft/payload.rs:122-141`).
- **A ceremony before work.** A draft is private to its creator (`afd_setup/src/sql/draft/lock.sql:8`), is reviewed and saved against a digest (`afd_api_wire/src/fleet/input.rs:48-62`), waits in `installing` for grant cards (`afd_setup/src/creation/activate.rs:77-92`), and expires 30 days after it closes (`afd_setup/src/creation/mod.rs:88`).
- **A document someone must write.** A run is refused without a Definition, a revision and a model (`afd_fleet/src/run/installed.rs:164-193`), and is offered only the tools the Definition lists (`docs/architecture/capabilities.md:23`).

**Solution summary:** A fleet becomes a name, a model, a thread, its memory, its grants, its triggers and its schedules. `POST W/fleets` writes the fleet and its first turn. A run is offered the tool catalog and holds exactly the credentials a person attached to its fleet. The fleet adds its own schedules and triggers by tool, starts a durable task by tool when a person's message asks for work that must keep going, and keeps standing guidance as pinned memory, which every run's prompt carries. Each fleet has a network toggle, private by default. The draft, the Definition and its revisions, the setup run kind, the review, Save, `installing`, `/setup`, the per-fleet budget, the event-gate rules and the grant approval card are deleted.

**Design flow:**

```
TODAY   first message ─► POST W/drafts ─► private draft + its own thread ─► setup run (propose_setup only)
        Save ─► POST W/fleets {draft, review_digest} ─► NEW fleet id + NEW thread, status installing
        grant cards + readiness checks ─► active ─► POST F/messages ─► the first real run, on an empty chat

AFTER   first message ─► POST W/fleets {message, operation_id, model_ref?, name?}
          ONE transaction: fleet.fleets(active) + thread.threads(id = fleet id) + thread.turns #1 queued
        run        the tool catalog, the fixed prompt text plus the fleet's pinned memory, the credentials attached to this fleet
        guidance   "always link the evidence" ─► memory_store, category core (exists) ─► in every later run's prompt
        access     the fleet says "I have no access to Sentry" ─► a person adds it under "Can access" ─► POST F/grants
        wakes on   "check every 15 minutes" ─► cron_add (exists) ─► fleet.schedules
                   "wake on Sentry alerts"  ─► trigger_add ─► fleet.triggers ─► the fleet page shows the URL
        task       "fix the null-cart error until PR 88 is green" ─► the fleet's model calls task_start
                   ─► the daemon writes the task and queues its first task turn ─► each task run ends with the
                   decision model judging the task's own completion statements: continue, wait, done or blocked
```

**Use case, a new fleet becoming the `runner-link-watch` fixture by chat:**

| Step | Rows written | What the runner does |
|---|---|---|
| "hello" | `fleet.fleets` (`active`), `thread.threads`, `thread.turns` #1 (`queued`, actor `steer:<person>`); then `thread.runs` | one model call with the fixed text and no memory; answers; no tool, no decision call |
| "You watch the runner-to-daemon link. You only read. Query Grafana at …" | `thread.turns` #2, `memory.entries` (category `core`) | calls `memory_store`; says it has no access to a `grafana` secret |
| The person adds `grafana` under "Can access" | `fleet.grants` (kind `secret`), an `access_changed` activity turn | nothing; the next run's `secrets_map` holds it and its host is reachable |
| "check it now" | `thread.turns`, `thread.runs`, `thread.items` | its prompt carries the pinned guidance; reads Grafana with `http_request`; reports |
| "do this every hour at seven past" | `fleet.schedules` (source `fleet`) | calls `cron_add` |
| Every hour | `thread.turns` (actor `cron:<schedule>`), `thread.runs` | reads Grafana and reports |
| "fix the dashboard query until PR 88's checks pass" | the turn's `plan`, `fleet.durable_task_items`, `fleet.durable_tasks` (`ready`, with its statements), a continuation turn; later one `runner.decision_calls` row per goal check | calls `task_start`; this run ends; the continuation runs as the task's first run |

## Rulings

**Ruled by Indy on Nov 24, 2026** (quotes in Discovery). Nothing here is open.

| # | Ruling |
|---|---|
| Scope | The ceremony and the record are both deleted in M256_001, in one Pull Request. Nobody writes a setup; `/setup` goes. |
| Start | A fleet starts with nothing, and nothing about its job is saved by its first message. The payer's balance is the only ceiling; a payer with no wallet row is refused. |
| Memory | Standing guidance is the fleet's pinned memory, put into every run's prompt under a short fixed text. |
| Access | A person attaches a secret or an integration to a fleet. A run holds only what is attached. |
| Wakes on | The fleet adds its own triggers by tool, as it adds schedules, with no limit beyond what the workspace has connected. |
| Network | Each fleet has "Allow all" and "Packages", both off at birth; a signed-in person flips them. The runner's posture stays as the operator's ceiling. Read-only is dropped. |
| Steering | Any fleet may steer any fleet in its workspace. |
| Gates | The per-fleet rule for events that wait for a person is deleted. |
| Tasks | The fleet's own model starts a task, writes its completion statements and links a later message to it. The decision model only judges whether it is done. A task follows the fleet's access at each run. No tenant route creates one. |
| Schema | The folds, fourteen tables to eight, are built as M256_002. |

**What gets looser or different.** Each line is a behaviour change Indy accepts by approving this report.

1. Every fleet is offered every published tool and the seven sub-agent tools (`afr_tools/src/catalog/mod.rs:119`), so every run prepares a sandbox. `notify`, `git` and `message` fail until their credential is attached.
2. Any fleet may read and steer any fleet in its workspace, as often as the payer's balance allows. A fleet with no access can drive one that holds access.
3. A fleet may add any webhook, GitHub subscription or Slack channel the workspace has connected. A hostile page a run reads can ask for the same.
4. A fleet set to "Allow all" can post what it has read to any public host. On an `allow_all` runner the sandbox shares the host's network whatever the toggle says, as today. A stored secret is still sent only to its own `host` field (`af_egress/src/policy/mod.rs:219-241`).
5. A private fleet's HTTP tools reach only the hosts of its attached secrets and GitHub when attached. `web_fetch` is refused there, `web_search` runs at the model provider and is unaffected, and `notify` needs its `pushover` secret to carry `host: api.pushover.net`. On an allow-list runner a secret whose host does not resolve to a public address refuses the fleet's runs until it is removed, as a named host does today (`afr_supervisor/src/egress/mod.rs:95-99`).
6. No fleet has a budget. A fleet that schedules itself can spend the payer's whole balance.
7. A fleet knows its job only from what it pinned in memory and from the conversation. Whatever it pins is in every later prompt until a person removes it, a hostile page's suggestion included.
8. Read-only is gone. A fleet may write with any credential attached to it.
9. One model starts a task and links later messages to it, where today two must agree and the link needs a probability of 0.95 (`afr_agent/src/decision/routing.rs:80-131`, `docs/architecture/goal_evaluation.md:383-389`). A person may edit a task's statements.
10. A fleet that has a task no longer loses its tools on plain questions (`afr_agent/src/loop/mod.rs:111-118`).
11. Approving and denying disappear; a person only answers a run's question. unverified: that nothing but the gate rules, the question and the grant request writes an approval gate; `git grep -n INSERT_GATE -- rustd` at PLAN settles it before §3 deletes them.
12. A removed grant leaves a live run with the secret it was issued. The next run and the next token mint do not have it.

**Practices carried from the setup chat.**

1. Each tool that changes a fleet (`memory_store` under `core`, `cron_add`, `trigger_add`, `task_start`, `task_input`) says in its description to call it only when the person's newest message asks, as `propose_setup` does today (`af_fleet_runtime/src/setup/mod.rs:34-40`).
2. A refused call returns to the model as `[kind] sentence`, and the model may correct it in the same run.
3. For the two task tools the daemon reads who sent the run's head turn; text alone starts no task.
4. A person can see and remove what the fleet did: its pinned memory, schedules, triggers and tasks are listed on the fleet page.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(fleet): a fleet is a chat from its first message; setup, the Definition and revisions are gone
- **Intent (one sentence):** A person starts a fleet by saying what they want and keeps talking to it, and what a fleet may touch is whatever a person attached to it.
- **Authoring handshake:** Indy wants authoring gone cleanly, with no optional setup left behind, kept simple, in one milestone. ASSUMPTIONS I'M MAKING: 1. Nothing is backward compatible: routes, wire and schema are edited in place, and the dev database is torn down at merge. 2. One Pull Request, built as six commits, one per Section §2 to §7; each compiles and passes its own Dimensions, and the full gate runs once on the final tree. 3. M256_002 builds the fourteen-to-eight folds on the schema this spec leaves.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_gate/src/policy/build/mod.rs` — the one place a run's policy is assembled; §2 and §3 change its inputs from a config to a fleet row and its grants.
2. `rustd/crates/daemon/afd_fleet/src/durable_task/accept/write.rs` — how a task not bound to the current run is written `ready` with a staged continuation turn; §5's `task_start` takes that branch.
3. `rustd/crates/daemon/afd_api_runner/src/handler/runner/schedule.rs` — a fenced run verb that writes a row with no Definition; §4's and §5's verbs mirror its fields.
4. `rustd/crates/runner/afr_agent/src/prompt/mod.rs` — the system prompt's builder; §2 replaces the installed instructions with the fixed text and pinned memory.
5. `rustd/crates/daemon/afd_turns/src/sql/admit_one.sql` — how a message becomes a queued turn; §2's first turn is written the same way.

## Files Changed (blast radius)

Counts are from the worktree at `d79e88a46`.

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M256_001_P1_API_CLI_UI_FLEET_FROM_FIRST_MESSAGE.md` | EDIT | this spec; moves to `active/`, then `done/` |
| `docs/architecture/*.md`, `docs/architecture/scenarios/*.md` (40 pages name a Definition, draft, revision or setup), `playbooks/operations/slack_incident_drill/001_playbook.md` | EDIT/DELETE | the design as ruled, in a doc-only commit before the code; `fleet_definitions.md` is deleted; `concurrency.md` loses the draft's locks |
| `rustd/crates/daemon/afd_setup/**` (164 production files, 10,975 lines; 56 test files) | DELETE/EDIT | §2. **Kept:** `access.rs`, `error/**`, `paging.rs`, `limits.rs`, `tuning.rs`, `types/**` trimmed to the fleet and model types, `replay/**`, `context/**`, `catalogue/**` moved off `Drafts`, and `creation/{mod,read,patch,delete}.rs` with a new `creation/create.rs` and their statements. Everything else goes |
| `rustd/crates/protocol/af_fleet_runtime/**` (6,763 production lines) | DELETE/EDIT | §2. **Kept:** `name/**`, `error/**`, `config/trigger/**` with the `raw` types it reads, `config/repository.rs` (the GitHub scope), `config/policy/http_egress.rs`, and the statement shape and bounds from `config/decision/`. `definition/**`, `setup/**` and the rest of `config/**` go |
| `rustd/Cargo.toml`, `rustd/Cargo.lock`, the member manifests naming either crate, `.ast-grep/rule-tests/rust-one-agent-loop-test.yml` | EDIT | dependencies dropped as modules go; `cargo shear` stays clean |
| `rustd/crates/daemon/afd_db/schema/**`, `afd_db/src/migration.rs`, `afd_db/src/test_util/**`, `afd_db/tests/**` | DELETE/EDIT/CREATE | the Schema table in Interfaces |
| `rustd/crates/daemon/afd_gate/**` | EDIT/DELETE | §2: the policy is built from the fleet row; the rule parks (`gate/{first,pass,park}.rs`) go. §3: grants feed it; `policy/grants.rs` goes |
| `rustd/crates/daemon/afd_fleet/src/**`, `afd_fleet/tests/**` | EDIT/DELETE | §2: admit, candidates, assignment, issue, `installed.rs`, renewal, payload, the checkpoint guard, the budget's gate and ceilings; `run/draft/**` (1,188 lines), `durable_task/adoption.rs` and the task intake go. §3: mint scope. §5: the binding without its decision authorization |
| `rustd/crates/daemon/afd_ingress/src/**`, `afd_api_ingress/src/handler/{webhook,mention}/**` | EDIT | §4: routing reads `fleet.triggers`. §3: the approval webhook goes |
| `rustd/crates/daemon/afd_cron/src/**` (`recipe/**` is 10 files; 29 production files name a recipe) | DELETE/EDIT | §4: trigger-sourced rows, recipes and overrides go |
| `rustd/crates/daemon/afd_approval/**`, `afd_credential/src/**`, `afd_billing/src/{budget.rs,nanos.rs,sql/select_tenant_charges_*.sql}` | EDIT/DELETE | §3: grants are rows a person writes; the request, the card, approve and deny go. §2: the fleet budget goes |
| `rustd/crates/daemon/afd_api_tenant/src/**`, `afd_api_runner/src/handler/runner/**`, `afd_http/src/**`, `afd_api/**`, `afd_api_wire/src/**`, `afd_fleet_lifecycle/**`, `afd_turns/**`, `afd_sse/src/frame/**`, `rustd/crates/foundation/af_core/src/{error_code,problem}/**`, `public/{openapi.json,llms.txt,agentsfleet-manifest.json}` | EDIT/DELETE | the Routes and Codes in Interfaces; `handler/draft/**` (1,106 lines) is deleted |
| `rustd/crates/protocol/{af_wire,af_observability}/**`, `rustd/crates/harness/{af_egress,af_secrets,af_providers}/**`, `rustd/crates/foundation/af_validate/src/**`, `rustd/crates/runner/{afr_supervisor,afr_agent,afr_tools,main}/**`, `rustd/guffbolt/src/tools/memory/**` | EDIT/DELETE | §2 to §5: the payload, the prompt, read-only's code, the memory tool's default category, the trigger and task verbs and tools; the setup tool, its loop and the draft branch go |
| `rustd/crates/daemon/{afd_bench,afd_gateway,afd_runner,main}/**`, `rustd/crates/e2e/af_e2e/tests/**` | EDIT | the other holders of a draft id, a revision id or `installing`, and their tests |
| `ui/packages/app/components/domain/setup/**` (1,414 lines), `ui/packages/app/lib/api/draft*.ts` (29 files, 2,063 lines), `ui/packages/app/lib/api/setup-proxy/**`, `ui/packages/app/lib/{setup-requirements.ts,api/fleets.ts,api/tasks.ts}`, `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/**`, `ui/packages/app/components/{layout,domain/conversation,domain/models,domain/tasks,domain/approvals,domain/schedules}/**`, `ui/packages/app/public/fleet-starters.json`, `ui/packages/app/tests/**` | DELETE/EDIT | §6 |
| `cli/src/commands/chat/**` (1,399 lines), `cli/src/program/tree/*.ts`, `cli/src/commands/{fleet,schedule}/**`, `cli/src/lib/api-paths.ts`, `cli/test/**` | DELETE/EDIT | §7 |
| `.github/workflows/test-unit-rustd.yml` | EDIT, with Indy's approval at build | its miri step names `afd_setup` modules that go |
| `~/Projects/docs` pages and `changelog.mdx` | EDIT, on that repository's own branch after merge | the published quickstart, fleet pages, API reference and changelog |

## Applicable Rules

- **`domains/sql.md`** — SQL-FILE, SQL-UPSERT, SQL-FKSTATE, SQL-LOCKCLOCK, TXN, IDMP. **`domains/http*.md`** — HTTP-GARDE, HTTP-VALID, HTTP-STATUS, HTTP-STATEEXT, HTTP-OPENAPI. **`domains/auth.md`** — the scope and the person check of each new and changed route and verb.
- **`languages/rust.md`**, **`languages/typescript.md`** — ERR-RS, RS-FROM, RS-MAPERR; the app's and the CLI's layer rules. **`tests.md`** — TEST-NEG, TEST-RACE, TEST-COUNTER, TEST-REDGREEN, TCF. **`domains/architecture.md`**, **`domains/logging.md`** — ARCH-WINS, ARCH-LAND, ARCH-PUBLISHED; LOG-EVENT, LOG-RENAME.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | fix the code; no suppression; `cargo shear` after each module deletion |
| `make test-unit-all` and its TypeScript coverage gates | yes | each deleted module takes its tests with it; each new branch gets one |
| `make test-integration-rustd` | yes | every integration Dimension is `#[ignore]`d and runs in this lane |
| `af_e2e/tests/layers.rs` | yes | no crate moves layer; two crates lose dependencies |
| OpenAPI drift and `INVENTORY` counts | yes | regenerate with `--no-banner`; update `route_inventory.rs` and `route_meta_total.rs` |
| `make dry-app` (Playwright on the mocked API) and `make acceptance` (live, Linux, on demand) | not in a green Pull Request | §6's specs run locally; the two acceptance specs run on a burst machine before the Pull Request opens |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | the create, the policy build, the grant store and the trigger store each stay one module |

## Prior-Art / Reference Implementations

- **Reference:** `afd_setup/src/creation/system.rs` and `afd_setup/src/draft/create.rs` — a fleet installed with no draft, and a create whose retry key is a body `operation_id` with a replay receipt; §2's `creation/create.rs` combines them in one transaction.
- **Reference:** `afd_api_tenant/src/handler/{secret/mod.rs,grant.rs}` — a workspace-scoped write and today's grant list and revoke; §3's routes mirror them. `afd_api_runner/src/handler/runner/schedule.rs` with `rustd/guffbolt/src/tools/schedule/cron.rs` — a tool family that writes rows through a fenced run verb; §4 mirrors both, with its tools in `afr_tools`.
- **Reference:** `afd_fleet/src/durable_task/accept/{commit,insert,write,input}.rs` — the task commit and the input's wake; §5 calls them and adds no second writer.
- **Divergence:** none from the HTTP standard. The CLI keeps its six practices (SPEC-PRIOR): layered command, handler and error; handlers that neither print nor exit; one renderer; structured JSON errors; three test tiers; JSON when stdout is not a terminal.

## Sections (implementation slices)

Build order: §1, a doc-only commit of the design, then §2 to §7, one commit each. §2 is the cut: it removes the record and makes a fleet with no Definition run, so no commit carries two paths. Between §2 and §5 a fleet holds no secret, no trigger and starts no new task; each returns in its Section. The app and the CLI compile throughout and are rewritten in §6 and §7. The runner protocol's `VERSION` moves once, in §2.

### §1 — Indy approves this report

No code. Indy reads the report as the dry run of the change and approves it, or sends it back, before the spec leaves `pending/`.

- **Dimension 1.1** — DONE (his approval is quoted in Discovery) — the report is approved → Test `first_message_report_signed_off` (manual): Indy reads the rulings, the twelve behaviour changes, the Sections and the Schema table. Expected: this line reads `DONE` and Discovery quotes his approval. Evidence: this spec, in the Pull Request.

### §2 — The cut: a fleet with no Definition is created and runs

**Create.** `afd_setup/src/creation/create.rs` writes the fleet, its thread and the turn in one transaction, then publishes the wake and the per-tenant fleet notice. **State authority:** `fleet.fleets` and `uq (workspace_id, name)`. **Guarded transition:** the replay receipt under scope `<workspace>:fleet:create:<person>` is written first. **Failure assumption:** a daemon that dies after the commit leaves a queued turn, which the next daemon's boot gap re-reads (M255_001 D8). `model_ref` is the body's, else the workspace's selected model, resolved at create and stored; with neither the create is refused. The name is the body's, else `fleet-` and the last 12 hex digits of the id.

**Run path.** Every statement that joined `fleet.revisions` reads the fleet row: the admit (`afd_turns/src/sql/admit_one.sql:17,47` loses `setup_current`), `select_ledger_candidates.sql` and `assign_turn_head.sql` (the tenant-runner clause reads `fleets.model_ref`; the `decision_policy` clause goes), `insert_run_with_turn.sql` (the head-revision check goes), `installed.rs`, `select_fleet_config.sql`, `renew_batch.sql`, `select_secret_access.sql`, `durable_task/sql/{issue_source,run}.sql`, and the fleet read and list (`afd_setup/src/creation/{sql/locked.sql,stored.rs,read.rs}`, `afd_fleet_lifecycle/src/sql/select_fleet_{page,by_name}.sql`). The three ingress statements lose their Definition join here and match no fleet until §4. The turn checkpoint drops `revision_id` and its comparison (`afd_fleet/src/run/turn_checkpoint/row.rs:84-87`), so a question's answer resumes whatever changed on the fleet, its model included (Indy: "it shoudnt matter"). **Implementation default:** a checkpoint restored under another model keeps the conversation and tool results and drops the reasoning only the old provider signed, because another provider cannot read it.

**Policy.** `afd_gate/src/policy/build/mod.rs` takes the fleet row in place of a `FleetConfig`: every published tool; an empty `secrets_map`, no mintable and no repository binding until §3; `context::resolve` with no override; no budget. The wallet check stays, and a payer with no wallet row is refused where it is admitted today (`afd_fleet/src/run/start_check/gates.rs:156`).

**Network.** `fleet.fleets.http_egress` (`restricted` or `public`) and `package_domains` feed `NetworkPolicy` as the Definition's `network` key does today (`afd_gate/src/policy/shape.rs:20-47`). `allow` is empty until §3. `read_only`, `read_post_paths` and their code in `af_egress` and `afr_supervisor/src/egress/mod.rs` are deleted. The runner's posture is unchanged.

**Prompt.** The payload carries the fleet's `name` and no instructions. `afr_agent/src/prompt/mod.rs` writes the fixed text in Interfaces, then, under one heading, every `core` entry of the fleet's own that the memory load carried. Other categories and other fleets' shared entries stay recall-only. The block is bounded by the load's existing byte budget (`af_wire::memory::LOAD_WINDOW_BYTES`) and counted against the model's window by the context planner, as instructions are today. The saved compaction's digest covers the fixed text only (`afr_agent/src/loop/context/mod.rs:57-68`). `memory_store`'s default category changes from `core` to `note`, so only an explicit `core` is pinned.

**Tasks keep running.** `fleet.durable_tasks.accepted_revision_id`, the authority guard and goal adoption go. A goal check reads the task's own `questions` and a constant next-action wording, the question and four criteria of `docs/architecture/goal_evaluation.md:547-556`, kept beside `afd_fleet/src/durable_task/decision/compile.rs`; it no longer rebuilds them from a revision (`decision/prepare.rs:194-197`). The intake goes: the message and item assessments, `afr_agent/src/decision/{interpretation,routing}.rs`, the binding's decision authorization, and the rule that strips a task fleet's tools. A task run still prepares before its sandbox by reading its inputs. The binding verb has no caller until §5.

**Deleted here.** `setup.drafts`, `fleet.revisions`, the `/drafts` routes, `POST F/revisions`, `GET F/setup-context`, the old create body, `Owner::Draft`, `SetupWork`, `RunKind::Setup`, `propose_setup`, the Definition and its parser, the review digest, `Start`, `PRIVATE_RETENTION`, `FleetStatus::Installing`, `PatchStatus::Active`, the gate rules and the fleet budget.

- **Dimension 2.1** — one message, three rows, one transaction → Test `a_first_message_creates_an_active_fleet_with_its_turn` (integration, `afd_db::test_util::statements`): `{message, operation_id}` → 201; one `fleet.fleets` row `active`, one thread whose id is the fleet's, one `queued` turn with actor `steer:<subject>`; one transaction begins. Red when the turn is written in a second.
- **Dimension 2.2** — a repeat creates nothing → Test `a_repeated_create_answers_the_same_fleet` (integration): 100 callers from a barrier with one `operation_id` → one fleet, one turn, 99 answers of 200 with `replayed: true`.
- **Dimension 2.3** — a changed repeat is refused → Test `a_repeated_create_with_other_text_is_refused` (integration): the same key with other text → 409 `UZ-SETUP-005` and no second fleet.
- **Dimension 2.4** — a repeat after deletion is gone → Test `a_repeated_create_after_delete_is_gone` (integration): the same key after the fleet is deleted → 410 `UZ-SETUP-013`.
- **Dimension 2.5** — no model, no rows → Test `a_create_with_no_usable_model_writes_nothing` (integration): no `model_ref` and no selected model → 409 `UZ-SETUP-008`, and no fleet, thread or turn row.
- **Dimension 2.6** — a taken name is refused → Test `a_taken_name_refuses_the_create` (integration): a taken `name` → 409 `UZ-AGT-006` and no row.
- **Dimension 2.7** — nothing is private → Test `a_second_member_reads_a_new_fleet_at_once` (integration): another member's `GET F` and `GET F/messages` answer 200 before any run.
- **Dimension 2.8** — a bare fleet's policy → Test `a_new_fleets_run_holds_the_catalog_and_no_secret` (integration): the first run's policy lists every published tool, an empty `secrets_map`, no repository binding, and a restricted network with an empty `allow`.
- **Dimension 2.9** — an empty wallet refuses the turn → Test `a_run_with_no_balance_is_refused` (integration): balance 0 → the turn is refused `balance_exhausted` and the fleet stays `active`.
- **Dimension 2.10** — no wallet row refuses the turn → Test `a_payer_with_no_wallet_row_is_refused` (integration): the wallet row deleted → the turn is refused `balance_exhausted`. On the parent commit the run is issued.
- **Dimension 2.11** — the model is a field → Test `a_model_change_is_one_patch` (integration): `PATCH F {model_ref}` → 200, the next run's provider is the new model, one `model_changed` turn.
- **Dimension 2.12** — an ineligible model is refused → Test `an_ineligible_model_ref_changes_nothing` (integration): a ref the workspace cannot use → 409 `UZ-SETUP-008` and the fleet's model unchanged.
- **Dimension 2.13** — a person opens the network → Test `a_person_opens_a_fleets_network` (integration): `PATCH F {http_egress:"public"}` → 200, and the next run's `NetworkPolicy` is public.
- **Dimension 2.14** — a machine cannot → Test `an_api_key_cannot_open_a_fleets_network` (integration): the same patch from an API key → 403 and the fleet unchanged.
- **Dimension 2.15** — pinned memory is in the prompt → Test `a_runs_prompt_carries_the_fleets_name_and_pinned_memory` (unit, `afr_agent`): a payload with two `core` entries → the system prompt holds the fixed text, the fleet's name and both entries.
- **Dimension 2.16** — nothing else is → Test `a_prompt_leaves_out_unpinned_and_shared_memory` (unit, `afr_agent`): a `note` entry, an entry stored with no category, and another fleet's shared `core` entry → none is in the prompt; with no pinned entry the memory heading is absent.
- **Dimension 2.17** — a goal check reads the task → Test `a_goal_check_uses_the_tasks_questions` (integration): a goal decision for a seeded task is prepared with the task's `questions` plus the constant next action.
- **Dimension 2.18** — no removed route → Test `no_setup_route_is_mounted` (unit, `route_inventory.rs`): `INVENTORY` holds no `/drafts`, `/revisions` or `/setup-context` path.
- **Dimension 2.19** — no removed table or column → Test `the_schema_names_no_draft_or_revision` (integration): after migration no `setup` schema and no `fleet.revisions` table exist, `thread.threads.fleet_id` is `NOT NULL` with `id = fleet_id` checked, and no column in `fleet`, `thread` or `runner.checkpoints` is named `draft_id` or ends in `revision_id`.
- **Dimension 2.20** — a fleet is never installing → Test `a_fleet_is_active_or_archived` (unit): `FleetStatus::ALL` has two members, and a stored `installing` fails to parse.
- **Dimension 2.21** — no draft owner on the wire → Test `a_run_payload_names_no_draft_owner` (unit): `{"owner":{"kind":"draft"}}` fails to decode, and the old create body is refused as an unknown field.
- **Dimension 2.22** — a paused run resumes after a model change → Test `a_questions_answer_resumes_after_the_model_changes` (integration): a run asks with `ask_user`; `PATCH F {model_ref}`; the answer → the turn is answered under the new model, and no checkpoint refusal is raised.
- **Dimension 2.23** — the answer arrives in the fleet's chat → Test `a_first_message_is_answered_on_the_fleet_stream` (e2e, `af_e2e`): two daemons, the real bus and Postgres, a native runner; the create, then `GET F/turns/stream` delivers the turn's completion.

### §3 — A person attaches access, and a run holds exactly that

`fleet.grants` replaces `connector.integration_grants`: one row per `(fleet_id, name)`, a `kind` the daemon derives from the vault entry as `afd_credential/src/secrets/mod.rs:166-185` routes it today (`secret` or `integration`), a `scope` for GitHub, and the person who added it. A row existing is the grant. `POST F/grants` and `DELETE F/grants/{id}` need a person; each writes an `access_changed` activity turn naming them. The name must be in the workspace vault, an integration must be connected, a secret's `host` must be readable by `af_egress::allowlist_host` when present, and `github` needs a scope, which `af_fleet_runtime/src/config/repository.rs` parses. The policy build passes the grants' names to `secrets::declared`, builds the repository binding and origin policies from the GitHub scope as it does from the Definition's binding today, and sets `allow` to the attached secrets' hosts. The token mint reads the fleet's current grants (`afd_fleet/src/run/scope.rs:93-110`). Slack's `addressed_only` reads the GitHub scope's write reach (`afd_ingress/src/slack/mod.rs:137-139`).

**Deleted here.** The grant request and its card (`afd_approval/src/request/**`, the `granted` step of `resolve_gate.sql`, `policy/grants.rs`, the ungranted park in `afd_fleet/src/run/deliver/mod.rs`), `fleet.approval_gates.active_grant_id`, approve and deny with their routes and CLI, the approval webhook and `Producer::GateContinuation`. A run's question and its answer stay.

- **Dimension 3.1** — an attached secret reaches the next run → Test `an_attached_secret_is_in_the_next_runs_policy` (integration): `POST F/grants {name:"sentry"}` → 201 with kind `secret` and one activity turn; the next run's `secrets_map` holds `sentry` and no other workspace secret, and `allow` holds its host.
- **Dimension 3.2** — a machine cannot attach → Test `an_api_key_cannot_add_a_grant` (integration): an API key with `grant:write` → 403 and no row.
- **Dimension 3.3** — the scope is required → Test `a_caller_without_grant_write_cannot_add_a_grant` (integration): a person without `grant:write` → 403 and no row.
- **Dimension 3.4** — an unknown name is refused → Test `a_secret_the_workspace_lacks_cannot_be_attached` (integration): a name absent from the vault → 404 and no row.
- **Dimension 3.5** — an unconnected integration is refused → Test `an_unconnected_integration_cannot_be_attached` (integration): `github` with no workspace install → 409 and no row.
- **Dimension 3.6** — GitHub needs a scope → Test `a_github_grant_without_a_scope_is_refused` (integration): `{name:"github"}` with no `scope`, or a scope the parser refuses → 400 and no row.
- **Dimension 3.7** — a host must be readable → Test `a_secret_with_an_unreadable_host_cannot_be_attached` (integration): a secret whose `host` the allowlist helper cannot read → 400 and no row; a secret with no `host` attaches and opens none.
- **Dimension 3.8** — a removal takes effect at the next run → Test `a_removed_grant_leaves_the_next_run_without_the_secret` (integration): `DELETE F/grants/{id}` → 204; the next run holds no such secret, and a token mint for a removed integration is refused.
- **Dimension 3.9** — the GitHub scope bounds the token → Test `a_token_is_minted_only_for_the_attached_repositories` (integration): a grant for `org/app` read → a mint for `org/other`, or for write, is refused.
- **Dimension 3.10** — a duplicate is one row → Test `attaching_the_same_secret_twice_is_one_row` (integration): the second `POST` → 200 with the existing row and no second activity turn.
- **Dimension 3.11** — an answer resumes after an attach → Test `a_questions_answer_resumes_after_a_grant_is_added` (integration): a run asks with `ask_user`; a grant is added; the answer → the turn's run restores its checkpoint and holds the new secret.
- **Dimension 3.12** — approve and deny are gone → Test `no_approve_or_deny_route_is_mounted` (unit, `route_inventory.rs`): `INVENTORY` holds the answer route and no approve, deny or approval-webhook path.

### §4 — Triggers are rows the fleet adds

`fleet.triggers` holds one row per webhook source, one GitHub App subscription and one Slack channel per fleet, the uniqueness today's parser gives (`af_fleet_runtime/src/config/trigger/set.rs:42-47`), with the fields today's trigger holds. A run adds, lists and removes them with `trigger_add`, `trigger_list` and `trigger_remove` in `afr_tools`, through the fenced run verbs in Interfaces. A person lists and removes them on `GET F/triggers` and `DELETE F/triggers/{id}`. An add is refused when the workspace has not connected the provider, when the signing secret is not in the workspace vault, when the shape fails the kept trigger checks, or when a webhook's source is `approval` or `github`. The three ingress statements read the table and keep their `workspace_id` predicate, so a delivery never reaches another workspace's fleet; a GitHub delivery no longer needs a grant on the fleet. Trigger-sourced schedules, recipes and overrides are deleted, so every schedule is an independent row.

- **Dimension 4.1** — a webhook row routes a delivery → Test `a_webhook_trigger_admits_a_signed_delivery` (integration): `trigger_add {kind:"webhook", source:"sentry", verifier, secret_name:"sentry-hook"}` → the row; a signed `POST /v1/webhooks/{fleet}/sentry` → one turn, actor `webhook:sentry`.
- **Dimension 4.2** — a bad signature is refused → Test `an_unsigned_delivery_to_a_trigger_is_refused` (integration): a wrong signature → 401 and no turn.
- **Dimension 4.3** — a missing signing secret is refused → Test `a_webhook_trigger_needs_a_stored_secret` (integration): `secret_name` absent from the workspace vault, or a reserved source → the tool's refusal and no row.
- **Dimension 4.4** — a GitHub row routes an App delivery → Test `a_github_trigger_routes_an_app_delivery` (integration): a subscription for `org/app` → one App delivery for it makes one turn, actor `github-app`.
- **Dimension 4.5** — another repository or workspace gets nothing → Test `an_app_delivery_outside_a_trigger_reaches_no_fleet` (integration): a delivery for `org/other`, and one for `org/app` from another workspace's install → no turn.
- **Dimension 4.6** — an unconnected provider is refused → Test `a_trigger_for_an_unconnected_provider_is_refused` (integration): a GitHub or Slack add in a workspace with no install → the tool's refusal and no row.
- **Dimension 4.7** — a mention routes by the row → Test `a_mention_reaches_the_fleet_subscribed_to_its_channel` (integration): one `mention` row for the channel → the mention's turn lands on that fleet; two fleets on one channel route to the named one, as today.
- **Dimension 4.8** — one row per source → Test `a_second_webhook_for_one_source_is_refused` (integration): a duplicate → the tool's refusal and one row.
- **Dimension 4.9** — a person removes one → Test `a_removed_trigger_admits_nothing` (integration): `DELETE F/triggers/{id}` → 204; a signed delivery to its URL, or to a source that never had a row → 404 and no turn.
- **Dimension 4.10** — a schedule has no recipe → Test `a_schedule_override_is_refused` (integration): `PATCH F/schedules/{id}` with an `override` body → 400; with `{cron}` → 200.

### §5 — The fleet's model starts a task; the decision model judges it

When the runner is task-capable and the run carries a decision profile, the catalog offers `task_start` and `task_input`, and an ordinary run's payload lists the fleet's unfinished tasks. Neither tool makes a decision call. Each posts the existing binding verb with one item: `task_start {title, source_turn_id, source_start, source_end, target, statements}` sends a new-work item, and `task_input {durable_task_id, source_turn_id, source_start, source_end}` sends a steer item for an existing task.

**State authority:** `fleet.durable_tasks` and the source turn's `plan`, which is written once (`trg_turns_refuse_plan_change`). **Guarded transition:** `bindings::accept` keeps its transaction and commits through `accept/commit.rs`. A binding made by a tool always takes the commit's not-bound branch (`afd_fleet/src/durable_task/accept/insert.rs:56-60`, `accept/write.rs:60-70`): a new task is `ready`, its continuation turn is staged, and the calling run stays an ordinary run. The continuation is then assigned as a task run, prepared before its sandbox as today. A steer item goes through `accept/input.rs`, which cancels a wait and queues the continuation as today. **Commit before release:** the verb answers after the commit.

The checks: the source turn is the run's head turn or one folded into it (`accept/source.rs:63-74`) and is a person's message; the objective is the named span of it; a new item's target is valid, as `bindings/preparation.rs:80-113` judges it; a new item has at least one statement within the kept bounds; the task limits and the unfinished-task cap. The binding's decision authorization is deleted. One source turn takes one plan: the same call again answers the first, and a different one is refused. The input receipt verb and its acknowledgements are unchanged, and the goal check is §2's. `PATCH F/durable_tasks/{id} {questions, expected_generation}`, a person's, edits a task's statements.

- **Dimension 5.1** — the worker starts a task → Test `a_task_start_writes_a_ready_task_and_its_continuation` (integration): a run headed by a person's message posts a one-item binding → one task `ready` whose `questions` are the call's, one staged continuation turn, the calling run unbound, and no `runner.decision_calls` row.
- **Dimension 5.2** — the continuation is the task's first run → Test `a_task_continuation_runs_with_tools` (integration): the continuation is assigned → its run is bound to the task and its policy lists the catalog.
- **Dimension 5.3** — a machine turn starts no task → Test `a_task_start_under_a_webhook_turn_is_refused` (integration): the source turn is a webhook's, a cron's or an API key's → `UZ-DT-004` and no task row.
- **Dimension 5.4** — the objective is the person's words → Test `a_task_start_with_a_span_outside_the_message_is_refused` (integration): a span past the source message, or a source turn the run does not hold → refused and no task row.
- **Dimension 5.5** — a task needs a statement → Test `a_task_start_with_no_statement_is_refused` (integration): an empty statement list → 400 and no task row.
- **Dimension 5.6** — the cap holds → Test `a_task_start_past_the_unfinished_cap_is_refused` (integration): the configured cap reached → `UZ-DT-005` and no task row.
- **Dimension 5.7** — one plan per message → Test `a_second_plan_under_one_turn_is_refused` (integration): a second, different `task_start` naming the same source turn → `UZ-DT-003` and one task; the same call again → 200 with the first task.
- **Dimension 5.8** — a refusal is a sentence → Test `a_refused_task_start_returns_its_kind_and_sentence` (unit, `afr_tools`): a `UZ-DT-004` and a `UZ-DT-005` answer → a tool result of the form `[kind] sentence`.
- **Dimension 5.9** — no judge, no task tools → Test `a_run_without_a_decision_profile_is_offered_no_task_tools` (unit): a runner that is not task-capable, or a run with no decision profile → the policy lacks `task_start` and `task_input`.
- **Dimension 5.10** — plain chat pays nothing → Test `a_plain_message_makes_no_decision_call` (integration): a message answered without a task tool on a fleet with a waiting task → no `runner.decision_calls` row, and the run held its tools.
- **Dimension 5.11** — a later message joins a task → Test `a_task_input_wakes_a_waiting_task` (integration): `task_input` for a waiting task → one input under its generation, the wait cancelled, one continuation staged.
- **Dimension 5.12** — an input needs a person and a live task → Test `a_task_input_under_a_machine_turn_is_refused` (integration): a webhook's source turn → `UZ-DT-004`; a finished or foreign task → `UZ-DT-001`; no input row in either case.
- **Dimension 5.13** — a person edits the statements → Test `a_question_edit_bumps_the_generation` (integration): the patch → new `questions` and `goal_digest`, generation + 1.
- **Dimension 5.14** — a stale or machine edit is refused → Test `a_stale_question_edit_changes_nothing` (integration): a stale generation → 412 `UZ-DT-013`; an API key → 403; the task unchanged.
- **Dimension 5.15** — access added mid-task does not block → Test `a_task_continues_after_a_grant_is_added` (integration): a grant is added while a task waits → its next turn is issued with the grant.

### §6 — The app: one chat and three short lists

`/w/{ws}/fleets/new` renders the fleet composer; Send calls `POST W/fleets` and replaces the URL with the fleet's page. Beside the chat the fleet page shows "Wakes on" (schedules and triggers, with remove and a webhook's URL), "Can access" (grants, with add and remove; the add lists the workspace's stored secrets and connected integrations) and "Remembers" (the pinned entries from `GET F/memories`, with remove). The model picker and the two network toggles each send one `PATCH F`. The task panel shows and edits a task's statements. The setup components, the draft client, the setup proxy, the `/setup` route, the unfinished-setups list, goal adoption and the approve and deny controls are deleted; the pull-request starter loses its setup wording.

- **Dimension 6.1** — one message, one chat → Test `a first message opens the fleet's chat` (integration, Playwright on the mocked API, `make dry-app`): type on New fleet → the URL is `/fleets/{id}`, the answer renders, the sidebar lists the fleet.
- **Dimension 6.2** — a refused create keeps the text → Test `a refused create returns its text to the composer` (unit, vitest): a 409 → the composer holds the message with Resend, and no fleet is listed.
- **Dimension 6.3** — a refused attach shows why → Test `a refused grant shows its reason` (unit, vitest): picking `sentry` sends one `POST F/grants`; a 404 → the refusal's message shows and no row is added.
- **Dimension 6.4** — no draft surface → Test `the fleet page asks for no draft or setup` (unit, vitest): the page reads `GET F/schedules`, `GET F/triggers`, `GET F/grants` and `GET F/memories`, and sends no `/drafts`, `/setup-context` or `/integration-grants` request.
- **Dimension 6.5** — a refused toggle stays off → Test `a refused network toggle stays off` (unit, vitest): switching "Allow all" on sends one `PATCH F {http_egress:"public"}`; a 403 leaves the switch off.
- **Dimension 6.6** — a pin can be removed → Test `removing a pinned memory deletes it` (unit, vitest): Remove on a "Remembers" row → one `DELETE F/memories/{key}` and the row leaves.
- **Dimension 6.7** — a stale statement edit reloads → Test `a stale statement edit reloads the task` (unit, vitest): an edit sends one `PATCH F/durable_tasks/{id}`; a 412 reloads the task and keeps the person's text.
- **Dimension 6.8** — the real path → Test `onboarding-chat-journey.spec.ts` (e2e, `make acceptance`): a signed-in person types a first message, reads the answer, attaches a secret and sees a later run use it.

### §7 — The CLI: create, chat, and the lists

`agentsfleet fleet create "<message>" [--name] [--model]`. `agentsfleet chat [<id|@name>]` talks to a fleet; with no fleet its first line creates one. `agentsfleet fleet grants list|add|remove`, `fleet triggers list|remove`, `fleet model set <id|@name> <ref>` and `fleet network set <id|@name> --allow-all|--private [--packages|--no-packages]` join `fleet schedule` and `fleet memory`, which stay. `--definition`, `--review`, `--save`, `--apply`, the draft subcommands, `fleet activate` and `fleet approvals approve|deny` go.

- **Dimension 7.1** — create from the terminal → Test `install-fleet-cli.spec.ts` (e2e, `make acceptance`): the built binary against a live daemon, `fleet create "watch checkout errors" --json` → `{fleet_id, turn_id}`, exit 0, and the fleet shows in the browser.
- **Dimension 7.2** — chat creates on its first line → Test `chat with no fleet creates one on the first line` (unit, in-process): one `POST W/fleets`, then the fleet stream.
- **Dimension 7.3** — a refused grant is a structured error → Test `fleet grants add prints the refusal as JSON` (unit, handler): a 404 → the JSON error with its code and a suggestion, exit 1.
- **Dimension 7.4** — a removed flag names its replacement → Test `chat --save is refused with the new command` (unit): exit 2, the message names `fleet create`.
- **Dimension 7.5** — a create needs a message → Test `fleet create with no message is a usage error` (unit, handler): no argument → exit 2 and no request sent.

## Interfaces

**Routes.**

```
POST W/fleets          {message, operation_id, model_ref?, name?} · fleet:write
                       201 {fleet, accepted: {turn_id, disposition: "started"|"queued"}, replayed: false}; a repeat: 200, replayed: true
GET F                  Fleet = {id, name, status, etag, model_ref, http_egress, package_domains, schedules[], triggers[], grants[],
                       memory_access, created_at, updated_at, events_processed, budget_used_nanos} · fleet:read
PATCH F                If-Match; {status?: "archived", name?, model_ref?, http_egress?, package_domains?}; 200 Fleet · fleet:write;
                       the two network fields need a person
GET F/grants           200 {items: Grant[]} · grant:read        Grant = {id, kind, name, scope?, created_by, created_at}
POST F/grants          {name, scope?}; 201 Grant, 200 when it exists · grant:write, a person
DELETE F/grants/{id}   204 · grant:write, a person
GET F/triggers         200 {items: Trigger[]} · fleet:read      Trigger = {id, kind, source, events?, repositories?, channel?, url?, created_at}
DELETE F/triggers/{id} 204 · fleet:write
PATCH F/durable_tasks/{id}   If-Match; {state, expected_generation} | {questions, expected_generation}, a person's
PATCH F/schedules/{id}       its own fields only; an `override` body is 400
unchanged              POST F/messages, F/memories, W/secrets, W/models, W/approvals (list) and its answer route
removed                W/drafts and its member routes; POST F/revisions; GET F/setup-context; F/integration-grants; the approve and deny routes;
                       POST /v1/webhooks/{fleet_id}/approval; Fleet's definition, setup, start, setup_etag, setup_revision, setup_revision_id,
                       policy_semantics, webhook_urls, pending_approvals; FleetSummary's description and triggers; a task's setup_revision
```

**Run verbs.** The trigger verbs carry `item_id`, `call_number` and `fencing_token`, as the schedule verb does.

```
POST L/triggers                 {kind: "webhook"|"app_event"|"mention", source?, events?, repositories?, verifier?, secret_name?, channel?}; 201 Trigger
GET L/triggers                  200 {items: Trigger[]}        DELETE L/triggers/{id}   204
POST L/durable_tasks/bindings   kept. {fencing_token, source_turn_id, plan_digest, items: [one]}; a new-work item is {title, source_start, source_end,
                                target, statements[]}, a steer item {durable_task_id, source_start, source_end}; 201 the bindings; a repeat: 200.
                                It loses accepted_revision_id and message_decision_call_id
removed                         the message and item assessments of POST L/durable_tasks/decisions
```

**Codes.** Kept under their numbers: `UZ-SETUP-004`, `UZ-SETUP-005`, `UZ-SETUP-008`, `UZ-SETUP-013`, `UZ-AGT-006`, `UZ-AGT-016`, and the `UZ-DT` family. New, numbered at PLAN from the next free in `af_core/src/error_code/fleet.rs`, each with its `REGISTRY` row and problem entry: a grant's name is not in the workspace (404); an integration or provider is not connected (409); a trigger already exists (409); a trigger's signing secret is not in the workspace (404). A caller that is not a person answers the existing 403. The rest of the `UZ-SETUP` family and the budget codes are deleted.

**Runner wire.** `RunPayload` loses `setup_revision_id`, `instructions`, `durable_tasks` and the draft owner, and gains the fleet's `name` and its unfinished tasks. The turn checkpoint's scope loses its revision. `NetworkPolicy` loses `read_only` and `read_post_paths`. `VERSION` moves once.

**The fixed prompt text,** with the fleet's name filled in.

```
You are {name}, a fleet in agentsfleet: a long-lived agent a team talks to in one chat.
Remember standing guidance with memory_store under the category core, only when the person's newest message asks you to.
It is shown to you at the start of every run. Store anything else under another category.
Schedule yourself with the cron tools, add a webhook, GitHub or Slack trigger with trigger_add, and start work that must
keep going with task_start, each only when the person's newest message asks.
You hold only the secrets and integrations a person attached to you. When you lack one, say which, and ask the person to
add it under Can access.
```

**Schema** (edited in place, pre-production).

| Table | Change, with what depends on it | Section |
|---|---|---|
| `setup.drafts`, `fleet.revisions`, `CREATE SCHEMA setup` and its grants in `roles_and_privileges.sql` | deleted, with `fk_fleets_head_revision` and `fk_revisions_draft` | §2 |
| `fleet.fleets` | + `model_ref TEXT NOT NULL`, `http_egress TEXT NOT NULL`, `package_domains BOOLEAN NOT NULL`; − `head_revision_id` and `idx_fleets_head_revision_id`, `setup_stream_ready`, `initial_start` | §2 |
| `thread.threads` | − `draft_id`, its key to `setup.drafts`, `idx_threads_draft_id` and the one-owner check; `fleet_id` is `NOT NULL` | §2 |
| `thread.turns` | − `operation_id`, `tenant_model_id`, `proposed_revision_id`, `save_requested`, `save_error_code`, their two keys and three indexes | §2 |
| `thread.runs` | − `draft_id`, `idx_runs_draft_id_status_fencing_token`, `ck_runs_one_owner`, `setup_revision_id`; `fleet_id` is `NOT NULL` | §2 |
| `fleet.durable_tasks` | − `accepted_revision_id` and `fk_durable_tasks_revision` | §2 |
| `runner.checkpoints` | − `revision_id`, `fk_checkpoints_revision` and `idx_checkpoints_revision_id` | §2 |
| `connector.integration_grants` → `fleet.grants`, in a new schema file | `id`, `fleet_id` (cascade), `kind TEXT NOT NULL`, `name TEXT NOT NULL`, `scope JSONB`, `created_by TEXT NOT NULL`, `created_at`, `updated_at`; unique `(fleet_id, name)`; `api_runtime` may `SELECT`, `INSERT` and `DELETE`. `status`, `requested_reason`, `approved_at`, `revoked_at` go | §3 |
| `fleet.approval_gates` | − `active_grant_id` and its key | §3 |
| `fleet.triggers`, new | `id UUID` key, `fleet_id` (cascade), `workspace_id`, `kind TEXT NOT NULL` (`webhook`, `app_event` or `mention`), `source TEXT NOT NULL` (the webhook's slug, `github` or `slack`), `events TEXT[]`, `repositories TEXT[]`, `verifier_scheme`, `verifier_header`, `secret_name`, `channel` (nullable, set per kind), `created_at`, `updated_at`; unique `(fleet_id, kind, source)`; indexes on `(workspace_id, kind, source)` and `(workspace_id, kind, channel)`; the grants `fleet.schedules` has | §4 |
| `fleet.schedules` | − `recipe` | §4 |

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Replayed create or plan | failure | a retry or a double click | the saved answer, `replayed: true`; other content → 409 or `UZ-DT-003` | `a_repeated_create_with_other_text_is_refused` |
| No usable model | failure | no eligible model | 409; nothing written | `a_create_with_no_usable_model_writes_nothing` |
| Lost wake or dead daemon | failure | a crash between the commit and the wake | the turn stays queued; the boot gap assigns it | `a_first_message_is_answered_on_the_fleet_stream` |
| Empty wallet or none | capacity | the payer cannot cover a run | the turn is refused `balance_exhausted` | `a_payer_with_no_wallet_row_is_refused` |
| A machine changes access | security | an API key calls `POST F/grants` or patches `http_egress` | 403; nothing changes | `an_api_key_cannot_add_a_grant` |
| A token outside its scope | security | a run asks for another repository or for write | the mint is refused | `a_token_is_minted_only_for_the_attached_repositories` |
| Dependency missing | failure | a secret or integration the workspace lacks | 404 or 409; no row | `a_secret_the_workspace_lacks_cannot_be_attached` |
| Forged delivery | security | a wrong signature on a trigger's URL | 401; no turn | `an_unsigned_delivery_to_a_trigger_is_refused` |
| A delivery crossing workspaces | security | another workspace's install names the same repository | no turn | `an_app_delivery_outside_a_trigger_reaches_no_fleet` |
| A machine turn starts or steers a task | security | a webhook body steers the model to a task tool | the verb refuses `UZ-DT-004`; no task or input | `a_task_start_under_a_webhook_turn_is_refused` |
| A task with no finish line | failure | the model proposes no statement | `task_start` is refused | `a_task_start_with_no_statement_is_refused` |
| Task cap reached | capacity | too many unfinished tasks | `UZ-DT-005` | `a_task_start_past_the_unfinished_cap_is_refused` |
| Stale statement edit | failure | two people edit one task | 412; the task is unchanged | `a_stale_question_edit_changes_nothing` |

## Invariants

1. A thread belongs to one fleet — `thread.threads.fleet_id` is `NOT NULL` with `id = fleet_id` checked in the schema (Dimension 2.19).
2. A run is issued only the credentials attached to its fleet — the policy build's one source of names is `fleet.grants` (Dimension 3.1).
3. Only a person adds or removes a grant, opens a fleet's network or edits a task's statements — each route requires the person extractor (Dimensions 3.2, 2.14 and 5.14).
4. A task starts, and an input joins one, only under a person's message and on that person's words — the binding passes its source turn through `check_assignment` and `check_human` and takes the text as a span of it (Dimensions 5.3, 5.4 and 5.12).
5. A delivery reaches only fleets of the workspace its install belongs to — each trigger read keeps its `workspace_id` predicate (Dimension 4.5).
6. No code names a draft, a Definition or a revision — the modules are deleted and `Owner` has one variant, so a reference does not compile (Dimension 2.21).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `fleet_created` | product | the create commits, once per fleet | `fleet_id`, `origin` (`cli`, `dashboard`) | no message text or name | `a_first_message_creates_an_active_fleet_with_its_turn` |
| `fleet_access_changed` | product | a grant is added or removed | `fleet_id`, `kind`, `change` | no secret name or value, no repository name | `an_attached_secret_is_in_the_next_runs_policy` |
| `trigger_changed` (log) | operator | a trigger row is added or removed | `fleet_id`, `kind`, `by` (`run` or `person`) | no signing secret, no URL | `a_webhook_trigger_admits_a_signed_delivery` |

The `SetupStep` events, `fleet_setup_finished`, `fleet_setup_cancelled`, `setup_proposal_refused` and `task_intent_unavailable` are removed, and `docs/architecture/product_analytics.md` changes in the same Pull Request.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Indy approved this report (§1) | `git grep -n -e "^- \*\*Dimension 1\.1\*\* — DONE" -- "docs/v2/*/M256_001_*"` | one line | P0 | |
| R2 | A first message creates one active fleet with its turn, once, and is answered on its stream (§2) | `make test-integration-rustd` | exit 0; `a_first_message_creates_an_active_fleet_with_its_turn ... ok`, `a_repeated_create_answers_the_same_fleet ... ok`, `a_first_message_is_answered_on_the_fleet_stream ... ok` | P0 | |
| R3 | A bare fleet runs with the catalog and no secret, the wallet is its only ceiling, and no draft or revision is left in the schema (§2) | `make test-integration-rustd` | exit 0; `a_new_fleets_run_holds_the_catalog_and_no_secret ... ok`, `a_payer_with_no_wallet_row_is_refused ... ok`, `the_schema_names_no_draft_or_revision ... ok`, `a_questions_answer_resumes_after_the_model_changes ... ok` | P0 | |
| R4 | A run's prompt is the fixed text and the fleet's pinned memory, and no setup route or status remains (§2) | `make test-unit-all` | exit 0; `a_runs_prompt_carries_the_fleets_name_and_pinned_memory ... ok`, `no_setup_route_is_mounted ... ok`, `a_fleet_is_active_or_archived ... ok` | P0 | |
| R5 | A run holds exactly what a person attached (§3) | `make test-integration-rustd` | exit 0; `an_attached_secret_is_in_the_next_runs_policy ... ok`, `an_api_key_cannot_add_a_grant ... ok`, `a_removed_grant_leaves_the_next_run_without_the_secret ... ok` | P0 | |
| R6 | Triggers route from rows the fleet added, inside its workspace (§4) | `make test-integration-rustd` | exit 0; `a_webhook_trigger_admits_a_signed_delivery ... ok`, `an_app_delivery_outside_a_trigger_reaches_no_fleet ... ok`, `a_mention_reaches_the_fleet_subscribed_to_its_channel ... ok` | P0 | |
| R7 | The fleet's model starts a task only under a person's message, and plain chat pays no decision call (§5) | `make test-integration-rustd` | exit 0; `a_task_start_writes_a_ready_task_and_its_continuation ... ok`, `a_task_start_under_a_webhook_turn_is_refused ... ok`, `a_plain_message_makes_no_decision_call ... ok` | P0 | |
| R8 | The app is one chat with its lists (§6) | `make dry-app` | exit 0; `a first message opens the fleet's chat` passed | P0 | |
| R9 | The CLI creates a fleet and attaches access (§7) | `make test-unit-all` | exit 0; `chat with no fleet creates one on the first line` and `fleet grants add prints the refusal as JSON` pass | P1 | |
| R10 | The live path works for a signed-in person (§6, §7) | `make acceptance SUITE=all` | exit 0; `onboarding-chat-journey.spec.ts` and `install-fleet-cli.spec.ts` passed | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the deleted modules | `git ls-files rustd/crates/daemon/afd_setup/src/draft rustd/crates/daemon/afd_setup/src/setup_run rustd/crates/daemon/afd_setup/src/review rustd/crates/protocol/af_fleet_runtime/src/definition rustd/crates/protocol/af_fleet_runtime/src/setup rustd/crates/daemon/afd_fleet/src/run/draft rustd/crates/daemon/afd_cron/src/recipe` | no output |
| drafts and revisions | `git grep -n -e "setup\.drafts" -e "fleet\.revisions" -e draft_id -e head_revision_id -e setup_revision_id -e accepted_revision_id -e proposed_revision_id -e goal_revision_id -e runtime_model_ref -e policy_semantics -e initial_start -e setup_etag -- rustd ui/packages cli public` | no output |
| the Definition and its engines | `git grep -n -w -e FleetConfig -e "Definition::parse" -e DecisionPolicy -e GatePolicy -e propose_setup -e SetupWork -e SetupStep -e "Owner::Draft" -e "RunKind::Setup" -e same_durable_task_authority -e INSTALLED_INSTRUCTIONS -- rustd` | no output |
| review, Save and `installing` | `git grep -n -e review_digest -e draft_candidate_id -e SaveSource -e save_requested -e PRIVATE_RETENTION -e "FleetStatus::Installing" -e "PatchStatus::Active" -e setup_stream_ready -e "/drafts" -- rustd ui/packages cli public` | no output |
| the grant card, approve and deny | `git grep -n -e request_for_setup -e first_ungranted -e "connector\.integration_grants" -e "integration-grants" -e requested_reason -e active_grant_id -e GateContinuation -e gate_continuation -e pending_approvals -- rustd ui/packages cli public` | no output |
| the task intake and read-only | `git grep -n -e task_intent_unavailable -e allow_actions -e "DecisionAssessment::Message" -e "DecisionAssessment::Item" -e read_post_paths -e allows_read_only -- rustd` | no output |
| recipes and the fleet budget | `git grep -n -e "Source::Trigger" -e daily_nanos -e monthly_nanos -e webhook_urls -- rustd ui/packages cli public` | no output |
| the setup clients | `git ls-files ui/packages/app/components/domain/setup "ui/packages/app/lib/api/draft*" ui/packages/app/lib/api/setup-proxy ui/packages/app/lib/setup-requirements.ts cli/src/commands/chat/setup-session.ts cli/src/commands/chat/setup-recovery.ts` | no output |

## Out of Scope

- The fourteen-to-eight schema folds: M256_002, on the schema this spec leaves.
- Deleting the runner's network posture: its own spec, with a sandbox mode that keeps a public fleet off the runner machine's network. Also later: renaming the two shrunken crates, billing on the call path, and the checkpoint tables beyond the dropped column.
- A tenant route that creates a durable task; a timer wake for a waiting task; moving today's drafts and revisions, since the dev database is torn down at merge.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a person types one sentence on New fleet and reads the fleet's answer in that fleet's chat, with nothing to set up.
2. **Preserved user behaviour** — messages, stops, schedules, memory, a run's question, running durable tasks, archive and delete work as they do today. Webhook URLs and GitHub and Slack routing keep working from rows.
3. **Optimal-way check** — one call that writes the fleet and its turn, and no document behind it. Every decision is ruled; the gap is Indy's approval of this report.
4. **Rebuild-vs-iterate** — rebuild how a fleet comes to exist, what a run's policy and prompt are read from, and how a task starts; iterate on the run loop, schedules and the goal check.
5. **What we build** — a create that takes a message; a policy read from the fleet and its grants; a prompt of fixed text and pinned memory; grants a person adds; a network toggle; trigger rows and task tools the fleet calls; three lists on the fleet page; the CLI's create, chat and lists.
6. **What we do NOT build** — a setup of any kind, a settings page, a starter Definition, a fleet budget, an access request from a run, a task route, a version counter, a slug derived from the message.
7. **Fit with existing features** — it builds on the cron tools, memory, the secret vault, a run's question and the task commit. It must not destabilize a secret's host binding, the one rule on where a credential goes.
8. **Surface order** — API and runner first, then the app and the CLI, because both lose their draft client once the daemon's routes are settled.
9. **Dashboard restraint** — the three lists show rows that exist. "Can access" offers only what the workspace already stores or has connected.
10. **Confused-user next step** — a fleet that lacks access says so in its answer and "Can access" is beside the chat; a refused create names its code and the page that fixes it; `agentsfleet fleet grants list` and `fleet triggers list` print what a fleet has.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** seven Sections: the approval, the cut, access, triggers, tasks, the app, the CLI. The cut comes first so no commit carries today's path beside the new one.
- **Alternatives considered:** keeping the record as what the chat writes to (the author's recommendation, declined by Indy for "Everything in M256_001"); today's path kept alive beside the new one until a last teardown commit (dropped: three Sections removed what it still needed); a version counter as the witness a revision id was (dropped: it refused a question's answer after any change); access asked by the run at first use (declined: nothing today stops a run mid-call and resumes it); a run that becomes a task run midway (dropped: a task run is prepared before its sandbox); a Tasks toggle, a `/task` message or a decision call on every message (declined by Indy); the decision model's bar for linking a later message (declined by Indy); deleting the runner's posture (declined here for the host-network risk).
- **Patch-vs-refactor verdict:** a **refactor** because two tables, a run kind, the task intake and the source of every run's policy and prompt are removed, two crates lose most of their code, and the approval machinery loses all but the question.

## Discovery (consult log)

- > Owner (Indy, Nov 24, 2026): "Today a user goes draft → setup → fleet → durable tasks. I want to start a fleet and keep chatting. The setup draft is confusing." and "No private state: no \"private until approved\" draft phase." and "No setup approval step. Approvals happen on GitHub and in other tools, not in a setup card." and "No expiry. Archive already exists; the user decides." — context: the goal.
- > Owner (Indy, Nov 24, 2026): "Ideally i wanna get rid of authoring drafts in a clean way?  no stale or unused optional ones just for the sake of it?", then "Everything in M256_001" after the sizing, having first said "let us not change untill we know what we are geting in to" — context: the scope. The author recommended deleting only the ceremony.
- > Owner (Indy, Nov 24, 2026): "Yes budget - approved payer's balance" and "Refuse the run (Recommended)" — context: the wallet. "Delete the rule (Recommended)" — context: the event gates. "i think it must be easy to associate secrets to fleets? only if its added then we use them?", then "A person attaches it (Recommended)" — context: access.
- > Owner (Indy, Nov 24, 2026): "The fleet adds them by tool", then "No limit" — context: triggers, against the author's recommendation to bound them by the fleet's grants. "Any fleet may" — context: steering, against the recommendation that a person allow it.
- > Owner (Indy, Nov 24, 2026): "i think the runner poster must be nuked and this must on a fleet based, the summary has the toggle. So remove from runner and add it to per fleet based, by defaut fleets are private.", then "Fleet toggle; runner stays the ceiling (Recommended)" and "Both off; a person flips (Recommended)" — context: the network. "Drop it" — context: read-only.
- > Owner (Indy, Nov 24, 2026): "the md file save in openclaw is our database here", "where it gets stored as memory", "i wanna keep it chat based … keep it open like openclaw like", then "Pinned memory in every prompt (Recommended)" — context: memory and the prompt.
- > Owner (Indy, Nov 24, 2026): "Questions per task", "The fleet's own model decides, like setup did", "the outcome completion is from a deciison model", "The fleet's own model, by tool" and "Follow the fleet's access (Recommended)" — context: tasks. The last link went against the author's recommendation to keep the decision model's bar.
- > Owner (Indy, Nov 24, 2026): "should we followe the practices of the draft into the fleet conversation chat?" and "Keep it simple and follow a realistic construct on an user will need" — context: the four carried practices. "D4 Agreed" — context: no tenant task route. "Tables merge more 14 down to 8" — context: the folds, repeated after the Fable review's three objections were put to him; every verdict cell of `schema_convergence.md` reads `RULED`, which closes M255_001 Dimension 1.1.
- > Owner (Indy, Nov 24, 2026): "approved", "i dont need a fresh review again" and "paused runs must resume cleanly if model changes, it shoudnt matter" — context: Dimension 1.1, the report as pushed at `556f29631`; no fourth review; Dimension 2.22. Every decision in this spec is ruled.
- **Consults** — nine read-only traces by Explore agents: at `3fbd9e40e` the setup draft, the fleet chat, durable tasks, and the app and CLI; at `d79e88a46` the trigger readers, the access readers, the revision and task readers, the setup-only client code, and OpenClaw's source at `630b7e1d`, which is one chat that keeps going after a machine-level form.
- **Model reviews** — three EGM rounds by Claude Fable 5.2 Goldfish reviewers with no context. Round 1, on a draft that kept an authored setup: comprehension 5 Blocker, 10 Major, 10 Nit; zoom-out 2, 7, 4; design 3, 5, 4; codebase-details 2, 9, 7; Indy's rulings replaced that design. Round 2, on the first no-Definition rewrite: design 4, 7, 4; implementation-readiness 5, 8, 3; codebase-details 1, 9, 5; fixed, the largest by Indy's "A person attaches it". Round 3, on the ruled design: design 3, 7, 4; implementation-readiness 5, 8, 3. **Fixed in this revision:** the version counter refused a question's answer after any change (dropped); a run could not become a task run midway (`task_start` queues the task's first turn); `task_input` skipped the existing wake path and the person check (both tools post the existing binding, on a span of a person's turn); the dual path contradicted three Sections and two `NOT NULL` keys (the cut comes first); unlisted statements dropped a bare fleet (named in §2); the kept modules were not separable (kept lists); the default memory category pinned everything (default changed); the GitHub scope named the wrong type; missing route, verb, code and schema detail; a sweep token that stays (`budget_used_nanos` is kept); too few failure paths (now 48 of 74 Dimensions prove a refusal, a replay or an absence). **Left as stated:** a hostile page can get something pinned (behaviour change 7), on Indy's ruling. That revision was not reviewed again, on Indy's word.
- **Open blockers** — whether anything besides the gate rules, the question and the grant request writes an approval gate (behaviour change 11); settled at PLAN.
- **Discovery candidates, recorded and not scope** (Indy's list, Nov 24, 2026) — billing on the call path, as a per-call usage frame or a platform model gateway; a delivery claim lifetime derived from the retry budget; sandbox drain as a kernel event; a metrics sampler audit; the mailinator teardown sweep. From the M255 close-out: the docs pages `fleets/library.mdx` and `api-reference/error-codes.mdx` still describe plugins, whose routes are gone; the docs Makefile's 10-second `openapi.json` download cap times out on slow links; `cargo shear` warns about unlinked `af_fleet_runtime/src/setup/tests/{schema,settle}.rs`, which this spec deletes; three files over 350 lines (`afr_supervisor`'s `downlink/terminal_tests.rs` and `renew/tests.rs`, the daemon's `config/settings.rs`); renewal slack after one silent round is about 1.5 s, with clock skew and a doubled reconnect pause not budgeted.
- **Deferrals and transfers** — out: the fourteen-to-eight folds to M256_002, on Indy's words "`setup.drafts` (rows 1 and 4) is decided by this milestone, and the folds after it follow." In: M255_001 Dimension 1.1, closed here. Each further one needs an owner quote, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
