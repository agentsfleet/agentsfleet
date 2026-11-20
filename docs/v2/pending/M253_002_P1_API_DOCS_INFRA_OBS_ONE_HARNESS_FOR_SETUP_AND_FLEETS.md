<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M253_002: Setup and fleet turns run on one harness, guffbolt, in the runner

**Prototype:** v2.0.0

**Milestone:** M253

**Workstream:** 002

**Date:** Nov 19, 2026

**Status:** PENDING

**Priority:** P1, every setup message and every fleet turn passes through this harness

**Categories:** API, DOCS, INFRA, OBS

**Batch:** B2, after M253_001

**Depends on:** M253_001, whose threads, turns and runs this uses

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 19, 2026) from Indy's decisions of Nov 19, 2026

**Canonical architecture:** `docs/architecture/runner_fleet.md` and `docs/architecture/runner_checkpoint_recovery.md`; this spec adds setup runs to both

---

## Overview

**Goal (testable):** A runner's guffbolt loop answers a setup turn on a draft's thread, and the accepted proposal becomes the draft's next candidate.

**Problem:** A setup message and a fleet message reach the model by two different machines. Setup calls the model inside `agentsfleetd` with its own tables, while fleet turns run in a runner. Every setup fix lands twice, and the daemon holds model calls it should not.

**Solution summary:**
- **guffbolt stands alone.** It is a crate at `rustd/guffbolt` with no `af_*`, `afr_*` or `afd_*` dependency. Anyone can build on it, and its `tools` feature ships default tools.
- **One way to extend it.** Each tool takes its backend when it is built, and a startup config picks the tools (§9).
- **Setup is a run.** A setup turn on a draft's thread is assigned to a runner like a fleet turn. Its guffbolt run has one tool, `propose_setup`, and gets no sandbox.
- **One machine.** The daemon makes no model call.

