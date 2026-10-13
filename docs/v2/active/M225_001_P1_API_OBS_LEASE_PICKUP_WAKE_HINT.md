<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M225_001: A steer is leased within 250 ms at p50, and every settled lease logs where its time went

**Prototype:** v2.0.0
**Milestone:** M225
**Workstream:** 001
**Date:** Oct 12, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — every steer waits about 2 s, up to about 4 s, before any runner looks at it
**Categories:** API, OBS
**Batch:** B1 — one Pull Request; the wake hint and the segment timings ship together so the second measures the first
**Branch:** `feat/m225-lease-pickup-wake-hint` (worktree `~/Projects/agentsfleet-m225-lease-pickup`)
**Baseline revision:** `4a53b948c727cfdc860373bb3c77b08f80bb418d`
**Test Baseline:** unit=10301 integration=947
**Baseline evidence:** playbooks/operations/acceptance/baselines/M225_001-20261013.md
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 12, 2026) from dev measurements during M213_001's close-out (runner journal on `zombie-dev-worker-ant`, `flyctl logs` on the dev API) and `rustd/crates/afd_fleet/src/lease/assign.rs`
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Datastore topology (the readiness index); `docs/architecture/lease_flow.md`; `docs/architecture/scaling.md` (idle poll cost)

---

## Overview

