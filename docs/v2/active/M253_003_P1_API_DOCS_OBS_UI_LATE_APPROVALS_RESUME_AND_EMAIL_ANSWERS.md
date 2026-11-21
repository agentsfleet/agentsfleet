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

A retried request for the same item returns the stored result, and no second effect happens. A request from a run below the thread's fence is refused first, with `RUN_STALE_FENCING_TOKEN`. No count caps a run's messages.

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

A fleet definition may list `x-agentsfleet.deliveries` entries of `{"type":"email","to":[…]}`. Each recipient is `owner` or `member:<user_id>`, with no repeats and no count cap. The save path checks each one against tenant membership and refuses a stranger with `DELIVERY_RECIPIENT_REFUSED`.

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

Every timed loop goes. A fact that changed only because time passed is checked when something reads or claims it; work a loop finished is done on the event that owes it; and the three jobs that need a clock book a one-shot wake-up through a `Wakeups` port: `book(job, at)`, with the job a closed enum (`DeliveryRetry`, `WaitCheck`, `ObjectPurge`) and its row id. The jobs and their callback handlers name only the port; QStash, the scheduler that already fires cron triggers (`afd_cron/src/qstash/`), is its one production adapter, and tests run an in-memory adapter that fires on a test clock. Another scheduler is a new adapter, not a change to any job. A wake-up is booked before its row commits, so a failed commit leaves a callback that finds nothing; the callback reads Postgres and does nothing when the work is done. One pass at daemon boot covers a crash. `sweeper:retention` went in M253_002 §14.

```
 event ──► book QStash "POST /ingress/wake {job} at T" ──► commit the row
 QStash at T ──► any replica ──► read the row ──► done: no-op │ owed: do it, book the next
 daemon boot ──► one pass over owed work
```

| Loop today | What replaces it |
|---|---|
| `sweeper:liveness` | the heartbeat statement (`heartbeat_with_transition_history.sql`) cordons an `active` runner returning after the offline window and writes `runner_offline` and `runner_cordoned`; the capacity locks refuse a host not seen within the window; the view derives `drained` from `draining` with no live run, and the `runner_drained` event goes |
| `sweeper:fleet-census` | nothing: its three gauges go |
| `sweeper:setup` | its bounded purge runs at boot and when a draft closes or is created |
| `sweeper:billing` | the transaction that logs usage lines settles them before it commits, debiting wallets in tenant order; an expired unissued decision call is released by the next reserve or run issue under the task lock |
| `sweeper:expiry` | nothing: a dead run's last slice, at most `RUN_TTL` at the runtime rate, is not billed; the runner delete guard checks only for a live run |
| `sweeper:tasks` | completion cleanup and fanout run in the webhook; an expired task reads `exhausted` and is written so at read or claim; retention treats `LEAST(terminal_at, expires_at)` as terminal; an uncertain effect is reconciled inline and on the `pull_request` webhook; a wait books QStash checks at 60, 120, 240 and 300 s |
| `sweeper:task-storage` | the newest-three trim runs at checkpoint commit; an unreferenced object books its R2 delete after its upload window, and a failed delete books again; turn checkpoints book their 7-day deletion at settle |
| `connector:outbound` 30 s pass | a delivery that fails books QStash for its `next_attempt_at`; a Slack notice pings on owe; boot claims what is owed |
| `runner_stream_scheduler` recovery probe | an idle runner's heartbeat asks for waiting work |