**Design flow:**
1. **Send.** `POST …/drafts/{draft_id}/messages` writes a queued turn on the draft's thread and publishes its wake.
2. **Assign.** The daemon locks the draft's thread, raises its fence, writes a run with `draft_id` set, and sends `Down::Run`. The run's setup part carries the head candidate, its revision, the message and history.
3. **Run.** The supervisor starts one guffbolt run with `propose_setup`. It loads the draft's compacted context through one fenced verb (§6) and makes the model calls itself.
4. **Check.** `propose_setup` parses the proposed definition with `af_fleet_runtime::definition::Definition::parse` and checks the base revision. A refusal goes back to the model as the tool result, and the run goes on.
5. **Report.** The report carries the answer, usage and the kept proposal. The daemon checks the proposal again in the report transaction, writes candidate revision N+1, and settles the run and the turn.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m253): one harness for setup and fleet turns
- **Intent (one sentence):** A person shaping a fleet in the setup tab is answered by the runner loop that later runs that fleet, so the two cannot drift.
- **Authoring handshake:**
  - **What Indy asked for:** guffbolt "must be a separate crate or a library outside the harness/ folder" and be "pushed as separate crate so anyone can use and build on". Setup is "an incarnated version of fleet with tools".
  - **ASSUMPTIONS I'M MAKING:**
    1. **Setup runs reserve nothing.** Usage is charged at report, as fleet turns are.
    2. **No retry cap.** The proposal loop has no cap inside a run, and a setup run's 300-second maximum runtime bounds it.
    3. **The old setup tables go.** Nothing reads them after this spec, so they are deleted.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/guffbolt/src/drive/mod.rs` — the loop: one model call, then its calls in order, until an answer, a question or a stop.
2. `rustd/crates/daemon/afd_authoring/src/setup_lease/mod.rs` — the daemon half of a setup run, built on this branch; M253_001 moves it to `afd_setup`.
3. `rustd/crates/protocol/af_wire/src/protocol/mod.rs` — the runner protocol a setup run rides.
4. `rustd/crates/protocol/af_fleet_runtime/src/definition/mod.rs` — `Definition::parse` at line 111, the fleet rules `propose_setup` applies.
5. `/Users/kishore/Projects/oss/rs/codex/codex-rs/core/src/tools/handlers/apply_patch.rs` — Codex's edit tool, which returns a refused edit to the model as the tool result.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/guffbolt/**` | CREATE | the harness: loop, tools, sub-agents, window hook, extensions, observer, README |
| `rustd/guffbolt/src/tools/propose/**` | CREATE | the proposal tool and its `Document` trait (§4) |
| `rustd/guffbolt/src/tools/builder/**` | CREATE | `ToolsBuilder`, `CallRef` and the startup tool config (§9) |
| `rustd/guffbolt/src/tools/{schedule,message,notify}/**` | CREATE | generic cron, schedule, message and notify tools over their backends (§9) |
| `rustd/guffbolt/src/tools/web/pushover.rs` | DELETE | Pushover becomes a `Notifier` backend in `afr_tools` (§9) |
| `rustd/crates/runner/afr_tools/src/verbs/**`, `src/pushover/**`, `src/generic/memory.rs` | EDIT | agentsfleetd-backed `Scheduler`, `Channel` and `Notifier` backends replace the verb tools; `MemoryBackend` folds into guffbolt's `MemoryStore` (§9) |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | the `guffbolt` member |
| `rustd/crates/harness/**` | DELETE or EDIT | pieces now in guffbolt leave; the rest depends on guffbolt |
| `rustd/crates/runner/afr_agent/**` | EDIT | embeds guffbolt with the runner's conversation, request and observer |
| `rustd/crates/runner/afr_tools/**` | EDIT | handlers adapt guffbolt tools; `SandboxEnv`, `Egressed` and `Leased` implement its traits; the `FleetSetup` document |
| `rustd/crates/runner/afr_supervisor/src/**` | EDIT | runs a setup run: no sandbox, `propose_setup` only, setup report; the remote model-call client goes |
| `rustd/crates/protocol/af_wire/src/**` | EDIT | the setup part of `RunPayload` and of the report; the context verb |
| `rustd/crates/daemon/afd_setup/src/**` | EDIT | the only setup path: assign, report, context verb, refused-proposal record |
| `rustd/crates/daemon/afd_setup/src/model/**` | DELETE or MOVE | model selection, balance gate and tool schema move to `src/run/**`; the rest is deleted |
| `rustd/crates/protocol/af_fleet_runtime/src/config/trigger/**`, `rustd/crates/daemon/afd_cron/src/validate.rs` | EDIT | one shared cron check |
| `rustd/crates/daemon/afd_gateway/src/**` | EDIT | no setup model frames |
| `rustd/crates/daemon/afd_bench/src/lane/**` | EDIT | no lanes for the daemon's model calls |
| `schema/*_setup_attempts.sql`, `*_setup_dispatches.sql`, `*_setup_activity_receipts.sql`, `*_setup_answer_receipts.sql`, `*_setup_replay_receipts.sql`, `*_setup_edit_decisions.sql`, `*_billing_setup_report_receipts.sql` | DELETE | tables of the daemon's model calls, named as M253_001 leaves them |
| `rustd/crates/daemon/afd_db/src/migration.rs` | EDIT | the shorter migration list |
| `rustd/crates/protocol/af_context/src/repository/**` | EDIT | load and save through the fenced verb (§6) |
| `deploy/fly/agentsfleetd-dev/agentsfleetd.toml`, `deploy/fly/agentsfleetd-prod/agentsfleetd.toml` | EDIT | drop `[inference]` and `[authoring.turn]`; Indy approved this edit |
| `rustd/crates/**/tests/**` | EDIT or DELETE | tests of the daemon's model calls leave; setup run cases replace them |
| `public/openapi.json` | EDIT | regenerated; the `RunnerPatch` change ships in its own commit (§7) |
| `codecov.yml` | EDIT | rust-afd paths include `rustd/guffbolt/` (decision D12) |
| `.orly/LOCAL.md` | EDIT | the layer rule names `rustd/guffbolt` as the one crate outside `rustd/crates/` (decision D12) |
| `docs/architecture/runner_fleet.md`, `runner_protocol.md`, `billing_and_provider_keys.md`, `docs/AUTH.md` | EDIT | setup runs; setup keys ride the run |
| `docs/v2/pending/M253_002_P1_API_DOCS_INFRA_OBS_ONE_HARNESS_FOR_SETUP_AND_FLEETS.md` | CREATE | this spec, moved to `active/` then `done/` |
| `CHANGELOG.md` | EDIT | setup on runners |
| `~/Projects/docs` branch `chore/m253-one-harness-changelog` | EDIT | runner protocol page and changelog |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE for every `af*` crate. guffbolt uses no `af_core` error shell: its core is generic over the embedder's `Conversation::Failure` (`guffbolt/src/conversation.rs:26`), and `tools` declares its own kinds (`guffbolt/src/tools/env/error.rs:10`).
- **`domains/sql.md`** — one file per table; edited in place before the first production deploy
- **`domains/architecture.md`** — layer folders; `runner/` and `daemon/` never depend on each other; guffbolt sits outside `crates/`
- **`domains/http-surface.md`** — OpenAPI regenerated with `--no-banner`
- **`tests.md`** — a negative test per failure mode; integration tests across the runner protocol
- **`domains/logging.md`** — every refusal logs its `error_code`

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `orly gate work` conform | yes, every commit | commit whole files; fix findings, never skip hooks |
| `make lint-all` | yes | clippy with warnings denied across the workspace and guffbolt |
| `docs.language` | yes, architecture pages | short sentences and paragraphs |
| codecov patch (95%) and rust line floor (98%) | yes | guffbolt seams and setup run cases cover the new lines |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | one file per guffbolt tool; the setup run splits assign, run and report |

## Prior-Art / Reference Implementations

- **Reference:** `~/Projects/oss/rs/codex/codex-rs` — tools an app declares, and `apply_patch` handing a refused edit back to the model. Mirror the refusal. Diverge on transport: guffbolt runs inside the runner's process, not behind JSON-RPC.
- **Reference:** `~/Projects/oss/pi` — the agent core ships no tools, and the coding agent adds them. Mirror it: guffbolt's core has none, and `tools` adds them.
- **Reference:** M247_001's fenced run path — mirror assign, renew, report and expiry. Diverge: a setup run has no sandbox and no checkpoint.

