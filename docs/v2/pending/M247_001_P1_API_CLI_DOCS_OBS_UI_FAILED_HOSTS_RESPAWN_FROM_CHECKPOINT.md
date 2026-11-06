<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M247_001: A host that stops mid-turn is cordoned, and its turn restarts on another runner from its last checkpoint

**Prototype:** v2.0.0

**Milestone:** M247

**Workstream:** 001

**Date:** Nov 06, 2026

**Status:** PENDING

**Priority:** P1, a killed host today restarts its turn from nothing, shows the user a broken reply, and keeps taking work when it returns

**Categories:** API, CLI, DOCS, OBS, UI

**Batch:** B1, the only Workstream of M247, the last push-scheduler milestone

**Depends on:** PR #803 (`feat/m236-durable-tasks`), whose checkpoints §4 loads and whose `durable_tasks_v1` is on by default; M246_001, whose tenant setting governs a reclaim across owners; M243_001, whose stream carries Hello and Cancel and replaces the heartbeat §2 lifts on; M242_002, whose `folded_through_event_id` cursor the checkpoint keeps; M241_001, the design page

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 06, 2026) from the EGM brief and source research on main 14539a73e

**Canonical architecture:** `docs/architecture/push_scheduler.md` §M247 and §A host dies mid-turn; `runner_fleet.md` and `lease_flow.md` describe the lease, slot and liveness model this changes

---

## Overview

**Goal (testable):** A runner killed mid-turn has its turn reclaimed by another runner after `LEASE_TTL`, resumed from the turn's last checkpoint, settled once and announced by one `run_restarted` frame, while the silent host is cordoned and every host counts its held sandboxes against its workers.

**Problem:**
- A host accepts new fleets past its memory. The census counts active leases and authoring attempts but no held sandboxes (`afd_runner/src/capacity/sql/census.sql`), while the keeper holds up to `workers − 1` frozen sandboxes beside busy workers (`afr_supervisor/src/holds/keeper.rs:151`).
- A silent host is recorded offline but never cordoned (`afd_runner/src/sweep/liveness/mod.rs:208-229`). When it returns it takes work at once, so a flapping host keeps dropping turns.
- The same sweep backdates a silent host's slots (`sql/sweep/expire_active_lease_slots.sql`). On a dead host that matches nothing; on a host still renewing it starts a second copy of a running turn.
- A dead holder's fleet can wait 90 s, not 30 s, when its lease resumed a held sandbox. The claim keeps the hold under the lease (`claim_admission_head.sql:19-20`), only a later beat's reconcile clears it, and a live holder's hold binds every other claimer (`:23-30`).
- A reclaimed turn starts clean (`afd_fleet/src/lease/envelope.rs:63-66`). Its checkpoint is keyed by the dead lease (`schema/2160_memory_context_checkpoints.sql`, `afd_context/src/repository/load.sql`).
- The browser reads the restarted reply as a gap (`ui/packages/app/lib/streaming/reply-stream.ts:24-27`), and the CLI prints the replayed text after the old.

**Solution summary:** The census counts held sandboxes, and the runner's keeper alone decides which hold to evict. The liveness sweep cordons a silent host in the transaction that records it offline, stops backdating slots, and a returning host's beats lift the automatic cordon. A reclaim publishes one `run_restarted` frame, the old run's frames are refused, and the browser and CLI reset the reply. The reclaimed lease resumes the turn from its last checkpoint. Operators see the cordon and its reason on the runner pages.

**Design flow:**

