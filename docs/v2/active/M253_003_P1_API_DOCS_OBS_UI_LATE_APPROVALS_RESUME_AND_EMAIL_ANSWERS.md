<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M253_003: A late approval resumes its turn, a retried effect lands once, and answers can be emailed

**Prototype:** v2.0.0

**Milestone:** M253

**Workstream:** 003

**Date:** Nov 19, 2026

**Status:** IN_PROGRESS

**Priority:** P1, a person who approves late or waits for an email is the customer this fleet serves

**Categories:** API, DOCS, OBS, UI

**Batch:** B3, after M253_002

**Depends on:** M253_001, whose turns, runs, items and deliveries this builds on

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 19, 2026) from Indy's decisions of Nov 19, 2026

**Canonical architecture:** `docs/architecture/thread_model.md` for the tables; `docs/architecture/runner_fleet.md` for continuations

---

## Overview

**Goal (testable):** An approval given after its deadline resumes the waiting turn, and a retried message post lands exactly once.

**Problem:**
- **Late approvals fail.** The sweep resolves an expired approval and releases its turn (`afd_approval/src/sql/expire_gates.sql:1-15`). After that, `resolve_gate.sql:9` matches only `resolved_at IS NULL`, so an approval two hours late does nothing.
- **A question ends a run as a failure.** `ask_user`'s `Waiting` ending is reported as `DETAIL_WAITING` (`afr_agent/src/loop/finish.rs:13-15`), so the person's answer has nowhere to go.
- **A retried post can double.** Message and schedule calls carry no key (`af_wire/src/message_verb/mod.rs:41`).
- **Scheduled answers go nowhere.** A scheduled turn has no connector to answer, so nobody hears about an outage it found.

**Solution summary:**
- **One human-input path.** Policy gates, anomaly gates, install grant cards and `ask_user` questions all live in `fleet.approval_gates`. An answer, early or late, resolves the gate and lets the turn continue.
- **Retries land once.** Every runner effect names its item, unique on `(run_id, sequence)`.
- **No reservation.** The per-task decision reservation goes.
- **Email delivery.** A turn's answer can be emailed to tenant owners and members.

**Design flow:**
1. **Wait.** A gate parks the turn as `waiting`. Or a run ends on `ask_user`, and its report stores a `question` gate and a checkpoint taken there.
2. **Answer.** A person answers through the existing approval routes or the Slack callback, before or after the deadline.
3. **Continue.** A policy-gate approval releases the parked turn to be assigned. A question's answer writes one continuation turn, whose run resumes from the checkpoint with the answer as the asking call's result.
4. **Deliver.** A settled turn owes one delivery per configured destination. The email lane takes it and sends it through the `smtp-relay` bag (`afd_mail/src/lib.rs:1-12`).

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m253): late approvals resume, retried effects land once, email delivery
- **Intent (one sentence):** A person who answers late still gets the work done, a retried post never doubles, and an outage reaches the owner's inbox.
- **Authoring handshake:**
  - **What Indy asked for:** "Late human approvals remain valid", "One primitive (Recommended)", "Key by the item (Recommended)", and email for the outage watch to "Fleet owner, verified members".
  - **ASSUMPTIONS I'M MAKING:**
    1. **One gate table.** `fleet.approval_gates` stays, because install grant cards have no turn and policy gates park before any run.
    2. **Late means after the deadline.** An approval stays answerable until it is answered, its fleet is archived, or its turn is stopped.
    3. **Recipients.** `owner` is every tenant member with role `owner`. A member is a tenant member, and "verified" means the identity provider gave the user an email address.
    4. **Usage at report.** A task's decision calls are charged at report, with no reservation.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_approval/src/sql/resolve_gate.sql` — the statement whose `resolved_at IS NULL` refuses a late answer.