## Sections (implementation slices)

### §1 — guffbolt is a standalone crate with its own tools

The crate builds alone from its own manifest and names no repository crate. Its core is the loop, typed tools, sub-agents, the window hook, extensions and the observer.

The `tools` feature adds files, `ls`, `find`, `grep`, shell, web, memory, a Model Context Protocol (MCP) client, `ask_user`, `tool_search`, clock and plan. agentsfleet adapts these tools and never forks them.

guffbolt's existing crate, tools and error shape stay; M253_001 renames its per-call `Turn` to `Step`. What changes here is listed per Section.

- **Dimension 1.1** — guffbolt names no repository crate → Test `guffbolt_manifest_names_no_repository_crate` (unit): parse `rustd/guffbolt/Cargo.toml`. Expected: no dependency name or `path` points at an `af_`, `afr_` or `afd_` crate.
- **Dimension 1.2** — guffbolt packages alone → Test `guffbolt_packages_without_the_workspace` (integration): `cargo package -p guffbolt --list`. Expected: exit 0, with `README.md` and no `crates/` path.
- **Dimension 1.3** — the core builds without tools → Test `guffbolt_core_builds_without_tools` (integration): `cargo check -p guffbolt --no-default-features`. Expected: exit 0.
- **Dimension 1.5** — an unknown tool answers the model → Test `a_call_naming_no_offered_tool_answers_not_offered` (unit): the model calls `ghost`. Expected: a result starting `[tool_not_offered]`, and the loop goes on.

### §2 — guffbolt's local defaults are safe outside a sandbox

A standalone embedder gets `LocalEnv`, `LocalOutbound` and `LocalMemory`.

`LocalEnv` starts each process in its own process group with a cleared environment, and a pseudo-terminal (PTY) when asked. A stop or timeout kills the whole group.

`LocalOutbound` refuses private, loopback, link-local, unique-local, CGNAT, unspecified and multicast addresses after resolution, through a `reqwest` DNS resolver hook that tests can replace. A new `[egress_denied]` code joins `tools/code.rs`. MCP over HTTP admits its URL through `Outbound::connect`, and its own client installs the same resolver.

- **Dimension 2.1** — a timeout kills the group → Test `a_timed_out_command_leaves_no_child_running` (integration): `sh -c 'sleep 60 & sleep 60'` with a 1-second timeout. Expected: both sleeps are gone.
- **Dimension 2.2** — the environment is cleared → Test `a_local_command_sees_only_named_variables` (integration): a parent variable `SECRET_X`. Expected: `env` in the child prints no `SECRET_X`.
- **Dimension 2.3** — private names are refused → Test `local_outbound_refuses_a_name_resolving_to_loopback` (unit): a host resolving to `127.0.0.1`. Expected: `[egress_denied]`, and no connection opened.
- **Dimension 2.4** — redirects cannot escape → Test `local_outbound_refuses_a_redirect_to_a_private_address` (integration): a test resolver mapping a public name to a test server, which answers 302 to `https://10.0.0.1/`. Expected: refused, and the private host gets no request.
- **Dimension 2.5** — MCP over HTTP refuses private addresses → Test `mcp_http_client_refuses_a_private_address` (unit): an MCP server URL resolving to `10.0.0.1`. Expected: `[egress_denied]` before any request.

### §3 — A run waits for a person only at a quiet point

`ask_user` ends the run as `Waiting { call_id, question }`, and `DriveState::resume` continues it with the answer as that call's result. `call_id` is the model provider's id for one call; agentsfleet stores it on the call's item.

Today the loop ends `Waiting` at once (`drive/mod.rs:137-139`). It will first take guffbolt's pause (`agents/pause.rs`), which is granted only when no child runs, so no child work is lost or duplicated.

- **Dimension 3.1** — a question waits for children → Test `a_question_waits_for_running_children_before_the_run_ends` (unit): a child sleeping 50 ms and a root `ask_user`. Expected: the ending comes after the child's answer is recorded.
- **Dimension 3.2** — a resumed run gets the answer → Test `a_resumed_run_answers_the_question_call` (unit): resume with "yes". Expected: the next request holds one tool result for that call, text "yes".
- **Dimension 3.3** — one question per step → Test `a_second_question_in_one_step_is_refused` (unit): two `ask_user` calls in one reply. Expected: the second gets `[question_pending]`.

### §4 — `propose_setup` checks a proposal and hands refusals back to the model

guffbolt's proposal tool works over a `Document` trait with three methods. `schema()` is what the model sees, `apply(head, proposal)` applies the rules, and `base()` is the revision to match. agentsfleet's `FleetSetup` keeps today's proposal: `kind`, `base_candidate_id`, `base_candidate_revision`, `config_ops` and `instruction_edits`. It applies them with `af_fleet_runtime::definition::edit`, then `Definition::parse` and `validate_initial_generation`.