- **Dimension 7.1** — no timed loop is started → Test `the_daemon_starts_no_sweeper` (unit): the supervised task names at boot. Expected: none starts with `sweeper:`, no `connector:outbound` pass, no recovery probe, and no `[maintenance.*]` key is accepted.
- **Dimension 7.2** — a returning silent runner is cordoned → Test `a_runner_back_after_the_offline_window_is_cordoned` (integration): `last_seen_at` past the window, then one heartbeat. Expected: `cordon_reason` set, `runner_offline` and `runner_cordoned` written, no assignment until the cordon lifts. Negative test `a_runner_never_seen_takes_no_work_before_its_first_beat`.
- **Dimension 7.3** — a report pays for itself → Test `a_report_settles_its_usage_in_its_transaction` (integration): one report. Expected: the lines are settled and the wallet is debited when the report commits. Negative test `a_report_whose_settlement_fails_writes_nothing`.
- **Dimension 7.4** — a dead runner can be deleted → Test `a_runner_whose_run_lapsed_unbilled_is_deleted` (integration). Expected: deleted.
- **Dimension 7.5** — a lost wake does not strand work → Test `an_idle_runners_heartbeat_is_assigned_waiting_work` (integration): queued work with no bus wake. Expected: the next heartbeat assigns it.
- **Dimension 7.6** — a failed Slack post is retried by its wake-up → Test `a_failed_delivery_books_and_honours_its_wake_up` (integration, local QStash): Slack answers 503 three times, then 200. Expected: one QStash booking at `next_attempt_at`, and the callback posts once. Negative test `a_wake_up_for_a_delivered_answer_does_nothing`.
- **Dimension 7.7** — a missed CI webhook still wakes its wait → Test `a_wait_with_no_webhook_is_woken_by_its_check` (integration, local QStash). Expected: the check reads GitHub and stages the turn. Negative test `a_check_after_the_webhook_does_nothing`.
- **Dimension 7.8** — task limits hold without a sweep → Test `a_task_past_its_wall_limit_is_exhausted_at_claim` (integration). Expected: not assigned, and `exhausted` with `TASK_LIMIT_EXHAUSTED`.
- **Dimension 7.9** — R2 storage is freed by its wake-up → Test `a_fourth_generation_deletes_the_oldest` (integration): four captures. Expected: three remain. Test `an_orphaned_object_is_deleted_by_its_wake_up` (integration, local QStash). Expected: the R2 delete runs after the upload window and the quota drops by its bytes.
- **Dimension 7.11** — the jobs know no scheduler → Test `no_wake_up_job_names_qstash` (unit): the delivery, wait and storage crates. Expected: none names `afd_cron::qstash` or a QStash type; each books through `Wakeups`. Test `the_in_memory_adapter_fires_a_booking_at_its_time` (unit).
- **Dimension 7.10** — a booking that cannot reach QStash is caught at boot → Test `owed_work_with_no_booking_runs_at_boot` (integration). Expected: the boot pass runs it once.

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