2. `rustd/crates/daemon/afd_approval/src/sql/expire_gates.sql` — the sweep that closes an approval and releases its turn today.
3. `rustd/crates/runner/afr_agent/src/loop/recovery_boundary.rs` — where checkpoints are taken, before each model call.
4. `rustd/crates/daemon/afd_outbound/src/poster/mod.rs` — the provider dispatch the email poster joins.
5. `rustd/crates/daemon/afd_mail/src/lib.rs` — the relay and letters, today used by invites.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/*_fleet_approval_gates.sql` | EDIT | `turn_id` FK, null for a grant card; the `question` gate type |
| `rustd/crates/daemon/afd_approval/src/**` | EDIT | late answers, the sweep, question gates, continuation turns |
| `rustd/crates/daemon/afd_gate/src/**` | EDIT | a parked turn stays waiting past its deadline |
| `rustd/crates/runner/afr_agent/src/loop/**` | EDIT | a checkpoint at the waiting point; `Waiting` reported as waiting, not failure |
| `rustd/crates/protocol/af_wire/src/report/**`, `src/lease/**` | EDIT | the report's question, and the continuation's answer in `RunPayload` |
| `rustd/crates/runner/afr_tools/src/verbs/**`, `src/runtime/**` | EDIT | effect requests name their item |
| `rustd/crates/protocol/af_wire/src/message_verb/**`, `schedule_verb/**` | EDIT | `MessageRequest`, `ScheduleCreateRequest`, `SchedulePatchRequest` and `ScheduleRunRequest` carry `item_id` |
| `rustd/crates/daemon/afd_api_runner/src/handler/runner/**` | EDIT | effects deduplicate by item |
| `rustd/crates/daemon/afd_fleet/src/task/**` and the run statements | EDIT | the reservation and its reads removed |
| `rustd/crates/daemon/afd_fleet/src/task/decision/sql/reserve_task.sql` | DELETE | the reservation |
| `schema/*_fleet_tasks.sql` | EDIT | `reserved_model_nanos` removed |
| `rustd/crates/protocol/af_wire/src/task/limits.rs`, `cli/src/commands/fleet/task-read.ts`, `ui/packages/app/lib/api/tasks.ts` | EDIT | the public reserved field removed |
| `rustd/crates/protocol/af_fleet_runtime/src/config/raw/document.rs` | EDIT | `x-agentsfleet.deliveries` |
| `rustd/crates/daemon/afd_setup/src/**` save path | EDIT | recipients checked at save |
| `rustd/crates/daemon/afd_outbound/src/**` | EDIT | the `Channel` enum and the email poster |
| `rustd/crates/daemon/afd_mail/src/**`, `templates/answer.{html,txt}` | EDIT, CREATE | the answer letter beside the invite |
| `rustd/crates/foundation/af_core/src/error_code/**` | EDIT | `DELIVERY_RECIPIENT_REFUSED` and `MAIL_RELAY_UNCONFIGURED`, each with registry and problem rows |
| `ui/packages/app/**` approval card | EDIT | "Deadline passed" with both buttons; the question's answer box |
| `docs/ux.html` | EDIT | the late card and the answer box |
| `public/openapi.json` | EDIT | regenerated |
| `docs/architecture/runner_fleet.md`, `billing_and_provider_keys.md`, `thread_model.md` | EDIT | continuations; no reservation; the gate columns |
| `docs/v2/pending/M253_003_P1_API_DOCS_OBS_UI_LATE_APPROVALS_RESUME_AND_EMAIL_ANSWERS.md` | CREATE | this spec, moved to `active/` then `done/` |
| `CHANGELOG.md` | EDIT | late approvals and email delivery |

## Applicable Rules

- **`domains/sql.md`** — schema edited in place before the first production deploy; statuses bound as named constants; the naming convention in `thread_model.md`
- **`domains/http.md`** — HTTP-OP for the existing decision route; HTTP-IDEMKEY on a repeated answer
- **`domains/auth.md`** — only a member of the gate's workspace answers it; recipients come from tenant membership
- **`domains/logging.md`** — no email address in a log line; `error_code` on every refusal
- **`languages/rust.md`** — ERR-RS, RS-MAPERR
- **`tests.md`** — a negative test per failure mode; integration tests across the report and answer paths

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | clippy, TypeScript and source rules |
| error-code registry check | yes | each new code lands with its family const, `REGISTRY` row and `problem/` entry |
| `docs.language` | yes | changed pages in short sentences |
| gitleaks | yes | the relay credential stays in the vault bag |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | the email poster mirrors the Slack poster's split |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/daemon/afd_outbound/src/slack/**` — mirror the poster: take the lane head, send, then mark delivered, release or abandon by holder token.
- **Reference:** `rustd/crates/daemon/afd_mail/src/invite/**` — mirror the askama letter, the relay mailer and the idempotency header.
- **Reference:** `~/Projects/oss/rs/codex/codex-rs` — an approval is something the turn waits on, and the answer resumes it. Diverge: the run has ended, so the answer admits a continuation turn.

## Sections (implementation slices)

### §1 — A question becomes an approval the turn waits on

`ask_user` ends the run as `Ending::Waiting`. The runner takes a checkpoint at that point, after the asking call's step is recorded, and reports `waiting` with the question. That replaces today's `DETAIL_WAITING` failure.

The report writes a `question` gate in `fleet.approval_gates` with its `turn_id`, the question, and a deadline from `af_fleet_runtime::config::DEFAULT_TIMEOUT`. The turn becomes `waiting`, and the card shows the question with an answer box.

- **Dimension 1.1** — a question becomes a gate → Test `an_ask_user_ending_writes_a_question_gate` (integration): a run ending on `ask_user`. Expected: one `question` gate for the turn, the turn `waiting`, and a checkpoint at the waiting point.
- **Dimension 1.2** — waiting is not a failure → Test `a_waiting_run_reports_waiting` (unit): a loop ending `Waiting`. Expected: the report's outcome is waiting, with no `DETAIL_WAITING`.

### §2 — An answer, early or late, lets the turn continue

`resolve_gate.sql` accepts a gate whose status is `pending` or `timed_out`. It refuses one already answered with `APPROVAL_ALREADY_RESOLVED` (`UZ-APPROVAL-006`), and one whose fleet is archived with `AGENTSFLEET_ARCHIVED` (`UZ-AGT-020`).

No sweep touches a gate. The read reports `timed_out` when the gate is unanswered and its `expires_at` has passed, so the card can say "Deadline passed", and the turn stays `waiting`. `expire_gates.sql` and `Inbox::expire` are deleted (§7).

A policy-gate approval releases the parked turn, and a denial fails it, as today. A question's answer writes one continuation turn with `resumes_turn_id`.

Its run resumes from the checkpoint, and `DriveState::resume` gives the answer as the asking call's result. A repeat of the same answer returns 200 with the same state.

- **Dimension 2.1** — a late approval continues → Test `an_approval_after_the_deadline_releases_the_turn` (integration): a policy gate timed out 2 hours ago, then approved. Expected: the turn is assigned and answered.
- **Dimension 2.2** — a late question answer resumes → Test `a_question_answered_after_the_deadline_resumes_the_run` (integration): a `question` gate timed out, then answered "yes". Expected: one continuation turn, and its run's next request holds "yes" as the asking call's result.
- **Dimension 2.3** — two answers make one decision → Test `two_answers_to_one_gate_resolve_it_once` (integration): approve and deny race. Expected: one wins, the other gets `UZ-APPROVAL-006`, and at most one continuation exists.
- **Dimension 2.4** — an archived fleet's gate closes → Test `an_answer_to_an_archived_fleets_gate_is_refused` (integration): archive, then approve. Expected: `UZ-AGT-020`, and no turn written.
- **Dimension 2.5** — another workspace cannot answer → Test `a_member_of_another_workspace_cannot_answer_a_gate` (integration): a token for workspace B. Expected: 404.
- **Dimension 2.6** — the card stays answerable → Test `an_expired_gate_card_still_offers_approve_and_deny` (e2e): a Playwright run of the conversation with a timed-out gate, the API mocked with `page.route`. Expected: "Deadline passed" with both buttons enabled.

### §3 — A retried effect lands once

Each message, schedule and cron call is an item, written when the call starts and unique on `(run_id, sequence)`. The runner sends the item's `item_id` with the effect request. The daemon records the effect's result on the item before it responds.

A retried request for the same item returns the stored result, and no second effect happens. A request from a run below the thread's fence is refused first, with `RUN_STALE_FENCING_TOKEN`. A deduplicated message does not count again against `MESSAGES_PER_RUN_MAX`.

A resumed run is a new run, and its model makes new calls. Its recovery note already says "Reconcile … before taking another action" (`recovery_boundary.rs:58`). It will also list the dead run's finished effects from that run's items, which stay until the turn settles.

- **Dimension 3.1** — a retried post lands once → Test `a_retried_message_for_one_item_posts_once` (integration): the same `item_id` sent twice. Expected: one Slack post, and both responses identical.
- **Dimension 3.2** — two calls are two effects → Test `two_calls_in_one_run_post_two_messages` (integration): two items. Expected: two posts.
- **Dimension 3.3** — a superseded run posts nothing → Test `an_effect_from_a_superseded_fence_is_refused` (integration): a send at fence 1 after fence 2 was assigned. Expected: `UZ-RUN-005`, and no post.
- **Dimension 3.4** — a resumed run sees what was done → Test `a_resumed_run_note_lists_the_dead_runs_posts` (integration): run 1 posts and dies. Expected: run 2's recovery note names that post.

### §4 — A durable task reserves nothing

`reserve_task.sql` is deleted, with `fleet.tasks.reserved_model_nanos` and every read of it. The reads are in `meter_lease.sql`, `claim_admission_head.sql`, `select_ledger_candidates.sql`, `task/waits/sql/owner.sql`, `task/sql/usage_record.sql`, `lock_task.sql`, `issue_lease.sql`, `insert_task.sql` and `read.sql`. The public field `reserved_model_nanos` in `af_wire::task::limits` goes with them.

The `used_decision_calls` increment moves into `account_task.sql`. The budget check becomes `used_model_nanos < max_model_nanos`.

- **Dimension 4.1** — no reservation is written → Test `a_task_decision_call_reserves_nothing` (integration): one decision call. Expected: `used_decision_calls` rises by one at report, and no reserved amount exists.
- **Dimension 4.2** — an exhausted task still stops → Test `a_task_over_its_model_budget_is_not_assigned` (integration): `used_model_nanos` at the limit. Expected: no assignment, and the task is `exhausted` with `TASK_LIMIT_EXHAUSTED`.

### §5 — A settled turn's answer can be emailed

A fleet definition may list `x-agentsfleet.deliveries` entries of `{"type":"email","to":[…]}`. Each recipient is `owner` or `member:<user_id>`, at most 10 per entry, with no repeats. The save path checks each one against tenant membership and refuses a stranger with `DELIVERY_RECIPIENT_REFUSED`.

When a turn settles answered or failed, the report owes one delivery per recipient, with provider `email` and the user id as destination. `afd_outbound` routes it through a `Channel` enum, Slack or Email, and `Posters` gains an email poster.

The poster checks membership again, looks up the address, and renders `templates/answer.{html,txt}`. The subject is the fleet name and the answer's first line. The body is the answer, and the link is `{dashboard}/w/{workspace_id}/fleets/{fleet_id}`.

It sends through the invite relay with idempotency key `delivery-{delivery_id}`. SMTP 4xx replies retry on today's jittered schedule. A 5xx reply abandons the delivery, and so does an unconfigured relay, with `MAIL_RELAY_UNCONFIGURED`.

- **Dimension 5.1** — the outage email arrives → Test `a_scheduled_turns_answer_is_emailed_to_the_owner` (integration): a schedule turn answering "5xx 4.1% since 08:42" with `owner` as recipient. Expected: one message to the owner holding that answer and the fleet's link.
- **Dimension 5.2** — a stranger cannot be a recipient → Test `saving_a_fleet_that_emails_a_non_member_is_refused` (integration): `member:<uuid of a user outside the tenant>`. Expected: 422 with `DELIVERY_RECIPIENT_REFUSED`.
- **Dimension 5.3** — a removed member gets nothing → Test `a_recipient_removed_before_sending_is_not_emailed` (integration): remove the member, then settle. Expected: the delivery is abandoned with `DELIVERY_RECIPIENT_REFUSED`, and nothing is sent.
- **Dimension 5.4** — a relay outage retries → Test `an_smtp_4xx_retries_then_abandons` (integration): a relay answering 451 every time. Expected: retries on the jittered schedule, then the delivery is abandoned.
- **Dimension 5.5** — one email per delivery → Test `a_redelivered_email_carries_one_idempotency_key` (integration): a send that times out and is retried. Expected: both tries carry `delivery-{delivery_id}`.

### §6 — M253_001's acceptance and benchmarks run at the end of M253

Carried from M253_001, by the owner's instruction. They run once on E2E Networks after M253_003's CI is green, in parallel, and the virtual machines are deleted after.

- **Dimension 6.1** (was M253_001 7.3) — the CLI reads turns → Test `fleet_turns_lists_turns` (acceptance, `cli/test/acceptance/logs-turns-live.spec.ts`): `agentsfleet fleet turns <fleet_id>` against a live daemon. Expected: JSON rows with `turn_id`.
- **Dimension 6.2** (was M253_001 9.1) — recovery is measured → Test `bench_thread_recovery_api_and_files` (manual): Indy reviews `bench/baselines/thread-recovery.rig.json`. Expected: p50 and last-fleet times for each size, with source hashes.
- **Dimension 6.3** (was M253_001 9.2) — each changed statement has a plan → Test `explain_plans_for_thread_statements` (manual): Indy reviews `bench/baselines/thread-sql.rig.json`. Expected: one plan per statement, none a sequential scan on `thread.turns` or `thread.runs`.

### §7 — The daemon runs no sweeper

Every background sweeper goes. A fact that changed only because time passed is checked when something reads or claims it, and work a sweeper finished is done on the event that owes it. `sweeper:retention` went in M253_002 §14.

| Sweeper today | What replaces it |
|---|---|
| `sweeper:liveness` | nothing: the runner's heartbeat writes `last_seen_at` (`afd_runner/src/heartbeat/cordon.rs:72`), assignment skips a silent runner (`assign_turn_head.sql:44`), and the view derives offline (`afd_runner/src/view/decode.rs:254`); `drained` is derived at read |
| `sweeper:fleet-census` | the gauges are read in the existing metrics export |
| `sweeper:setup` | nothing: closed drafts and replay receipts are kept until a retention policy is chosen; a receipt past its window already reads as new (`afd_setup/src/replay/mod.rs:163`) |
| `sweeper:billing` | the report's transaction settles its own usage lines through the Postgres ledger; an expired unissued decision call is recorded when its task is next claimed |
| `sweeper:expiry` | the assignment that takes over a lapsed run bills that run's last slice in the same transaction; a Stop bills it too |
| `sweeper:tasks` | wall and run limits are checked at assignment; a GitHub completion fans out in its webhook handler; a wait reads CI status when its task is claimed or read; an uncertain effect is reconciled when its task is claimed |
| `sweeper:task-storage` | capturing a generation deletes the ones past the newest three; a task's checkpoints are deleted when it ends; a turn's checkpoints when it settles |
| `connector:outbound` 30 s pass | one pass at daemon boot, then the bus ping; a retry waits in the worker |
| `runner_stream_scheduler` recovery probe | an idle runner's heartbeat asks for waiting work |

- **Dimension 7.1** — no sweeper is started → Test `the_daemon_starts_no_sweeper` (unit): the supervised task names at boot. Expected: none starts with `sweeper:`, and no `[maintenance.*]` key is accepted.
- **Dimension 7.2** — a silent runner gets no work → Test `a_silent_runner_is_skipped_and_reads_offline` (integration): `last_seen_at` past the offline window. Expected: no assignment, and the runner reads `offline`.
- **Dimension 7.3** — a report pays for itself → Test `a_report_settles_its_usage_in_its_transaction` (integration): one report. Expected: the lines are settled and the wallet is debited when the report commits. Negative test `a_report_whose_settlement_fails_writes_nothing`.
- **Dimension 7.4** — a dead run's last slice is billed → Test `a_takeover_bills_the_lapsed_runs_last_slice` (integration): a run lapses and another runner takes the turn. Expected: one usage line for the lapsed slice, settled.
- **Dimension 7.5** — a lost wake does not strand work → Test `an_idle_runners_heartbeat_is_assigned_waiting_work` (integration): queued work with no bus wake. Expected: the next heartbeat assigns it.
- **Dimension 7.6** — an undelivered answer is sent at boot → Test `an_undelivered_answer_is_sent_at_boot` (integration): a delivery owed before a restart. Expected: one post after boot.
- **Dimension 7.7** — task limits hold without a sweep → Test `a_task_past_its_wall_limit_is_exhausted_at_claim` (integration): a task past its wall limit. Expected: not assigned, and `exhausted` with `TASK_LIMIT_EXHAUSTED`.
- **Dimension 7.8** — a CI completion wakes its wait → Test `a_ci_completion_webhook_wakes_its_wait` (integration): one GitHub completion. Expected: the matching wait is signalled in the webhook's request. Negative test `a_missed_webhook_is_read_when_the_task_is_claimed`.
- **Dimension 7.9** — checkpoint storage is freed on its event → Test `a_fourth_generation_deletes_the_oldest` (integration): four captures. Expected: three remain, and the quota drops by the deleted bytes. Test `an_ended_tasks_checkpoints_are_deleted` (integration).

### §8 — Our limits match Codex and Pi

An agent-invented cap goes. Where Codex or Pi has a number, ours matches it; where neither caps something, ours is deleted; nothing is tightened. A bound on untrusted input stays a garde rule at the 1M text ceiling or the 32 MiB frame. Sources: `/tmp/m247-review/limits-codex-pi-guffbolt.md` (Codex `0e8c7683`, Pi `ee6f36496`).

| Area | Change |
|---|---|
| History | delete `HISTORY_TURNS_MAX`, `HISTORY_BYTES_MAX`, `TURN_TEXT_BYTES_MAX` and the trim; a fleet run saves its compacted summary as setup does; a run carries the earlier turns' tool calls and output from `thread.items` |
| Message | `Delta.text` bound rises to the text ceiling; the proposal's 64 / 32 caps and `MAX_CRON_LEN` go |
| Tools | an image over the bound is resized to 2,048 px on its longest side, as Codex does, not refused |
| Time | the shell's 10 min max becomes Pi's 2^31 - 1 ms (24.8 days), the ceiling that keeps a deadline from overflowing `Instant` (`guffbolt/src/tools/process/oneshot.rs`); sleep 12 h; MCP call 300 s; `SETUP_RUNTIME` and `MODEL_CALLS_MAX` go and setup checks a positive balance as fleets do; `MAX_RUNTIME` goes and the budget at renewal ends a looping run |
| Subagents | 6 open; the per-run cap goes; depth 2 and the waits stay |
| Wire and reads | executor frame 64 MiB; thread page 25 by default, 100 at most |
| Reports | the tool-trace, file-change, item tool-name, fold, recovery-text and usage caps go; the frame bounds a report |

Unchanged: the 1M message, the body sizes, shell output and listing budgets, the 2,000-line and 8 MiB reads, the MCP name, the 32 MiB runner frame, the 2 MiB control frame, NATS, `HOLDS_MAX`, and every limit the survey did not cover.

- **Dimension 8.1** — a run carries the whole chat → Test `a_long_chat_run_carries_every_finished_turn` (unit, `afd_fleet` history): 20 finished turns of 20 KiB each. Expected: 20 turns, none cut.
- **Dimension 8.2** — a fleet keeps its summary across runs → Test `the_next_fleet_turn_loads_the_saved_summary` (integration, `af_e2e`): a fleet chat that compacts. Expected: the next run's first request holds the saved summary and none of the turns it replaced. Negative test `a_fleet_summary_at_a_superseded_fence_is_refused`.
- **Dimension 8.3** — a follow-up sees the earlier tool calls → Test `a_follow_up_run_carries_the_earlier_turns_tool_calls` (unit, `afd_fleet` history): a finished turn with two items. Expected: the next run's history holds both calls with their output.
- **Dimension 8.4** — a large image is resized → Test `an_image_over_the_bound_is_resized_to_2048` (unit, guffbolt): a 4,096 px PNG. Expected: the model gets a 2,048 px image. Negative test `an_image_that_does_not_decode_is_refused`.
- **Dimension 8.5** — DONE — waits match Codex and Pi → Tests `sleep_waits_as_asked_capped_at_twelve_hours_and_wakes_for_input`, `shell_runs_sh_c_under_the_timeout_asked_held_to_its_ceiling` and `a_server_that_never_answers_fails_each_call_at_its_timeout` (guffbolt public API), `should_hold_the_timeout_the_model_names_under_the_ceiling` (`afr_tools`). Expected: a 24 h sleep waits 12 h; a 3,000,000,000 ms shell timeout is held to 2,147,483,647 ms; the MCP deadline is 300 s.
- **Dimension 8.6** — setup has no runtime ceiling → Test `a_setup_run_renews_past_five_minutes` (integration, `afd_fleet`): a setup run renewed past 300 s. Expected: renewed. Negative test `a_setup_run_on_an_empty_wallet_is_refused`.
- **Dimension 8.7** — the budget, not a clock, ends a looping run → Test `a_run_past_twelve_hours_renews` (unit, `afd_fleet` renew). Expected: renewed. Negative test `a_run_over_its_budget_is_not_renewed`.
- **Dimension 8.8** — DONE — subagents match Codex → Tests `a_seventh_open_subagent_is_refused` (unit, guffbolt; Codex refuses past `DEFAULT_AGENT_MAX_THREADS`, `core/src/agent/registry.rs:104-109`), `a_run_starts_more_than_sixteen_subagents` and `close_agent_frees_a_seat_under_the_open_cap_and_no_start_count_binds` (public API).
- **Dimension 8.9** — frames and pages match Codex → Test `the_executor_carries_a_frame_over_16_mib` (unit, `afr_executor`) and `a_thread_page_of_100_is_served` (integration, `afd_turns`). Negative test `a_thread_page_of_101_is_refused`.
- **Dimension 8.10** — a report is bounded only by its frame → Test `a_report_with_300_calls_and_100_changes_is_accepted` (unit, `af_wire`). Negative test `a_report_past_the_frame_is_refused`.
- **Dimension 8.11** — a long cron expression reaches the parser → Test `a_long_cron_expression_is_judged_by_the_parser` (unit, `af_fleet_runtime`): a 200-character expression. Expected: the parser's refusal, never a length refusal.

## Interfaces

- **Approval routes:** unchanged. `POST /v1/workspaces/{workspace_id}/approvals/{gate_id}/{decision}` and the Slack callback resolve a gate; a `question` gate takes `{"answer":"…"}` with the `answered` decision.
- **Effect requests:** `MessageRequest`, `ScheduleCreateRequest`, `SchedulePatchRequest` and `ScheduleRunRequest` carry `"item_id":"019f…"`, beside the run's fence.
- **Fleet definition:** `x-agentsfleet.deliveries: [{"type":"email","to":["owner","member:019f…"]}]`. Recipients are roles or user ids, so a definition holds no email address.
- **Delivery:** provider `email`, destination the user id; the address is looked up at send time.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Racing answers | race | approve and deny together | one wins; the other gets `UZ-APPROVAL-006` | `two_answers_to_one_gate_resolve_it_once` |
| Archived fleet | failure | an answer after archive | `UZ-AGT-020` | `an_answer_to_an_archived_fleets_gate_is_refused` |
| Foreign answer | security | another workspace's token | 404 | `a_member_of_another_workspace_cannot_answer_a_gate` |
| Retried effect | replay | the runner retries a post | the stored result, no second effect | `a_retried_message_for_one_item_posts_once` |
| Stale effect | replay | a superseded run | `UZ-RUN-005` | `an_effect_from_a_superseded_fence_is_refused` |
| Exhausted task | exceeded quota | the model budget is spent | not assigned; `exhausted` | `a_task_over_its_model_budget_is_not_assigned` |
| Stranger recipient | security | a recipient outside the tenant | 422 at save | `saving_a_fleet_that_emails_a_non_member_is_refused` |
| Removed member | security | membership ends before sending | abandoned; nothing sent | `a_recipient_removed_before_sending_is_not_emailed` |
| Relay down | dependency unavailable | SMTP 4xx | retry, then abandon | `an_smtp_4xx_retries_then_abandons` |
| Ambiguous send | timeout | the relay times out after accepting | the retry carries the same idempotency key | `a_redelivered_email_carries_one_idempotency_key` |

## Invariants

1. A gate is decided once. `resolve_gate.sql` updates it only while its status is `pending` or `timed_out`, and the continuation insert reads that row in the same transaction.
2. An effect lands once per item. The item is unique on `(run_id, sequence)`, and the daemon records the result on it before responding.
3. A definition holds no email address. The recipient type accepts only `owner` or `member:<uuid>`, so an address does not parse.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `approval_answered` | product | a person answers a gate | `gate_id`, `status`, `late`, `delay_seconds` | no answer text | `an_approval_after_the_deadline_releases_the_turn` |
| `effect_deduplicated` | operator | a retried request finds its item done | `run_id`, `item_id`, `effect_kind` | none needed | `a_retried_message_for_one_item_posts_once` |
| `email_delivered` | product | the relay accepts the letter | `delivery_id`, `fleet_id`, `attempt` | no address, no body | `a_scheduled_turns_answer_is_emailed_to_the_owner` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A late approval continues the turn (§2) | `make test-integration-rustd` | `an_approval_after_the_deadline_releases_the_turn` passes | P0 | |
| R2 | A late question answer resumes the run (§1, §2) | `make test-integration-rustd` | `a_question_answered_after_the_deadline_resumes_the_run` passes | P0 | |
| R3 | A retried post lands once (§3) | `make test-integration-rustd` | `a_retried_message_for_one_item_posts_once` passes | P0 | |
| R4 | No reservation remains (§4) | `git grep -n -w -E "reserved_model_nanos|reserve_task" -- . ':!docs/v2' ':!bench/baselines' ':!CHANGELOG.md'` | no output | P0 | |
| R5 | The outage answer reaches the owner by email (§5) | `make test-integration-rustd` | `a_scheduled_turns_answer_is_emailed_to_the_owner` passes | P0 | |
| R6 | A stranger cannot be emailed (§5) | `make test-integration-rustd` | `saving_a_fleet_that_emails_a_non_member_is_refused` passes | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit lanes | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration lane | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `reserve_task.sql` | `git grep -n -w "reserve_task" -- . ':!docs/v2' ':!bench/baselines' ':!CHANGELOG.md'` | no output |
| `DETAIL_WAITING` | `git grep -n -w "DETAIL_WAITING" -- . ':!docs/v2'` | no output |

## Out of Scope

- Inbound email (E2): not planned.
- Email to addresses outside the tenant: not planned, per Indy's "Fleet owner, verified members".
- Deduplicating effects across a resumed run by call id: a resumed run's model makes new calls, so the recovery note carries that knowledge instead.

---

## Product Clarity (authoring record)

1. **Successful user moment** — At 09:00 the outage fleet finds 5xx at 4.1%, and the owner reads "Outage watch: 5xx 4.1% since 08:42, likely deploy #812" in their inbox.
2. **Preserved user behaviour** — Approvals before the deadline, the approval inbox, the Slack callback, Slack answers and task budgets work as before.
3. **Optimal-way check** — The email lane reuses the outbound worker and the invite relay, and gates keep their table, so no new transport or store is added.
4. **Rebuild-vs-iterate** — Iterate: gates, the outbound worker and the mailer exist, and each gains one path.
5. **What we build** — late answers, question gates with a waiting checkpoint, continuation turns, effect items, the reservation's removal, and the email lane.
6. **What we do NOT build** — inbound email, recipients outside the tenant, and a new approval route.
7. **Fit with existing features** — builds on M247 respawn, which resumes a continuation from its checkpoint; Slack delivery shares the worker and must keep its behaviour.
8. **Surface order** — runner and daemon first, then the card.
9. **Dashboard restraint** — no new screens; the card gains "Deadline passed" and an answer box.
10. **Confused-user next step** — a refused recipient names the field and says recipients must be tenant members.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** questions (§1), answers (§2), effect items (§3), reservation removal (§4), email (§5).
- **Alternatives considered:**
  - **Fold every approval into `thread.items`.** Install grant cards have no turn and policy gates park before any run, so they would need a second home anyway; rejected.
  - **Deduplicate effects by the provider's call id.** Checkpoints come before the model call, so a resumed run makes new call ids; rejected.
- **Patch-vs-refactor verdict:** a **patch** because each piece extends an existing path.

## Discovery (consult log)

- **Consults** — Indy's decisions, quoted:
  - > Owner (Indy, Nov 19, 2026): "Late human approvals remain valid." — context: §2.
  - > Owner (Indy, Nov 19, 2026): "One primitive (Recommended)" — context: §1, one gate table for every human input.
  - > Owner (Indy, Nov 19, 2026): "Key by the item (Recommended)" — context: §3.
  - > Owner (Indy, Nov 19, 2026): "Fleet owner, verified members (Recommended)" — context: §5 recipients.
  - > Owner (Indy, Nov 19, 2026): "W1 setup context (Recommended), E1 email delivery (Recommended)" — context: E1 in scope, E2 not.
  - > Owner (Indy, Nov 19, 2026): "Accept all (Recommended)" — context: decisions D4–D10 and D12.
    - **D5:** remove `reserve_task`.
    - **D9:** an idempotency key per call on message, schedule and cron.
  - > Owner (Indy, Nov 19, 2026): "donot do the lint, test-unit, test-integration check but rather do it at the end of every workstream. but for every dimension do cargo check, cargo fmt (clippy) so we know code compiles." — context: per-Dimension compile checks; S rows at the end of each Workstream.
  - > Owner (Indy, Nov 19, 2026): "Ensure the rust code for individual files is 99% and overall is 99% if possible or else stick to the numbers we have currently." — context: coverage targets.
  - > Owner (Indy, Nov 21, 2026): "I dont want setup, task-storage, tasks, expiry, liveness" and "i dont want any of the sweeper ultrathink and convice me why they are needed" — context: every daemon sweeper; four jobs (billing, the lost wake, the lost delivery ping, the missed CI webhook) move to their events.
  - > Owner (Indy, Nov 21, 2026): "New §7 in M253_003 (Recommended)" — context: where the sweeper removal lands.
  - > Owner (Indy, Nov 21, 2026): "I want to signoff debate and then have you fix the item 7 of sweepers. so consult when you start 7. on sweepers" — context: §7 is built only after Indy debates and signs off its design.
- **Consults, agent defaults from the EGM review** — these narrow Indy's decisions and are listed in the PR for him:
  - **The gate table stays.** Install grant cards and policy gates have no run, so approvals do not fold into items.
  - **Effect keys protect retries only.** A resumed run's model makes new calls, so `(run_id, sequence)` dedupes a retried request, and the recovery note covers a resume.
  - **Budget without reservation.** Concurrent decision calls on one task can overshoot `max_model_nanos` by one call's cost.
  - **Recipients.** `owner` means every tenant owner.
- **Model reviews** — EGM Stage 9 (implementation readiness), three reviewers, Claude Opus 5.5, Nov 19, 2026: 16 Blockers, 32 Majors, 26 Nits. Every Blocker is resolved above, wording findings are applied, and none is declined.
- **History caps** — An agent added `HISTORY_TURNS_MAX` (8), `HISTORY_BYTES_MAX` (64 KiB) and `TURN_TEXT_BYTES_MAX` (16 KiB) (`af_wire/src/run/payload.rs:8-12`) in M211_005 without the owner's OK.
  - > Owner (Indy, Nov 21, 2026): "Delete all three caps, but what is a frame limit"
  - > Owner (Indy, Nov 21, 2026): "first let indy signoff" — context: no cap changes until he has read the inventory of every limit (`/tmp/m247-review/limits-inventory.md`) and the Codex/Pi comparison (`/tmp/m247-review/limits-codex-pi-guffbolt.md`).
  - > Owner (Indy, Nov 21, 2026): "Change the limits of us, i am leaning towards a combination of Codex and Pi" and then "I am approving the above + the earlier" — context: the caps proposal (`/tmp/m247-review/caps-removal-proposal.md`) and the ranked knockoff list: delete the history caps and save the fleet summary; carry earlier tool calls; resize images; delete setup 300 s, `MODEL_CALLS_MAX`, the 12 h ceiling, `MAX_CRON_LEN`, the shell max, the per-run subagent cap and the report-only caps; sleep 12 h, MCP 300 s, 6 open subagents, executor frame 64 MiB, thread page 25/100. Unchanged: the control frame, the 8 MiB read, NATS, `HOLDS_MAX`, and every row the Codex/Pi survey did not cover. To be built as §8.
  - > Owner (Indy, Nov 21, 2026): "yes signedoff approved" — context: the final per-section table of removed caps; §8 is approved. §7 is a separate sign-off.
- **Open blockers** — §7 waits for Indy's sign-off on its design before any code.
- **Deferrals and transfers** — none.