**Goal (testable):** `test_marked_fleet_is_leased_on_the_next_poll` — a fleet marked ready is offered on the very next lease poll whichever partition the cursor names, and an idle poll's scheduler costs two Dragonfly commands (the directory read and one rotation peek) and no Postgres statement beyond the request's own authentication read.
**Problem:** A fleet owner sends a steer and nothing happens for 2–4 s before the model even starts. Measured on dev, Oct 12, 2026: pickup took 1.9 s, 2.2 s, 2.95 s and 3.8 s across four steers; in the 2.95 s case the steer was admitted at 03:34:23.232 and leased at 03:34:26.178, after 11 polls answered `no leasable work`. The reply itself took `wall_ms` 1,285 for a one-line chat, and nothing says how much of that was sandbox, model or report.
**Solution summary:** Each poll reads one of `READY_PARTITIONS` (16) readiness hashes in rotation (`afd_fleet/src/lease/assign.rs`), and four workers on the 1 s `NO_WORK_RETRY_AFTER_MS` turn the rotation once in about 4 s. Admission now also writes the fleet's partition number into one small directory hash under a fresh token; a poll reads that directory first, peeks the oldest dirty partition ahead of the rotation peek it already makes, and deletes the entry under its token only when the peek found the partition empty. Every directory command runs under a short deadline, so a sick slot costs a poll that deadline and not Dragonfly's 5 s request timeout. The runner's four workers offset their first poll so they stop polling in lockstep, and the first activity frame of a lease posts without waiting for the batch timer. The runner's `lease_completed` and `lease_failed` lines gain per-segment durations, so the time a reply takes is readable without a collector.

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(lease): wake the poll on fresh work and log each lease's segments
- **Intent (one sentence):** A steer starts running within a quarter second, and an operator can read where the rest of the reply time went.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_dragonfly/src/ready.rs` — the readiness index: the hint-never-record rule, minted tokens and compare-and-clear; the directory follows the same rules, and `key_of` shows how a key is prefixed and namespaced.
2. `rustd/crates/afd_fleet/src/lease/assign.rs` — the ready-first poll and its cost tally (`PollCost`), which the new read must keep honest.
3. `docs/architecture/runner_fleet.md` §Datastore topology — the sixteen hashes, why they are partitioned on a cluster, and the reclaim sweeper that repairs a lost mark.
4. `rustd/crates/afr_supervisor/src/lease_loop.rs` — where `lease_completed` and `lease_failed` are logged, and the order of bundle, hydrate, sandbox, run and settle the segment boundaries follow.
5. `rustd/crates/afd_fleet/src/lease/deliver.rs` — where `runner_lease_issued` is logged (its name is declared in `answer.rs`); the daemon holds the admission's `created_at` and the issue instant on one clock, so `queue_wait_ms` is computed here and nowhere else.
6. `rustd/crates/afr_supervisor/src/activity.rs` — the batcher: `FLUSH_EVERY`, the bounded poster queue and the held-frame accounting the first-frame flush must keep.
7. `rustd/crates/afr_supervisor/src/worker_pool.rs` — where workers spawn together and sleep the same hint, which is why they poll in lockstep today.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_dragonfly/src/ready.rs` | EDIT | `mark` also writes the partition's directory entry under a fresh token; a directory read and a compare-and-delete, both under the hint deadline |
| `rustd/crates/afd_dragonfly/src/ready/partition.rs` | EDIT | the directory key's name beside `READY_INDEX_KEY`, hash-tagged to one slot and routed through `physical_key` like the partitions |
| `rustd/crates/afd_dragonfly/src/ready/directory.rs` | NEW | the directory: `note_dirty` (called by `mark`), `dirty` oldest first, `spend` by token, every command under the hint deadline |
| `rustd/crates/afd_dragonfly/src/ready/directory/tests.rs` | NEW | oldest-first order, skipped fields, the deadline on a command that never answers, the warning a lost write leaves |
| `rustd/crates/afd_dragonfly/src/ready/partition/tests.rs` | EDIT | the directory key's shape |
| `rustd/crates/afd_dragonfly/src/lib.rs` | EDIT | re-exports `Dirty` |
| `rustd/crates/afd_fleet/src/lease/assign.rs` | EDIT | read the directory first, peek one of its oldest dirty partitions ahead of the rotation peek, keep the rotation peek on every poll; tally every read in `PollCost` |
| `rustd/crates/afd_core/src/timing.rs` | EDIT | `READY_HINT_DEADLINE_MS` beside the other lease constants; the worker poll offset derives from `NO_WORK_RETRY_AFTER_MS` |
| `rustd/crates/afd_fleet/src/lease/assign/hint.rs` | NEW | the directory's part in a poll: the two peeks, the spread over the oldest three entries, the interleave, and when an entry is spent (`assign.rs` is at its length cap) |
| `rustd/crates/afd_fleet/src/lease/assign/hint/tests.rs` | NEW | the spread, keep-or-spend and the interleave |
| `rustd/crates/afd_fleet/src/lease/assign/diagnostics.rs` | EDIT | the two hint warnings and their events |
| `rustd/crates/afd_fleet/src/lease/assign/offer.rs` | EDIT | held-fleet reads join the Dragonfly tally |
| `rustd/crates/afd_fleet/src/lease/assign/measured.rs` | EDIT | `dragonfly_commands` on the measurement |
| `rustd/crates/afd_fleet/src/lease/assign/tests.rs` | EDIT | the hint warnings render every Dragonfly failure |
| `rustd/crates/afd_fleet/tests/integration_lease_wake.rs`, `rustd/crates/afd_fleet/tests/integration_lease_wake/order.rs` | NEW | the live §1 proofs; `order.rs` holds the ones on which dirty partition a poll is sent to |
| `rustd/crates/afd_fleet/tests/integration_lease_wake/scene.rs` | NEW | the private-index scene those proofs poll |
| `rustd/crates/afd_fleet/tests/fleet_suite.rs` | EDIT | registers the suite |
| `rustd/crates/afd_observability/src/metrics/declared/fleet.rs` | EDIT | `agentsfleet_lease_poll_dragonfly_commands_total`, `agentsfleet_fleet_ready_hint_failures_total` |
| `rustd/crates/afd_observability/src/metrics/label/fleet.rs` | EDIT | the `ReadyHint` set: write, read, delete |
| `rustd/crates/afd_observability/src/producers/fleet.rs` | EDIT | `lease_polled` takes the command count; `ready_hint_failed` |
| `rustd/crates/afd_observability/tests/producers_drive.rs` | EDIT | drives both producers |
| `docs/metrics.census.tsv` | EDIT | the two new families |
| `rustd/crates/afd_bench/tests/lease_instrument.rs` | EDIT | the new `lease_polled` argument |
| `rustd/crates/afd_admission/tests/replay_marks.rs` | EDIT | a landed mark now sends two `HSET`s, the mark and its entry |
| `rustd/crates/afd_fleet/src/lease/deliver.rs` | EDIT | `queue_wait_ms` on `runner_lease_issued`, measured on the daemon clock (the event name is declared in `answer.rs`; the line is logged here) |
| `rustd/crates/afd_fleet/tests/integration_lease_gates/history.rs` | EDIT | the `queue_wait_ms` proof |
| `rustd/crates/afd_dragonfly/tests/integration_namespace/sweep.rs` | EDIT | the directory is one of the keys a namespaced index writes, inside its namespace |
| `rustd/crates/afr_supervisor/src/lease_loop.rs` | EDIT | the lease's wall clock and timeline; bundle and memory timed; the terminal line moved into `segments.rs` |
| `rustd/crates/afr_supervisor/src/lease_loop/segments.rs` | NEW | the timeline, the stopwatch that records on drop, and `lease_completed` / `lease_failed` with the segment fields |
| `rustd/crates/afr_supervisor/src/lease_loop/segments_tests.rs` | NEW | the segment-field proofs on both terminal lines (`lease_telemetry_tests.rs` is already past the length cap on main) |
| `rustd/crates/afr_supervisor/src/lease_loop/drive.rs` | EDIT | the run clock starts at the first model call; model and tool time read off it |
| `rustd/crates/afr_supervisor/src/lease_loop/workspace.rs` | EDIT | the sandbox segment, stopped after landing so every refusal exit records it |
| `rustd/crates/afr_supervisor/src/lease_loop/settle.rs` | EDIT | reads the lease's own start; the report body's `wall_ms` is unchanged |
| `rustd/crates/afr_supervisor/src/activity.rs` | EDIT | the first frame of a lease, and the first answer chunk, post without waiting for `FLUSH_EVERY` |
| `rustd/crates/afr_supervisor/src/activity/timing.rs` | NEW | the run clock: first answer chunk and tool time, read off the frames the sink already sees |
| `rustd/crates/afr_supervisor/src/activity/tests.rs` | EDIT | two batching proofs updated for the early flush |
| `rustd/crates/afr_supervisor/src/activity/early_tests.rs` | NEW | first-frame, ordering, drop-count and run-clock proofs |
| `rustd/crates/afr_supervisor/src/worker_pool.rs` | EDIT | workers offset their first poll by a share of the idle interval |
| `rustd/crates/afr_supervisor/src/worker_pool/tests.rs` | EDIT | the stagger proof on a paused clock |
| `docs/architecture/runner_fleet.md` | EDIT | the directory in the datastore table |
| `docs/architecture/lease_flow.md` | EDIT | the poll step reads the directory first and keeps the rotation peek; §3 step 4 records the egress wording |
| `docs/architecture/scaling.md` | EDIT | idle poll cost restated as two Dragonfly reads plus the one Postgres authentication read line 19 already names, at the three places that say one Dragonfly command: §Per-request volume (the `HRANDFIELD` row), §Measured ceilings (the `1.00 Dragonfly command` row) and anti-pattern 7 |
| `rustd/crates/afr_egress/src/error.rs` | EDIT | a fixed refusal (`RequestPolicyNotAllowed`, `SecretNotFound`) tells the model the rules are fixed for the run, so it stops retrying variants — the 167K-token run in Out of Scope |
| `rustd/crates/afr_egress/src/error/tests.rs` | EDIT | pins the two sentences |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/PlatformCatalogTable.tsx` | EDIT | the platform Fleet library table ends at its rows, like the Models table |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView.tsx` | EDIT | the section stops stretching the table to the viewport |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/library/components/WorkspaceLibraryList.tsx` | EDIT | the workspace Fleet library table ends at its rows |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/library/page.tsx` | EDIT | the section stops stretching the table to the viewport |
| `ui/packages/app/lib/api/model-library-types.ts` | EDIT | every provider the picker lists gets a display name; an unknown slug is title-cased instead of shown raw |
| `ui/packages/app/lib/api/model_library.test.ts` | EDIT | the label table and the fallback |
| `ui/packages/app/components/domain/ModelCells.tsx` | NEW | one provider badge and one model cell, shared by the workspace Models table and the admin Model library table |
| `ui/packages/app/components/domain/ModelCells.test.tsx` | NEW | the shared cells: vendor label, unknown slug, the platform-managed lock, the copy button |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCells.tsx` | EDIT | provider and model cells delegate to the shared ones |
| `ui/packages/app/app/(dashboard)/admin/models/components/CatalogueList.tsx` | EDIT | the raw-slug badge and the local model cell give way to the shared ones |
| `ui/packages/app/tests/e2e/acceptance/logs-detail.spec.ts` | EDIT | the acceptance journey follows the event dialog PR #748 shipped: the identifier left the heading for the footer's copy button |
| `ui/packages/app/tests/event-details-dialog.test.tsx` | EDIT | pins what the journey asserts at unit level, so the two fail together |
| `ui/packages/app/app/(dashboard)/admin/models/components/MakeDefaultDialog.tsx` | EDIT | the provider shows through the shared `ProviderBadge`, not as a raw uppercase slug |
| `ui/packages/app/tests/make-default-dialog.test.tsx` | NEW | the dialog's provider label |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView.test.tsx` | EDIT | pins the catalogue section's shrink-not-grow classes |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/library/components/WorkspaceLibraryList.test.tsx` | EDIT | pins the list's shrink-not-grow classes |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/library/page.test.tsx` | EDIT | pins the section's shrink-not-grow classes |
| `docs/architecture/datastore_scaling.md` | EDIT | the poll reads the directory beside its partition; why a one-slot hint is tolerable |
| `docs/architecture/data_flow.md` | EDIT | the lease handler's readiness step reads the directory and spends its entry |
| `rustd/crates/afd_fleet/src/lease/pull/claimed.rs` | EDIT | module doc: a poll peeks the rotation's partition and the directory's |
| `rustd/crates/afd_bench/src/instrument.rs` | EDIT | reads `agentsfleet_lease_poll_dragonfly_commands_total` back |
| `rustd/crates/afd_bench/src/instrument/tests.rs` | EDIT | the new counter field |
| `playbooks/operations/acceptance/baselines/M225_001-20261013.md` | NEW | the comparison-revision baseline report |

**Ride-along rows** — the `afr_egress` and `ui/` rows are scope (Indy, Oct 12, 2026 — carried from `fix/egress-refusal-tells-the-model-to-stop` and asked for at CHORE(open); outside the lease work, inside this Pull Request):**

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — UFS (the directory key, the hint deadline, the poll offset and every duration field are named constants), NDC, NLR, FLL.
- `.orly/dispatch/write_rust.md` — preserved error variants, deterministic concurrency tests for the mark-versus-clear race and the stalled poster.
- `docs/RUST_ERROR_STANDARD.md` — any new fallible signature in `afd_dragonfly`.
- `.orly/dispatch/name_architecture.md` — a new Dragonfly key is named against `docs/architecture/runner_fleet.md` before it is written.
- `.orly/docs/LOGGING_STANDARD.md` — the new `lease_completed` fields.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| LOGGING | yes — new fields on an existing event | field names from the logging standard; no lease content, only durations |
| UFS | yes — new key and field names | named constants beside the existing ones |
| Architecture consult | yes — new Dragonfly key | the doc edit lands in the same commit as the key |
| File & Function Length (≤350/≤50/≤70) | yes — `assign.rs`, `ready.rs`, `activity.rs` | split the directory helpers into `ready/directory.rs` if `ready.rs` nears the cap; `batch` in `activity.rs` is near 50 lines already, so the early flush is a helper, not more arms in the `select!` |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_dragonfly/src/ready.rs` `clear_if_unchanged` — the directory clears the same way: a token minted at the mark, compared at the delete, inside one key, so the script never crosses a slot. `docs/architecture/datastore_scaling.md` §What the cluster probe showed records `CROSSSLOT` for a two-key script, which is why the delete cannot look at the partition hash.
- **Reference:** `rustd/crates/afr_supervisor/src/activity.rs` `batch` — `try_send` into the bounded poster queue, the batch kept when every slot is held, `held` incremented before ownership passes; the early flush goes through the same door.