Cron syntax checking moves from `afd_cron/src/validate.rs` into `af_fleet_runtime`, so runner and daemon share one check.

An accepted proposal is kept for the report, and the next edit in the same run applies to it; the last accepted one is reported. A refused one returns its reason as the tool result, and the model may try again in the same run. The model's action, `initialize` or `edit`, is recorded on the usage line.

- **Dimension 4.1** — a valid edit is kept → Test `propose_setup_keeps_a_valid_edit_against_the_head` (unit): a cron trigger `{"type":"cron","schedule":"0 9 * * *","timezone":"UTC","message":"Daily check"}` at `/config/x-agentsfleet/triggers/-`, base 1. Expected: kept, with base 1.
- **Dimension 4.2** — an invalid definition answers the model → Test `propose_setup_returns_the_parse_failure_to_the_model` (unit): schedule `61 * * * *`. Expected: the result names the field, nothing is kept, and the loop goes on.
- **Dimension 4.3** — a stale base is refused → Test `propose_setup_refuses_a_stale_base_revision` (unit): base 1 while the head is 2. Expected: refused, naming revision 2.
- **Dimension 4.5** — cron checking is shared → Test `af_fleet_runtime_refuses_an_invalid_cron_schedule` (unit): `61 * * * *`. Expected: refused, and `afd_cron` calls the same function.
- **Dimension 4.4** — a corrected retry wins → Test `a_refused_proposal_corrected_in_the_same_run_is_reported` (integration): a refusal, then a valid proposal. Expected: the report carries only the valid one.

### §5 — The runner answers a setup turn

A setup run runs like a fleet run, except that it builds no sandbox, has only `propose_setup`, and reports the kept proposal.

`RunPayload.setup` keeps today's `SetupWork` fields: `sequence`, the candidate's `id`, `revision` and `definition`, `save_requested` and `tool`. The model key is carried in the run payload, and a tenant runner never receives a platform key.

The answer's text deltas reach the daemon, which publishes them on the thread's items subject and stores none. The daemon runs `Definition::parse` and the base check again in the report transaction before writing candidate revision N+1.

- **Dimension 5.1** — a setup turn gets an answer and a candidate → Test `a_setup_run_answer_writes_the_next_candidate` (integration): message 2 asking for a first version. Expected: the turn is answered, and the draft gains candidate revision 1.
- **Dimension 5.2** — no sandbox for setup → Test `a_setup_run_builds_no_sandbox` (integration): a setup run. Expected: no sandbox prepared, and the run still answers.
- **Dimension 5.3** — the report check is a safety net → Test `a_report_whose_proposal_fails_the_daemon_check_writes_no_candidate` (integration): a report carrying a definition the daemon refuses. Expected: no candidate, the turn answered with its text, and one `setup_proposal_refused` event.
- **Dimension 5.4** — a stale fence writes nothing → Test `a_superseded_setup_run_report_is_refused` (integration): a report at fence 1 after fence 2 was assigned. Expected: refused with `RUN_STALE_FENCING_TOKEN` (`UZ-RUN-005`), and no candidate.
- **Dimension 5.5** — an expired setup run is rerun → Test `an_expired_setup_run_reruns_the_turn` (integration): a runner that stops renewing. Expected: the turn is assigned again at the next fence, up to `af_core::limits::EVENT_LEASES_MAX` runs, then fails with `ProviderUsageUnknown`.
- **Dimension 5.6** — no platform key for a tenant runner → Test `a_tenant_runner_is_not_assigned_a_platform_key_setup_run` (integration): a draft using platform inference and a tenant runner. Expected: no assignment to that runner.

### §6 — A long setup keeps its compacted context

The draft's `memory.context_checkpoints` row holds the compacted setup history as an `af_context::Checkpoint`. The runner loads and saves it with `Up::ContextLoad` and `Up::ContextSave` on its connection. A save keeps today's `resource_version` compare-and-swap and also joins `thread.runs` on the run's fence, so a stale run is refused with `RUN_STALE_FENCING_TOKEN`.

guffbolt's window hook compacts when the window fills, and the saved state is the hook's output.

- **Dimension 6.1** — the context loads on the next turn → Test `the_next_setup_turn_loads_the_saved_context` (integration): turn 20 after a compaction. Expected: its first request holds the summary, not 19 raw messages.
- **Dimension 6.2** — a stale run cannot overwrite → Test `a_superseded_setup_run_cannot_save_context` (integration): a save at fence 1 after fence 2 saved. Expected: refused with `RUN_STALE_FENCING_TOKEN`, and fence 2's row unchanged.

### §7 — Setup's last gaps close

Two gaps close here:
- **Usage on notices.** A setup turn's notice reads usage from `billing.usage_lines` for its run, not from dispatch receipts (`observe/rows.rs:98-107`).
- **Refused proposals are recorded.** A proposal refused at report is logged with its `error_code` and kept as the turn's failure detail.

