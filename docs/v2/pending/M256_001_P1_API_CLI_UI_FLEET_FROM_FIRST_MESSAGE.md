<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M256_001: A person's first message creates a fleet, and the setup draft is gone

**Prototype:** v2.0.0

**Milestone:** M256

**Workstream:** 001

**Date:** Nov 24, 2026

**Status:** PENDING

**Priority:** P1, a new person meets a private draft, a setup card and a Save before a fleet does any work

**Categories:** API, CLI, UI

**Batch:** B1, alone; five decisions (D1 to D5) are open and nothing is built until Indy rules on them

**Depends on:** M255_001 (merged as #862), whose Dimension 1.1, the rulings on `docs/architecture/schema_convergence.md`, is carried here as Dimension 1.5 and R1

**Provenance:** LLM-drafted (Claude Fable 5.2, Nov 24, 2026) from Indy's brief and four read-only source traces at `3fbd9e40e`; the author re-read every cited line, and re-checked each cited file at `d79e88a46`, where one changed and its cited line still reads the same

**Canonical architecture:** `docs/architecture/thread_model.md` and `docs/architecture/schema_convergence.md`; `docs/architecture/direction.md` is changed by this spec (see "What the direction costs")

---

## Overview

**Goal (testable):** A person's first message creates an active fleet, its thread, its first revision and its first turn in one transaction, the answer streams in that fleet's chat, and no surface holds a draft, a review, a Save or an expiry.

**Terms.** W is `/v1/workspaces/{workspace_id}`, D is `W/drafts/{draft_id}`, F is `W/fleets/{fleet_id}`, L is `/v1/runners/me/runs/{run_id}`. Rust paths that start with a crate name are under `rustd/crates/<layer>/`; app paths are under `ui/packages/app/`.

- A **turn** is one input on a fleet's thread, and a **run** is one try at answering it. A run's **head turn** is the turn it was assigned for.
- A **revision** is one immutable copy of a fleet's **Definition**. The **head revision** is the one a new run reads.
- A Definition has two prose members, `description` and `instructions_md`, which the model reads and which grant nothing. Its `config` is binding: `tools`, `credentials`, `network`, `repositories`, `triggers`, `budget`, `gates`, `model` and `decision_policy`. This spec says **prose** for the first two and **access** for `config`.
- A **durable task** is one accepted objective a fleet works on across several runs. `decision_policy` holds the completion questions that say when it is done.
- A turn **parks** when it waits on a person's approval.
- **A person's message** is a turn that passes `check_human` (`afd_fleet/src/durable_task/accept/source.rs:77-92`): a signed-in person's message, or a mention by an allowed Slack person. An API key, a webhook, a cron, a continuation and another fleet are not people.

**Problem:** Read from source at `3fbd9e40e`.

- **Two conversations and two ids.** The setup chat lives on the draft's thread (`afd_setup/src/sql/draft/create.sql:3-18`). Save mints a new fleet id with a new thread (`afd_setup/src/creation/chat_save.rs:133`, `creation/sql/insert.sql:3-16`) and moves only the proposal (`creation/sql/install.sql:3-6`). The fleet's chat opens empty.
- **The setup chat does no work.** Its run has one tool, `propose_setup`, no sandbox and no network (`afd_fleet/src/run/draft/payload.rs:122-141`, `afd_fleet/src/run/draft/mod.rs:11-25`). "watch checkout errors" typed there returns a proposed setup, never a look at checkout.
- **Five states before work.** A draft is `editing`, `saving` or `closed`; a fleet is `installing` before it is `active`. Only its creator reads a draft (`afd_setup/src/sql/draft/lock.sql:8`). A closed draft expires after 30 days and nothing deletes it (`afd_setup/src/creation/mod.rs:88`, `creation/sql/close_drafts.sql:2`).
- **A review, a Save and grant cards.** Save needs the review digest (`afd_api_wire/src/fleet/input.rs:48-62`). Activation opens one approval card per missing integration grant and waits for every check (`afd_setup/src/creation/activate.rs:77-92`).
- **No direct path.** `SaveSource` has one variant, `draft` (`afd_api_wire/src/fleet/input.rs:22-27`), so no client creates a fleet without a draft. No tenant route creates a durable task; the one writer is the runner's `POST L/durable_tasks/bindings` (`afd_fleet/src/durable_task/bindings/mod.rs:14-46`).

**Solution summary:** `POST W/fleets` takes the first message and writes the fleet, its thread, revision 1 and turn 1 in one transaction. The fleet is `active` at birth and readable by every member with `fleet:read`. The setup run kind, `setup.drafts`, the draft routes, the review digest, Save, `installing` and the 30-day expiry are deleted. Five things stay open: who writes a setup afterwards (D1), the fate of `/setup` (D2), what a new fleet starts with (D3), direct task creation (D4) and the schema rows (D5).

**Design flow:**

```
TODAY: five steps, two threads, two ids
 person types "watch checkout errors" on /w/{ws}/fleets/new    WorkspaceChat (app/(dashboard)/w/[workspaceId]/fleets/new/page.tsx:28)
 1 POST W/drafts {message, operation_id, save, source, setup_model_ref}        lib/api/draft-message.ts:14-15
   handler  draft::write::create (afd_api_tenant/src/lib.rs:118-121) ─► Drafts::create (afd_setup/src/draft/create.rs)
   rows     setup.drafts(editing) + thread.threads(id = draft id) + thread.turns #1 + thread.replay_receipts
   run      Owner::Draft: propose_setup only ─► report ─► fleet.revisions(draft_id, revision 1), the candidate
 2 GET D/turns/stream (lib/api/draft-events.ts:44) ─► DraftConversation shows the answer and the setup card
 3 Save (components/domain/setup/saving/SetupSaveFlap.tsx:34-35)
   POST W/fleets {source:{kind:"draft",draft_candidate_id}, name, review_digest, start} + Idempotency-Key   lib/api/drafts.ts:199-201
   handler  fleet::native::create::save ─► Creations::save ─► afd_setup/src/creation/reserve.rs
   rows     fleet.fleets(installing, NEW id) + thread.threads(id = fleet id); the revision's draft_id ─► fleet_id; draft ─► saving
 4 activation: grant cards, every check ready ─► fleet active; draft closed, expires in 30 days
 5 person opens /w/{ws}/fleets/{id}, an empty chat ─► POST F/messages (lib/api/fleet-steer.ts:43-54)
   rows     thread.turns queued (afd_turns/src/sql/admit_one.sql:31-49) ─► assign_turn_head.sql ─► insert_run_with_turn.sql
   run      Owner::Fleet: sandbox and tools ─► commit_report.sql ─► the first real work

AFTER: one call, one thread, one id (option 1 of D1 to D3 shown)
 person types "watch checkout errors" on /w/{ws}/fleets/new    the fleet composer (SteerComposer) with the model picker
 1 POST W/fleets {message, operation_id, model_ref?, name?}
   handler  fleet::native::create ─► Creations::create, new code over the statements Creations::materialise runs today
   rows     ONE transaction: fleet.fleets(active) + thread.threads(id = fleet id) + fleet.revisions(revision 1) + thread.turns #1 queued
   app      router.replace ─► /w/{ws}/fleets/{id}; ChatView + useFleetTurnStream read GET F/turns/stream
   run      Owner::Fleet with revision 1's starter tools ─► commit_report.sql ─► the answer; real work in the first run
 2 POST F/messages, the same thread from then on
 3 "/setup always link the evidence" ─► POST F/messages {message, setup: true} ─► that run alone holds propose_setup
   report   a prose proposal becomes fleet.revisions(revision 2) in the report's transaction; a "Setup changed" turn shows it
 4 access: a person's PATCH F from /w/{ws}/fleets/{id}/setup or `agentsfleet fleet setup`; live at once, no card
```

**Use case, "watch checkout errors" typed into a new fleet (option 1 of D1 to D3):**

| Step | Rows written | What the runner does | Durable task? |
|---|---|---|---|
| The person sends the message | `fleet.fleets` (`active`), `thread.threads` (id = fleet id), `fleet.revisions` (revision 1, the starter), `thread.turns` #1 (`queued`, actor `steer:<person>`) | nothing yet; the daemon publishes `af.thread.{id}.wake` | no |
| Assignment | `thread.threads` fence + 1, `thread.turns` #1 `received`, `thread.runs` (`active`) | takes `Down::Run` with `Owner::Fleet` | no: revision 1 has no `decision_policy` |
| Run 1 | `thread.items` per tool call; at report `thread.turns` #1 settled with the answer | works with the starter's tools, asks where the errors live, and names the access it lacks with a link to `/setup` | no |
| The person types `/setup watch the checkout service and link every error` | `thread.turns` #2 (flagged); at report `fleet.revisions` revision 2 (instructions) and a `setup_changed` activity turn | writes its instructions through `propose_setup` | no |
| The person grants access on `/setup` | `fleet.revisions` revision 3 by `PATCH F` (a credential name, a host, a cron trigger, completion questions); `fleet.schedules` | nothing; the next run reads revision 3 | not yet |
| The cron fires, or a webhook arrives | `thread.turns` (actor `cron:<schedule>`), `thread.runs`, `thread.items` | reads the error source and reports | no: only a person's message binds a task |
| The person writes "fix the null-cart error until the checks on PR 88 pass" | `thread.turns`; then `runner.decision_calls`, the turn's `plan`, `fleet.durable_task_items`, `fleet.durable_tasks` | the decision model answers `new_work`, the model proposes the item, the runner posts the binding | **yes** |

A fleet can hold a durable task once its head revision carries `decision_policy` (`afd_fleet/src/durable_task/bindings/mod.rs:36-38`) and a runner reports `durable_tasks_v1` (`afd_fleet/src/run/sql/run/assign_turn_head.sql:55-56`). The next person's message that asks for an objective then binds one. "Watch" itself is a cron or webhook trigger: a task has no timer wake, and its default limits are 7 days and 12 runs (`af_wire/src/durable_task/limits.rs:27-34`).

## Open decisions (Indy rules; nothing is built before)

Each decision goes to Indy through AskUserQuestion. His ruling replaces "OPEN" and is quoted in Discovery. Option 1 is the author's recommendation in every case.

**What the direction costs.** Five edges of "no private state, no setup approval, no expiry", named before the options.

1. The review before Save is the one moment a person sees what a model-written setup grants. Without it, either a model writes access unseen or a person writes it by hand. D1 chooses.
2. A fleet is `active` before its access exists. Today the install-time cards ask for each integration grant first. Afterwards a turn parks at first use (`afd_fleet/src/run/deliver/mod.rs:106-139`, which source calls "the backstop, not the design"), and a parked head turn holds its thread until a person answers.
3. The first message is readable by every member with `fleet:read` once sent, and every "hello" is a fleet in the sidebar until someone archives it.
4. `direction.md` says behaviour goes "into one reviewed Definition" and "Prose cannot grant capabilities". D1 options 1 to 3 keep the second sentence; every option rewrites the first (ARCH-WINS, ARCH-LAND).
5. Today's draft transcripts stay private to their creator. The dev database is torn down at merge, so nothing is moved.

### D1 — Who writes a fleet's setup once there is no draft? OPEN

```
                     1 /setup turns write        2 a person writes      3 any person's turn     4 the fleet writes
                       prose                       all of it              writes prose            all of it
propose_setup is     only on a message the       never; the tool is     on every person's       on every person's
  offered            person sent as /setup       deleted                message                 message
a run may write      description, instructions   nothing                description,            prose and access
                                                                        instructions
access is written    a person (PATCH F)          a person (PATCH F)     a person (PATCH F)      the run
  by
text a webhook left  can shape the instructions  changes nothing        can rewrite the         can add a host, name a
  in the thread      a /setup turn writes                               instructions on any     secret, add a cron
                                                                        later "hello"
to add a schedule    the setup page or the CLI   the same               the same                say so in chat
```

- **1. `/setup` turns write prose, a person writes access (recommended).** The client marks a message as a setup request (`setup: true`; typed as `/setup …` in the composer and the CLI). Only that run holds `propose_setup`, and the tool writes `description` and `instructions_md`. Access changes by a person's `PATCH F`, live at once, with no card and no digest. The daemon decides from a field, where today "call it only when asked" is advice in the tool's description. Cost: a schedule, a host or a secret name is a form or a command, never a sentence. A run reads its thread, so text an earlier webhook left there can still colour what a `/setup` turn writes; the thread shows "Setup changed" at once.
- **2. A person writes all of it.** No setup tool. The fleet works from its conversation and memory, and a person types instructions and access into the setup page. It deletes the most code and has no path from a model to the setup. Cost: nothing a person says in chat becomes a standing instruction.
- **3. Any person's turn writes prose.** Option 1 without the flag. Cost: a poisoned page read last week can rewrite the instructions on the next "hello", and those instructions then steer every cron and webhook run.
- **4. The fleet writes all of it.** The shortest path for the person. A run that reads a hostile page can add a network host and name a workspace secret with no person involved, so the author recommends against it.

Rejected before the list: keeping the draft and hiding it. It keeps two threads, `setup.drafts` and a private stage, which Indy ruled out.

### D2 — What happens to `/setup`? OPEN

```
today  /w/{ws}/fleets/{id}/setup ─► redirect ─► fleets/new?fleet=&setup_revision= ─► a private draft chat ─► review ─► Apply
       (app/(dashboard)/w/[workspaceId]/fleets/[id]/setup/page.tsx:15-17, lib/workspace-routes.ts:83-85)
 1 settings page   /setup renders the head revision: instructions, access, triggers, readiness. Edits are PATCH F.
 2 drawer          the route is deleted; the same form opens as a drawer on the fleet's chat
 3 command only    no page and no drawer; /setup in the composer shows the setup as a card, and edits are typed or sent by CLI
```

- **1. A settings page (recommended).** A person needs one address to read what the fleet may touch and to change it; refusals and the CLI link to it. The route and its links (`components/layout/RecentChats.tsx:84`) stay; the chat, the draft and Apply go. Cost: one page to build, with the form the drawer has today.
- **2. A drawer.** One route fewer. Cost: the setup has no address, so a refusal cannot link to it.
- **3. Command only.** Nothing to build beyond the card. Cost: an access edit has no form, so it is a typed patch or a CLI flag.

Keeping `/setup` as today's private improvement chat is rejected: it keeps `setup.drafts` and the review for every later change.

### D3 — What does a new fleet start with? OPEN, budget ruled: the payer's balance is the ceiling

Revision 1 must exist before the first run, and no model has written it. Today a run with `tools: []` is offered no tool (`docs/architecture/capabilities.md:23`), a Definition with empty instructions is refused at run time (`afd_fleet/src/run/installed.rs:244-248`), and `daily_nanos` is required (`af_fleet_runtime/src/config/budget.rs:92-98`).

```
revision 1 holds        1 a starter Indy names           2 nothing                        3 the person picks
tools, network          the sandbox tools and web reads  none                             a starter chosen in the composer
credentials             none                             none                             none
daily budget            a number Indy names              none; daily_nanos optional       the starter's
run 1 of "watch         reads public pages, runs code    answers in text only             depends on the pick
  checkout errors"      in its sandbox, holds no secret
```

- **1. A starter Indy names (recommended).** One Definition in the daemon's config: its tool list, its network rule and its daily budget are Indy's words, so no limit is invented. The first run can do real work and can leak nothing, because it holds no secret. unverified: that `network` can say "read any public host"; `af_fleet_runtime/src/config/policy` settles it.
- **2. Nothing.** No tools and no ceiling. The first run can only talk until a person grants access. Making `daily_nanos` optional reverses "A fleet's own budget is the only ceiling on its spend" (Indy, Nov 22, 2026, `af_fleet_runtime/src/config/budget.rs:3-5`): the payer's balance becomes the ceiling.
- **3. The person picks.** The composer gains a starter control beside the model picker. Nothing is hidden. Cost: one more choice before the first message.

In every option a fleet may run with empty instructions, and its name is the client's `name` or `fleet-<suffix of its id>`, renamed later by `PATCH F`. No slug is derived from the message.

### D4 — How does a person create a durable task directly? RULED: option 1, chat only here

```
today   a person's message ─► run ─► decision call (intent) ─► the model proposes items ─► POST L/durable_tasks/bindings ─► task
 1 chat   unchanged here: agentsfleet fleet steer @name "<objective>"; a tenant route gets its own spec if Indy wants one
 2 route  POST F/durable_tasks {objective, target, operation_id} in this milestone
 3 flag   POST F/messages {message, task:{target}} ─► the turn is marked new work; the runner binds without the intent call
```

- **1. Chat only here (recommended).** A fleet is created directly by `agentsfleet fleet create` or the New fleet page. A task is asked for in a message, as today. A route is a new endpoint that writes money-bearing rows, so it gets its own spec and review (SPEC-BOUNDARY).
- **2. A route now.** The binding needs a live run and prepares its GitHub target outside the transaction (`afd_fleet/src/durable_task/accept/mod.rs:47-55`, `bindings/preparation.rs:80-113`). A route is therefore a second write path that skips `authorize::binding` (`afd_fleet/src/durable_task/decision/authorize.rs:55-108`), and a `ready` task still waits for a capable runner.
- **3. A flag on the message.** One writer stays. It skips the same authorization, which then needs a replacement.

### D5 — The schema rows inherited from M255_001 Dimension 1.1. RULED: option 3, the reviews' end state, 14 tables to 8

`docs/architecture/schema_convergence.md` asks, for each of fourteen tables, whether it folds into threads, turns, runs and items. Two reviews answered: the design reviews ("the reviews' end state") and a later Fable 5.2 review that disagreed with five folds. With drafts gone, rows 1 and 4 need no fold: the table and every `draft_id` are deleted in §7.

```
row  table                                        1 Fable verdicts,   2 as 1, and effects  3 the reviews'      4 rows 1 and 4
                                                    effects untouched   leave with a task    end state           now, rest later
 1   setup.drafts                                 DELETED             DELETED              DELETED             DELETED
 4   thread.runs: fleet_id and draft_id           draft_id dropped    the same             the same            the same
 3   thread.compactions                           stays; one joined   the same             onto thread.threads UNDECIDED
                                                  read at run start
 11  fleet.durable_task_inputs                    columns on its item the same             rows of thread.items UNDECIDED
 12  fleet.durable_task_effects                   stays as it is      cascades from task   rows of thread.items UNDECIDED
 10  fleet.durable_task_items                     stays               stays                rows of thread.items UNDECIDED
 14  runner.decision_payloads                     stays               stays                rows of thread.items UNDECIDED
 8   thread.replay_receipts                       stays               stays                takes task receipts  UNDECIDED
 2, 5, 6, 7, 9, 13  threads, turns, items,        stay                stay                 stay                stay
     deliveries, durable_tasks, decision_calls
     tables left of the 14                        12                  12                   8                   13
```

- **1. The Fable verdicts, effects untouched (recommended).** It takes the one saving read from source (row 11: one table, four joins and one insert per steer) and one unmeasured read (row 3), and declines every fold the Fable review found unsafe. Cost: the folds are built in M256_002.
- **2. As 1, and an effect leaves with its task.** One purge function and two triggers go. It reverses "the schema keeps an effect past purge on purpose" (`schema_convergence.md:72`).
- **3. The reviews' end state.** Fewest tables. The Fable review's objections stand: the report deletes other runs' items, so task rows there need a guard in every such statement.
- **4. Rows 1 and 4 now.** The other cells stay `UNDECIDED`, so R1 cannot pass here and moves to M256_002 with Indy's quote.

**Implementation defaults Indy may overrule** (agent choices, SPEC-DECIDE):

- **Who may write access.** `PATCH F` on access keys needs a signed-in person with `fleet:write`; an API key is refused. Each edit writes a `setup_changed` activity turn naming the person. Today's Save sits on the same scope (`afd_http/src/route/fleet.rs:186-191`).
- **A `/setup` message is never folded** into a live run. It queues and heads its own run, so the flag and the tool always meet.
- **One spec, seven Sections.** The schema folds are M256_002. A task route, if ruled, is its own spec.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(fleet): a fleet from the first message; the setup draft, review and Save are gone
- **Intent (one sentence):** A person starts a fleet by saying what they want and keeps talking to the same fleet, with no private draft, setup card or expiry in between.
- **Authoring handshake:** Indy wants a fleet to exist from the first message, in one conversation, with no private stage, no setup approval and no expiry, and he has not yet chosen the design or the fate of `/setup`. ASSUMPTIONS I'M MAKING: 1. `/setup` means the app route `/w/{ws}/fleets/{id}/setup` and the private setup chat behind it; no `/setup` chat command exists today. 2. "No setup approval" removes the review digest, Save and the install-time grant cards; an approval a run meets while working stays. 3. Nothing is backward compatible: routes, wire and schema are edited in place. 4. The Sections below are written for option 1 of each decision and are rewritten after the rulings, before the spec leaves `pending/`.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_setup/src/creation/system.rs` — a fleet installed from a Definition with no draft and no digest, in two transactions through `installing`; §2 reuses its statements in one.
2. `rustd/crates/daemon/afd_turns/src/sql/admit_one.sql` — how a fleet message becomes a queued turn; §2's first turn is written the same way.
3. `rustd/crates/daemon/afd_fleet/src/run/draft/mod.rs` — what a draft's run is today; §3 moves its one tool onto a flagged fleet run and §7 deletes the rest.
4. `rustd/crates/daemon/afd_setup/src/creation/progress.rs` — the readiness checks whose fate §2 tabulates.
5. `docs/architecture/schema_convergence.md` — the fourteen rows D5 rules on.

## Files Changed (blast radius)

This Pull Request is docs-only and creates the spec. The rest is the build's reach under option 1 of each decision. `git grep -l -w draft_id` names 111 files under `rustd`, 14 under `ui/packages` and 10 under `cli`; statements on the table (`git grep -l -E "(FROM|INTO|UPDATE|JOIN) +setup\.drafts"`) sit in 52 files under `rustd`; `afd_setup` holds 227 tracked files. The Rust field `setup.drafts` on `Setup` shares the spelling and is renamed, not swept. The per-file list of every hit is added when D1 is ruled (Discovery, open blockers).

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M256_001_P1_API_CLI_UI_FLEET_FROM_FIRST_MESSAGE.md` | CREATE | this spec; moves to `active/`, then `done/` |
| `docs/architecture/{direction,user_flow,capabilities,fleet_definitions,thread_model,api_resources,data_flow,web_app,product_analytics,schema_convergence,postgres_schemas,concurrency,runner_fleet,shared_chat_verification}.md` | EDIT | the design as ruled, in a doc-only commit before the code; each verdict cell reads `RULED` |
| `rustd/crates/daemon/afd_setup/src/creation/**` | EDIT | §2: the create writes the fleet and its first turn; `chat_save.rs`, `activate.rs`, `conversations.rs` and the draft half of `reserve.rs` go in §7 |
| `rustd/crates/daemon/afd_api_tenant/src/handler/fleet/**`, `afd_api_tenant/src/lib.rs`, `afd_http/src/route/{workspace,fleet}.rs`, `afd_api/tests/route_inventory.rs`, `route_meta_total.rs`, `afd_api_wire/src/fleet/**`, `public/openapi.json` | EDIT | §2 to §4: the create body, the message's `setup` field, `PATCH F`'s edit fields; OpenAPI regenerated |
| `rustd/crates/daemon/afd_fleet/src/run/{answer,deliver,report}/**`, `afd_fleet/src/run/sql/**`, `afd_turns/src/**` | EDIT | §3: a flagged turn is never folded; its payload offers the tool; its report writes a revision |
| `rustd/crates/protocol/{af_wire,af_fleet_runtime,af_observability}/src/**`, `rustd/crates/runner/{afr_supervisor,afr_agent,afr_tools}/src/**` | EDIT | §3, §7: the tool's writable paths, `Owner`, the funnel events; the runner loses its draft branch |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/**`, `ui/packages/app/components/domain/{setup,conversation}/**`, `ui/packages/app/components/layout/**`, `ui/packages/app/lib/api/**`, `ui/packages/app/tests/e2e/**` | EDIT/DELETE | §5: the New fleet composer creates a fleet; the setup page per D2; the draft client, list and Save go |
| `cli/src/program/tree/*.ts`, `cli/src/commands/{chat,fleet}/**`, `cli/test/**` | EDIT/DELETE | §6: `fleet create`, `fleet setup`; `chat` talks to a fleet |
| `rustd/crates/daemon/afd_db/schema/*_setup_drafts.sql` and `*_{thread_threads,thread_runs,thread_turns,fleet_revisions,fleet_fleets}.sql`, `afd_db/src/migration.rs` | DELETE/EDIT | §7: the table, `draft_id`, the three one-owner checks, the setup-only turn columns, the revision columns `review_digest`, `review_context` and `installed`, `setup_stream_ready` and `initial_start` |
| `rustd/crates/daemon/afd_setup/src/{draft,message,setup_run}/**`, `afd_setup/src/privacy.rs`, `afd_api_tenant/src/handler/draft/**`, `afd_fleet/src/run/draft/**` | DELETE | §7: everything a draft owned. Kept: `afd_setup/src/context/**` (fleet runs load and save through it), `replay/**` (§2's receipt), `access.rs` and `review/**` (the readiness read) |
| `rustd/crates/daemon/{afd_billing,afd_bench,afd_gateway,afd_runner,afd_approval,afd_fleet_lifecycle}/src/**`, `afd_turns/src/view/stop/**`, `afd_http/src/{envelope.rs,handler/setup.rs,openapi/path.rs}`, `rustd/crates/daemon/main/src/plane/**`, `rustd/crates/foundation/af_core/src/{problem,error_code}/**`, `public/{llms.txt,agentsfleet-manifest.json}` | EDIT | §7: the other holders of a draft id, the setup run kind, the install-time grant request or `installing` |
| `~/Projects/docs` pages and `changelog.mdx` | EDIT, on that repository's own branch after merge | the published quickstart, fleet pages, API reference and changelog |

## Applicable Rules

- **`domains/sql.md`** — SQL-FILE, SQL-UPSERT, SQL-FKSTATE, SQL-LOCKCLOCK, TXN, IDMP
- **`domains/http.md`**, **`domains/http-responses.md`**, **`domains/http-surface.md`** — HTTP-GARDE, HTTP-VALID, HTTP-STATUS, HTTP-STATEEXT, HTTP-OPENAPI
- **`domains/auth.md`** — the capability scope and the person check of each new and changed route
- **`languages/rust.md`** and **`languages/typescript.md`** — ERR-RS, RS-FROM, RS-MAPERR; the app's and the CLI's layer rules
- **`tests.md`** — TEST-NEG, TEST-RACE, TEST-COUNTER, TEST-REDGREEN, TCF
- **`domains/architecture.md`** and **`domains/logging.md`** — ARCH-WINS, ARCH-LAND, ARCH-PUBLISHED; LOG-EVENT, LOG-RENAME

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | fix the code; no suppression |
| `make test-unit-all` and its TypeScript coverage gates | yes, §2 to §7 | each deleted component takes its tests with it; each new branch gets one |
| `make test-integration-rustd` | yes, §2 to §4, §7 | every integration Dimension is `#[ignore]`d and runs in this lane |
| OpenAPI drift and `INVENTORY` counts | yes, §2 to §4, §7 | regenerate with `--no-banner`; update `route_inventory.rs` and `route_meta_total.rs` |
| Playwright (`make dry-app`) | not in a green Pull Request | §5's specs run locally before the Pull Request opens |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, the create path and the report | the first turn's write stays its own statement file; the proposal check stays its own module |

## Prior-Art / Reference Implementations

- **Reference:** `afd_setup/src/creation/system.rs` — `Creations::materialise` installs a Definition as an active fleet with no draft and no digest. It has no production caller, it uses two transactions with a compensating delete, and it refuses empty instructions. §2 reuses its insert and revision statements in one transaction.
- **Reference:** `afd_setup/src/draft/create.rs` — a create whose retry key is a body `operation_id` with a replay receipt; §2 keeps that shape, the `Gone` answer for a deleted target included.
- **Reference:** `af_fleet_runtime/src/setup/` and `afd_setup/src/setup_run/report.rs` — the proposal rules and the report check that applies them again; §3 moves both onto the fleet's report.
- **Reference:** `afd_setup/src/creation/revision/{apply,persist}.rs`, `af_fleet_runtime/src/definition/edit/` and `afd_setup/src/creation/schedules.rs` — a fleet's next revision written under `If-Match`, the op applier, and a trigger turned into a schedule; §4 reuses all three without the draft and the digest.
- **Divergence:** none from the HTTP standard. The CLI keeps its six practices (SPEC-PRIOR): layered command, handler and error; handlers that neither print nor exit; one renderer; structured JSON errors; three test tiers; JSON when stdout is not a terminal.

## Sections (implementation slices)

Build order: §1, then a doc-only commit of the ruled design, then §2, §3, §4, §5, §6, §7. The old and new create bodies are both read until §7, as are the draft routes, so each Section lands green; §7 deletes what the clients stopped calling in §5 and §6. Every Section assumes option 1 of each decision.

### §1 — Indy rules on D1 to D5

No code. Each ruling is put through AskUserQuestion with its picture, and the spec is amended before it leaves `pending/`. R1 here is M255_001's R1, carried as P0 (SPEC-MOVE).

- **Dimension 1.1** — who writes setup is chosen → Test `setup_authority_signed_off` (manual): Indy answers D1. Expected: his quote in Discovery and "OPEN" replaced under D1. Evidence: this spec, in the Pull Request.
- **Dimension 1.2** — the fate of `/setup` is chosen → Test `setup_route_signed_off` (manual): Indy answers D2. Expected and evidence as 1.1.
- **Dimension 1.3** — the starting setup is chosen → Test `starting_setup_signed_off` (manual): Indy answers D3 and, for option 1, names the starter's tools, network rule and daily budget. Expected and evidence as 1.1.
- **Dimension 1.4** — DONE (his quote is in Discovery) — direct task creation is chosen → Test `direct_task_signed_off` (manual): Indy answers D4. Expected and evidence as 1.1.
- **Dimension 1.5** — DONE (every verdict cell reads `RULED`; his quote is in Discovery) — every schema row is ruled → Test `schema_convergence_signed_off` (manual): Indy answers D5. Expected: every verdict cell of `docs/architecture/schema_convergence.md` starts with `RULED`, and Discovery quotes him.

### §2 — `POST W/fleets` creates an active fleet with its first turn

One transaction writes the fleet, its thread, revision 1 and the turn, and commits before the wake and the workspace notice are published. **State authority:** `fleet.fleets` and `uq (workspace_id, name)`. **Guarded transition:** the replay receipt under scope `<workspace>:fleet:create:<person>` is written first, so a repeat reads the saved answer. **Failure assumption:** a daemon that dies after the commit leaves a queued turn; the next daemon's boot gap re-reads the queue (M255_001 D8). A body with `definition` and no `message` creates the fleet with no turn; it is a person's write, held to the same rule as `PATCH F`.

What each activation check becomes:

| Check today (`afd_setup/src/creation/progress.rs:46-67,100-110`) | After |
|---|---|
| `source`: instructions are not empty | gone; a fleet runs with empty instructions |
| `policy`: the semantics marker | stays in the message admit (`afd_turns/src/sql/admit_one.sql:17,47`) |
| `model` | read at create (409 `UZ-SETUP-008`) and at assignment, as a fleet run reads it today |
| `wallet` | read at assignment; the turn is refused with its reason, as a fleet run is today |
| `credentials` | read when the run is issued; the refusal is the turn's failure |
| `connections`, `grants` | the turn parks at first use; a person answers in the approvals inbox |
| `stream`, `activation` | deleted with `installing` |

- **Dimension 2.1** — one message, four rows, one transaction → Test `a_first_message_creates_an_active_fleet_with_its_turn` (integration, `afd_db::test_util::statements`): `{message:"watch checkout errors", operation_id}` → 201; one `fleet.fleets` row `active`, one thread whose id is the fleet's, revision 1 owned by the fleet, one `queued` turn with actor `steer:<subject>`; the four writes begin one transaction. Red when the turn is written in a second.
- **Dimension 2.2** — a repeat creates nothing → Test `a_repeated_create_answers_the_same_fleet` (integration): 100 callers from a barrier with one `operation_id` → one fleet, one turn, 99 answers with `replayed: true`; the same key with other text → 409 `UZ-SETUP-005`, the receipt's own code (`afd_setup/src/error/mod.rs:57`); the same key after the fleet is deleted → 410.
- **Dimension 2.3** — no model, no rows → Test `a_create_with_no_usable_model_writes_nothing` (integration): a workspace with no eligible model → 409 `UZ-SETUP-008`, and no fleet, thread, revision or turn row.
- **Dimension 2.4** — a taken name is refused → Test `a_taken_name_refuses_the_create` (integration): an explicit taken `name` → 409 `UZ-AGT-006` and no row; with no `name` two creates get two different `fleet-…` names.
- **Dimension 2.5** — a Definition creates a fleet → Test `a_definition_creates_a_fleet_without_a_message` (integration): `{definition, operation_id}` from a signed-in person → 201 `active`, no turn; a Definition the parser refuses → 400 with the parser's code and no row; an API key → 403.
- **Dimension 2.6** — nothing is private → Test `a_second_member_reads_a_new_fleet_at_once` (integration): another member's `GET F` and `GET F/messages` answer 200 before any run.
- **Dimension 2.7** — an empty wallet fails the turn, not the create → Test `a_first_turn_with_no_balance_fails_with_its_reason` (integration): balance 0 → 201, then the turn is refused `balance_exhausted` and the fleet stays `active`.
- **Dimension 2.8** — the answer arrives in the fleet's chat → Test `a_first_message_is_answered_on_the_fleet_stream` (e2e, `af_e2e`): two daemons, the real bus and Postgres, a native runner; the create, then `GET F/turns/stream` delivers the turn's completion. A second case drops the wake and restarts a daemon; the turn still completes.

### §3 — A `/setup` turn writes prose, and nothing else

`POST F/messages` gains `setup: true`, accepted only from a person's message. A flagged message is never folded: it queues and heads its own run. That run's payload adds `propose_setup` to the head revision's tools. The tool writes `description` and `instructions_md`; any other path is refused in the runner and again at the report, by the same `af_fleet_runtime::setup` function. The report re-reads the head turn's flag and producer, because a tenant's runner is not trusted. An accepted proposal becomes the next revision in the report's transaction, with a `setup_changed` activity turn. A refused proposal leaves the head as it was, and the turn still completes with its answer and the refusal's code.

- **Dimension 3.1** — a flagged turn changes prose → Test `a_setup_turn_rewrites_the_instructions` (integration): a report with an instruction edit → revision 2 is the head, the turn is settled with its answer, one activity turn.
- **Dimension 3.2** — an ordinary turn holds no setup tool → Test `an_unflagged_run_is_offered_no_setup_tool` (unit): a person's plain message → `ExecutionPolicy.tools` lacks `propose_setup`, and a report carrying a proposal for it is refused.
- **Dimension 3.3** — only a person can flag → Test `a_machine_cannot_send_a_setup_message` (integration): `setup: true` from an API key → 403; a webhook body containing `/setup` is an ordinary turn with no tool.
- **Dimension 3.4** — a flagged message is not folded → Test `a_setup_message_heads_its_own_run` (integration): sent while a run is live → disposition `queued`, never `folded`.
- **Dimension 3.5** — access is not writable → Test `a_proposal_that_touches_access_is_refused` (unit): an op on any `/config` path → refusal kind `access_not_writable`, one case per access key. Its twin `a_report_cannot_widen_access` (integration) sends that proposal in a report: the head stays, the turn completes.
- **Dimension 3.6** — a moved base writes nothing → Test `a_stale_base_proposal_writes_no_revision` (integration): a person's `PATCH F` commits between issue and report → the turn completes with `UZ-SETUP-003` as its save error, and the head is the person's revision.

### §4 — A person changes access with `PATCH F`, and `/setup` shows it

`PATCH F` gains `config_ops`, `instruction_edits` and `runtime_model_ref`, the types `PATCH D` takes today. A valid edit is the next revision at once, with a `setup_changed` activity turn naming the person. No approval row is written for it. A cron trigger writes `fleet.schedules` in the same transaction, as Save does today. The setup page reads `GET F` and writes `PATCH F`.

- **Dimension 4.1** — an edit is live at once → Test `a_patch_writes_the_next_revision_at_once` (integration): add a network host → 200, revision N+1 is the head, the next run's policy carries the host, `fleet.approval_gates` gains no row, one activity turn names the person.
- **Dimension 4.2** — a stale tag changes nothing → Test `a_stale_setup_tag_changes_nothing` (integration): an old `If-Match` → 412 with the current tag; the head is unchanged.
- **Dimension 4.3** — writing needs `fleet:write` → Test `a_reader_cannot_patch_setup` (integration): a `fleet:read` caller → 403, no revision.
- **Dimension 4.4** — a machine cannot write access → Test `an_api_key_cannot_patch_access` (integration): an API key with `fleet:write` sends `config_ops` → 403, no revision; its `{name}` patch still answers 200.
- **Dimension 4.5** — a run-time approval still parks → Test `an_ungranted_integration_credential_parks_its_turn` (integration): after a `PATCH` names an integration credential no gate exists; the first turn that uses it parks with one gate row, and an approval resumes it.
- **Dimension 4.6** — the page edits without a chat → Test `the setup page changes access with no conversation` (e2e, Playwright): open `/setup`, add a schedule, reload; the schedule shows and no `/drafts` request was sent.

### §5 — The app: the first message opens the fleet's chat

`/w/{ws}/fleets/new` renders the fleet composer. Send calls `POST W/fleets`, then replaces the URL with the fleet's page. A message typed as `/setup …` is sent with `setup: true`. The draft client, the unfinished-setups list, the Save flap and the pending-setup chat stop being rendered here and are deleted in §7.

- **Dimension 5.1** — one message, one chat → Test `a first message opens the fleet's chat` (e2e, Playwright): type on New fleet → the URL is `/fleets/{id}`, the answer renders, the sidebar lists the fleet.
- **Dimension 5.2** — a refused create keeps the text → Test `a refused create returns its text to the composer` (unit): a 409 → the composer holds the message with Resend, and no fleet is listed.
- **Dimension 5.3** — no draft surface → Test `the new fleet page lists no unfinished setups` (unit): the page sends no `/drafts` request.
- **Dimension 5.4** — the composer flags a setup message → Test `a slash setup message is sent flagged` (unit): `/setup be brief` → the body is `{message:"be brief", setup:true}`; `/setup` alone opens the setup page and sends nothing.

### §6 — The CLI: create a fleet and chat with it

`agentsfleet fleet create ["<message>"] [--definition <path>] [--name] [--model]` and `agentsfleet fleet setup <id|@name> [--set <pointer>=<json>]…`, which prints the head revision or sends one `PATCH F`. `agentsfleet chat [<id|@name>]` talks to a fleet; with no fleet its first line creates one, and `/setup …` sends a flagged message. `--review`, `--save`, `--apply` and the draft subcommands go.

- **Dimension 6.1** — create from the terminal → Test `fleet create prints the fleet and its first turn` (e2e, subprocess against a live daemon): `fleet create "watch checkout errors" --json` → `{fleet_id, turn_id}`, exit 0.
- **Dimension 6.2** — a bad Definition is a structured error → Test `fleet create refuses an unreadable definition` (unit, handler): a malformed file → the JSON error with its code and a suggestion, exit 2, no request sent.
- **Dimension 6.3** — chat creates on its first line → Test `chat with no fleet creates one on the first line` (unit, in-process): one `POST W/fleets`, then the fleet stream.
- **Dimension 6.4** — setup from the terminal → Test `fleet setup sends one patch` (unit, handler): `--set /config/x-agentsfleet/network/allow/-='"api.example.com"'` → one `PATCH F` with the read tag; a 412 prints the current tag and exits 1.
- **Dimension 6.5** — a removed flag names its replacement → Test `chat --save is refused with the new command` (unit): exit 2, the message names `fleet create`.

### §7 — The draft, its run kind and `installing` are deleted

`setup.drafts`, the `/drafts` routes, `POST F/revisions`, `GET F/setup-context`, `Owner::Draft`, `SetupWork`, `RunKind::Setup`, the old create body, `Start`, the review digest and its two revision columns, `PRIVATE_RETENTION`, the install-time grant request and `FleetStatus::Installing` go, with every column that named a draft. `PatchStatus::Active` and `agentsfleet fleet activate` exist only to retry an `installing` fleet, so they go too. A thread's owner is its fleet. The runner protocol's `VERSION` moves once, here.

- **Dimension 7.1** — no draft route → Test `no_draft_route_is_mounted` (unit, `route_inventory.rs`): `INVENTORY` holds no `/drafts` path and `POST W/drafts` answers 404.
- **Dimension 7.2** — no draft column → Test `the_schema_names_no_draft` (integration): after migration no `setup` schema exists and no column is named `draft_id`.
- **Dimension 7.3** — a fleet is never installing → Test `a_fleet_is_active_or_archived` (unit): `FleetStatus::ALL` has two members, and a stored `installing` fails to parse.
- **Dimension 7.4** — a thread without a fleet is refused → Test `a_thread_needs_its_fleet` (integration): an insert into `thread.threads` with a NULL `fleet_id` fails its constraint.
- **Dimension 7.5** — no draft owner on the wire → Test `a_run_payload_names_no_draft_owner` (unit): `{"owner":{"kind":"draft"}}` fails to decode, and the old create body is refused as an unknown field.

## Interfaces

```
POST W/fleets     {message, operation_id, model_ref?, name?}  |  {definition, operation_id, message?, model_ref?}
                  201 {fleet: Fleet, accepted?: {turn_id, disposition}, replayed: false}; a repeat answers 200 with replayed: true
                  Location: F. The body's operation_id is the retry key. model_ref absent means the workspace's default model.
                  fleet:write; a `definition` body also needs a signed-in person
POST F/messages   {message, operation_id?, setup?}; setup: true needs a person's message and is never folded
PATCH F           If-Match (the Fleet's etag); {status?: "archived", name?, config_ops?, instruction_edits?, runtime_model_ref?}; 200 Fleet
                  fleet:write; the three edit fields also need a signed-in person
removed in §7     W/drafts and every D route; POST F/revisions; GET F/setup-context; SaveSource; Start; review_digest;
                  the fleet status `installing`; the Fleet fields `start` and `setup.state`; the `stream` and `activation` checks
runner wire       Owner has one variant; SetupWork is gone; a flagged run's report may carry setup.proposal; VERSION moves once
```

`Fleet.setup.checks` stays as the readiness read the setup page shows. The CLI commands are §6's.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Replayed create | failure | a retry or a double click | the saved fleet and turn, `replayed: true`; other text → 409 `UZ-SETUP-005`; a deleted fleet → 410 | `a_repeated_create_answers_the_same_fleet` |
| No usable model | failure | no eligible model in the workspace | 409 `UZ-SETUP-008`; nothing written | `a_create_with_no_usable_model_writes_nothing` |
| Taken name | failure | `uq (workspace_id, name)` | 409 `UZ-AGT-006`; nothing written | `a_taken_name_refuses_the_create` |
| Malformed Definition | failure | the parser refuses it | 400 with the parser's code; nothing written | `a_definition_creates_a_fleet_without_a_message` |
| Empty wallet | capacity | the payer has no balance | the create succeeds; the turn is refused with its reason | `a_first_turn_with_no_balance_fails_with_its_reason` |
| Lost wake or dead daemon | failure | a crash between the commit and the wake | the turn stays queued; the boot gap assigns it; the client's retry replays | `a_first_message_is_answered_on_the_fleet_stream` |
| A run widens its own access | security | a hostile page or message steers the model | the proposal is refused in the runner and at the report | `a_report_cannot_widen_access` |
| A machine asks for a setup change | security | an API key or a webhook body | 403 for a flag; no tool for a body | `a_machine_cannot_send_a_setup_message` |
| A machine writes access | security | an API key with `fleet:write` | 403; no revision | `an_api_key_cannot_patch_access` |
| Race: person against run | failure | `PATCH F` commits during a `/setup` run | the run's proposal is refused; the person's revision stays | `a_stale_base_proposal_writes_no_revision` |
| Stale tag | failure | two people edit setup | 412 with the current tag | `a_stale_setup_tag_changes_nothing` |
| Missing grant | failure | a run reaches an ungranted integration | the turn parks; its thread waits for the answer | `an_ungranted_integration_credential_parks_its_turn` |

## Invariants

1. A thread belongs to one fleet — `thread.threads.fleet_id` is `NOT NULL` with `id = fleet_id` checked in the schema (Dimension 7.4).
2. A run never writes access — `af_fleet_runtime::setup` refuses every `/config` path, and the report calls the same function (Dimension 3.5).
3. Only a run whose head turn a person flagged can change setup — the daemon's report re-reads that turn's `setup` flag and passes it through `check_human` before it writes a revision (Dimensions 3.2 and 3.3).
4. No code names a draft owner — `Owner` has one variant, so a draft branch does not compile (Dimension 7.5).
5. A fleet and its first turn commit together — the create handler passes one transaction to all four writes, and the statement counter sees one begin (Dimension 2.1).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `fleet_created` | product | the create commits, once per fleet | `fleet_id`, `source` (`message` or `definition`), `origin` (`cli`, `dashboard`) | no message text, name or Definition | `fleet_created_fires_once_per_create` |
| `fleet_setup_changed` | product | a revision past the first commits | `fleet_id`, `revision`, `by` (`run` or `person`) | no instruction or config text | `a_setup_change_is_counted_once` |
| `setup_proposal_refused` (existing log) | operator | a report's proposal is refused | `error_code`, `fleet_id`, `run_id` | no proposal body | `a_report_cannot_widen_access` |

The draft steps of `SetupStep`, `fleet_setup_finished` and `fleet_setup_cancelled` are removed, and `docs/architecture/product_analytics.md` changes in the same Pull Request.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Indy ruled on D1 to D5 and on every schema row (§1; carries M255_001 R1) | `git grep -n -e "^### D[1-5] .* OPEN$" -e "UNDECIDED .$" -- docs/v2/active docs/architecture/schema_convergence.md` | no output | P0 | |
| R2 | A first message creates one active fleet with its turn, once (§2) | `make test-integration-rustd` | exit 0; `a_first_message_creates_an_active_fleet_with_its_turn ... ok`, `a_repeated_create_answers_the_same_fleet ... ok` | P0 | |
| R3 | The first answer arrives on the fleet's stream, a lost wake included (§2) | `make test-integration-rustd` | exit 0; `a_first_message_is_answered_on_the_fleet_stream ... ok` | P0 | |
| R4 | Only a person's `/setup` turn changes prose, and no run widens access (§3) | `make test-integration-rustd` | exit 0; `a_setup_turn_rewrites_the_instructions ... ok`, `a_machine_cannot_send_a_setup_message ... ok`, `a_report_cannot_widen_access ... ok` | P0 | |
| R5 | A person's edit is the next revision at once, and a machine's is refused (§4) | `make test-integration-rustd` | exit 0; `a_patch_writes_the_next_revision_at_once ... ok`, `an_api_key_cannot_patch_access ... ok` | P0 | |
| R6 | The app opens the fleet's chat on the first message (§5) | `make dry-app` | exit 0; `a first message opens the fleet's chat` passed | P0 | |
| R7 | The CLI creates a fleet and edits its setup (§6) | `make test-unit-all` | exit 0; the §6 tests pass | P1 | |
| R8 | No draft, review, Save or `installing` remains (§7) | the Dead Code Sweep commands | no output from any | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the draft table and its columns | `git grep -n -e "FROM setup\.drafts" -e "INTO setup\.drafts" -e "UPDATE setup\.drafts" -e "JOIN setup\.drafts" -e draft_id -- rustd ui/packages cli public` | no output |
| the draft run kind | `git grep -n -e "Owner::Draft" -e SetupWork -e "RunOwner::Draft" -e "RunKind::Setup" -- rustd` | no output |
| review and Save | `git grep -n -e review_digest -e draft_candidate_id -e SaveSource -e save_requested -- rustd ui/packages cli public` | no output |
| expiry and `installing` | `git grep -n -e PRIVATE_RETENTION -e "FleetStatus::Installing" -e setup_stream_ready -e "AGENTSFLEET_STATUS.INSTALLING" -e "PatchStatus::Active" -e request_for_setup -- rustd ui/packages cli` | no output |
| the draft client and its components | `git ls-files "ui/packages/app/lib/api/draft*" "ui/packages/app/components/domain/setup/conversation/Draft*" ui/packages/app/components/domain/setup/conversation/UnfinishedSetups.tsx ui/packages/app/components/domain/setup/saving` | no output |

## Out of Scope

- The schema folds D5 ruled in (rows 3, 10, 11, 12 and 14 fold; rows 2, 6 and 8 take them): M256_002. Rows 1 and 4 are §7 here.
- A tenant route that creates a durable task: its own spec, if D4 is ruled that way.
- A timer wake for a waiting durable task, so that a task could "watch": not planned; a trigger does it.
- Billing on the call path, and runner checkpoints (`runner.checkpoint_*`): Indy's separate tracks.
- Moving today's private draft transcripts: the dev database is torn down at merge.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a person types one sentence on New fleet and reads the fleet's answer in that fleet's chat, with nothing to review or save.
2. **Preserved user behaviour** — messages, stops, triggers, schedules, memory, run-time approvals, durable tasks, archive and delete on a fleet work as they do today.
3. **Optimal-way check** — the most direct way is one call that writes the fleet and its turn. The gaps are who writes the setup afterwards (D1) and what the first run can reach (D3).
4. **Rebuild-vs-iterate** — rebuild the creation path and delete the draft; iterate everywhere a fleet already runs.
5. **What we build** — a create that takes a message or a Definition, a flagged setup message, a direct setup edit, a setup page, and their CLI commands.
6. **What we do NOT build** — a private stage, a setup approval card and an expiry (Indy ruled each out); a task route and schema folds (their own specs); a slug derived from the message (no crate in the workspace does it).
7. **Fit with existing features** — it builds on the fleet chat and the thread model. It must not destabilize run-time approvals, which become the one place a person approves.
8. **Surface order** — API first, then the app and the CLI, then the teardown, so every step lands green.
9. **Dashboard restraint** — the setup page shows readiness only for access the fleet declares.
10. **Confused-user next step** — a refused create names its code and the page that fixes it (models, billing); a fleet that lacks access says so in its answer with a link to `/setup`; `agentsfleet fleet setup` prints the same setup.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** seven Sections in build order: decisions, create, the setup turn, the person's edit, app, CLI, teardown. The teardown is last so each replacement lands green before its predecessor is deleted.
- **Alternatives considered:** the other options of D1 to D5, each with its cost in that decision; a hidden draft (rejected: it keeps what Indy ruled out); a fleet born `installing` and activated by its first run (rejected: a first message could not run); creating the fleet at the first run's report (rejected: the turn would have no thread); schema folds and a task route in this spec (moved out: neither serves the goal, and each needs its own review).
- **Patch-vs-refactor verdict:** a **refactor** because the draft owns a table, a run kind, nine routes and a client on each surface, and hiding it would keep all of them.

## Discovery (consult log)

- > Owner (Indy, Nov 24, 2026): "Today a user goes draft → setup → fleet → durable tasks. I want to start a fleet and keep chatting. The setup draft is confusing." — context: the goal.
- > Owner (Indy, Nov 24, 2026): "No private state: no \"private until approved\" draft phase." and "No setup approval step. Approvals happen on GitHub and in other tools, not in a setup card." and "No expiry. Archive already exists; the user decides." — context: the three rulings every listed option obeys.
- > Owner (Indy, Nov 24, 2026): "I'm still deciding (1) the design and (2) what happens to `/setup`. Put both in the spec as named open decisions, each with options and your recommendation." and "build nothing until I rule on them." — context: D1, D2 and the Batch line.
- > Owner (Indy, Nov 24, 2026): "`setup.drafts` (rows 1 and 4) is decided by this milestone, and the folds after it follow." — context: D5, and the folds as M256_002.
- > Owner (Indy, Nov 24, 2026): "D4 Agreed" — context: D4 option 1. A task is asked for in a message; a tenant task route gets its own spec.
- > Owner (Indy, Nov 24, 2026): "D1 why do you need to write a fleet setup?  How can we avoid that?", "D2 Refer D1", "D3 Why should it start with somethig?", "D5 I am not following here?" — context: questions, not rulings. D1 to D3 are being reshaped around a fleet that needs no authored setup; D5 is re-explained.
- > Owner (Indy, Nov 24, 2026): "Yes budget - approved payer's balance" — context: D3. A fleet with no budget of its own is bounded by the payer's balance.
- > Owner (Indy, Nov 24, 2026): "Tables merge more 14 down to 8" — context: D5 option 3, chosen in the dialog and repeated after the three Fable objections were put to him: a report deletes other runs' items, effects stop outliving their fleet, and the compaction save locks `thread.threads`. `M256_002` builds the folds with those as its tests.
- > Owner (Indy, Nov 24, 2026): "Ideally i wanna get rid of authoring drafts in a clean way?  no stale or unused optional ones just for the sake of it? How can we do that? How will the user experience be" — context: D1 and D2. No optional setup surface survives; the options are being reshaped around a fleet with no authored Definition.
- **Owner decisions** — D1: OPEN. D2: OPEN, follows D1. D3: OPEN, budget ruled. D4: RULED, option 1. D5: RULED, option 3. Each is put through AskUserQuestion; a question that times out is not an answer.
- **Consults** — four read-only source traces at `3fbd9e40e` (setup draft, fleet chat, durable tasks, app and CLI), by Explore agents; the author re-read every line this spec cites. Findings that shaped it: `Creations::materialise` has no production caller; an integration grant already parks at first use; a task has no timer wake; a run with no tools can only talk. The brainstorm's Goldfish swarm was not run: its own rule needs the owner's readiness answer first, and the review below used the same reviewers.
- **Model reviews** — EGM review of the first draft, four Claude Fable 5.2 Goldfish reviewers with no context, read-only at `3fbd9e40e`: comprehension 5 Blocker, 10 Major, 10 Nit; zoom-out 2, 7, 4; design 3, 5, 4; codebase-details 2, 9, 7. Each cited line was re-read before its disposition. **Fixed by rewriting:** the schema folds and the task route left the spec (zoom-out B1; design M4, M5); the starting setup became D3 and the budget left the agent defaults (zoom-out B2; design M1; codebase B1); `decision_policy` is access (design B1; zoom-out M2); the setup tool rides a flag the daemon checks, a flagged message is never folded, and the report re-reads the turn (design B2); an access edit needs a signed-in person and leaves an activity turn (design B3); each activation check has a stated fate (design M2); the teardown is last (zoom-out M3); the hidden draft and the private improvement chat are rejected outside the option lists (comprehension B4, B5); D5 names every row and each option's cost, and its fourth option says R1 moves (comprehension B1 to B3; design M5); the terms are defined before use. **Fixed in place:** three citations, the replay code, the `materialise` description, the sweep patterns and the kept modules (codebase B2, M3 to M9 and nits). **Left open, each to Indy:** a public proposal a person applies (design's option 1c) is not listed, because it is a setup card; a higher scope than `fleet:write` for access edits is not proposed, because Save sits on `fleet:write` today. The implementation-readiness reviewer and a re-audit run after the rulings, when the Sections are rewritten.
- **Open blockers** — D1 to D5, each resolved by Indy's ruling. The per-file blast list for `draft_id`, `setup.drafts` and `afd_setup`, written when D1 is ruled (SPEC-BLAST). Whether `network` can express the starter's rule (D3). `direction.md`'s "one reviewed Definition" sentence, reworded by Indy with the D1 ruling.
- **Discovery candidates, recorded and not scope** (Indy's list, Nov 24, 2026) — billing on the call path, as a per-call usage frame or a platform model gateway; a delivery claim lifetime derived from the retry budget; sandbox drain as a kernel event; a metrics sampler audit; the mailinator teardown sweep. From the M255 close-out: the docs pages `fleets/library.mdx` and `api-reference/error-codes.mdx` still describe plugins, whose routes are gone; the docs Makefile's 10-second `openapi.json` download cap times out on slow links; `cargo shear` warns about unlinked `af_fleet_runtime/src/setup/tests/{schema,settle}.rs`; three files over 350 lines (`afr_supervisor`'s `downlink/terminal_tests.rs` and `renew/tests.rs`, the daemon's `config/settings.rs`); renewal slack after one silent round is about 1.5 s, with clock skew and a doubled reconnect pause not budgeted.
- **Deferrals and transfers** — none out. In: M255_001 Dimension 1.1 and its R1, on Indy's words of Nov 24, 2026 quoted in that spec's Discovery, carried here as Dimension 1.5 and R1. Each further one needs an owner quote, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