## Sections (implementation slices)

### §1 — The poll finds fresh work on its next read

**Implementation default:** one directory hash per deployment, partition number → token minted at the mark, hash-tagged to one slot and namespaced through `physical_key` like the partition keys. A poll reads the directory, peeks the dirty partition with the oldest token, then makes the rotation peek it makes today; the two candidate lists merge under `MAX_READY_CANDIDATES_PER_POLL`. A dirty peek that finds its partition empty deletes the entry only if the token is unchanged, with a script that touches the directory alone. Both directory commands run under `READY_HINT_DEADLINE_MS`; a timeout or error warns once with the operation and partition, counts, and falls through to the rotation. The rotation peek never skips a poll, so a partition whose fleets are all mid-run (a running fleet keeps its mark, and the candidate query skips live slots) cannot starve the other fifteen. Rejected: a server-held long poll, because it needs cross-machine wake-ups through Dragonfly and holds a request per worker (Indy, Oct 12, 2026: "New spec, wake hint (Recommended)").

- **Dimension 1.1** — DONE — A fleet marked in a partition holding at most `MAX_READY_CANDIDATES_PER_POLL` marks is offered on the next poll, whichever partition the cursor names. A partition above that is sampled, so the promise is "visited on the next poll", not "leased on the next poll" → Test `test_marked_fleet_is_leased_on_the_next_poll`
- **Dimension 1.2** — DONE — An idle poll with an empty directory costs exactly two Dragonfly commands and zero Postgres statements in the scheduler (`PollCost`, published as `agentsfleet_lease_poll_dragonfly_commands_total`). A poll with one dirty entry costs three Dragonfly commands plus the candidate query, and a fourth command when the entry is spent. The request's authentication read is the HTTP guard's, outside the tally, and stays in `scaling.md`'s budget → Test `test_idle_poll_cost_is_bounded`
- **Dimension 1.3** — DONE — A mark written between a poll's directory read and its delete survives: the mark rewrote the token, and the compare-and-delete sees a different one → Test `test_mark_racing_a_clear_survives`
- **Dimension 1.4** — DONE — A lost directory entry costs only latency: the rotation peek, made on every poll, visits the fleet's partition within one full turn of this daemon's cursor → Test `test_lost_dirty_bit_falls_back_to_rotation`
- **Dimension 1.5** — DONE — Two fleets marked in different partitions are both leased within one poll interval by two different workers: the directory holds both partitions, and a poll that takes one leaves the other's entry for the next poll → Test `test_two_marked_fleets_lease_within_one_interval`
- **Dimension 1.6** — DONE — A directory entry is deleted under its token once a poll tried its partition and could not lease from it: nothing leased at all, its only fleet leased, or its peek failed. A poll that leased from the rotation's partition keeps the entry, because the hinted fleets may never have been tried; one that leased from the dirty partition keeps it while that peek held other fleets. A partition whose only marked fleet is mid-run loses its entry after one dry poll, the next mark there restores it, and a fleet marked in another partition is reached by the hint or, with its directory write lost, by the rotation peek every poll makes → Test `test_busy_dirty_partition_does_not_starve_rotation`
- **Dimension 1.7** — DONE — With several dirty entries a poll peeks one of the three oldest tokens, taken in turn as the cursor moves: a fresh cursor serves two partitions dirtied in turn in that order, and consecutive polls do not all race for the oldest partition's fleets → Test `test_consecutive_polls_take_the_oldest_entries_in_turn`
- **Dimension 1.8** — DONE — A directory command, or the peek of the partition the directory named, that exceeds `READY_HINT_DEADLINE_MS` costs the poll that deadline and no more and counts once; a failed hinted peek spends its entry so the next poll is not sent back; the poll makes the rotation peek alone; an admission whose directory write timed out still succeeds → Test `test_hint_deadline_falls_through`