- **Dimension 7.1** — notices show run usage → Test `a_setup_notice_reports_the_runs_usage` (integration): a settled setup run with 1,200 input tokens. Expected: the notice shows 1,200.
- **Dimension 7.2** — a refused proposal is recorded → Test `a_refused_report_proposal_logs_its_error_code` (integration): §5.3's case. Expected: one `setup_proposal_refused` event with `error_code`, `draft_id` and `run_id`.

### §8 — The daemon makes no model call

Model selection, the balance gate and the `propose_setup` schema move from `afd_setup/src/model/**` to `afd_setup/src/run/**`. The rest of `model/**` is deleted: dispatch, provider calls, receipts, progress, compaction calls and the edit-intent veto. So are `afd_setup/src/lease/**`, `afr_supervisor/src/authoring/**` after its proposal code moves, the `Up` and `Down` setup model frames, `LeaseKind::Authoring`, `AcceptedKinds::AUTHORING`, the tables, and the bench lanes.

The daemon's `[inference]` and `[authoring.turn]` sections go; the 300-second setup runtime, `max_model_calls` and `max_proposal_corrections` become constants in `afd_setup`. The runner protocol becomes `agentsfleet.runner.v4` alone, and `Revision` and v2 are deleted.

**Implementation default:** the dev database is reset after merge, per Indy's "Rule holds, reset dev".

- **Dimension 8.1** — no setup model frame remains → Test `the_runner_protocol_has_no_setup_model_frames` (unit): every `Down` variant's tag. Expected: none carries a model call.
- **Dimension 8.2** — another protocol version is refused → Test `a_runner_on_another_protocol_version_is_refused` (integration): hello with an unknown version. Expected: `UZ-RUN-023`, and no run assigned.
- **Dimension 8.3** — no setting feeds setup runs → Test `setup_limits_are_constants` (unit): `afd_setup`'s limits. Expected: 300-second runtime from a constant, and no `TurnSettings` type remains.

### §9 — guffbolt is extended one way: each tool takes its backend

A backend is what a tool calls to do its work. Each tool takes its backend when it is built, as pi's tools take `BashOperations` (`coding-agent/src/core/tools/bash.ts:74`). No registry is looked up at call time.

`Tools::builder()` returns a `ToolsBuilder`, which builds the whole set a loop holds. Each setter follows the `with_*` convention: it takes the builder and returns it. `with_env`, `with_outbound`, `with_memory`, `with_scheduler`, `with_channel` and `with_notifier` set backends, and `with_tool` adds a tool already built with its own backend.

`build(&ToolsConfig)` returns `Result<Tools<ContextTool>, MissingBackend>`. It builds the tools the startup config names and refuses to start when one lacks its backend, naming both.

`Context` keeps only per-call facts: the `CallRef`, the run id and the wake. A third party writes a tool whose `new` takes its own backend, with no edit to guffbolt. To share such a tool, they publish its backend trait beside it, as guffbolt does with `Scheduler`.

The cron tools, `schedule` and `message` move into guffbolt as generic tools over `Scheduler` and `Channel`. agentsfleet builds them with backends over its fenced verbs, so its runs behave as today. Pushover leaves guffbolt and becomes a `Notifier` backend in `afr_tools`, which guffbolt's `notify` tool calls. `afr_tools`'s `MemoryBackend` folds into guffbolt's `MemoryStore`, so memory has one layer.

agentsfleet builds a run's tools when the run starts, because each run has its own sandbox, egress guard and memory.

Local defaults stay honest:
- `LocalScheduler` keeps jobs in a file under a directory the embedder names. It returns due jobs only when the embedder calls `due`.
- `LocalChannel` hands each line to a callback the embedder gives it.
- guffbolt ships no local `Notifier`, because every notifier is a vendor.

Each call gets a `CallRef`: its step, its index in the step, and the provider's call id. A backend derives its idempotency key from it, as agentsfleet's item `(run_id, sequence)` does in M253_003. A retried call keeps its `CallRef`, and so does a journalled call that a resume sends again. A resume that asks the model again makes new calls, which M253_003's recovery note covers.

Text the model reads names no host product: no "fleet", "workspace", "daemon" or "agentsfleet". Memory's `Reach` becomes `private` and `shared`.

- **Dimension 9.1** — a missing backend stops startup → Test `a_tool_without_its_backend_refuses_to_start` (unit): a config naming `cron_add` and a builder with no scheduler. Expected: an error naming `cron_add` and the scheduler, and no loop starts.
- **Dimension 9.2** — a third party's tool needs no guffbolt edit → Test `a_third_party_tool_takes_its_own_backend` (unit): a test trait `Crm`, its backend, and a tool built with it and added through `with_tool`. Expected: the tool's answer carries the backend's value.
- **Dimension 9.3** — the config picks the offered tools → Test `the_startup_config_offers_only_named_tools` (unit): `["files", "memory_store"]`. Expected: exactly those two are offered.
- **Dimension 9.4** — the local scheduler survives a restart → Test `local_scheduler_returns_a_due_job_after_reopening` (integration): add a job, drop the scheduler, reopen its directory, then call `due` past the moment. Expected: the job, once.
- **Dimension 9.5** — agentsfleetd backs a fleet's scheduling → Test `a_fleet_runs_cron_add_creates_its_schedule` (integration): a fleet run calls `cron_add`. Expected: one schedule for the fleet, as today.
- **Dimension 9.6** — Pushover is a notifier backend → Test `pushover_backend_sends_through_the_egress_guard` (unit, `afr_tools`): `notify` with the Pushover backend. Expected: one request to Pushover's host, through the run's egress guard.
- **Dimension 9.7** — a retried call keeps its reference → Test `a_retried_call_keeps_its_call_ref` (unit): a backend that fails once, then the retry. Expected: both attempts see the same `CallRef`.
- **Dimension 9.8** — the model reads no host product → Test `guffbolt_tool_specs_name_no_host_product` (unit): every built-in tool's spec. Expected: no "fleet", "workspace", "daemon" or "agentsfleet".