- **Dimension 8.1** — DONE — a run carries the whole chat → Test `a_long_chat_run_carries_every_finished_turn` (unit, `afd_fleet` history): 20 finished turns of 20 KiB each. Expected: 20 turns, none cut.
- **Dimension 8.2** — DONE — a fleet keeps its summary across runs → Test `the_next_fleet_turn_loads_the_saved_summary` (integration, `af_e2e`): a fleet chat that compacts. Expected: the next run's first request holds the saved summary and none of the turns it replaced. Negative test `a_fleet_summary_at_a_superseded_fence_is_refused`.
- **Dimension 8.3** — DONE — a follow-up sees the earlier tool calls → Test `a_follow_up_run_carries_the_earlier_turns_tool_calls` (unit, `afd_fleet` history): a finished turn with two items. Expected: the next run's history holds both calls with their output.
- **Dimension 8.4** — DONE — a large image is resized → Test `an_image_over_the_bound_is_resized_to_2048` (unit, guffbolt): a 4,096 px PNG. Expected: the model gets a 2,048 px image. Negative test `an_image_that_does_not_decode_is_refused`.
- **Dimension 8.5** — DONE — waits match Codex and Pi → Tests `sleep_waits_as_asked_capped_at_twelve_hours_and_wakes_for_input`, `shell_runs_sh_c_under_the_timeout_asked_held_to_its_ceiling` and `a_server_that_never_answers_fails_each_call_at_its_timeout` (guffbolt public API), `should_hold_the_timeout_the_model_names_under_the_ceiling` (`afr_tools`). Expected: a 24 h sleep waits 12 h; a 3,000,000,000 ms shell timeout is held to 2,147,483,647 ms; the MCP deadline is 300 s.
- **Dimension 8.6** — DONE — setup has no runtime ceiling → Test `a_setup_run_renews_past_five_minutes` (integration, `afd_fleet`): a setup run renewed past 300 s. Expected: renewed. Negative test `a_setup_run_on_an_empty_wallet_is_refused`.
- **Dimension 8.7** — DONE — the budget, not a clock, ends a looping run → Test `a_run_past_twelve_hours_renews` (unit, `afd_fleet` renew). Expected: renewed. Negative test `a_run_over_its_budget_is_not_renewed`.
- **Dimension 8.8** — DONE — subagents match Codex → Tests `a_seventh_open_subagent_is_refused` (unit, guffbolt; Codex refuses past `DEFAULT_AGENT_MAX_THREADS`, `core/src/agent/registry.rs:104-109`), `a_run_starts_more_than_sixteen_subagents` and `close_agent_frees_a_seat_under_the_open_cap_and_no_start_count_binds` (public API).
- **Dimension 8.9** — DONE — frames and pages match Codex → Test `the_executor_carries_a_frame_over_16_mib` (unit, `afr_executor`) and `a_thread_page_of_100_is_served` (integration, `afd_turns`). Negative test `a_thread_page_of_101_is_refused`.
- **Dimension 8.10** — DONE — a report is bounded only by its frame → Test `a_report_with_300_calls_and_100_changes_is_accepted` (unit, `af_wire`). Negative test `a_report_past_the_frame_is_refused`.
- **Dimension 8.11** — DONE — a long cron expression reaches the parser → Test `a_long_cron_expression_is_judged_by_the_parser` (unit, `af_fleet_runtime`): a 200-character expression. Expected: the parser's refusal, never a length refusal.

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
  - > Owner (Indy, Nov 21, 2026): "I agree on the removals. But i want ot debate each one, example  sweeper:expiry - i feel this runner cordon and drain will handle this? we dont want to overengineer here at the start." — context: §7. `sweeper:expiry` goes; a dead run's last slice (at most 35 s, $0.0035) is not billed.
  - > Owner (Indy, Nov 21, 2026): "If we use with JETSTREAM on can this work?" and "ultrathink to see how we can remove all of the sweepers?" then "QStash wake-ups (Recommended)" — context: §7's design. JetStream stays off; the three clock jobs book one-shot QStash wake-ups.
  - > Owner (Indy, Nov 21, 2026): "also ensure all the crates are using the latest updated version, except any anamolies to stick to libc < 1.x version and so on." — context: new §9. Every external dependency moves to its newest release; a crate held back names its reason (libc stays on 0.2).
  - > Owner (Indy, Nov 21, 2026): "Cache + split + 4 jobs (Recommended)" — context: the coverage lane timed out (run `44865888670`, daemon shard at its 30 min cap). Each shard restores its dependency builds through `rust-cache` (the repo declined `sccache` in `test-unit-rustd.yml`), the daemon layer splits into `daemon-api` and `daemon`, and Cargo builds with four jobs.
  - > Owner (Indy, Nov 21, 2026): "that will be our first test fleet as seen in @rustd/crates/protocol/af_fleet_runtime/tests/fixtures/fleets/runner-link-watch/ I want the other fleets you have to be reviewed and the ones we dont need or duplicates to be nuked." then "Nuke 9, keep chief-of-staff (Recommended)" — context: fleet fixtures. incident-responder, incident-verifier, security-reviewer, zoho-recruiter and three unused `cases.json` go; zoho-sprint's cron read-back moves to runner-link-watch, and platform-ops' and steer-probe's template checks to `definition/with_model_and_context.json`; the parity walk takes the three accepted `definition/` files.
  - > Owner (Indy, Nov 21, 2026): "Why is the definition/ outside? and why is the goal-repair separate in decision policy i think it must be merge to a fleet fixture?" and "Ensure the abstraction is agnostic to qstash for the anamoly fixators" — context: the parser cases move to `tests/fixtures/definitions/`; the decision policy moves beside `ci-repairer`'s definition; §7's clock jobs book through a `Wakeups` port with QStash as one adapter.
  - > Owner (Indy, Nov 21, 2026): "Delete it, match Codex (Recommended)" — context: §8, the fold batch. A fold hands a working run every pending message of its conversation, cut only to the frame, as Codex reads all pending input at its next step (`core/src/session/turn.rs:577`); the steer status read keeps its own 64-message look-back, a query-cost bound.
  - > Owner (Indy, Nov 21, 2026): "How does codex operate?" then "Match Codex (Recommended)" — context: §8, tool output. Codex shows a 3-line preview (`tui/src/tool_output.rs:19`), gives the model 10k tokens, and keeps the whole output in its session file (`core/src/context_manager/history.rs:631-632`). The trace row keeps its short preview; the trace lists every call (its 200-call and 64 KiB caps go); the item record keeps the whole output (its 64 KiB, 1 MiB and 256 KiB caps go).
  - > Owner (Indy, Nov 21, 2026): "yes approved - 2. Raise the NATS draft frame from 32 KiB to 960 KiB; no server change needed." — context: §8, `Delta.text`. DRAFT_FRAME_BYTES is NATS's default 1 MiB `max_payload` less a 64 KiB envelope; a live setup update may be about 159 KiB.
  - > Owner (Indy, Nov 21, 2026): "Delete the client copies (Recommended)" — context: the app and CLI re-checked live-stream frames against their own copies of the daemon's sizes; they now check shape only and trust the daemon's bound.
  - > Owner (Indy, Nov 21, 2026): "Reject some", then rejected "3 Status read of 64 msgs" and left 1, 2 and 4 unpicked (accepted): the shell's 24.8-day ceiling; the operator settings' maxima (ttl ≤ 1 h, offline-after ≤ 3 h, sandbox idle hold ≤ 1 day); an item record cut only past the 32 MiB link message. The steer status read now looks at the whole backlog. Then: "And i have approve 1-2, 4,5,6,7" — deviations 5 (image 8 MiB read, 5 MiB result), 6 (UZ-RUN-011 for a late task run) and 7 (docs after merge) are accepted too.
  - > Owner (Indy, Nov 21, 2026): "Delete it (Recommended)" — context: §8, the decision request's 16 KiB state field (`STATE_MAX_BYTES`); the decision model's `input_max_bytes` is the only bound. Codex and Pi have no judge model to match.
  - > Owner (Indy, Nov 21, 2026): "yes no recipient count and without the per-run message cap. Why are you reasoning crap on caps or limits, thsi is absurd and stupid" — context: §3 and §5 carry no count caps; `.orly/LOCAL.md` now says no limit exists without a provider, transport or operating-system reason or the owner's number.
  - > Owner (Indy, Nov 21, 2026), on a failed summary save: "One rule for both (Recommended)" — context: in setup and fleet chats alike, a superseded run's save fails the run; any other save error is logged, saving stops, and the answer is still sent.
  - > Owner (Indy, Nov 21, 2026), on earlier tool calls in a run that declares no tools: "Text, as Pi does (Recommended)" — context: Pi's summary request writes them as `[Assistant tool calls]: …` and `[Tool result]: …` (`pi/packages/coding-agent/src/core/compaction/utils.ts:145,150`); Codex sends tool items to OpenAI with no tools declared (`codex-rs/core/src/compact.rs:334`).
- **Open blockers** — none. Unverified at sign-off: QStash's per-message price and its plan's longest delay against the 7-day turn-checkpoint deletion.
- **Deferrals and transfers** — none.