```
t0       host A dies mid-turn; its last renewal set its lease to lapse by t0 + 30 s
t0+30 s  A's lease lapses ── the only respawn trigger (LEASE_TTL)
         runner B claims: slot expired, no hold (A's lease consumed it at issue)
           └─► fence + 1; A's lease → expired; billing carried forward
         issue under B's host lock: leases + authoring + other fleets' holds < workers
           └─► the lease names A's lease ─► the turn's last checkpoint (#803)
           └─► run_restarted{event_id} on the tail ─► browser and CLI reset the reply
         A's late frames and late report: refused by the fence
t0+90 s  liveness sweep: A silent past RUNNER_OFFLINE_AFTER
           └─► one transaction: runner_offline + cordon(missed_heartbeats) + runner_cordoned
A beats again: runner_online ─► 30 s with no newer offline record ─► cordon lifted, runner_uncordoned
```

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): cordon a failed host and restart its turn from checkpoint
- **Intent (one sentence):** a host that dies mid-turn costs the user one visible restart, not a lost or doubled reply, and stops taking work until it proves healthy.
- **Authoring handshake:** Indy wants a failed bare-metal host cordoned automatically and its work respawned on another runner, at scale, with durable tasks on by default. ASSUMPTIONS I'M MAKING:
  1. M242 to M246 have merged before this starts, so the claim, renewal and liveness writes sit where those specs moved them. CHORE(open) re-locates each Files Changed role on `origin/main`.
  2. #803 defines where the current turn's last checkpoint is recorded and how a runner resumes from it. §4 wires the reclaim to that pointer and builds nothing #803 already built.
  3. Pre-production: a fresh dev cutover is acceptable, so the runner keeper and the daemon's hold count ship together with no compatibility window.
  4. Tool effects between the last checkpoint and the kill may run twice (the brief's non-goal "exactly-once external effects"); the respawned host starts with a fresh sandbox disk. Discovery asks Indy to accept both.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/afd_runner/src/capacity/mod.rs` and `capacity/sql/census.sql` — the one host-capacity authority, read under the host-row lock; §1 widens its count.
2. `rustd/crates/afd_runner/src/sweep/liveness/mod.rs` — the offline record §2 cordons beside, and the slot backdating it deletes.
3. `rustd/crates/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` and `lease/assign/ledger.rs` — the reclaim: the prior lease, the fence bump and the billing it carries.
4. `rustd/crates/afr_supervisor/src/holds/keeper.rs` — the hold cap and the saturation eviction §1 makes the single eviction rule.
5. `ui/packages/app/lib/streaming/fleet-stream-reply-registry.ts` and `reply-stream.ts` — where a second stream start becomes a gap today.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M247_001_P1_API_CLI_DOCS_OBS_UI_FAILED_HOSTS_RESPAWN_FROM_CHECKPOINT.md` | CREATE, then MOVE to `active/` and `done/` | this spec |
| `rustd/crates/afd_runner/src/capacity/mod.rs`, `capacity/sql/census.sql`, `capacity/sql/lock_host.sql`, `capacity/sql/mod.rs` | EDIT | §1, §2: the `Claimant`, the hold count, the cordon flag |
| `rustd/crates/afd_fleet/src/lease/issue.rs`, `lease/renew/mod.rs`, `lease/commit.rs`; `rustd/crates/afd_authoring/src/lease/claim/mod.rs`, `lease/renew.rs` | EDIT | §1: each lock caller names its claimant |
| `rustd/crates/afd_fleet/src/lease/sql/lease/insert_lease_with_event.sql` | EDIT | §1: the issued lease clears its fleet's hold |
| `rustd/crates/afr_supervisor/src/holds/keeper.rs`, `holds/keeper_tests.rs` | EDIT | §1: the one-free-worker reserve |
| `rustd/crates/afd_runner/tests/capacity/holds.rs`, `capacity/mod.rs`, `capacity/fixture.rs`, `tests/integration_hold_plans.rs` | CREATE, EDIT | §1 tests and the census plan |
| `rustd/crates/afd_fleet/tests/integration_held_sandbox/resume.rs` | EDIT | §1: the lease consumes the hold |
| `schema/2900_runner_runners_cordon.sql`; `rustd/crates/afd_db/src/migration.rs` and its position assertions | CREATE, EDIT | §2: the cordon columns (placeholder slot) |
| `rustd/crates/afd_core/src/timing.rs` | EDIT | §2: `UNCORDON_AFTER` and its asserted relation to `HEARTBEAT_INTERVAL` |
| `rustd/crates/afd_runner/src/sweep/liveness/mod.rs`, `liveness/tests.rs` | EDIT | §2: cordon with the offline record; the slot release goes |
| `rustd/crates/afd_runner/src/sql/sweep/expire_active_lease_slots.sql` | DELETE | §2: the second respawn trigger |
| `rustd/crates/afd_runner/src/sql/sweep/mod.rs`, `sql/sweep/select_due_runners.sql`, `sql/sweep/cordon_silent_runner.sql` | EDIT, CREATE | §2: the cordon statement; the due query drops the release arm |
| `rustd/crates/afd_runner/src/heartbeat/mod.rs`, `heartbeat/cordon.rs`, `sql/runner/lift_automatic_cordon.sql`, `sql/runner/select_runner_assigned_policy.sql`, `sql/runner/mod.rs` | EDIT, CREATE | §2: the lift beside the liveness write (M243's stream session once it replaces the heartbeat) |
| `rustd/crates/afd_runner/src/cordon.rs`, `lib.rs` | CREATE, EDIT | §2: `CordonReason` and its stored spelling |
| `rustd/crates/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql` | EDIT | §2: a cordoned claimer gets no candidate (M243's offer read once it replaces the scan) |
| `rustd/crates/afd_runner/tests/integration_sweeps/mod.rs`, `seed.rs`; `rustd/crates/afd_fleet/tests/integration_runner_beat/cordon.rs`, `integration_runner_beat/mod.rs` | EDIT, CREATE | §2 tests; the sweep test stops asserting backdated slots |
| `rustd/crates/afd_api_wire/src/tail/mod.rs`; `rustd/crates/afd_fleet/src/lease/bracket.rs`, `lease/pull/mod.rs`, `lease/pull/restart.rs` | EDIT, CREATE | §3: `RunRestarted`, published after an issued reclaim |
| `rustd/crates/afd_fleet/src/lease/activity/mod.rs`, `lease/sql/select_lease_target.sql` | EDIT | §3: a superseded lease's frames are refused |
| `rustd/crates/afd_api_tenant/src/handler/stream/mod.rs` | EDIT | §3: the stream's frame list names `run_restarted` |
| `ui/packages/app/lib/api/events-types.ts`, `lib/streaming/fleet-stream-dispatch.ts`, `fleet-stream-reply-registry.ts`, `fleet-stream-reply-frames.ts`, `fleet-stream-row.ts`, `fleet-stream-registry.reply.test.ts`, `reply-stream.test.ts` | EDIT | §3: the browser resets the reply |
| `ui/packages/app/components/domain/useFleetEventStream.ts`, `fleetMessageReaders.ts`, `FleetReplyBody.tsx`, `FleetReplyBody.test.tsx` | EDIT | §3: the restart marker |
| `cli/src/commands/fleet_steer_events.ts`, `cli/test/fleet-steer-tail.unit.test.ts` | EDIT | §3: the CLI's restart line |
| `rustd/crates/afd_fleet/src/lease/reclaim.rs`, `lease/assign/ledger.rs`, `lease/sql/lease/claim_admission_head.sql` | EDIT | §4: `Reused` carries the prior lease |
| `rustd/crates/afd_context/src/repository/load.sql`, `repository/mod.rs`, and the files #803 adds for its checkpoint pointer and the lease answer's checkpoint field, named at CHORE(open) | EDIT | §4: the reclaim reads the turn's last checkpoint (open blocker 1) |
| `rustd/crates/afd_fleet/tests/integration_respawn.rs`, `tests/fleet_suite.rs` | CREATE, EDIT | §3, §4 integration tests |
| `rustd/crates/afd_api_wire/src/operator.rs`, `admin/mod.rs`; `rustd/crates/afd_runner/src/view/decode.rs`, `view/mod.rs`, `sql/runner_view/list_runners_first.sql`, `list_runners_after.sql`, `runner_detail.sql` | EDIT | §5: `cordon` on runner reads; `runner_uncordoned` |
| `rustd/crates/afd_fleet/tests/integration_runner_views.rs` | EDIT | §5: the route test |
| `ui/packages/app/lib/api/runners-types.ts`, `app/(dashboard)/admin/runners/components/RunnerStatus.tsx`, `[runnerId]/components/RunnerStatusLine.tsx`, `RunnerStatusLine.test.tsx`, `runner-activity.ts`, `runner-activity.test.ts`, `runner-copy.ts` | EDIT | §5: the cordon badge and the lift event |
| `rustd/crates/afd_observability/src/metrics/declared/fleet.rs`, `producers/fleet/mod.rs`, `tests/producers_drive.rs`; `docs/metrics.census.tsv` | EDIT | §5: `agentsfleet_runner_cordon_changes_total` |
| `rustd/crates/agentsfleet_runner/tests/respawn.rs`, `tests/runner_suite.rs`, `Cargo.toml` | CREATE, EDIT | §6: the kill drill and its dev-dependencies |
| `public/openapi.json` | EDIT | §3, §5: regenerated with `--no-banner` |
| `docs/architecture/push_scheduler.md`, `runner_fleet.md`, `lease_flow.md`, `runner_execution.md`, `scaling.md`, `observability.md` | EDIT | §6: held sandboxes counted; the cordon rule; respawn from checkpoint; the sweep row and the predecessor proposal |
| `~/Projects/docs` `runners.mdx`, `changelog.mdx` on `chore/m247-respawn-changelog` | EDIT | §6: the public cordon rule and the restart frame |

## Applicable Rules

- **`domains/sql.md`** — SCH, MIG, STS, SQL-READER, SQL-TSTYPE, SQL-ONEFACT, SQL-LOCKCLOCK, SQL-SWEEPCLOCK, SQL-PREPARE, SQL-PLAN, SQL-FILE, TXN
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-RACE, RS-TESTCONST
- **`code.md`** — NDC, NLR, NLG, TGU, TIM, OWN, CAS, ORP, FLL, CODE-SEAM
- **`tests.md`** — TEST-KILL, TEST-CONTEND, TEST-INJECT, TEST-PARTIAL, TEST-RESIDUAL, TEST-NEG, TEST-REAL, TFX
- **`languages/typescript.md`** — TS-SCHEMA, TS-NAME, TS-FINALLY
- **`domains/http-surface.md`** — HTTP-OPENAPI, HTTP-ENUM, HTTP-STABILITY, HTTP-SURFACE
- **`domains/http-responses.md`** — HTTP-NULL, HTTP-TIME, HTTP-FIELD
- **`domains/logging.md`** — LOG-EVENT, LOG-CODE, LOG-INFO, OBS
- **`docs.md`** — DOC-02, DOC-03, DOC-08; **`domains/architecture.md`** — ARCH-HOMES, ARCH-PUBLISHED

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `docs.language` at `orly gate verify` and `orly gate pr` | yes, six architecture pages | sentences under 25 words; "automatically" always names its trigger, timing and effect (DOC-08) |
| `make lint-all` (clippy `-D warnings`, `cargo shear`, TypeScript lint) | yes | `lint-rustd` per Section; the drill's new dev-dependencies are used |
| Statement preparation test (SQL-PREPARE) | yes, three new statement files | each new `.sql` file is prepared by the existing test |
| Metrics census and producer test | yes, one family | census row, declaration and producer in one commit |
| OpenAPI regeneration | yes, one frame, one field, one event value | regenerate from `rustd/` with `--no-banner` |
| Playwright (`make dry-app`) | yes, the chat reply and the runner page render new states | run the chat and runner specs locally; a green Pull Request does not prove them |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes: `heartbeat/mod.rs` 319, `pull/mod.rs` 318, `liveness/mod.rs` 312 | the lift in `heartbeat/cordon.rs`; the restart publish in `pull/restart.rs`; `liveness/mod.rs` loses the slot release before it gains the cordon |

## Prior-Art / Reference Implementations

- **Reference:** Kubernetes taint-based evictions (https://kubernetes.io/docs/concepts/scheduling-eviction/taint-and-toleration/#taint-based-evictions) — the node controller adds the `unreachable` taint when heartbeats stop and removes it when the node reports Ready, while `kubectl cordon` is never lifted by the controller. Mirrored: the automatic cordon lifts itself; an operator's does not. Divergence: the lift waits `UNCORDON_AFTER`, not the first beat, to damp a flapping host.
- **Reference:** `afd_runner/src/sweep/liveness/mod.rs` `record_offline` — one transaction gated on the offline event's insert; the cordon joins it in place of the slot release.
- **Reference:** `afd_fleet/src/lease/bracket.rs` `publish_received` — a daemon-owned tail frame, best-effort, published once per row; `run_restarted` follows it.
- **Reference:** `agentsfleet_runner/tests/run.rs` (the real binary, `--unsandboxed`, against a daemon) and `afd_api/tests/authoring/saving/activity/fault.rs` (a test trigger that refuses a write) — the drill's and the injection tests' shapes.

## Sections (implementation slices)

### §1 — A host's held sandboxes count against its workers

The census adds live holds on the host's slots (`last_runner_id` is the host, `hold_expires_at` after the lock's clock) to its active leases and authoring attempts, read through `idx_fleet_slots_last_runner_id_held`. `capacity::lock` takes a `Claimant`. A fleet claim leaves its own fleet's hold out, since the claim resumes it. An authoring claim counts every hold. Renewal and report count none, because a renewal keeps a worker it already holds. The lease insert clears its fleet's hold in the statement that writes the lease, so a hold is never counted beside the lease that took it. A dead holder's fleet is then reclaimable at `LEASE_TTL`.

The runner's keeper is the one owner of eviction. While a worker is free it keeps `busy + held ≤ workers − 1`, retiring the oldest hold first. A lease that starts for a fleet without resuming its hold retires that hold, matching the daemon's clear. Every retirement rings the beat at once through the existing saturation wake. The daemon never picks a hold to free; it counts what the runner last reported, and its existing release list stays for fleets the runner may no longer hold. **Implementation default:** the runner decides because it alone holds the frozen memory; a daemon-chosen victim needs a second message and can differ from the keeper's oldest-first choice. The reserve keeps a cold fleet from waiting out a 10-minute hold on a host whose holds now count. Release order: the keeper ships with the daemon's hold count in one cutover, because a daemon counting holds against today's keeper refuses cold fleets until holds lapse.

- **Dimension 1.1** — a lease and another fleet's hold fill a two-worker host → Test `test_a_lease_and_a_hold_refuse_a_third_fleet` (integration, `afd_runner/tests/capacity/holds.rs`): `worker_count` 2, one active lease, a live hold of fleet H → a fleet claim for C and an authoring claim both read `can_claim` false. Rejects a census that still counts leases only.
- **Dimension 1.2** — the claimant's own hold takes no second worker → Test `test_the_claimants_own_hold_takes_no_second_worker` (integration): the same rows → a claim for H reads `can_claim` true.
- **Dimension 1.3** — a renewal never counts holds → Test `test_a_renewal_counts_no_holds` (integration): `worker_count` 2, two active leases, one live hold → both renewals extend. A census counting holds here would end two running turns.
- **Dimension 1.4** — an issued lease clears its fleet's hold, and a refused one keeps it → Test `test_an_issued_lease_clears_its_fleets_hold` (integration, `integration_held_sandbox/resume.rs`): the holder's claim issued → `hold_expires_at` null; the same claim refused at capacity → `hold_expires_at` unchanged.
- **Dimension 1.5** — a hold never takes the last free worker → Test `test_a_hold_never_takes_the_last_free_worker` (unit, `keeper_tests.rs`, paused time): workers 3, busy 1, one hold, a new lease starts → the hold is retired and the beat rings; workers 2, both busy, one lease ends → its park is refused and the sandbox destroyed.
- **Dimension 1.6** — the hold count reads the held index → Test `test_the_census_counts_holds_from_the_held_index` (integration, `integration_hold_plans.rs`): `EXPLAIN (ANALYZE, BUFFERS)` of the census over 20,000 slot rows names `idx_fleet_slots_last_runner_id_held` and no sequential scan of `runner.fleet_slots`.

### §2 — A silent host is cordoned with its offline record, and only expiry respawns

`schema/2900_runner_runners_cordon.sql` adds `cordon_reason TEXT` and `cordoned_at TIMESTAMPTZ(3)` to `runner.runners`, both null or both set. 2900 is a placeholder in 2900-2940, renumbered from `origin/main` at CHORE(open), because #803 takes `schema/2190-2690`. The readers are the capacity lock, the candidate scan, the lift and the runner views. Reasons are application constants, `missed_heartbeats` today (STS). When `record_offline` inserts a new offline event, the same transaction cordons an `active`, uncordoned host and writes `runner_cordoned` with its reason; a refused write rolls the offline event back with it. `status` stays operator intent, because authentication admits only `active` hosts (`afd_state/src/credentials/rows.rs:69-74`), and a cordoned status would keep a returning host from ever beating. A cordoned host's `can_claim` is false, and the candidate scan returns it nothing; its renewals and reports are unchanged.

The sweep's slot backdating goes: `expire_active_lease_slots.sql`, both callers, and the due query's arm that fed the operator path. **Implementation default:** delete it because a dead host's slots lapse `LEASE_TTL` (30 s) after its last renewal, 60 s before `RUNNER_OFFLINE_AFTER` (`afd_core/src/timing.rs:25,53`), so the backdate's own `lease_expires_at > now` predicate matches nothing there. It matches only a host still renewing, where it starts a second copy of a running turn. Operator cordon and revoke need it neither: authentication refuses those hosts' renewals, so their slots lapse within `LEASE_TTL`.

A beat from a cordoned host lifts an automatic cordon once the host's newest online-or-offline event is a `runner_online` at least `UNCORDON_AFTER` old, writing `runner_uncordoned`. `UNCORDON_AFTER` is three `HEARTBEAT_INTERVAL`s (30 s), asserted at least two. **Implementation default:** lift automatically because self-hosted runners that sleep or reboot past 90 s would otherwise wait for a person (Indy, 10:19 AM). Each new silence re-cordons the host, so a flapping host costs one restart per episode. Beats never lift an operator's cordon, because it lives in `status`. The lift runs where liveness is recorded: the heartbeat on `main`, M243's stream session after it.

- **Dimension 2.1** — a silent host is cordoned with its offline record → Test `test_a_silent_host_is_cordoned_with_its_offline_record` (integration, `afd_runner/tests/integration_sweeps/`): `last_seen_at` 91 s ago → one `runner_offline`, one `runner_cordoned` with reason `missed_heartbeats`, both columns set; a second pass writes nothing.
- **Dimension 2.2** — a refused cordon leaves no offline record → Test `test_a_refused_cordon_leaves_no_offline_record` (integration): a test trigger refuses the runner update → no offline event and no cordon; the next pass after the trigger drops records both.
- **Dimension 2.3** — the sweep leaves a renewing host's lease alone → Test `test_the_sweep_leaves_a_renewing_hosts_lease_alone` (integration): a host silent 91 s whose lease renewed 5 s ago → the slot's `lease_expires_at` is unchanged, the next renewal extends, and a second runner gets no candidate until the lease lapses.
- **Dimension 2.4** — a cordoned host claims nothing → Test `test_a_cordoned_host_claims_nothing` (integration, `integration_runner_beat/cordon.rs`): a cordoned host beats and polls → no work, with reason `cordoned`; the waiting fleet's fence is unchanged; an issue under its lock reads `can_claim` false.
- **Dimension 2.5** — the cordon lifts only after a quiet return → Test `test_an_automatic_cordon_lifts_after_three_beats` (integration): return 31 s old → the beat lifts and writes `runner_uncordoned`; return 29 s old → still cordoned; return 31 s old under a newer offline record → still cordoned.
- **Dimension 2.6** — an operator's cordon outlasts an automatic lift → Test `test_an_operator_cordon_outlasts_an_automatic_lift` (integration): an auto-cordoned host an operator then cordons → its beat answers `UZ-RUN-009`, and `status` stays `cordoned`.

### §3 — One restart frame, and the superseded run goes quiet

`TailFrame::RunRestarted` is published once per issued reclaim lease: after the issue commits, before the lease answer leaves the daemon, so it precedes the new run's first chunk. A reclaim refused at issue publishes nothing. The activity target read now refuses a lease whose fence is below its slot's, answering `UZ-RUN-005`. This reverses the choice at `afd_fleet/src/lease/activity/mod.rs:126-131`, because after a restart the old run's frames would interleave with the new run's sequence. M243's cached target honours the same rule.

On `run_restarted` the browser's registry disposes that event's decoder, clears its gap mark, drops the unconfirmed draft and reasoning, keeps finished tool calls, and marks the row restarted. `FleetReplyBody` shows "Restarted on another runner". The next `stream_start` at sequence 0 opens a fresh decoder. A second start with no restart frame stays a gap, so a lost frame falls back to the durable reader's repair. The CLI's steer follow prints one `restarted on another runner` line for its own event.

- **Dimension 3.1** — one frame per issued reclaim, under contention → Test `test_an_issued_reclaim_publishes_one_restart_frame` (integration, `afd_fleet/tests/integration_respawn.rs`): 100 claimers released together on one lapsed lease → one reclaim issued, one `run_restarted` on the tail; a reclaim refused at capacity → none; a first delivery → none.
- **Dimension 3.2** — a superseded lease can neither post nor settle → Test `test_a_superseded_lease_can_neither_post_nor_settle` (integration): after the reclaim issues, the old lease's activity post answers `UZ-RUN-005` with no tail frame, and its report leaves `settled_at` untouched.
- **Dimension 3.3** — the browser resets on a restart → Test `a run_restarted frame resets the reply instead of marking a gap` (unit, `fleet-stream-registry.reply.test.ts`): chunks 0-2 "abc", `run_restarted`, chunks 0-1 "xy" → reply "xy", `replyRecovering` false, the row marked restarted.
- **Dimension 3.4** — a start with no restart frame is still a gap → Test `a second stream start with no restart frame is still a gap` (unit, `reply-stream.test.ts`): chunks 0-1, then a start at 0 → `write` returns false and the gap is marked.
- **Dimension 3.5** — the CLI marks its own event's restart once → Test `a restart frame prints one restart line for its own event` (unit, `fleet-steer-tail.unit.test.ts`): chunk "a", `run_restarted` for this event, `run_restarted` for another, chunk "a" → three lines, one of them `restarted on another runner`.

### §4 — The reclaimed lease resumes the turn from its last checkpoint

Every part of this Section waits on #803: finalize against `origin/main` after #803 merges. The claim already reads the prior lease to carry its billing (`claim_admission_head.sql`, `prior_lease`), and `Reused` gains that lease's id. The reclaim's delivery reads the current turn's last checkpoint through #803's pointer, and the lease answer carries it. The runner resumes the turn from it, `durable_tasks_v1` being on by default. The run's context checkpoint is read from the predecessor's row as its starting generation. The checkpoint records the run's `folded_through_event_id`, M242_002's fold cursor. Folds at or below it are already in the restored turn and are not delivered again; later folds are delivered once. With no checkpoint the turn restarts clean, and M242_002's re-fold reads every linked fold once. An unreadable one also restarts clean and logs `respawn_checkpoint_unreadable` at warn with its error code. The reclaim is the same claim M246 filters, so the tenant's setting decides which runners may take it. The expiry billing sweep bills the dead lease to its deadline once, and the reclaim bills under the carried posture and model.

**Implementation default:** no predecessor column on `runner.leases`, because the event's lease order is already its fence order under `idx_leases_fleet_event_id_fencing_token`, and the claim already reads the prior lease (SQL-ONEFACT). `push_scheduler.md`'s proposed link is withdrawn in §6.

- **Dimension 4.1** — the reclaim resumes from the last checkpoint → Test `test_a_reclaimed_lease_resumes_from_the_last_checkpoint` (integration, `integration_respawn.rs`): lease L1 records the turn's checkpoint, then lapses → L2's answer names that checkpoint and L1 as its predecessor.
- **Dimension 4.2** — no checkpoint restarts clean → Test `test_a_reclaim_with_no_checkpoint_starts_the_turn_clean` (integration): L1 recorded none → L2's answer carries none and the reclaim kind.
- **Dimension 4.3** — an unreadable checkpoint restarts clean → Test `test_an_unreadable_checkpoint_restarts_the_turn_clean` (integration): L1's checkpoint fails validation → L2 carries none, and one `respawn_checkpoint_unreadable` warn names L1.
- **Dimension 4.4** — each folded message reaches the model once → Test `test_respawn_delivers_each_folded_message_once` (integration): m1 folded at the checkpoint's `folded_through_event_id`, m2 folded after it → L2 delivers m2 once and never m1, and both admissions settle once.
- **Dimension 4.5** — a reclaim waits for an owned runner under "only my runners" → Test `test_respawn_waits_for_an_owned_runner_under_only_mine` (integration): L1 on the tenant's runner lapses → a platform runner's poll gets no work; the tenant's second runner reclaims.
- **Dimension 4.6** — a respawned turn bills each slice once → Test `test_a_respawned_turn_bills_each_slice_once` (integration): two billing sweeps after L1 lapses → L1's runtime ends at its deadline, billed once; L2's lines carry L1's posture and model under their own `work_id`.

### §5 — Operators see the cordon

`RunnerItem` and `RunnerDetail` gain `cordon` (`reason`, `cordoned_at`, `x-stability: beta`), omitted when the host is not cordoned. `RunnerEventType` gains `runner_uncordoned`; the automatic `runner_cordoned` carries its reason in metadata. **Implementation default:** `runner_uncordoned` ships inside `/v1`, and the Pull Request description names its one consumer, the web runner activity feed, as HTTP-ENUM requires of such a value. The runner list and detail show "Cordoned automatically: no heartbeat since <time>. Lifts after 30 s of heartbeats."; the activity feed gains the lift headline. `agentsfleet_runner_cordon_changes_total{change}` counts cordons and lifts.

- **Dimension 5.1** — runner reads carry the cordon → Test `test_runner_reads_carry_an_automatic_cordon` (integration, `integration_runner_views.rs`): a cordoned host → `cordon.reason` `missed_heartbeats` with its instant; an uncordoned host → no `cordon` key; history lists the cordon and the lift.
- **Dimension 5.2** — the status line shows the reason and the lift rule → Test `shows an automatic cordon with its reason and lift rule` (unit, `RunnerStatusLine.test.tsx`): `cordon` present → the badge and its time; absent → no badge.

### §6 — The kill drill, and the pages

The drill runs two real runner processes, `--unsandboxed`, against the daemon's router served in process over the lane's Postgres and Dragonfly. A local model server streams one chunk and then holds the turn open. Each architecture page states the target as the design (SPEC-FRAME).

- **Dimension 6.1** — a runner killed mid-turn restarts elsewhere, once → Test `killed_runner_mid_turn_restarts_on_another_host` (e2e, `agentsfleet_runner/tests/respawn.rs`): `SIGKILL` runner A after its first chunk → runner B reclaims the same `fleet_event_id` under a higher fence, no earlier than A's lease deadline and within one placement tick of it (the 1 s poll on `main`); the admission settles once; one `run_restarted` reaches the tail; A's lease is billed to its deadline once.
- **Dimension 6.2** — the pages read clean → Test `orly gate verify` (integration): 0 `docs.language` findings on the six edited architecture pages.

## Interfaces

- **Tail frame:** `{"kind":"run_restarted","event_id":"0193a4c2-5d1e-7b3a-9f10-2c4d6e8f0a1b","restarted_at":"2026-11-06T10:43:12.000Z"}`. Best-effort like every tail frame; the stream's description lists it.
- **Runner reads:** `"cordon": {"reason": "missed_heartbeats", "cordoned_at": "2026-11-06T10:42:30.000Z"}` on `GET /v1/fleets/runners` items and `GET /v1/fleets/runners/{id}`, omitted when absent. `reason` is a string enum with one value today.
- **Runner events:** `runner_uncordoned` joins `RunnerEventType` and the web lifecycle set; `runner_cordoned` metadata gains `reason` on the automatic path.
- **Capacity:** `afd_runner::capacity::lock(transaction, runner, claimant)`, with `Claimant::Fleet(fleet_id)`, `Claimant::Authoring` or `Claimant::Existing`. `Capacity::can_claim` is false for a cordoned host.
- **Timing:** `afd_core::timing::UNCORDON_AFTER` = 3 × `HEARTBEAT_INTERVAL`, with a `const` assertion that it is at least two intervals.
- **Reclaim:** `Reused` carries the prior lease's id; the lease answer's checkpoint field is #803's.
- **Keeper:** holds never exceed `workers − busy − 1` while a worker is free; a retirement rings the beat.
- **Error codes:** none new. A superseded lease's frame answers `UZ-RUN-005`; an operator-cordoned host's beat answers `UZ-RUN-009`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Host dies mid-turn | failure | power loss, `SIGKILL` | the lease lapses at its deadline; another runner reclaims under a higher fence; one restart frame | `killed_runner_mid_turn_restarts_on_another_host` |
| Host renews but its beats are lost | failure | a broken heartbeat path | cordoned, never respawned; its turn finishes under its fence | `test_the_sweep_leaves_a_renewing_hosts_lease_alone` |
| Cordon write refused | failure | a datastore error mid-transaction | the offline record rolls back too; the next pass records both | `test_a_refused_cordon_leaves_no_offline_record` |
| Flapping host | failure | repeated silences past 90 s | re-cordoned each episode; each return waits `UNCORDON_AFTER` | `test_an_automatic_cordon_lifts_after_three_beats` |
| Cordoned host polls | security | a returning host before its lift | no work; no fence moves | `test_a_cordoned_host_claims_nothing` |
| Operator cordon on an auto-cordoned host | security | an operator acts during the quarantine | beats refused; status stays the operator's | `test_an_operator_cordon_outlasts_an_automatic_lift` |
| Holds fill a host | capacity | many frozen sandboxes | the keeper keeps one worker free; a full host refuses a third fleet | `test_a_lease_and_a_hold_refuse_a_third_fleet`, `test_a_hold_never_takes_the_last_free_worker` |
| Hold count lags the keeper | capacity | a hold retired since the last beat | a claim is refused for at most one beat, the safe direction; the retirement rings the beat at once | `test_a_hold_never_takes_the_last_free_worker` |
| Stale hold under a renewal | capacity | a hold row the keeper already dropped | renewal counts no holds, so running work is never ended | `test_a_renewal_counts_no_holds` |
| Reclaim race | race | many runners poll one lapsed lease | one fence wins; one frame | `test_an_issued_reclaim_publishes_one_restart_frame` |
| Superseded run still alive | race | a partitioned host keeps streaming | frames and report refused by the fence | `test_a_superseded_lease_can_neither_post_nor_settle` |
| Restart frame lost | dependency unavailable | pub/sub drops it | the browser reads a gap; the durable reader repairs the reply | `a second stream start with no restart frame is still a gap` |
| Checkpoint missing or unreadable | failure | a turn killed before its first checkpoint; a corrupt document | the turn restarts clean; a warn names the predecessor | `test_an_unreadable_checkpoint_restarts_the_turn_clean` |
| Folded input replayed or dropped | replay | a kill between fold and checkpoint | delivery resumes after the checkpoint's `folded_through_event_id` | `test_respawn_delivers_each_folded_message_once` |
| No owned runner under "only my runners" | capacity | the tenant's runners are all down | the work waits; no platform runner takes it | `test_respawn_waits_for_an_owned_runner_under_only_mine` |
| Double billing | replay | a sweep and a reclaim both meter | the dead lease bills to its deadline once | `test_a_respawned_turn_bills_each_slice_once` |

## Invariants

1. Lease expiry is the only respawn trigger — the claim takes a slot only when `lease_expires_at` is past the lock's clock (`claim_admission_head.sql`), and with `expire_active_lease_slots.sql` deleted only renewal and the holder's own release write that column. `test_the_sweep_leaves_a_renewing_hosts_lease_alone` is the counterexample.
2. A host never runs more sandboxes than `worker_count` at a claim — the census of leases, authoring attempts and other fleets' holds is read under the host-row lock, and the lease insert commits in that transaction. `test_a_lease_and_a_hold_refuse_a_third_fleet`.
3. An admission settles once — `meter_lease.sql` settles only under the admission's current `lease_fence` with `settled_at` null. `killed_runner_mid_turn_restarts_on_another_host`.
4. Beats never lift an operator's state — the lift statement writes only the cordon columns, guarded on `cordon_reason`, and authentication refuses every non-`active` host. `test_an_operator_cordon_outlasts_an_automatic_lift`.
5. One restart frame per issued reclaim — the publish follows the committed issue of a `Kind::Reclaim` lease and nothing else. `test_an_issued_reclaim_publishes_one_restart_frame`.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `agentsfleet_runner_cordon_changes_total` | operator | the sweep cordons a host; a beat lifts a cordon | `change` (`cordoned`, `lifted`); `fixed:2` | no runner id or host name | `test_a_silent_host_is_cordoned_with_its_offline_record`, `test_an_automatic_cordon_lifts_after_three_beats` |
| `runner_cordoned` / `runner_cordon_lifted` log events | operator | the same two moments, at `info` | `runner_id`, `reason`, `last_seen_at` | no token or label values | `test_a_silent_host_is_cordoned_with_its_offline_record` |
| `respawn_checkpoint_unreadable` log event | operator | a reclaim finds an invalid checkpoint, at `warn` | `error_code`, `lease_id`, `predecessor_lease_id` | no checkpoint content | `test_an_unreadable_checkpoint_restarts_the_turn_clean` |

`agentsfleet_fleet_runs_started_total{kind="reclaimed"}` already counts respawns and is unchanged. No product analytics event changes.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A runner killed mid-turn restarts elsewhere, settled once (§6) | `make test-integration-rustd` | exit 0; output holds `killed_runner_mid_turn_restarts_on_another_host ... ok` | P0 | |
| R2 | A silent host is cordoned and claims nothing (§2) | `make test-integration-rustd` | output holds `test_a_cordoned_host_claims_nothing ... ok` | P0 | |
| R3 | A lease and a hold refuse a third fleet (§1) | `make test-integration-rustd` | output holds `test_a_lease_and_a_hold_refuse_a_third_fleet ... ok` | P0 | |
| R4 | The sweep backdates no slot (§2) | `git grep -n -w -e EXPIRE_ACTIVE_LEASE_SLOTS -e expire_slots -e release_slots -- rustd` | no output | P0 | |
| R5 | One restart frame per reclaim under contention (§3) | `make test-integration-rustd` | output holds `test_an_issued_reclaim_publishes_one_restart_frame ... ok` | P0 | |
| R6 | The browser resets the reply on a restart (§3) | `make test-unit-all` | exit 0; output holds `resets the reply instead of marking a gap` | P0 | |
| R7 | The CLI marks a restart once (§3) | `make test-unit-all` | output holds `prints one restart line for its own event` | P1 | |
| R8 | A reclaim resumes from checkpoint with each message once (§4, after #803) | `make test-integration-rustd` | output holds `test_respawn_delivers_each_folded_message_once ... ok` | P0 | |
| R9 | OpenAPI names the restart frame (§3) | `grep -c run_restarted public/openapi.json` | a count of 1 or more | P1 | |
| R10 | The architecture pages read clean (§6) | `orly gate verify` | exit 0 | P1 | |
| R11 | The docs changelog branch exists (§6) | `git -C ~/Projects/docs rev-parse --verify chore/m247-respawn-changelog` | exit 0 | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `expire_active_lease_slots.sql`, `EXPIRE_ACTIVE_LEASE_SLOTS` | `git grep -n -w -e EXPIRE_ACTIVE_LEASE_SLOTS -e expire_active_lease_slots -- ':!docs/v2'` | no output |
| `expire_slots`, `release_slots`, `EXPIRE_PAST`, `CONTEXT_SLOTS` | `git grep -n -w -e expire_slots -e release_slots -e EXPIRE_PAST -e CONTEXT_SLOTS -- rustd` | no output |

Hits under `docs/v2/` are records (`docs/v2/reviews/schema-usage-audit-2026-09-18.md`) and stay.

Schema this milestone leaves with no runtime reader or writer is dropped in a new slot in this Pull Request, under Indy's blanket approval (Discovery). Each drop gets a row above, with a `git grep -n -w` over `rustd`, `ui` and `cli` proving nothing still reads or writes it.

## Out of Scope

- A host-down signal (`af.runner.<id>.down`): it would only speed respawn, and expiry stays the trigger; not planned here.
- Packing by memory and processor: slot counting only, as the brief's non-goals state.
- An operator action that returns a cordoned runner to `active`, and admitting cordoned or draining hosts at authentication (`runner_fleet.md:296-298`): a security change for its own spec.
- Deleting the frozen hold, which the brief's milestone list named: the M247 decision counts holds instead.
- Workspace disk snapshots and exactly-once tool effects: see the open blocker.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A user watching a chat sees the half-written reply clear under "Restarted on another runner" and finish once. The operator's runner list shows the dead host as cordoned, with when it went quiet.
2. **Preserved user behaviour** — Sending messages, an unbroken run's reply stream, gap recovery for real gaps, operator cordon, drain and revoke, and a fleet's follow-up resuming its held sandbox all work as before.
3. **Optimal-way check** — The most direct path is respawn at lease expiry plus a restart frame. The gap is up to `LEASE_TTL` (30 s) of waiting; a host-down signal could shorten it later (Out of Scope).
4. **Rebuild-vs-iterate** — Iterate: the claim, the fence, reclaim and carried billing exist; this adds a count, a cordon, a frame and a checkpoint read.
5. **What we build** — The hold count and the keeper reserve; the auto-cordon and its lift; the sweep without backdating; the restart frame, the refusal of superseded frames, and the browser and CLI resets; the checkpoint read on reclaim; `cordon` on runner reads; one metric.
6. **What we do NOT build** — A down signal (speeds only); bin packing (uniform sandboxes); an operator uncordon (the automatic one lifts itself); disk snapshots (their own milestone).
7. **Fit with existing features** — Compounds with M246's tenant setting, M243's Cancel and Hello, M242's folding and #803's checkpoints. It must not destabilize the fence: one settlement per admission.
8. **Surface order** — Both at once: the daemon's frame and the browser and CLI resets land together, because each decoder misreads a restart without the others.
9. **Dashboard restraint** — No host-health panel and no per-host capacity figure; only the cordon badge and the chat's restart marker, until memory accounting is real.
10. **Confused-user next step** — A chat user reads the restart marker. An operator reads the cordon's reason and lift rule on the runner page, and the `runners` docs page explains both.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** capacity first, because it needs no schema; the cordon with its slot second; the frame third, buildable before #803; the checkpoint read after #803; the operator surface on the cordon's columns; the drill last, over all of it.
- **Alternatives considered:**
  - The daemon picks the hold to evict at a claim: rejected, because it frees memory it does not hold, needs a second message, and can pick a different victim than the keeper.
  - Auto-cordon as `status = cordoned`: rejected, because authentication admits only `active` hosts, so the host could never beat its way back.
  - An operator-only uncordon: rejected, because a self-hosted runner that sleeps past 90 s would stay cordoned until a person acts.
  - Keeping the backdate for operator cordon and revoke: rejected, because authentication already refuses those hosts' renewals.
  - A predecessor column on `runner.leases`: rejected, because the fence order under one event already is the chain.
  - A durable restart mark on the event row: rejected, because a lost frame falls back to the gap path, which already repairs the reply.
- **Patch-vs-refactor verdict:** a **patch** because the claim, fence and reclaim paths stay; it adds a count, a cordon, a frame and a read.

## Discovery (consult log)

- **Consults** — Source review on `main` `14539a73e`, Nov 06, 2026, read in the M241 worktree, whose commits touch only the bench, two poll counters and docs:
  - The census counts active leases and authoring attempts only (`afd_runner/src/capacity/sql/census.sql`). Its callers are the fleet issue, the authoring claim, both renewals and the report commit.
  - The keeper caps holds at `workers − 1` (`keeper.rs:151`); at saturation it retires other fleets' holds and wakes the beat (`keeper.rs:175-185`, `afr_supervisor/src/heartbeat/mod.rs:213,226`).
  - Holds live in `runner.fleet_slots.hold_expires_at` (`schema/1820`), reconciled each beat (`afd_runner/src/heartbeat/holds.rs:47-83`).
  - The offline record and the slot backdate share one transaction (`liveness/mod.rs:208-229`); non-`active` hosts are backdated every pass (`:189-191`). A renewal needs an unexpired slot (`meter_lease.sql:34`).
  - Authentication admits only `active` runners (`afd_state/src/credentials/rows.rs:69-74`), and no operator action returns a runner to `active` (`afd_runner/src/admin.rs`, the transition table).
  - A reclaim carries billing and starts clean (`envelope.rs:63-66`). The activity target resolves superseded leases on purpose (`activity/mod.rs:126-131`). Usage lines are unique by `(work_type, work_id, seq)` (`schema/2040`).
  - The brief's milestone list said M247 deletes the frozen hold; the M247 decision counts holds instead. `push_scheduler.md` proposed a predecessor link; §4 declines it.
- **Owner decisions** —
  > Indy (Nov 06, 2026: 08:50 AM): "And we are not in production yet, so i want to rethink the approach on admissions, lease, and so on as i am focussed on scale. If something fails in one host bare metal, it should automatically cordon and respawn in another runner" — context: §2 and §4.
  > Indy (Nov 06, 2026: 10:19 AM): "This will be on by default durable_tasks_v1. ... my plan also was to have folks spin a runner and point to the agentsfleetd (in cloud) and keep using it." — context: §4 resumes under durable tasks; §2's automatic lift serves runners that sleep.
  > Indy (Nov 06, 2026: 10:43 AM): "1. Yes placement rules are via the tags and labels 2. Respawn across ownership - this is a tenant setting ... Based on the scope i would like to build this ... 4. Yes, but donot overengineer now, we can improve this." — context: Dimension 4.5; slot counting over bin packing.
  > Indy (Nov 06, 2026: 11:57 AM): "Yes i am ok with postgres store and accepting messages, fences runs" — context: the fence decides every restart.
  > Indy (Nov 06, 2026: 02:13 PM): "And i prefer we create all the spects and close M241 and push a PR?" — context: this spec lands with M241.
- **Assignment decisions** (relayed by the orchestrating session, Nov 06, 2026): holds count except the claimant's own, and authoring leases count; a silent host is cordoned in its offline transaction; expiry is the only respawn trigger; respawn resumes the current turn's last checkpoint; `run_restarted` resets the browser and CLI; no input is dropped or replayed, and usage lines stay unique. The agent's choices are the Implementation defaults in §1-§5.
  > Indy (Nov 06, 2026: 04:18 PM): "yes" — context: blanket approval, asked as "Should each milestone drop the tables and columns it leaves with no reader or writer?": a milestone drops, in a new slot in its own Pull Request, any table, column or index it leaves with no runtime reader or writer (`.orly/standards/domains/sql.md` destructive-change approval).
- **Model reviews** — none yet; the Elephant-Goldfish Model (EGM) review runs before this spec leaves `pending/`.
- **Open blockers** —
  1. #803: §4, R8 and the schema slot numbers are finalized against `origin/main` after #803 merges: its checkpoint pointer, its load, the runner's resume, and where the checkpoint keeps the fold cursor.
  2. For Indy: a respawned turn starts on a fresh sandbox disk, because workspace snapshots are not built (`runner_execution.md`). A tool effect between the last checkpoint and the kill runs again. My call: accept both for M247 and give snapshots their own milestone. The alternative is holding respawn until snapshots exist.
  3. Sequencing: CHORE(open) re-locates the liveness write (M243's stream replaces the heartbeat), the placement read (M243's offers), the renewal callers (M245's batch) and the tenant setting's names (M246).
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