### §10 — Items record edits and the newest turn (from M253_001 §5)

`thread.items` gains the columns `thread_model.md` defines: `kind`, `tool`, `status`, `changes` and `run_id`. An `edit` item records its path and its added and removed line counts, computed by the runner's file tools, and the app and CLI read them instead of working them out from arguments.

Each input reaching a run carries its turn id. guffbolt notes the newest turn when a step begins and records it with the `CallRef`, and the daemon refuses an item naming a turn the run does not serve.

- **Dimension 10.1** (was M253_001 5.1) — an edit records its counts → Test `an_edit_item_records_path_and_line_counts` (integration): a run editing `src/a.rs` by +24 −8. Expected: one `edit` item with those counts.
- **Dimension 10.2** (was M253_001 5.2) — items name the newest turn → Test `items_after_a_second_message_name_the_second_turn` (unit): input arrives before step 3. Expected: steps 1 and 2 name turn 1, and step 3 names turn 2.

## Interfaces

- **Runner protocol:** `agentsfleet.runner.v4` alone. `Down::Run { run: RunPayload }`, where the payload names one owner, a fleet or a draft. A setup run's payload carries `setup { head_candidate_id, head_revision, message, history }`.
- **Setup report:** `ReportRequest.setup` carries the kept proposal as today's `SetupProposal`, with `base_candidate_id`, `base_candidate_revision` and the definition as JSON text, or none for a text-only answer.
- **Context verb:** `Up::ContextLoad { run_id, fence }` and `Up::ContextSave { run_id, fence, resource_version, checkpoint }`, answered in `Down`; a fence below the current one is refused with `RUN_STALE_FENCING_TOKEN`.
- **guffbolt public API:** `Tool<Cx>`, `Tools<T>`, `Document`, `Env`, `Outbound`, `MemoryStore`, `Ending::{Answered, Waiting, Stopped, Failed}` and `DriveState::resume`.
- **Backends:** `Tools::builder() -> ToolsBuilder`; `with_env`, `with_outbound`, `with_memory`, `with_scheduler`, `with_channel`, `with_notifier` and `with_tool`, each `(self, …) -> Self`; `build(&ToolsConfig) -> Result<Tools<ContextTool>, MissingBackend>`; the `Scheduler`, `Channel` and `Notifier` traits; and `CallRef { step, index, call_id }`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Invalid proposal | failure | the model proposes a definition `Definition::parse` refuses | the tool result names the field; the model may retry | `propose_setup_returns_the_parse_failure_to_the_model` |
| Stale base | race | another turn wrote revision N+1 first | refused, naming the head | `propose_setup_refuses_a_stale_base_revision` |
| Report check refuses | failure | the daemon's rules differ from the runner's build | no candidate; turn failed; event logged | `a_report_whose_proposal_fails_the_daemon_check_writes_no_candidate` |
| Runner dies mid-setup | failure | the run expires | rerun at fence +1, capped | `an_expired_setup_run_reruns_the_turn` |
| Stale report | replay | a superseded run reports late | 409; nothing written | `a_superseded_setup_run_report_is_refused` |
| Stale context save | replay | a superseded run saves late | 409; the row stays | `a_superseded_setup_run_cannot_save_context` |
| Platform key to a tenant runner | security | a tenant runner would get platform inference | not assigned | `a_tenant_runner_is_not_assigned_a_platform_key_setup_run` |
| Other protocol version | failure | a runner on another version connects | refused at hello | `a_runner_on_another_protocol_version_is_refused` |
| Runaway local command | capacity | a standalone command forks past its timeout | the group is killed | `a_timed_out_command_leaves_no_child_running` |
| Private egress | security | a standalone tool fetches a private address | refused | `local_outbound_refuses_a_redirect_to_a_private_address` |
| Missing backend | failure | a configured tool's backend was never set on the builder | startup refused, naming the tool and the backend | `a_tool_without_its_backend_refuses_to_start` |
| Retried effect | replay | a backend times out after acting | the retry keeps its `CallRef`, so the backend dedupes it | `a_retried_call_keeps_its_call_ref` |

## Invariants

