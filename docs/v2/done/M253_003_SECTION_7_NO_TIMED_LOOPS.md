# M253_003 Section 7: The daemon runs no sweeper — DEFERRED

**Status:** DEFERRED (parked Nov 21, 2026). Reopen when the owner decides what, if anything, replaces each deleted loop after testing.

> Owner (Indy, Nov 21, 2026): "so move the 7 no timed loops to parked then? and continue to 1, and so on?" — context: the wake-up design below (a `Wakeups` port, QStash bookings, a boot pass) was cut ("cut the crap on qstash approach of a scheduler"); every timed loop was deleted with no replacement ("Delete all now"), keeping the workers that react to bus pings; a failed delivery is final ("i dont want to create any retry now"); "I want to keep the system simple and grow from there."

What shipped instead of this Section: `main/src/sweepers/`, the outbound worker's timed pass, the scheduler's `ASSIGN_FALLBACK_TICK` and every `[maintenance.*]` key are gone; afd_cron's QStash scheduler for fleet schedules stays.

The original Section, kept for reference:

## §7 — The daemon runs no sweeper

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