### §2 — Every terminal lease line logs its segments

- **Dimension 2.1** — DONE — `lease_completed` and `lease_failed` carry `bundle_ms`, `memory_ms`, `sandbox_ms`, `model_ms`, `tools_ms`, `report_ms` and `first_token_ms`. The six durations have boundaries that do not overlap: bundle fetch; memory hydrate; sandbox take-or-prepare and land; model calls; tool calls; hold-or-destroy, memory push, report spool and report post. `first_token_ms` is the first answer chunk measured from the first model call, inside `model_ms`, never from the sink's birth (today's sink starts before hydrate, so its first-chunk duration includes setup). The six sum within 5 ms of the `wall_ms` the terminal line itself carries, taken when it is logged, after the report post; `tools_ms` is present and zero for a run that called no tool; the report body's own `wall_ms`, captured before the post, is unchanged → Test `test_lease_completed_logs_its_segments`
- **Dimension 2.2** — DONE — A lease that fails before a segment starts logs that segment as absent, not zero, on `lease_failed` → Test `test_unstarted_segment_is_absent`
- **Dimension 2.3** — DONE — `runner_lease_issued` on the daemon carries `queue_wait_ms`, the admission's `created_at` to the issue instant, both read on the daemon's clock. The runner never computes it: the event's `created_at` is stamped on Fly's clock and the runner host keeps its own, so a runner-side subtraction would carry the clock skew between the two → Test `test_runner_lease_issued_logs_queue_wait`

### §3 — The runner neither waits a timer nor polls in lockstep