1. A thread has exactly one owner. A `CHECK` in `schema/*_thread_threads.sql` requires exactly one of `fleet_id` and `draft_id`, and a run's owner is its thread.
2. A report or context save below the current fence writes nothing. The settle and save statements match `fencing_token` in their `WHERE`, and zero rows map to 409.
3. guffbolt depends on no repository crate. Dimension 1.1's test runs in `make test-unit-all`.
4. The daemon cannot use the provider clients. `afd_setup` depends on none of `af_providers`, `af_inference` and `rig-core`.
5. guffbolt's model-facing text names no host product. Dimension 9.8's test runs in `make test-unit-all`.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `setup_run_assigned` | operator | a runner is assigned a setup run | `draft_id`, `run_id`, `runner_id`, fence | no message text, no key | `a_setup_run_answer_writes_the_next_candidate` |
| `setup_proposal_refused` | operator | the report check refuses | `draft_id`, `run_id`, `error_code` | no definition body | `a_refused_report_proposal_logs_its_error_code` |
| `setup_run_settled` | product | a setup run reports | `draft_id`, outcome, duration, tokens | no message text | `a_setup_notice_reports_the_runs_usage` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | guffbolt builds and packages alone (§1) | `cd rustd && cargo package -p guffbolt --list` | exit 0 | P0 | |
| R2 | A setup turn is answered by a runner and writes a candidate (§5) | `make test-integration-rustd` | `a_setup_run_answer_writes_the_next_candidate` passes | P0 | |
| R3 | A refused proposal returns to the model, and a corrected one wins (§4) | `make test-integration-rustd` | `a_refused_proposal_corrected_in_the_same_run_is_reported` passes | P0 | |
| R4 | Stale reports and saves write nothing (§5, §6) | `make test-integration-rustd` | both `superseded` cases pass | P0 | |
| R5 | Local defaults contain processes and egress (§2) | `make test-unit-all` | `a_timed_out_command_leaves_no_child_running` passes | P1 | |
| R6 | The daemon calls no model (§8) | `git grep -n -w -E "af_providers|af_inference|rig-core" -- rustd/crates/daemon/afd_setup/Cargo.toml` | no output | P0 | |
| R7 | guffbolt is extended one way, and a missing backend stops startup (§9) | `make test-unit-all` | `a_tool_without_its_backend_refuses_to_start` passes | P0 | |
| S1 | Declared commands green | `make lint-all` | exit 0 | P0 | |
| S2 | Declared commands green | `make test-unit-all` | exit 0 | P0 | |
| S3 | Declared commands green | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Declared commands green | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `AuthoringModelResult` and the other setup model frames | `git grep -n -w -E "AuthoringModelResult|AuthoringActivityAck|AuthoringRenewed|AuthoringReportAck|AuthoringModel|AuthoringRenew|AuthoringReport" -- . ':!docs/v2' ':!bench/baselines' ':!CHANGELOG.md'` | no output |
| the daemon's setup model path | `git grep -n -E "mod model|crate::model" -- rustd/crates/daemon/afd_setup` | no output |
| the runner's remote model client | `git grep -n -E "mod authoring" -- rustd/crates/runner` | no output |
| `[authoring.turn]` | `git grep -n -F "[authoring.turn]" -- deploy` | no output |
| guffbolt's Pushover tool | `git grep -n -w "Pushover" -- rustd/guffbolt` | no output |
| `Context`'s three seam fields | `git grep -n -E "pub (env|outbound|memory):" -- rustd/guffbolt/src/tools/mod.rs` | no output |
| `afr_tools`'s second memory layer | `git grep -n -w "MemoryBackend" -- rustd` | no output |

## Out of Scope

- Human input as one primitive, effect keys, no task reservation, and email delivery: M253_003.
- A JSON-RPC server for guffbolt: not planned; the runner embeds it in-process.
- Inbound email: not planned.
- A backend for talking to other agents, and WebAssembly plugins: not in M253. The Model Context Protocol (MCP) client already loads tools at runtime.

---

## Product Clarity (authoring record)

1. **Successful user moment** — In the setup tab, "check every morning at 9" is answered, and the Summary shows "Next wake-up 09:00 UTC".
2. **Preserved user behaviour** — The setup tab's views, queued, thinking, answered and Save and publish, look and work as before.
3. **Optimal-way check** — The shortest path reuses the fleet run path with one tool; nothing shorter avoids two machines.
4. **Rebuild-vs-iterate** — Rebuild: the daemon's model path is deleted, not adapted.
5. **What we build** — guffbolt as a crate, `propose_setup`, setup runs in the supervisor, the context verb, and the daemon's model path removed.
6. **What we do NOT build** — a JSON-RPC server for guffbolt, and a sandbox for setup, which `propose_setup` does not need.
7. **Fit with existing features** — compounds with M247 respawn; it must not destabilise fleet runs, which share every statement.
8. **Surface order** — runner protocol first, then daemon, then docs; no screen change.
9. **Dashboard restraint** — no new screens; operator events only.
10. **Confused-user next step** — a refused setup names the field the model got wrong in the turn's failure detail, and the person rephrases.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** guffbolt first (§1–§3), then backends (§9), because the proposal tool is built through the builder. Then the proposal tool (§4), setup runs (§5), context (§6), gaps (§7), and removal last (§8).
- **Alternatives considered:**
  - **Codex app-server as a child process.** Its JSON-RPC boundary adds a process and a wire per run, and the runner must still adapt sandbox and egress, so it was rejected.
  - **Keep the daemon's model path and have the runner call it.** That keeps two paths to the model, which is the drift this spec removes.
