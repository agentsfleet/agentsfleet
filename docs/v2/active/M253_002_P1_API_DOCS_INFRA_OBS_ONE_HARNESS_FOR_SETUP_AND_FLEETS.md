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

**Status:** IN_PROGRESS

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
| `rustd/guffbolt/src/tools/builder/**` | CREATE | `ToolsBuilder` and the startup tool config (§9) |
| `rustd/guffbolt/src/transcript/mod.rs` | EDIT | `CallRef`, beside `Call`: `Conversation::dispatch` takes it, so it builds without the `tools` feature (§9) |
| `rustd/Cargo.toml` | EDIT | members listed by layer, `af_e2e` added, `af_inference` moved (§12) |
| `rustd/crates/protocol/af_context/**` | EDIT | no Postgres store, no `postgres` feature, no `sqlx` edge (§12) |
| `rustd/crates/daemon/afd_setup/src/context/**` | CREATE | the checkpoint store and its SQL, moved from `af_context` (§12) |
| `rustd/crates/harness/af_inference/**` | MOVE | from `protocol/` (§12) |
| `rustd/crates/e2e/af_e2e/**` | CREATE | the daemon-and-runner suites, the recovery bench and the layer tests (§12) |
| `rustd/crates/daemon/main/tests/**`, `rustd/crates/daemon/main/benches/recovery/**` | MOVE | runner-driving suites and the recovery bench to `af_e2e` (§12) |
| `rustd/crates/runner/main/examples/kernel_lane/**` | MOVE | from `afr_sandbox/examples/` (§12) |
| `rustd/crates/daemon/afd_db/tests/**`, `rustd/crates/daemon/afd_fleet/tests/**` | MOVE | `afd_db`'s statement tests, breaking the dev cycle (§12) |
| `rustd/crates/daemon/afd_http/src/**`, `rustd/crates/daemon/afd_api_*/src/**` | EDIT | one services trait per plane; no cross-crate re-exports (§12) |
| `rustd/crates/**/src/**` | EDIT | callers import each item from the crate that owns it (§12) |
| `make/test-unit.mk`, `make/test-integration-rustd.mk`, `make/bench.mk` | EDIT | lanes follow the moved crates (§12) |
| `.orly/LOCAL.md` | EDIT | the `e2e/` layer folder (§12) |
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
| `schema/*_setup_attempts.sql`, `*_setup_dispatches.sql`, `*_setup_activity_receipts.sql`, `*_setup_answer_receipts.sql`, `*_setup_edit_decisions.sql`, `*_billing_setup_report_receipts.sql` | DELETE | tables of the daemon's model calls, named as M253_001 leaves them. `setup_replay_receipts` stays: it is the HTTP idempotency store |
| `rustd/crates/daemon/afd_db/src/migration.rs` | EDIT | the shorter migration list |
| `rustd/crates/daemon/afd_db/schema/*_thread_items.sql`, `afd_fleet/src/run/sql/upsert_tool_calls.sql`, `afd_fleet/src/run/tool_detail/**`, `af_wire/src/tool_detail/**` | EDIT | items carry kind, tool, status, changes and run id, and name the newest turn (§10) |
| `rustd/crates/runner/afr_memory/src/lib.rs` | EDIT | `MemoryBackend` folds into guffbolt's `MemoryStore` (§9) |
| `rustd/crates/runner/main/src/host.rs`, `afr_agent/src/{offer,router}/**`, `afr_sandbox/examples/kernel_lane/**` | EDIT | tools are built per run through `ToolsBuilder` (§9) |
| `rustd/crates/daemon/afd_credential/src/**` | EDIT | the tool-support check selection calls moves here (§8) |
| `rustd/crates/daemon/main/src/serve/clients.rs` | EDIT | the storage client gets its own builder (§8) |
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