- **Dimension 3.1** — DONE — The first activity frame of a lease is handed to the poster at once, once per lease, through the same bounded queue with the same held accounting; later frames batch as today. If every poster slot is held the frame waits in the batch and never on the emitting path → Test `test_first_frame_posts_without_waiting`
- **Dimension 3.2** — DONE — The first answer chunk also flushes, carrying any buffered earlier frames ahead of it, so order holds and the first visible token does not wait for `FLUSH_EVERY` → Test `test_first_answer_chunk_flushes_in_order`
- **Dimension 3.3** — DONE — Drop and abandonment counts are exact with the early flush: a stalled poster plus a full queue drops exactly the batches it dropped before → Test `test_early_flush_keeps_drop_accounting`
- **Dimension 3.4** — DONE — Worker `n` of `N` makes its first idle poll after `n × NO_WORK_RETRY_AFTER_MS / N`, so four workers on a 1 s hint poll about 250 ms apart and the median wait for a fresh mark is about 125 ms rather than 500 ms → Test `test_workers_stagger_first_poll`

### §4 — Measured on dev

- **Dimension 4.1** — Ten steers on dev after deploy: pickup p50 ≤250 ms and max ≤1,000 ms, read from `queue_wait_ms` on the daemon's `runner_lease_issued` lines (`flyctl logs`), never from the runner journal → Test `test_dev_pickup_p50_under_250ms`
- **Dimension 4.2** — Two `github-pr-reviewer` fleets in two workspaces, one `pull_request` delivery each within the same second: both `runner_lease_issued` lines carry `queue_wait_ms` ≤1,000 and name different runner workers → Test `test_dev_two_fleets_pick_up_in_parallel`

## Interfaces