- **Patch-vs-refactor verdict:** a **refactor** because the setup model path moves process and the harness moves crate.

## Discovery (consult log)

- **Consults** — Indy's decisions, Nov 19, 2026, quoted:
  - > Owner (Indy, Nov 19, 2026): "guffbolt must be a separate crate or a library outside the harness/ folder" — context: crate location.
  - > Owner (Indy, Nov 19, 2026): "I wanna really have guffbolt abstacted and will be pushed as separate crate so anyone can use and build on." — context: §1.
  - > Owner (Indy, Nov 19, 2026): "Tools inside guffbolt" — context: the `tools` feature.
  - > Owner (Indy, Nov 19, 2026): "Both: guffbolt tool, fleet rules (Recommended)" — context: §4.
  - > Owner (Indy, Nov 19, 2026): "I think this must feed or loop the model again like Codex or Pi" — context: §4, no retry cap.
  - > Owner (Indy, Nov 19, 2026): "Okay to mee the setup is just an incarnated version of fleet with tools?" — context: §5.
  - > Owner (Indy, Nov 19, 2026): "A: Rides the lease (Recommended)" — context: §5, setup keys ride the run.
  - > Owner (Indy, Nov 19, 2026): "Rule holds, reset dev (Recommended)" — context: §8.
  - > Owner (Indy, Nov 19, 2026): "Yes, same commit (Recommended)" — context: §8, deploy TOML edits.
  - > Owner (Indy, Nov 19, 2026): "Accept all (Recommended)" — context: decisions D4, D6, D8, D10 and D12.
    - **D4:** no retry cap, and a short setup runtime.
    - **D6:** the owner rides the run payload.
    - **D8:** `fleet_id` plus `draft_id` on the run.
    - **D10:** web and memory tools move into guffbolt.
    - **D12:** the codecov paths and the `.orly/LOCAL.md` layer rule.
  - > Owner (Indy, Nov 20, 2026): "Defer 5.1 to M253_002" and "Move 5.2 into M253_002 (Recommended)" — context: §10, transferred from M253_001.
  - > Owner (Indy, Nov 20, 2026): "Move to afr_tools (Recommended)" — context: §9, Pushover leaves guffbolt.
  - > Owner (Indy, Nov 20, 2026): "I think i need ability like memory_store, they are in guffbolt i presume, and the calls do api to save them? similar way the pushover and scheduler can be. When i start i can configure the needed (like pi or codex or zig's nullclaw or any ~/Projects/oss/rs/* equivalent)" — context: §9.
  - > Owner (Indy, Nov 20, 2026): "I need a common better way to do this." — context: §9, one extension pattern.
  - > Owner (Indy, Nov 20, 2026): "Yes i prefer the proposal" — context: §9, one extension pattern.
  - > Owner (Indy, Nov 20, 2026): "use standard rust specific like `with` or so, so its obvious on what the builder will retun. I assume the Tool builders builds more." — context: §9, `ToolsBuilder` with `with_*` setters that builds the whole `Tools` set.
  - > Owner (Indy, Nov 20, 2026): "is capability a good word or abstraction" and then "Backends via builder (Recommended)" — context: §9. "Capability" already has many meanings in `rustd` (auth, Linux sandbox capability sets, decision models, the runner's task capability), and a type-keyed registry hides a tool's needs until it is called. So each tool takes its backend when it is built.
  - > Owner (Indy, Nov 19, 2026): "donot do the lint, test-unit, test-integration check but rather do it at the end of every workstream. but for every dimension do cargo check, cargo fmt (clippy) so we know code compiles." — context: per-Dimension compile checks; S rows at the end of each Workstream.
  - > Owner (Indy, Nov 19, 2026): "Ensure the rust code for individual files is 99% and overall is 99% if possible or else stick to the numbers we have currently." — context: coverage targets 99% per file and overall, never below the 98% line and 95% patch floors.
  - > Owner (Indy, Nov 19, 2026): "E2E cloud burst" — context: Linux proof, paid, his go per run.
  - > Owner (Indy, Nov 19, 2026): "I think i did the elephant goldfish brainstrome before." and "youalready have the context" — context: orly-spec step 2.
- **Model reviews** — EGM Stage 9 (implementation readiness), three reviewers, Claude Opus 5.5, Nov 19, 2026: 9 Blockers, 39 Majors, 30 Nits. Every Blocker is resolved in the Sections as an implementation default, recorded in `.egm/implementation_readiness_report.md`; wording findings are applied, and none is declined.
- **Open blockers** — none.
- **Deferrals and transfers** — none.