- **Dimension 1.1** — DONE (checked by `make lint-rustd`'s one line `cargo publish -p guffbolt --dry-run --allow-dirty --locked`, which packages guffbolt and builds it alone against crates.io; a planted `af_core` path dependency fails it) — guffbolt names no repository crate → Test `guffbolt_manifest_names_no_repository_crate` (unit): parse `rustd/guffbolt/Cargo.toml`. Expected: no dependency name or `path` points at an `af_`, `afr_` or `afd_` crate.
- **Dimension 1.2** — DONE (same check as 1.1) — guffbolt packages alone → Test `guffbolt_packages_without_the_workspace` (integration): `cargo package -p guffbolt --list`. Expected: exit 0, with `README.md` and no `crates/` path.
- **Dimension 1.3** — DONE (same check as 1.1) — the core builds without tools → Test `guffbolt_core_builds_without_tools` (integration): `cargo check -p guffbolt --no-default-features`. Expected: exit 0.
- **Dimension 1.5** — DONE — an unknown tool answers the model → Test `a_call_naming_no_offered_tool_answers_not_offered` (unit): the model calls `ghost`. Expected: a result starting `[tool_not_offered]`, and the loop goes on.

### §2 — guffbolt's local defaults are safe outside a sandbox

A standalone embedder gets `LocalEnv`, `LocalOutbound` and `LocalMemory`.

`LocalEnv` starts each process in its own process group with a cleared environment, and a pseudo-terminal (PTY) when asked. A stop or timeout kills the whole group.

`LocalOutbound` refuses private, loopback, link-local, unique-local, CGNAT, unspecified and multicast addresses after resolution, through a `reqwest` DNS resolver hook that tests can replace. A new `[egress_denied]` code joins `tools/code.rs`. MCP over HTTP admits its URL through `Outbound::connect`, and its own client installs the same resolver.

- **Dimension 2.1** — DONE — a timeout kills the group → Test `a_timed_out_command_leaves_no_child_running` (integration): `sh -c 'sleep 60 & sleep 60'` with a 1-second timeout. Expected: both sleeps are gone.
- **Dimension 2.2** — DONE — the environment is cleared → Test `a_local_command_sees_only_named_variables` (integration): a parent variable `SECRET_X`. Expected: `env` in the child prints no `SECRET_X`.
- **Dimension 2.3** — DONE — private names are refused → Test `local_outbound_refuses_a_name_resolving_to_loopback` (unit): a host resolving to `127.0.0.1`. Expected: `[egress_denied]`, and no connection opened.
- **Dimension 2.4** — DONE — redirects cannot escape → Test `local_outbound_refuses_a_redirect_to_a_private_address` (integration): a test resolver mapping a public name to a test server, which answers 302 to `https://10.0.0.1/`. Expected: refused, and the private host gets no request.
- **Dimension 2.5** — DONE — MCP over HTTP refuses private addresses → Test `mcp_http_client_refuses_a_private_address` (unit): an MCP server URL resolving to `10.0.0.1`. Expected: `[egress_denied]` before any request.

### §3 — A run waits for a person only at a quiet point

`ask_user` ends the run as `Waiting { call_id, question }`, and `DriveState::resume` continues it with the answer as that call's result. `call_id` is the model provider's id for one call; agentsfleet stores it on the call's item.

Today the loop ends `Waiting` at once (`drive/mod.rs:137-139`). It will first take guffbolt's pause (`agents/pause.rs`), which is granted only when no child runs, so no child work is lost or duplicated.

- **Dimension 3.1** — DONE — a question waits for children → Test `a_question_waits_for_running_children_before_the_run_ends` (unit): a child sleeping 50 ms and a root `ask_user`. Expected: the ending comes after the child's answer is recorded.
- **Dimension 3.2** — DONE — a resumed run gets the answer → Test `a_resumed_run_answers_the_question_call` (unit): resume with "yes". Expected: the next request holds one tool result for that call, text "yes".
- **Dimension 3.3** — DONE — one question per step → Test `a_second_question_in_one_step_is_refused` (unit): two `ask_user` calls in one reply. Expected: the second gets `[question_pending]`.

### §4 — `propose_setup` checks a proposal and hands refusals back to the model

guffbolt's proposal tool works over a `Document` trait with three methods. `schema()` is what the model sees, `apply(head, proposal)` applies the rules, and `base()` is the revision to match. agentsfleet's `FleetSetup` keeps today's proposal: `kind`, `base_candidate_id`, `base_candidate_revision`, `config_ops` and `instruction_edits`. It applies them with `af_fleet_runtime::definition::edit`, then `Definition::parse`. Instructions have one bound, 32,768 characters, for a first setup and an edit alike; the first draft's smaller 12,288 bound is removed. The proposal rules (normalisation, the person's wording, text anchors, the stale-base check and the first-setup check) live once, in `af_fleet_runtime::setup`; the daemon's attempt path calls them until §8 deletes it.

Cron syntax checking moves from `afd_cron/src/validate.rs` into `af_fleet_runtime`, so runner and daemon share one check.

An accepted proposal is kept for the report, and the next edit in the same run applies to it; the last accepted one is reported. A refused one returns its reason as the tool result, and the model may try again in the same run.

- **Dimension 4.1** — DONE — a valid edit is kept → Test `propose_setup_keeps_a_valid_edit_against_the_head` (unit): a cron trigger `{"type":"cron","schedule":"0 9 * * *","timezone":"UTC","message":"Daily check"}` at `/config/x-agentsfleet/triggers/-`, base 1. Expected: kept, with base 1.
- **Dimension 4.2** — DONE — an invalid definition answers the model → Test `propose_setup_returns_the_parse_failure_to_the_model` (unit): schedule `61 * * * *`. Expected: the result names the field, nothing is kept, and the loop goes on.
- **Dimension 4.3** — DONE — a stale base is refused → Test `propose_setup_refuses_a_stale_base_revision` (unit): base 1 while the head is 2. Expected: refused, naming revision 2.
- **Dimension 4.5** — DONE — cron checking is shared → Test `af_fleet_runtime_refuses_an_invalid_cron_schedule` (unit): `61 * * * *`. Expected: refused, and `afd_cron` calls the same function.
- **Dimension 4.4** — DONE — a corrected retry wins → Test `a_refused_proposal_corrected_in_the_same_run_is_reported` (integration): a refusal, then a valid proposal. Expected: the report carries only the valid one.

### §5 — The runner answers a setup turn

A setup run runs like a fleet run, except that it builds no sandbox, has only `propose_setup`, and reports the kept proposal.

`RunPayload.setup` keeps today's `SetupWork` fields: `sequence`, the candidate's `id`, `revision` and `definition`, `save_requested` and `tool`. The model key is carried in the run payload, and a tenant runner never receives a platform key.

The answer's text deltas reach the daemon, which publishes them on the thread's items subject and stores none. The daemon runs `Definition::parse` and the base check again in the report transaction before writing candidate revision N+1.

- **Dimension 5.1** — DONE — a setup turn gets an answer and a candidate → Test `a_setup_run_answer_writes_the_next_candidate` (integration): message 2 asking for a first version. Expected: the turn is answered, and the draft gains candidate revision 1.
- **Dimension 5.2** — DONE — no sandbox for setup → Test `a_setup_run_builds_no_sandbox` (integration): a setup run. Expected: no sandbox prepared, and the run still answers.
- **Dimension 5.3** — the report check is a safety net → Test `a_report_whose_proposal_fails_the_daemon_check_writes_no_candidate` (integration): a report carrying a definition the daemon refuses. Expected: no candidate, the turn answered with its text, and one `setup_proposal_refused` event.
- **Dimension 5.4** — a stale fence writes nothing → Test `a_superseded_setup_run_report_is_refused` (integration): a report at fence 1 after fence 2 was assigned. Expected: refused with `RUN_STALE_FENCING_TOKEN` (`UZ-RUN-005`), and no candidate.
- **Dimension 5.5** — an expired setup run is rerun → Test `an_expired_setup_run_reruns_the_turn` (integration): a runner that stops renewing. Expected: the turn is assigned again at the next fence, up to `af_core::limits::EVENT_RUNS_MAX` runs, then fails with `ProviderUsageUnknown`.
- **Dimension 5.6** — no platform key for a tenant runner → Test `a_tenant_runner_is_not_assigned_a_platform_key_setup_run` (integration): a draft using platform inference and a tenant runner. Expected: no assignment to that runner.

### §6 — A long setup keeps its compacted context

The draft's `memory.context_checkpoints` row holds the compacted setup history as an `af_context::Checkpoint`. The runner loads and saves it with `Up::ContextLoad` and `Up::ContextSave` on its connection. A save keeps today's `resource_version` compare-and-swap and also joins `thread.runs` on the run's fence, so a stale run is refused with `RUN_STALE_FENCING_TOKEN`.

guffbolt's window hook compacts when the window fills, and the saved state is the hook's output.

- **Dimension 6.1** — DONE — the context loads on the next turn → Test `the_next_setup_turn_loads_the_saved_context` (integration): turn 20 after a compaction. Expected: its first request holds the saved summary, and none of the turns the summary replaced.
- **Dimension 6.2** — a stale run cannot overwrite → Test `a_superseded_setup_run_cannot_save_context` (integration): a save at fence 1 after fence 2 saved. Expected: refused with `RUN_STALE_FENCING_TOKEN`, and fence 2's row unchanged.

### §7 — Setup's last gaps close

Two gaps close here:
- **Usage on notices.** A setup turn's notice reads usage from `billing.usage_lines` for its run, not from dispatch receipts (`observe/rows.rs:98-107`).
- **Refused proposals are recorded.** A proposal refused at report is logged with its `error_code` and kept as the turn's failure detail.

- **Dimension 7.1** — notices show run usage → Test `a_setup_notice_reports_the_runs_usage` (integration): a settled setup run with 1,200 input tokens. Expected: the notice shows 1,200.
- **Dimension 7.2** — a refused proposal is recorded → Test `a_refused_report_proposal_logs_its_error_code` (integration): §5.3's case. Expected: one `setup_proposal_refused` event with `error_code`, `draft_id` and `run_id`.

### §8 — The daemon makes no model call

Model selection, the balance gate and the `propose_setup` schema move from `afd_setup/src/model/**` to `afd_setup/src/setup_run/**`. The rest of `model/**` is deleted: dispatch, provider calls, receipts, compaction calls and the edit-intent veto. So are the attempt path in `afd_setup/src/run/**`, `afr_supervisor/src/authoring/**` after its proposal code moves, the `Up` and `Down` setup model frames, `TaggedRun::Authoring`, `AcceptedKinds::AUTHORING`, the attempt tables, and the attempt parts of the bench lanes. `RunKind::Authoring` is the kind of the new draft run too, so it is renamed `RunKind::Setup`, not deleted.

Three things survive the deletion:
- The preview frame (`Delta`, `TextKind`, `DELTA_BYTES`, `DELTA_EVENT`), which the run path's preview and the draft stream use.
- The tool-support check selection calls (`af_inference::Configured`). It moves behind `afd_credential`, so `afd_setup` names no provider crate.
- The proposal behaviours the person sees: argument normalisation, restoring the person's wording, resolving text anchors, refusing an edit on Save and refusing invalid edits. They move to the runner's `FleetSetup` (§4).

The daemon's storage client, built today from the setup model network, gets its own builder. The draft stream's `context.progress` event keeps a producer: the runner publishes compaction progress (§6).

The daemon's `[inference]` and `[authoring.turn]` sections go; the 300-second setup runtime, `max_model_calls` and `max_proposal_corrections` become constants in `afd_setup`. The runner protocol becomes `agentsfleet.runner.v4` alone, and `Revision` and v2 are deleted.

**Implementation default:** the dev database is reset after merge, per Indy's "Rule holds, reset dev".

- **Dimension 8.1** — no setup model frame remains → Test `the_runner_protocol_has_no_setup_model_frames` (unit): every `Down` variant's tag. Expected: none carries a model call.
- **Dimension 8.2** — DONE — another protocol version is refused → Test `a_runner_on_another_protocol_version_is_refused` (integration): hello with an unknown version. Expected: `UZ-RUN-023`, and no run assigned.
- **Dimension 8.3** — no setting feeds setup runs → Test `setup_limits_are_constants` (unit): `afd_setup`'s limits. Expected: 300-second runtime from a constant, and no `TurnSettings` type remains.

### §9 — guffbolt is extended one way: each tool takes its backend

A backend is what a tool calls to do its work. Each tool takes its backend when it is built, as pi's tools take `BashOperations` (`coding-agent/src/core/tools/bash.ts:74`). No registry is looked up at call time.

`Tools::builder()` returns a `ToolsBuilder`, which builds the whole set a loop holds. Each setter follows the `with_*` convention: it takes the builder and returns it. `with_env`, `with_outbound`, `with_memory`, `with_scheduler`, `with_channel` and `with_notifier` set backends, and `with_tool` adds a tool already built with its own backend.

`ToolsConfig` lists tool names, and a single `*` matches an MCP server's tools whose names arrive at connect time. This follows pi's `--tools read,grep,'mcp__radius__*'` (`coding-agent/src/cli/args.ts:320`) and zeroclaw's tool filter globs. `build(&ToolsConfig)` returns `Result<Tools<ContextTool>, MissingBackend>`. It builds the tools the startup config names and refuses to start when one lacks its backend, naming both.

`Context` keeps only per-call facts: the `CallRef`, the run id and the wake. A third party writes a tool whose `new` takes its own backend, with no edit to guffbolt. To share such a tool, they publish its backend trait beside it, as guffbolt does with `Scheduler`.

The cron tools, `schedule` and `message` move into guffbolt as generic tools over `Scheduler` and `Channel`. agentsfleet builds them with backends over its fenced verbs, so its runs behave as today. Pushover leaves guffbolt and becomes a `Notifier` backend in `afr_tools`, which guffbolt's `notify` tool calls. Fleets see the tool as `notify`; a fleet definition that named `pushover` names `notify`. A refused outbound request reads `egress_denied` everywhere, replacing `afr_tools`'s `address_not_allowed`. `afr_tools`'s `MemoryBackend` folds into guffbolt's `MemoryStore`, so memory has one layer.

agentsfleet builds a run's tools when the run starts, because each run has its own sandbox, egress guard and memory.

Local defaults stay honest:
- `LocalScheduler` keeps jobs in a file under a directory the embedder names. It returns due jobs only when the embedder calls `due`.
- `LocalChannel` hands each line to a callback the embedder gives it.
- guffbolt ships no local `Notifier`, because every notifier is a vendor.

Each call gets a `CallRef`: its step, its index in the step, and the provider's call id. A backend derives its idempotency key from it, as agentsfleet's item `(run_id, sequence)` does in M253_003. A retried call keeps its `CallRef`, and so does a journalled call that a resume sends again. A resume that asks the model again makes new calls, which M253_003's recovery note covers. `step` counts the conversation's completed steps plus one, so a resumed run numbers its steps after the saved ones and never repeats an earlier `(step, index)`; a per-run count restarts at 1 and would make a backend drop a new call as a repeat. `CallRef` is `Clone`, not `Copy`, because the call id is an owned string.

Text the model reads names no host product: no "fleet", "workspace", "daemon" or "agentsfleet". Memory's `Reach` becomes `private` and `shared`.

- **Dimension 9.1** — a missing backend stops startup → Test `a_tool_without_its_backend_refuses_to_start` (unit): a config naming `cron_add` and a builder with no scheduler. Expected: an error naming `cron_add` and the scheduler, and no loop starts.
- **Dimension 9.2** — a third party's tool needs no guffbolt edit → Test `a_third_party_tool_takes_its_own_backend` (unit): a test trait `Crm`, its backend, and a tool built with it and added through `with_tool`. Expected: the tool's answer carries the backend's value.
- **Dimension 9.3** — the config picks the offered tools → Test `the_startup_config_offers_only_named_tools` (unit): `["file_read", "memory_store", "mcp__docs__*"]` with a docs MCP server holding two tools. Expected: exactly those four tools are offered.
- **Dimension 9.4** — the local scheduler survives a restart → Test `local_scheduler_returns_a_due_job_after_reopening` (integration): add a job, drop the scheduler, reopen its directory, then call `due` past the moment. Expected: the job, once.
- **Dimension 9.5** — agentsfleetd backs a fleet's scheduling → Test `a_fleet_runs_cron_add_creates_its_schedule` (integration): a fleet run calls `cron_add`. Expected: one schedule for the fleet, as today.
- **Dimension 9.6** — DONE — Pushover is a notifier backend → Test `pushover_backend_sends_through_the_egress_guard` (unit, `afr_tools`): `notify` with the Pushover backend. Expected: one request to Pushover's host, through the run's egress guard.
- **Dimension 9.7** — a retried call keeps its reference → Test `a_retried_call_keeps_its_call_ref` (unit): a backend that fails once, then the retry. Expected: both attempts see the same `CallRef`.
- **Dimension 9.8** — DONE — the model reads no host product → Test `guffbolt_tool_specs_name_no_host_product` (unit): every built-in tool's spec. Expected: no "fleet", "workspace", "daemon" or "agentsfleet".

### §10 — Items record edits and the newest turn (from M253_001 §5)

`thread.items` gains the columns `thread_model.md` defines: `kind`, `tool`, `status`, `changes` and `run_id`. An `edit` item records its path and its added and removed line counts, computed by the runner's file tools, and the app and CLI read them instead of working them out from arguments.

Each input reaching a run carries its turn id. guffbolt notes the newest turn when a step begins and records it with the `CallRef`, and the daemon refuses an item naming a turn the run does not serve.

- **Dimension 10.1** (was M253_001 5.1) — an edit records its counts → Test `an_edit_item_records_path_and_line_counts` (integration): a run editing `src/a.rs` by +24 −8. Expected: one `edit` item with those counts.
- **Dimension 10.2** (was M253_001 5.2) — items name the newest turn → Test `items_after_a_second_message_name_the_second_turn` (unit): input arrives before step 3. Expected: steps 1 and 2 name turn 1, and step 3 names turn 2.

### §11 — Text has one ceiling, codex's

Fleet instructions (32,768 characters), a setup chat message (32,768 bytes) and a message typed into a running fleet (8,192 bytes) lose their limits. One ceiling replaces them, as codex's server keeps (`MAX_USER_INPUT_TEXT_CHARS = 1 << 20`, `codex-rs/protocol/src/user_input.rs:11`): 1,048,576 characters, declared once, refused with a sentence naming the ceiling. Below it, the limit a person meets is the model's context window, which every call already checks. The model's `message` tool loses its 4,096-byte and 8-posts-per-run limits; the channel's own limit refuses what it cannot take. Anything else the text still crosses (a bus frame, a runner frame, the HTTP framework's default body size) is found by the tests below and reported, not hidden.

- **Dimension 11.1** — DONE — a long setup message is answered → Test `a_setup_message_over_32_kib_is_answered` (integration): a 200 KiB message. Expected: stored, sent to the setup run, and answered, or the model's context refusal.
- **Dimension 11.2** — a long fleet message reaches the run → Test `a_fleet_message_over_8_kib_reaches_the_run` (integration): a 64 KiB message to a running fleet. Expected: the run receives it whole.
- **Dimension 11.3** — DONE — long instructions save → Test `instructions_over_32k_characters_save` (unit): 100,000 characters. Expected: the definition parses and saves.
- **Dimension 11.4** — text past the ceiling is refused clearly → Test `a_message_past_the_ceiling_is_refused_naming_it` (unit): 1,048,577 characters. Expected: refused with a sentence naming 1,048,576, and nothing stored.
- **Dimension 11.5** — DONE — the model's posts are not capped → Test `the_message_tool_posts_long_and_often` (unit): a 10,000-byte post and a ninth post in one run. Expected: both sent.
- **Dimension 11.6** — the ceiling is reachable → Test `a_setup_message_at_the_ceiling_in_three_byte_characters_is_accepted` (integration): 1,048,576 three-byte characters through the app's setup proxy and the daemon's setup and steer routes. Expected: accepted; one character more is refused, and the setup chat shows "Input exceeds the maximum length of 1,048,576 characters."
- **Dimension 11.7** — DONE — a run that cannot be framed fails at once → Test `a_run_too_large_for_its_frame_fails_with_its_code` (integration): a run payload past the runner frame. Expected: the turn fails at assignment with a named code, with no lapse and no reassignment; a fleet-to-fleet message stores its words once.
- **Dimension 11.8** — a long message keeps its live frame → Test `a_long_message_is_trimmed_in_its_tail_frame` (unit): a 1 MiB message. Expected: its tail frame carries at most 64 KiB of the text, marked trimmed, and the stored turn holds it whole.
- **Dimension 11.9** — a run carries anything the routes accept → Test `a_run_carrying_ceiling_instructions_and_a_ceiling_message_is_framed` (unit): instructions and a message each at 1,048,576 three-byte characters. Expected: the run frame encodes within the runner frame ceiling, 32 MiB. The data buffer is shared by both directions (`af_stream/src/framing/memory.rs:19-22`), so its 64 MiB floor holds one 32 MiB frame each way; the worst run the routes accept is about 12.6 MiB.

### §12 — Crates are grouped small, by Microsoft's guidelines

After §8's deletion, the crates follow Microsoft's Pragmatic Rust Guidelines (https://microsoft.github.io/rust-guidelines/: M-SMALLER-CRATES, M-CRATES-FLAT-FOLDER, M-FEATURES-ADDITIVE, M-SINGLE-ITEM-PATH, M-FOREIGN-REEXPORTS, M-INTEGRATION-TESTS) inside the layer rules of `.orly/LOCAL.md`. The survey found no crate to split or merge; it found a runner that links `sqlx` (`af_context/Cargo.toml:18` turns on `af_core`'s `sqlx` feature: `sqlx ← af_core ← af_context ← afr_agent ← agentsfleet_runner`), one layer break (`agentsfleetd` dev-depends on six `afr_*` crates), three dev-dependency cycles, cross-crate re-export aliases, and one hub trait (`afd_http::Services` names 29 crates). In order:

1. `rustd/Cargo.toml` lists crates by layer, each with its purpose line.
2. `af_context::repository` and its SQL move to `afd_setup::context`; `af_context` loses its `postgres` feature and its `sqlx` edge.
3. `af_inference` moves from `protocol/` to `harness/`.
4. The dev cycles break: `afd_db`'s statement tests move to `afd_fleet/tests`, and `kernel_lane` becomes an example of `agentsfleet_runner`.
5. A sixth folder, `crates/e2e/`, holds `af_e2e`: the daemon-and-runner suites and the recovery bench; nothing depends on it, and `agentsfleetd` drops its runner dev-dependencies.
6. Each item has one public path: the cross-crate `pub use` aliases go.
7. `afd_http::Services` splits into one trait per plane, each in its `afd_api_*` crate, so a handler names only the services it calls.
8. The layer rule is a unit test.
9. The unit-test shards (`make/test-unit.mk`: `test-unit-rustd-runner`, `-daemon`, `-daemon-libs`) are rechecked against the regrouped layers: each crate runs in exactly one shard, and a shard is a layer group, so a crate move cannot drop or double a crate's tests.

- **Dimension 12.1** — every edge obeys its layer → Test `every_crate_depends_on_its_own_layer_or_lower` (unit): the workspace manifests, with dev, build and target tables. Expected: no edge to a higher layer, none between `runner/` and `daemon/`, none into `e2e/`. Negative twin `a_daemon_crate_naming_a_runner_crate_is_refused`.
- **Dimension 12.2** — no dev-dependency cycle → Test `no_crate_dev_depends_on_a_crate_that_depends_on_it` (unit). Expected: none. Negative twin `a_planted_dev_cycle_is_named`.
- **Dimension 12.3** — folder and prefix match → Test `every_crate_folder_names_its_crate_and_its_prefix_matches_its_layer` (unit). Negative twin `a_crate_in_the_wrong_folder_is_named`.
- **Dimension 12.4** — the runner links no datastore client → Test `the_runner_graph_names_no_datastore_client` (unit): normal and build edges from `agentsfleet_runner`. Expected: no `sqlx`, no `async-nats`, no `af_core` feature `sqlx`. Negative twin `a_runner_crate_enabling_af_core_sqlx_is_refused`.
- **Dimension 12.5** — lower layers hold no SQL → Test `foundation_and_protocol_crates_hold_no_sql` (unit). Expected: no `*.sql` under `foundation/` or `protocol/`.
- **Dimension 12.6** — the moved store still refuses a stale run → Test `a_save_at_a_superseded_fence_is_refused` (integration, `afd_setup`). Expected: `RUN_STALE_FENCING_TOKEN`, row unchanged.
- **Dimension 12.7** — one public path per item → Test `no_crate_re_exports_another_workspace_crates_items` (unit): scan `rustd/crates/**/src` for `pub use` of a workspace crate. Expected: none outside the crate that owns the item.
- **Dimension 12.8** — a handler names only its plane's services → Test `no_handler_crate_names_the_shared_services_hub` (unit). Expected: no `afd_api_*` crate bounds a handler by `afd_http::Services`; each bounds by its own plane trait.
- **Dimension 12.9** — the unit shards follow the layers → Test `every_crate_runs_in_exactly_one_unit_shard` (unit): the shard lists in `make/test-unit.mk` against the workspace members. Expected: every crate in one shard, none in two, and each shard a layer group.

VERIFY: `rustd/guffbolt` is unchanged by §12; the test count before and after each move is equal (`cargo test --workspace --all-features -- --list --include-ignored`); the Linux kernel lane passes after step 4.

### §13 — Subagents are guffbolt's

Subagents are a guffbolt capability, end to end. guffbolt owns the tools, the registry, running each child with its own drive over its own transcript and a narrowed toolset, and each child's state in `DriveState`, so a resumed loop brings its children back. An embedder turns subagents on with `ToolsBuilder` and caps, and sees children only through the observer it already gives the root loop. agentsfleet's `afr_agent/src/nested/**` (2,294 lines) and the subagent entries in `afr_tools`' catalog go; a fleet offers subagents by naming the tools in its `ToolsConfig`, as it names any guffbolt tool.

The tools follow codex's names (`codex-rs/core/src/tools/handlers/multi_agents_spec.rs`): `spawn_agent` (with `fork_turns`, starting the child from the parent's last turns), `send_input`, `followup_task`, `wait_agent`, `list_agents`, `close_agent` and `interrupt_agent`. A finished child goes idle, keeping its transcript, and takes a `followup_task` until `close_agent` or the loop ends. There is no `resume_agent`: children live inside one loop, and the loop's own resume brings them back.

- **Dimension 13.1** — a finished child takes a follow-up → Test `a_finished_child_takes_a_followup_task_with_its_transcript` (unit, guffbolt). Expected: the child runs again and its request holds its earlier turns.
- **Dimension 13.2** — a child can start from the parent's turns → Test `spawn_agent_with_fork_turns_starts_from_the_parents_last_turns` (unit, guffbolt). Expected: the child's first request holds the parent's last N turns, and none before.
- **Dimension 13.3** — close ends a child → Test `close_agent_ends_an_idle_child_and_frees_its_seat` (unit, guffbolt). Expected: the child ends, `list_agents` shows it closed, and the run may start another within its caps.
- **Dimension 13.4** — the tools are guffbolt's, named as codex names them → Test `the_subagent_tools_are_guffbolts_and_named_as_codex_names_them` (unit, guffbolt). Expected: the seven names, built through `ToolsBuilder`, and covered by `guffbolt_tool_specs_name_no_host_product`.
- **Dimension 13.5** — a loop's end still ends its children → Test `a_run_that_ends_ends_its_idle_children` (unit, guffbolt). Expected: every idle or running child ends `interrupted`.
- **Dimension 13.6** — a resumed loop brings its children back → Test `a_resumed_loop_restores_its_children_from_drive_state` (unit, guffbolt). Expected: each child's transcript and status return from the saved `DriveState`.
- **Dimension 13.7** — agentsfleet holds no subagent code → Test `agentsfleet_defines_no_subagent_tool` (unit): `afr_agent/src/nested/` is gone, and no `afr_*` crate names a subagent tool. Expected: a fleet run offers the seven tools through guffbolt alone.

### §14 — Dead weight goes

Three things the schema sign-off found are removed, before production, with no compatibility path.

- **Plugins.** The paused fleet library (`setup.plugin_entries`, `setup.plugin_revisions`, `afd_setup/src/plugin/`, the Plugin routes, the app's `/plugins` page and starters) is deleted end to end.
- **`runner.lifetime_counters`.** Four per-runner tallies upserted in seven run statements and read only by the operator's runner detail. They become one OpenTelemetry counter, `agentsfleet_runner_runs_total{runner_id, outcome}`, emitted where a run settles or expires, and the table goes.
- **`runner.registration_receipts`.** A replay carried no token (`afd_runner/src/register.rs:27-29`), so the receipt never made a retry useful. A host registers once: `(tenant_id, host_id)` is unique on `runner.runners`, a retry answers `409` naming the runner, and the caller recovers with the tenant's token rotation. HTTP-IDEMKEY's natural-key exception is generalised in orly's source rule (own branch and PR in `~/Projects/orly`) and re-materialised here with `orly update`.

- **Dimension 14.1** — Plugins are gone → Test `no_plugin_table_route_or_page_remains` (unit): no `setup.plugin_*` schema file or statement, no Plugin route in the route inventory, no `/plugins` app page. Expected: none.
- **Dimension 14.2** — run outcomes are metrics → Test `a_settled_run_counts_its_outcome_on_its_runner` (unit): a run that succeeds, one that fails and one that expires. Expected: `agentsfleet_runner_runs_total` rises once per outcome with the runner's id, and no statement names `runner.lifetime_counters`.
- **Dimension 14.3** — a host registers once → Test `a_retried_registration_of_one_host_answers_409_naming_the_runner` (integration): the same `(tenant, host_id)` twice. Expected: one runner; the second answers `409` with its id; a rotation then gives a token that connects.

## Interfaces

- **Runner protocol:** `agentsfleet.runner.v4` alone. `Down::Run { run: RunPayload }`, where the payload names one owner, a fleet or a draft. A setup run's payload carries today's `SetupWork`: the sequence, the head candidate's id, revision and definition, and `save_requested`. The message rides `event.request_json`, and history rides `RunPayload.history`.
- **Setup report:** `ReportRequest.setup` carries the kept proposal as today's `SetupProposal`, with `base_candidate_id`, `base_revision` and the definition as JSON text, or none for a text-only answer.
- **Context verb:** `Up::ContextLoad { run_id, fence }` and `Up::ContextSave { run_id, fence, resource_version, checkpoint }`, answered in `Down`; a fence below the current one is refused with `RUN_STALE_FENCING_TOKEN`.
- **guffbolt public API:** `Tool<Cx>`, `Tools<T>`, `Document`, `Env`, `Outbound`, `MemoryStore`, `Ending::{Answered, Waiting, Stopped, Failed}` and `DriveState::resume`.
- **Backends:** `Tools::builder() -> ToolsBuilder`; `with_env`, `with_outbound`, `with_memory`, `with_scheduler`, `with_channel`, `with_notifier` and `with_tool`, each `(self, …) -> Self`; `build(&ToolsConfig) -> Result<Tools<ContextTool>, MissingBackend>`; the `Scheduler`, `Channel` and `Notifier` traits; and `CallRef { step, index, call_id }`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Invalid proposal | failure | the model proposes a definition `Definition::parse` refuses | the tool result names the field; the model may retry | `propose_setup_returns_the_parse_failure_to_the_model` |
| Stale base | race | another turn wrote revision N+1 first | refused, naming the head | `propose_setup_refuses_a_stale_base_revision` |
| Report check refuses | failure | the daemon's rules differ from the runner's build | no candidate; the turn is answered with its text and holds the refusal as its failure detail; event logged | `a_report_whose_proposal_fails_the_daemon_check_writes_no_candidate` |
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
4. The daemon cannot use the provider clients. `afd_setup` depends directly on none of `af_providers`, `af_inference` and `rig-core`; the tool-support check reaches `af_inference` only through `afd_credential`.
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
| the daemon's setup model path | `git grep -n -E "^(pub(\(crate\))? )?mod model;|crate::model::" -- rustd/crates/daemon/afd_setup/src/lib.rs rustd/crates/daemon/afd_setup/src` | no output |
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
  - > Owner (Indy, Nov 20, 2026): "Tool names only, but also refer ~/Projects/oss/rs/codex, ~/Projects/oss/js/pi too as a reference and the ~/Projects/oss/rs/*claw" — context: §9, ToolsConfig.
  - > Owner (Indy, Nov 20, 2026): "Rename to notify (Recommended)" — context: §9, the published tool name.
  - > Owner (Indy, Nov 20, 2026): "egress_denied everywhere (Recommended)" — context: §2 and §9, one refusal code.
  - > Owner (Indy, Nov 20, 2026): "Drop it from the spec (Recommended)" — context: §4, no action column on the usage line.
  - > Owner (Indy, Nov 20, 2026): "Yes, one line in lint-rustd (Recommended)" — context: §1, Dimensions 1.1–1.3 are one publish dry run in `make lint-rustd` until guffbolt moves to its own repository, where it becomes that repository's CI and pre-publish step.
  - > Owner (Indy, Nov 20, 2026): "Defer 5.1 to M253_002" and "Move 5.2 into M253_002 (Recommended)" — context: §10, transferred from M253_001.
  - > Owner (Indy, Nov 20, 2026): "Move to afr_tools (Recommended)" — context: §9, Pushover leaves guffbolt.
  - > Owner (Indy, Nov 20, 2026): "I think i need ability like memory_store, they are in guffbolt i presume, and the calls do api to save them? similar way the pushover and scheduler can be. When i start i can configure the needed (like pi or codex or zig's nullclaw or any ~/Projects/oss/rs/* equivalent)" — context: §9.
  - > Owner (Indy, Nov 20, 2026): "I need a common better way to do this." — context: §9, one extension pattern.
  - > Owner (Indy, Nov 20, 2026): "Yes i prefer the proposal" — context: §9, one extension pattern.
  - > Owner (Indy, Nov 20, 2026): "use standard rust specific like `with` or so, so its obvious on what the builder will retun. I assume the Tool builders builds more." — context: §9, `ToolsBuilder` with `with_*` setters that builds the whole `Tools` set.
  - > Owner (Indy, Nov 20, 2026): "is capability a good word or abstraction" and then "Backends via builder (Recommended)" — context: §9. "Capability" already has many meanings in `rustd` (auth, Linux sandbox capability sets, decision models, the runner's task capability), and a type-keyed registry hides a tool's needs until it is called. So each tool takes its backend when it is built.
  - > Owner (Indy, Nov 20, 2026): "I think remove this  plus 12K for the model's first draft only" — context: §4. Neither codex (`project_doc_max_bytes` = 32,768) nor pi has a separate first-draft bound; `MAX_INITIAL_INSTRUCTIONS_CHARS` and `validate_initial_generation` are removed, and every setup keeps the 32,768-character bound.
  - > Owner (Indy, Nov 20, 2026): "I think just remove the 32KB limit as well, let us test and figure it out. Since codex or pi doesnt have that when user chats, understand we save and codify it. But i think its pointless to limit." then "Remove all three, no ceiling", then, after asking "no limits in codex and pi on mesages?" and reading codex's `MAX_USER_INPUT_TEXT_CHARS = 1 << 20`, "Match codex: 1M chars (Recommended)"; and for the `message` tool's 4,096 bytes and 8 posts per run, "Drop both" — context: §11.
  - > Owner (Indy, Nov 20, 2026): "just group the crates in a better way and keep it modular and small following microsoft standards" and then "I want all in this PR, i dont wish to create new specs" — context: §12, and this batch's commits grouped by crate layer. The regrouping runs after §8 inside M253_002; no new spec.
  - > Owner (Indy, Nov 20, 2026): selected "1–4 + 7: core fixes (Recommended), 5: new e2e crate (Recommended), 6: one public path (Recommended), Also split afd_http Services" and "af_e2e (Recommended)" — context: §12, approving the regrouping proposal (`/tmp/m247-review/M253_002-crates-proposal.md`) before any crate moves.
  - > Owner (Indy, Nov 20, 2026): selected "Make 1M reachable (Recommended), Runner frame fails fast (Recommended), Trim live-tail frames (Recommended)" — context: §11.6–§11.8, after the §11 survey found the app proxy (128 KiB), axum (2 MiB), the runner frame (2 MiB) and live-tail frames each stop text below the ceiling.
  - > Owner (Indy, Nov 20, 2026): "Codex-aligned for runs (Recommended)", then "Also why is subagent code in (afr_tools/src/catalog/mod.rs:103-111, implemented in afr_agent/src/nested/ should they be in guffbolt" and "Move them into guffbolt (Recommended)" — context: §13.
  - > Owner (Indy, Nov 20, 2026): "cut the stupid crap and make guffbolt powerful i am tired of your slop, subagents are a functionality of guffbolt and remove that from agentsfleet. What made you think its agentsfleet?" — context: §13. guffbolt owns subagents end to end; `afr_agent/src/nested/**` goes.
  - > Owner (Indy, Nov 20, 2026): "I think i want to remove the dead setup.plugin_entries, plugin_revisions, any backend code, apis and the front end coe. just clean all that up." — context: §14, Plugins.
  - > Owner (Indy, Nov 20, 2026): "Why do you need to keep track of lifetime_counters? Can you just send it to observability grafana?" — context: §14, run outcomes become a metric.
  - > Owner (Indy, Nov 20, 2026): "Why do you need this? runner.registration_receipts ... To me this is crap design." and then "Change the rule, drop receipts (Recommended)" — context: §14 and HTTP-IDEMKEY in orly.
  - > Owner (Indy, Nov 20, 2026): "Match codex: 64 MiB" — context: §11.9, the runner frame ceiling.
  - > Owner (Indy, Nov 20, 2026): "I prefer 128 prior to that ensure you ultrathink why 128 and how can you fix by 64, if you make 64 will it be like oneway 32? if so i am fine with 64 and move on" — context: §11.9. The data buffer is shared by both directions, so a 64 MiB buffer gives 32 MiB each way; the frame is 32 MiB, and the Fly daemon configs move from 48 MiB to 64 MiB (owner-approved deploy edit).
  - > Owner (Indy, Nov 20, 2026): "Delete the two lines (Recommended)" — context: §8, `authoring_renewal_*` in `deploy/baremetal/agentsfleet-runner.toml`.
  - > Owner (Indy, Nov 20, 2026): "we are not in prod yet, so i think you should think in that aspect and codify, so no backward compatibility needed. In fact when M252 milestone is merged, we will teardown dev db." — context: codified in `.orly/LOCAL.md`.
  - > Owner (Indy, Nov 20, 2026): "Is the test-unit-rustd the correct separation? Add that question when you abstract and devour the crates." — context: §12 step 9, Dimension 12.9.
  - > Owner (Indy, Nov 20, 2026): "Just complete code first" and "The tests can be verified later on is what i meant" — context: the rest of M253_002. Each Section lands with its tests written and compiled (`fmt`, `clippy -D warnings`); the tests run together at the end on two E2E Networks virtual machines and CI, and a Dimension is marked DONE only after its test passes there.
  - > Owner (Indy, Nov 20, 2026): "I prefer this  leaving it out and trusting the model to find `propose_change` and its a harness ability to detect and ask back or what" — context: §9.8. guffbolt's `git` tool no longer points the model at `propose_change`; its refusal of a remote reaches the model as the tool result, and agentsfleet adds no hint.
  - > Owner (Indy, Nov 19, 2026): "donot do the lint, test-unit, test-integration check but rather do it at the end of every workstream. but for every dimension do cargo check, cargo fmt (clippy) so we know code compiles." — context: per-Dimension compile checks; S rows at the end of each Workstream.
  - > Owner (Indy, Nov 19, 2026): "Ensure the rust code for individual files is 99% and overall is 99% if possible or else stick to the numbers we have currently." — context: coverage targets 99% per file and overall, never below the 98% line and 95% patch floors.
  - > Owner (Indy, Nov 19, 2026): "E2E cloud burst" — context: Linux proof, paid, his go per run.
  - > Owner (Indy, Nov 19, 2026): "I think i did the elephant goldfish brainstrome before." and "youalready have the context" — context: orly-spec step 2.
- **Model reviews** — EGM Stage 9 (implementation readiness), three reviewers, Claude Opus 5.5, Nov 19, 2026: 9 Blockers, 39 Majors, 30 Nits. Every Blocker is resolved in the Sections as an implementation default, recorded in `.egm/implementation_readiness_report.md`; wording findings are applied, and none is declined.
- **Verification evidence** — Nov 20, 2026, commit `6f97d0ef1`: each DONE Dimension from §4 on names a test that passed in CI run `44756156167` (unit) or `44756156230` (integration), or on E2E Networks VM 30591 (`make test-integration-rustd`). 9.5 and 11.6 failed there; the rest did not run because a lane stops at its first failing suite.
- **Open blockers** — none.
- **Deferrals and transfers** — none.