```
runner_lease_issued  lease_id=… fleet_id=… event_id=… queue_wait_ms=…      (daemon, one clock)
lease_completed      lease_id=… wall_ms=… bundle_ms=… memory_ms=… sandbox_ms=… model_ms=… tools_ms=… report_ms=… first_token_ms=…   (runner)
lease_failed         lease_id=… the same fields, absent where the segment never started                               (runner)
Dragonfly: fleet:ready:{p} (unchanged) + one directory hash, partition → token, named in runner_fleet.md
Wire: LeaseResponse unchanged; retry_after_ms unchanged (1,000); the activity POST unchanged
Constants: READY_HINT_DEADLINE_MS (afd_core timing.rs); worker offset = n × NO_WORK_RETRY_AFTER_MS / N
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Directory write fails or times out | Dragonfly error, or `READY_HINT_DEADLINE_MS` passes, on the admission path | warned with operation and partition, counted; the admission succeeds; the rotation finds the fleet within one turn |
| Directory read fails or times out | Dragonfly error, or the deadline passes, on the poll | counted; the poll makes the rotation peek alone and took at most the deadline longer |
| Mark races a delete | admission marks while a poll deletes | the token differs; the entry stays |
| Stale entry | the fleet was drained by another runner | the peek finds nothing; compare-and-delete removes the entry |
| A peek fails or times out | the hinted partition, or the rotation's, is on a sick node | a failed hinted peek counts under `kind="peek"` and spends its entry; either peek's fleets are still offered when the other fails; the poll fails only when both do |
| Busy partition | its marked fleets are mid-run, or not this runner's labels | the entry is deleted after one poll that could not lease from it; the next mark there restores it; a label-restricted fleet whose entry another runner deleted, and a steer admitted while its fleet ran, are found by the held-sandbox read or the rotation, today's speed |
| A partition holds more marks than one peek samples | thousands ready in one partition | the fleet is leased within a few visits; the hint shortens the wait to the visit, not to one sample |

## Invariants

1. The readiness index and the directory are hints; the streams are the record — enforced by the reclaim sweeper re-deriving readiness, already proven, and by Dimensions 1.4 and 1.6.
2. An idle poll's scheduler touches no Postgres — enforced by the `PollCost` tally asserted in Dimension 1.2. The request's authentication read belongs to the HTTP guard (`afd_http/src/auth/guard.rs`), is counted in `scaling.md`, and is why the idle back-off stays at 1,000 ms.
3. The rotation peek is made on every poll, dirty entry or not — enforced by Dimension 1.6.
4. No directory command, and no peek of the partition the directory named, waits longer than `READY_HINT_DEADLINE_MS` — enforced by Dimension 1.8 and `test_every_hint_command_gives_up_at_the_hint_deadline`.
5. The early flush changes when a frame is posted, never whether it is counted — enforced by Dimension 3.3.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `lease_completed`, `lease_failed` (new fields) | ops | a runner settles or fails a lease | the six segment durations, `first_token_ms`, lease identifier | durations only — no prompt, reply or tool content | `test_lease_completed_logs_its_segments` |
| ready hint failure counter | ops | a directory command fails or times out | operation, partition | no fleet content | `test_hint_deadline_falls_through` |
| `runner_lease_issued` (new field) | ops | the daemon issues a lease | `queue_wait_ms`, lease, fleet and event identifiers | a duration and identifiers only | `test_runner_lease_issued_logs_queue_wait` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_marked_fleet_is_leased_on_the_next_poll` | a fleet outside partition 0 marked, fresh cursor at 0 → the first poll leases it, at 4 Dragonfly commands, and its spent entry is gone |
| 1.2 | integration | `test_idle_poll_cost_is_bounded` | empty index → 2 Dragonfly commands, 0 Postgres round trips; a stale entry over an empty partition → 4 and 0, and the entry is spent |
| 1.3 | integration | `test_mark_racing_a_clear_survives` | delete with the stale token after a new mark → the entry remains with the new token |
| 1.4 | integration | `test_lost_dirty_bit_falls_back_to_rotation` | mark, then every directory entry spent → leased within 16 polls |
| 1.5 | integration | `test_two_marked_fleets_lease_within_one_interval` | two fleets in two partitions marked in turn → the directory lists them oldest first, and each fleet's runner leases it on its first poll |
| 1.6 | integration | `test_busy_dirty_partition_does_not_starve_rotation` | fleet A leased and running; A re-marked → one dry poll deletes A's entry; fleet B then marked in another partition → leased on the next poll |
| 1.7 | integration | `test_consecutive_polls_take_the_oldest_entries_in_turn` | three fleets marked in turn, each leasable only by its own runners → polls one to three lease the first, the third and the second, so the second poll passed the oldest entry left; the order beneath it is `test_dirty_partitions_come_back_oldest_first`, the turn-taking `test_polls_take_the_oldest_three_entries_in_turn` |
| 1.8 | integration | `test_hint_deadline_falls_through` | a string where the directory hash should be → the mark lands, the write and every read count as hint failures, and the rotation leases the fleet; the deadline itself is proved by `test_a_hint_that_never_answers_returns_at_its_deadline` in `afd_dragonfly` |
| 2.1 | unit | `test_lease_completed_logs_its_segments` | a fake daemon lease → seven fields present; the six durations sum within 5 ms of the wall time at the log line |
| 2.2 | unit | `test_unstarted_segment_is_absent` | a lease with no bundle whose memory hydrate is refused → `lease_failed` with `memory_ms` and `report_ms` present; `bundle_ms`, `sandbox_ms`, `model_ms`, `tools_ms` and `first_token_ms` absent |
| 2.3 | integration | `test_runner_lease_issued_logs_queue_wait` | an admission created at T, leased at T+350 ms on a paused test clock → `queue_wait_ms=350` |
| 3.1 | unit | `test_first_frame_posts_without_waiting` | one frame emitted, no tick → the poster receives it; a second frame → stays batched until the tick |
| 3.2 | unit | `test_first_answer_chunk_flushes_in_order` | a start frame, then a progress frame, a tool completion and the first answer chunk inside one period → two posts, `[start]` then `[progress, completion, chunk]`, in emission order |
| 3.3 | unit | `test_early_flush_keeps_drop_accounting` | poster stalled, four full batches → exactly the batches over the cap are dropped and counted; held returns to zero on drain |
| 3.4 | unit | `test_workers_stagger_first_poll` | four workers, 1,000 ms hint, paused clock → first polls at 0, 250, 500, 750 ms |
| 4.1 | manual | `test_dev_pickup_p50_under_250ms` | ten steers → p50 ≤250 ms, max ≤1,000 ms; `flyctl logs` lines pasted |
| 4.2 | manual | `test_dev_two_fleets_pick_up_in_parallel` | one PR event per workspace in the same second → two `runner_lease_issued` lines, different workers, both ≤1,000 ms |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Fresh work is leased on the next poll (§1) | `make test-integration-rustd` | exit 0, `test_marked_fleet_is_leased_on_the_next_poll` passed | P0 | |
| R2 | Idle poll scheduler cost bounded at two Dragonfly reads and no Postgres (§1) | `make test-integration-rustd` | `test_idle_poll_cost_is_bounded` passed | P0 | |
| R3 | A busy dirty partition cannot starve the rotation (§1) | `make test-integration-rustd` | `test_busy_dirty_partition_does_not_starve_rotation` passed | P0 | |
| R4 | Each terminal lease line logs its segments (§2) | `make test-unit-all` | `test_lease_completed_logs_its_segments` passed | P0 | |
| R5 | The first frame of a lease posts at once (§3) | `make test-unit-all` | `test_first_frame_posts_without_waiting` passed | P0 | |
| R6 | Dev pickup p50 ≤250 ms (§4) | manual — ten `queue_wait_ms` values in Session Notes | p50 ≤250, max ≤1,000 | P0 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.orly/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `.orly/dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `.orly/docs/TEMPLATE.md`.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- The model's own time: a one-line chat spent `wall_ms` 1,285 on dev; §2 measures how much is time to first token, and a faster model or provider is a catalogue decision, not this spec. A 500 ms reply end to end is reachable only if time to first token on the dev model (GLM 5.3) allows it; §3 measures `first_token_ms` before anyone promises that number.
- Runner OTLP export on dev (`telemetry_export_disabled`): the runner's host collector is "built later" per `docs/architecture/observability.md`. Until then the §2 segment fields are readable only in the runner journal; `queue_wait_ms` lives on the daemon and is readable in `flyctl logs` today.
- A refused egress write costing 167,429 tokens and 166 s as the model retried variants (dev event `1791777420543-1356`): a cost bug, not a latency one; recorded for Indy to place, in this spec or its own.
- A shorter idle back-off (250 ms): every poll authenticates with one Postgres read (`afd_http/src/auth/guard.rs:88` → `afd_state/src/credentials/mod.rs:70`, by `token_hash`, no memo), so four polls a second per worker quadruples that read across every runner. Measured through the real endpoint before it is tuned; the stagger in Dimension 3.4 gets the median down without it.
- Reshaping the directory (a bitmap, per-partition keys, a mark-count hash): one 16-field hash on one slot, read once per poll, is sized by polls per second plus marks per second, and is benchmarked before it is replaced. A bitmap drops the token; per-partition keys need 16 reads to discover; a count must survive duplicate marks.
- A wake that interrupts a sleeping worker (a blocking list pop or pub/sub feeding daemon-local waiters): rejected with the long poll, because it holds a request per worker and needs lost-wake reconciliation; the client's remaining sleep is inside the §4 target, not outside it.

## Product Clarity (authoring record)

1. **Successful user moment** — A fleet owner sends "how are you" and the reply starts in about the time the model takes, with no dead pause first.
2. **Preserved user behaviour** — Every steer, webhook and schedule is still leased once and settled once; the wire does not change.
3. **Optimal-way check** — A long poll would cut pickup to one round trip (about 60 ms); the wake hint gets to about 250 ms with one extra read and no held requests, which Indy chose.
4. **Rebuild-vs-iterate** — Iterate: the readiness index stays; one key joins it.
5. **What we build** — A directory hash with tokens, a poll that reads it ahead of the rotation it keeps, a deadline on the hint, a first frame that posts at once, staggered workers, seven segment fields on both terminal lines.
6. **What we do NOT build** — A long poll; runner OTLP on dev; model or provider changes.
7. **Fit with existing features** — Idle poll cost rises from one Dragonfly read to two and keeps the one Postgres authentication read it already pays; `docs/architecture/scaling.md` restates the budget in the same Pull Request.
8. **Surface order** — N/A — no user surface; operators read the journal.
9. **Dashboard restraint** — N/A — no dashboard change.
10. **Confused-user next step** — An operator greps the lease's `lease_completed` or `lease_failed` line and reads which segment was slow.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the wake hint, the runner's early flush and stagger, and the segment timings ship together, because §4's proof reads its numbers from §2's fields and §3 is what makes §1's gain visible on screen.
- **Alternatives considered:** a server-held long poll, or a blocking-list or pub/sub wake feeding daemon-local waiters (rejected by Indy for its moving parts); fewer partitions (rejected: it shortens the rotation but keeps the wait proportional to it); a shorter idle back-off (rejected: every poll authenticates against Postgres, so it multiplies that read across every runner); a dirty set with an empty-check-and-clear script (rejected: the partition hash and the directory sit on different slots, so the script is `CROSSSLOT`).
- **Patch-vs-refactor verdict:** this is a **patch** because the readiness index and the poll keep their shape and gain one read.

## Discovery (consult log)

- **Consults** — Measured on dev, Oct 12, 2026, during M213_001's close-out: four steers picked up in 1.9 s, 2.2 s, 2.95 s and 3.8 s; four workers poll about once a second each (`runner_lease_no_work` bursts in `flyctl logs`); the dev host reaches the API in about 60 ms, so the network is not the wait. `READY_PARTITIONS` is 16 (`afd_dragonfly/src/ready/partition.rs:31`) and `NO_WORK_RETRY_AFTER_MS` is 1,000 (`afd_core/src/timing.rs:75`).
  > Indy (2026-10-12): "How do we make this performant? and reduce 1.2s to may be 500ms? What is causign the 1.2s, can we add logging tracing to the code to see what happens?" — context: the request this spec answers.
  > Indy (2026-10-12): "New spec, wake hint (Recommended)" — context: the mechanism, over a long poll.
- **Metrics review** — `lease_completed` and `lease_failed` gain seven fields, `runner_lease_issued` gains one, and one failure counter joins the readiness metrics; no analytics or funnel playbook update required: operator log only.
- **Review (Oct 12, 2026, before CHORE(open))** — six points folded in. (1) `queue_wait_ms` moved from the runner to the daemon (Dimension 2.3): the admission's `created_at` is Fly's clock and the runner journal is OVH's, so only the daemon has both instants on one clock. (2) The idle poll budget is two Dragonfly reads since `13c02638d`; `docs/architecture/scaling.md` says one at three places, all named in Files Changed. (3) Two fleets in parallel gained a proof (Dimensions 1.5 and 4.2): the directory holds both partitions and two workers take one each. (4) The §2 segment fields ride the runner journal only, because runner OTLP is off on dev; open for Indy below. (5) The model's own time stays out of scope; `first_token_ms` shows it, and the 500 ms wish is measured before it is promised. (6) The 167K-token retry is a cost bug; open for Indy below.
- **Review (Oct 12, 2026, Codex and ChatGPT, before CHORE(open))** — both reviews returned "ship with these changes" and agreed on every finding; Aiwa verified each against source and folded them in. (1) The empty-check-and-clear script crossed slots (`partition.rs:144` hash-tags each partition; `datastore_scaling.md` §What the cluster probe showed records `CROSSSLOT`): the dirty set became a directory hash of partition → minted token, deleted by compare-and-delete inside the one key (Dimensions 1.1 to 1.3, Prior-Art). (2) A busy partition would have starved the rotation, because a running fleet keeps its mark and the candidate query skips live slots (`lease/sql/lease.rs:169`): the rotation peek now runs on every poll and dirty partitions are served oldest token first (Dimensions 1.6, 1.7, Invariant 3). (3) `HRANDFIELD` samples 64 of a partition (`ready.rs:247`), so "next poll" promises a visit, not a lease (Dimension 1.1, Failure Modes). (4) Every poll authenticates with a Postgres read (`guard.rs:88`, `credentials/mod.rs:70`), so the idle back-off stays at 1,000 ms, the cost claim names that read, and `scaling.md` line 19 stands (Dimension 1.2, Invariant 2, Out of Scope). (5) The directory is one slot every poll reads first, so a sick node would stall every poll for Dragonfly's 5 s timeout (`config.rs:17`), where a sick partition stalls one in sixteen: directory commands carry `READY_HINT_DEADLINE_MS` (Dimension 1.8). Aiwa first argued the existing mark had the same exposure and withdrew that on the blast-radius difference. (6) Workers spawn together and sleep the same hint (`worker_pool.rs:49`, `:147`), so the 250 ms median assumed a stagger nobody wrote: Dimension 3.4. (7) The 250 ms batch timer (`activity.rs` `FLUSH_EVERY`) would have hidden §1's gain behind a dead quarter second: Dimensions 3.1 to 3.3, through the existing poster queue. (8) The timing sum skipped memory hydrate (`lease_loop.rs:277`), `wall_ms` is captured before the report post (`settle.rs`), the sink's clock starts before setup, and `lease_failed` logged nothing: Dimension 2.1 and 2.2 rewritten with disjoint boundaries. (9) The 3.8 s baseline sentence quoted the 2.95 s timestamps; corrected. (10) "Delivers exactly once" overclaimed; now "leased once and settled once". Not taken: reshaping the directory before a benchmark, and a pub/sub wake (both Out of Scope).
- **CHORE(open) (Oct 12, 2026)** — worktree cut from `origin/main` after the M223 merge; the three commits of `fix/egress-refusal-tells-the-model-to-stop` (spec review, egress wording, review fold) cherry-picked in and that branch deleted locally and on origin. Ride-along scope added at Indy's ask:
  > Indy (2026-10-12): "prior to chore(open) in the fix/egress-refusal-tells-the-model-to-stop changes must be in your new worktree from main, since merged a PR in main. If you have cherrypicked then prune fix/egress-refusal-tells-the-model-to-stop" — context: the worktree and the branch prune.
  > Indy (2026-10-12): "along with this change cna you make both the `Fleet Library` table adhere to the table like [the Models table]" — context: both Fleet library tables stop filling the viewport.
  > Indy (2026-10-12): "Can this be capitalized or so? it doesnt look good." — context: the provider picker in Create model showed raw slugs (`fireworks`, `openrouter`, `atlas-cloud`) beside `Anthropic`.
- **Decisions during EXECUTE (Oct 12, 2026)** — Building §1 showed the reviewed rule still starved the hint: a leased fleet keeps its mark while it runs, so its partition stays dirty, and oldest-first kept choosing it while a new steer elsewhere waited for the rotation. Dimension 1.6 now deletes an entry after a poll that could not lease from it.
  > Indy (2026-10-12): "Delete after a dry poll (Recommended)" — context: asked whether a busy directory entry is deleted after one poll that could not lease from it, at the cost of a label-restricted fleet falling back to the rotation.
- **REVIEW (Oct 13, 2026, gstack `/review`)** — seven readers: testing, maintainability, security, performance, design, simplification, and an adversarial pass. Fixed: the hinted peek ran under the 5 s request timeout and kept a failed entry, so one sick node could stall every poll (now under the hint deadline and spent on failure, `peek` label, Dimension 1.8); the Fleet library tables clipped long lists because `min-h-0` went with `flex-1` (shrink kept, grow dropped); tool time summed overlapping child calls (now the union of open calls); a lease from the rotation's partition deleted a hint it never tried (kept now, Dimension 1.6); a failed rotation peek threw away a good hint; `queue_wait_ms` on a reclaimed lease counted the previous run (logged for fresh leases only); provider lookups resolved inherited keys (`Object.hasOwn`); vendor spellings for the runner registry's providers; the Make-default dialog's raw slug; the platform-default row's muted name; a missing provider no longer looks like a vendor; stale docs in three architecture pages, the census and two metric docs; `LeaseRun::lease` over the method cap; tests added for every coverage gap named. Skipped with reasons in the Pull Request: the extra round trip of the spend before delivery, per-poll allocations, a log line per failed poll.
- **Bench (Oct 13, 2026, `make bench-lease PROFILE=rig` against `bench/baselines/lease.rig.json`)** — drain lease p95 56.5 → 48.1 ms, drain report p95 21.5 → 16.9 ms, leases per second 171.6 → 176.9, idle Postgres statements per poll 0 → 0, idle Dragonfly calls per poll 1.0 → 2.0 (the directory read), wasted claim rate 0.278 → 0.333.
- **Decisions after review (Oct 13, 2026)** — Indy answered the five open review items "1A 2A 3A 4A", then for the fifth: "I select this - My call is A: keep the start-only stagger and let §4's ten dev steers measure pickup p95." (a) Every poll was sent to the same oldest dirty partition and the rig's wasted claim rate rose 20%: a poll now takes one of the three oldest entries in turn by its cursor (Dimension 1.7). (b) The directory slot has a deadline and no breaker: the chaos run that freezes its node measures what a poll pays, and that number decides. (c) A steer admitted mid-run loses its entry to the next dry poll and is found by the holder's held-first read the moment the run ends, or by the rotation: no re-note; §4's dev steers show whether the rotation path happens. (d) Oldest-first trusts each replica's clock: documented in `directory.rs` and `runner_fleet.md`, since skew reorders entries and loses none. (e) The worker stagger sets only the first poll: kept; §4 measures pickup p95.
- **Open for Indy** — (a) Does M225 also point dev's runner at a collector, or do the segment fields stay journal-only until the "runner collector built later" note in `docs/architecture/observability.md` lands? (b) Does the refused-write retry cap (make `request_policy_not_allowed` and `secret_not_found` terminal for a request shape, cap identical refusals per lease) fold into M225 or get its own spec?
- **Skill-chain outcomes** — pending.
- **Deferrals** — §4 (Dimensions 4.1, 4.2) to after the merge: it needs this branch on dev, and dev deploys only from `main` (`.github/workflows/deploy-dev.yml` `on: push: branches: main`), so §4 stays IN_PROGRESS through the Pull Request.

> Indy (2026-10-12): "Defer §4 to after merge (Recommended)" — context: the ten dev steers and the two-workspace check wait for the merge's dev deploy.
