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
**Batch:** B1 — one Pull Request; the doorbell and the segment timings ship together so the second measures the first
**Branch:** `feat/m225-lease-pickup-wake-hint` (worktree `~/Projects/agentsfleet-m225-lease-pickup`)
**Baseline revision:** `4a53b948c727cfdc860373bb3c77b08f80bb418d`
**Test Baseline:** unit=10301 integration=947
**Baseline evidence:** playbooks/operations/acceptance/baselines/M225_001-20261013.md
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 12, 2026) from dev measurements during M213_001's close-out (runner journal on `zombie-dev-worker-ant`, `flyctl logs` on the dev API) and `rustd/crates/afd_fleet/src/lease/assign.rs`
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Datastore topology (the readiness index); `docs/architecture/lease_flow.md`; `docs/architecture/scaling.md` (idle poll cost)

---

## Overview

**Goal (testable):** `test_marked_fleet_is_leased_on_the_next_poll` — a fleet marked ready is offered on the very next lease poll whichever partition the cursor names, and an idle poll's scheduler costs one Dragonfly command (the rotation peek) and no Postgres statement beyond the request's own authentication read.
**Problem:** A fleet owner sends a steer and nothing happens for 2–4 s before the model even starts. Measured on dev, Oct 12, 2026: pickup took 1.9 s, 2.2 s, 2.95 s and 3.8 s across four steers; in the 2.95 s case the steer was admitted at 03:34:23.232 and leased at 03:34:26.178, after 11 polls answered `no leasable work`. The reply itself took `wall_ms` 1,285 for a one-line chat, and nothing says how much of that was sandbox, model or report.
**Solution summary:** Each poll reads one of `READY_PARTITIONS` (16) readiness hashes in rotation (`afd_fleet/src/lease/assign.rs`), and four workers on the 1 s `NO_WORK_RETRY_AFTER_MS` turn the rotation once in about 4 s. Admission's mark now also rings its partition's doorbell, one sharded pub/sub channel per partition on that partition's own slot; each replica's subscription hub turns the ring into a bit in a process-local doorbell, and a poll peeks the first rung partition at or after its cursor beside the rotation peek it already makes, offers the rotation's fleets first, and leaves the bit clear unless fleets it never tried may still wait there. A lost ring costs the rotation's speed; a gap or a lag is counted and rings nothing, and the ring and the rung peek give up at 100 ms. The runner's four workers offset their first poll so they stop polling in lockstep, and the first activity frame of a lease posts without waiting for the batch timer. The runner's `lease_completed` and `lease_failed` lines gain per-segment durations, so the time a reply takes is readable without a collector.

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(lease): wake the poll on fresh work and log each lease's segments
- **Intent (one sentence):** A steer starts running within a quarter second, and an operator can read where the rest of the reply time went.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_dragonfly/src/ready.rs` — the readiness index: the hint-never-record rule, minted tokens and compare-and-clear; the doorbell (`ready/doorbell.rs`) rings after the mark, and `key_of` shows how a key is prefixed and namespaced.
2. `rustd/crates/afd_fleet/src/lease/assign.rs` — the ready-first poll and its cost tally (`PollCost`), which the new read must keep honest.
3. `docs/architecture/runner_fleet.md` §Datastore topology — the sixteen hashes, why they are partitioned on a cluster, and the reclaim sweeper that repairs a lost mark.
4. `rustd/crates/afr_supervisor/src/lease_loop.rs` — where `lease_completed` and `lease_failed` are logged, and the order of bundle, hydrate, sandbox, run and settle the segment boundaries follow.
5. `rustd/crates/afd_fleet/src/lease/deliver.rs` — where `runner_lease_issued` is logged (its name is declared in `answer.rs`); the daemon holds the admission's `created_at` and the issue instant on one clock, so `queue_wait_ms` is computed here and nowhere else.
6. `rustd/crates/afr_supervisor/src/activity.rs` — the batcher: `FLUSH_EVERY`, the bounded poster queue and the held-frame accounting the first-frame flush must keep.
7. `rustd/crates/afr_supervisor/src/worker_pool.rs` — where workers spawn together and sleep the same hint, which is why they poll in lockstep today.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_dragonfly/src/ready.rs`, `rustd/crates/afd_runner/src/sweep/reclaim.rs`, `rustd/crates/afd_runner/tests/integration_reclaim.rs`, `rustd/crates/afd_runner/tests/integration_reclaim_faults.rs`, `rustd/crates/afd_runner/src/sweep/reclaim/tests.rs` | EDIT | `mark` also rings its partition's doorbell channel with the mark's token, after the mark lands; every reclaim sweeper re-mark goes through `mark_quiet`, because its pass re-marks fleets mid-run and claims long runs' idle entries; the sweeper tests prove the quiet over a subscription a probe confirmed |
| `rustd/crates/afd_dragonfly/src/ready/partition.rs`, `rustd/crates/afd_dragonfly/src/ready/partition/tests.rs`, `rustd/crates/afd_dragonfly/src/lib.rs` | EDIT | each partition's wake channel, its key plus `:wake`, so it shares the partition's hash tag and slot; its shape pinned; `Doorbell` re-exported |
| `rustd/crates/afd_dragonfly/src/ready/doorbell.rs`, `rustd/crates/afd_dragonfly/src/ready/doorbell/tests.rs`, `rustd/crates/afd_dragonfly/src/streams.rs`, `rustd/crates/afd_dragonfly/src/streams/tail.rs`, `rustd/crates/afd_dragonfly/Cargo.toml` | NEW, EDIT (the last three) | the doorbell: one bit per partition, `ring`, `ring_all`, `take` from a cursor, `listen` on the hub, a gap or a lag counted and nothing rung; the ring and `peek_rung` under `READY_WAKE_DEADLINE_MS` (the tail's `SPUBLISH` name shared); a mark's ring and the report of one that failed; the bits, the race, a frame, a gap and a ring nobody answers proven |
| `rustd/crates/afd_fleet/src/lease/assign.rs`, `rustd/crates/afd_fleet/src/lease/assign/claim.rs`, `rustd/crates/afd_fleet/src/lease/restore.rs`, `rustd/crates/afd_fleet/src/lease/sql/lease.rs`, `rustd/crates/afd_fleet/src/lease/store.rs` | EDIT | the candidate query sorts the cursor's partition's ids first; take the first rung partition from the doorbell and peek it beside the rotation peek, kept on every poll; tally every read in `PollCost`; the claim path moves to `assign/claim.rs` and its stream reads, mark clear and dead-entry acknowledgement join the tally; the store holds the doorbell every clone shares |
| `rustd/crates/afd_fleet/src/lease/assign/wake.rs`, `rustd/crates/afd_fleet/src/lease/assign/wake/tests.rs` | NEW | the doorbell's part in a poll: the rung take, the two peeks sent together, the cursor-first offer, and when a partition is rung again (`assign.rs` is at its length cap); ring-again, the offer order and a refused rung peek proven |
| `rustd/crates/afd_fleet/src/lease/assign/diagnostics.rs`, `rustd/crates/afd_fleet/src/lease/assign/tests.rs` | EDIT | the rung-peek warning and its event; it renders every Dragonfly failure and names its partition |
| `rustd/crates/afd_fleet/src/lease/assign/offer.rs`, `rustd/crates/afd_fleet/src/lease/assign/measured.rs` | EDIT | held-fleet reads join the Dragonfly tally; a pass reports the offered fleets it never tried, which decides a ring again; `dragonfly_commands` on the measurement |
| `rustd/crates/afd_fleet/tests/integration_lease_wake.rs`, `rustd/crates/afd_fleet/tests/integration_lease_wake/order.rs`, `rustd/crates/afd_fleet/tests/integration_lease_wake/broken.rs`, `rustd/crates/afd_fleet/tests/integration_lease_wake/tally.rs`, `rustd/crates/afd_fleet/tests/integration_lease_plan.rs` | NEW | the live §1 proofs; `integration_lease_plan.rs` the scan's cursor-first sort key under a generic plan; `order.rs` holds the ones on which rung partition a poll takes; `broken.rs` the polls whose peeks or claim the datastore refuses; `tally.rs` the claim path's dropped-entry and restored-group commands |
| `rustd/crates/afd_fleet/tests/integration_lease_wake/scene.rs` | NEW | the private-index scene those proofs poll, with a hub ringing its doorbell from the index's own channels |
| `rustd/crates/afd_fleet/tests/fleet_suite.rs` | EDIT | registers the suites, and the fake Redis support `afd_dragonfly` and `afd_admission` already share |
| `rustd/crates/afd_observability/src/metrics/declared/fleet.rs` | EDIT | `agentsfleet_lease_poll_dragonfly_commands_total`, `agentsfleet_fleet_ready_wake_failures_total` |
| `rustd/crates/afd_observability/src/metrics/label/fleet.rs` | EDIT | the `ReadyWake` set: ring, peek |
| `rustd/crates/afd_observability/src/producers/fleet.rs` | EDIT | `lease_polled` takes the command count; `ready_wake_failed` |
| `rustd/crates/afd_observability/tests/producers_drive.rs`, `docs/metrics.census.tsv` | EDIT | drives both producers; the two new families |
| `VERSION`, `cli/package.json`, `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | the 0.60.0 version bump, held in step by `make check-version` |
| `rustd/crates/afd_bench/tests/lease_instrument.rs`, `rustd/crates/afd_bench/src/lane/cardinality/probe.rs`, `rustd/crates/afd_bench/src/lane/steer.rs` | EDIT | the new `lease_polled` argument; the cardinality probe binds the scan's new `$8`, so the plan it explains carries the ordering key |
| `rustd/crates/afd_admission/tests/replay_marks.rs` | EDIT | a landed mark rings once, a refused one never |
| `rustd/crates/afd_fleet/src/lease/deliver.rs`, `rustd/crates/afd_fleet/src/lease/deliver/queue_wait.rs`, `rustd/crates/afd_fleet/src/lease/deliver/queue_wait_tests.rs`, `rustd/crates/afd_fleet/src/lease/pull.rs`, `rustd/crates/afd_fleet/src/lease/pull/held.rs`, `rustd/crates/afd_fleet/src/lease/envelope.rs`, `rustd/crates/afd_fleet/src/lease/test_dead.rs`, `rustd/crates/afd_fleet/src/lease/history_tests.rs` | EDIT, NEW (`queue_wait.rs` and its tests) | `queue_wait_ms` on `runner_lease_issued`, measured on the daemon clock (the event name is declared in `answer.rs`; the line is logged here); read at the lease's write, the poll's own time counted from its start on the monotonic clock; `Acquired.redelivered` marks an owed entry, which logs none; the two history tests that fail a read take turns, because they share the process's failure counter |
| `rustd/crates/afd_fleet/tests/integration_lease_gates/history.rs`, `rustd/crates/afd_fleet/tests/integration_lease_fairness.rs`, `rustd/crates/afd_fleet/tests/support/fleet_lease_reads.rs` | EDIT | the `queue_wait_ms` proof; the cursor-first proof beside the rotation-fairness one, and the sticky affinity row it seeds |
| `rustd/crates/afd_dragonfly/tests/integration_namespace/sweep.rs` | EDIT | the stream round trip moved to a helper, so the namespaced surface's key list reads at a glance |
| `rustd/crates/agentsfleetd/src/plane.rs`, `rustd/crates/agentsfleetd/src/plane/tests.rs`, `rustd/crates/agentsfleetd/src/serve/optional.rs` | EDIT | one doorbell per process, listened for on the live tail's hub and handed to the lease store, its listening proven; the hub-down warning names the pickup fallback |
| `rustd/crates/afd_dragonfly/src/hub/pump.rs`, `rustd/crates/afd_dragonfly/src/hub.rs`, `rustd/crates/afd_dragonfly/tests/hub_socket_faults.rs`, `rustd/crates/afd_core/src/timing.rs` | EDIT | the wait after a redial a dead primary refuses climbs across laps instead of restarting, proven against a server that refuses resubscribes; `CHANNEL_CAPACITY` for the lag test; `READY_WAKE_DEADLINE_MS` |
| `rustd/crates/afd_runner/tests/support.rs` | EDIT | the sweeper tests' ring listener: a confirmed subscription, and a ring matched by its token |
| `rustd/crates/afr_supervisor/src/lease_loop.rs` | EDIT | the lease's wall clock and timeline; bundle and memory timed; the terminal line moved into `segments.rs` |
| `rustd/crates/afr_supervisor/src/lease_loop/segments.rs` | NEW | the timeline, the stopwatch that records on drop, and `lease_completed` / `lease_failed` with the segment fields |
| `rustd/crates/afr_supervisor/src/lease_loop/segments_tests.rs` | NEW | the segment-field proofs on both terminal lines (`lease_telemetry_tests.rs` is already past the length cap on main) |
| `rustd/crates/afr_supervisor/src/lease_loop/drive.rs`, `rustd/crates/afd_wire/src/report.rs`, `public/openapi.json` | EDIT | the run clock starts at the first model call; model and tool time read off it; the report's `time_to_first_token_ms` reads the same clock, and its published description says setup is not counted |
| `rustd/crates/afr_supervisor/src/lease_loop/workspace.rs` | EDIT | the sandbox segment, stopped after landing so every refusal exit records it |
| `rustd/crates/afr_supervisor/src/lease_loop/settle.rs` | EDIT | reads the lease's own start; the report body's `wall_ms` is unchanged |
| `rustd/crates/afr_supervisor/src/activity.rs` | EDIT | the first frame of a lease, and the first answer chunk, post without waiting for `FLUSH_EVERY` |
| `rustd/crates/afr_supervisor/src/activity/timing.rs` | NEW | the run clock: first answer chunk and tool time, read off the frames the sink already sees |
| `rustd/crates/afr_supervisor/src/activity/tests.rs`, `rustd/crates/afr_supervisor/src/activity/early_tests.rs` | EDIT, NEW (`early_tests.rs`) | two batching proofs updated for the early flush; first-frame, ordering, drop-count and run-clock proofs |
| `rustd/crates/afr_supervisor/src/worker_pool.rs`, `rustd/crates/afr_supervisor/src/worker_pool/tests.rs` | EDIT | workers offset their first poll by a share of the idle interval; the stagger proof on a paused clock |
| `docs/architecture/runner_fleet.md` | EDIT | the doorbell in the datastore table |
| `docs/architecture/lease_flow.md` | EDIT | the mark rings and the poll peeks the rung partition beside the rotation's; §3 step 4 records the egress wording |
| `docs/architecture/scaling.md`, `docs/architecture/datastore_scaling.md` | EDIT | idle poll cost stays one Dragonfly read plus the authentication read line 19 names, and a rung poll peeks a second partition with it: §Per-request volume, §Measured ceilings and anti-pattern 7; the steer-ingress row counts the ring; in datastore_scaling.md, the doorbell is one channel per partition, on that partition's slot, so no slot carries every mark and every poll |
| `rustd/crates/afr_egress/src/error.rs`, `rustd/crates/afr_egress/src/admission.rs`, `rustd/crates/afr_egress/src/vault.rs`, `rustd/crates/afr_secrets/src/statics.rs` | EDIT | a fixed refusal (`RequestPolicyNotAllowed`, `SecretNotFound`) tells the model the rules are fixed for the run, so it stops retrying variants — the 167K-token run in Out of Scope; a wrong field of a secret the fleet has is `SecretFieldNotFound`, which names the fields it has (names only, from `StaticSecrets::field_names`) |
| `rustd/crates/afr_egress/src/error/tests.rs`, `rustd/crates/afr_egress/src/error/raise.rs`, `rustd/crates/afr_egress/src/admission/credential_tests.rs`, `rustd/crates/afr_egress/src/vault/tests.rs`, `rustd/crates/afr_secrets/src/statics/tests.rs` | EDIT | pins the sentences, the wrong-field one for a minted and a static credential, and that field names come back without values |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/PlatformCatalogTable.tsx` | EDIT | the platform Fleet library table ends at its rows, like the Models table |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView.tsx` | EDIT | the section stops stretching the table to the viewport |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/library/components/WorkspaceLibraryList.tsx` | EDIT | the workspace Fleet library table ends at its rows |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/library/page.tsx` | EDIT | the section stops stretching the table to the viewport |
| `ui/packages/app/lib/api/model-library-types.ts` | EDIT | every provider the picker lists gets a display name; an unknown slug is title-cased instead of shown raw; the label `Map` holds only the spellings title case would miss |
| `ui/packages/app/lib/api/model_library.test.ts`, `ui/packages/app/tests/admin-models-ui.test.ts`, `ui/packages/app/tests/admin-models-management.test.ts`, `ui/packages/app/tests/models-registry-table.test.tsx` | EDIT | the label table and the fallback; both Provider columns sort by the label they show; the Model library's shrink-not-grow classes |
| `ui/packages/app/components/domain/ModelCells.tsx` | NEW | one provider badge, neutral because a name is not a status, and one model cell, shared by the workspace Models table and the admin Model library table; only the library trims the name to a fixed width |
| `ui/packages/app/components/domain/ModelCells.test.tsx` | NEW | the shared cells: vendor label, unknown slug, the platform-managed lock, the copy button |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCells.tsx`, `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/registry-view.ts` | EDIT | provider and model cells delegate to the shared ones; the Provider sort reads the label |
| `ui/packages/app/app/(dashboard)/admin/models/components/CatalogueList.tsx`, `ui/packages/app/app/(dashboard)/admin/models/components/ModelsView.tsx` | EDIT | the raw-slug badge and the local model cell give way to the shared ones; the Provider sort reads the label; the library ends at its rows, as the Fleet library tables do |
| `ui/packages/app/tests/e2e/acceptance/logs-detail.spec.ts` | EDIT | the acceptance journey follows the event dialog PR #748 shipped: the identifier left the heading for the footer's copy button |
| `ui/packages/app/tests/event-details-dialog.test.tsx` | EDIT | pins what the journey asserts at unit level, so the two fail together |
| `ui/packages/app/app/(dashboard)/admin/models/components/MakeDefaultDialog.tsx` | EDIT | the provider shows through the shared `ProviderBadge`, not as a raw uppercase slug, inside a description rendered as a div, so the badge is valid markup |
| `ui/packages/app/tests/make-default-dialog.test.tsx` | NEW | the dialog's provider label |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView.test.tsx` | EDIT | pins the catalogue section's shrink-not-grow classes |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/library/components/WorkspaceLibraryList.test.tsx` | EDIT | pins the list's shrink-not-grow classes |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/library/page.test.tsx` | EDIT | pins the section's shrink-not-grow classes |
| `docs/architecture/data_flow.md`, `docs/architecture/concurrency.md` | EDIT | the lease handler's readiness step peeks the rung partition; the doorbell channels join the pub/sub table, and the section's name follows |
| `rustd/crates/afd_fleet/src/lease/pull/claimed.rs` | EDIT | module doc: a poll peeks the rotation's partition and the rung one; passes the poll's start instant on |
| `rustd/crates/afd_bench/src/instrument.rs`, `rustd/crates/afd_bench/src/instrument/tests.rs` | EDIT | reads `agentsfleet_lease_poll_dragonfly_commands_total` back; the new counter field |
| `playbooks/operations/acceptance/baselines/M225_001-20261013.md` | NEW | the comparison-revision baseline report |

**Ride-along rows** — the `afr_egress` and `ui/` rows are scope (Indy, Oct 12, 2026 — carried from `fix/egress-refusal-tells-the-model-to-stop` and asked for at CHORE(open); outside the lease work, inside this Pull Request):**

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — UFS (the wake channel's segment, the poll offset and every duration field are named constants), NDC, NLR, FLL.
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
| File & Function Length (≤350/≤50/≤70) | yes — `assign.rs`, `ready.rs`, `activity.rs` | the doorbell lives in `ready/doorbell.rs`, so `ready.rs` keeps its headroom; `batch` in `activity.rs` is near 50 lines already, so the early flush is a helper, not more arms in the `select!` |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_dragonfly/src/hub.rs` `subscribe` and `rustd/crates/afd_dragonfly/src/streams/tail.rs` `publish` — the doorbell rides the one subscription connection the live tail already shares, hears a lost subscription as the gap the tail is told about, and rings with the tail's sharded publish, so a ring routes to its partition's node.
- **Reference:** `rustd/crates/afr_supervisor/src/activity.rs` `batch` — `try_send` into the bounded poster queue, the batch kept when every slot is held, `held` incremented before ownership passes; the early flush goes through the same door.

## Sections (implementation slices)

### §1 — The poll finds fresh work on its next read

**Implementation default:** `ReadyIndex::mark` rings its partition after the mark lands: one `SPUBLISH` on `fleet:ready:{p}:wake`, which carries the partition's hash tag and slot, its payload the mark's token. Every replica's subscription hub subscribes the sixteen channels at boot (`Doorbell::listen`) and sets the partition's bit in a process-local 16-bit `Doorbell`; a gap or a lag is counted and rings nothing, since ringing all sixteen would hide real rings behind each poll's own partition for a turn. A poll takes the first rung partition at or after its cursor and peeks it beside the rotation's partition, both at once; the candidate scan takes the rotation's list first and the rung one's after it, and returns at most `MAX_READY_CANDIDATES_PER_POLL`. The poll rings the partition again, in process, only when the candidate scan offered a fleet from it that the poll never tried; a dropped poll gives its ring back. The ring and the rung peek give up at `READY_WAKE_DEADLINE_MS` (100 ms), and the hub's wait after a redial a dead primary refuses climbs instead of restarting. The rotation peek never skips a poll, so a partition whose fleets are all mid-run (a running fleet keeps its mark, and the candidate query skips live slots) cannot starve the other fifteen. The reclaim sweeper marks through `mark_quiet`, which rings nothing. Rejected: a server-held long poll, because it holds a request per worker (Indy, Oct 12, 2026: "New spec, wake hint (Recommended)"); the one-slot readiness directory this branch first built, because it put back the hot slot partitioning removed (Indy, Oct 13, 2026: "Swap in #750 (Recommended)").

- **Dimension 1.1** — DONE — A fleet marked in a partition holding at most `MAX_READY_CANDIDATES_PER_POLL` marks is offered on the next poll after its ring is heard, whichever partition the cursor names. A partition above that is sampled, so the promise is "visited on the next poll", not "leased on the next poll". The cursor's partition's fleets are offered and claimed ahead of the rung partition's, so one rotation of 16 polls still leases every placeable fleet, and a poll that leases from the cursor's partition rings the rung one again when the scan offered a fleet from it that the poll never tried; when the cursor's partition fills the scan's ceiling with fleets none of which leases, the rung ones are cut untried, the dry poll leaves the bit clear, and the rotation reaches them within a turn → Test `test_marked_fleet_is_leased_on_the_next_poll`
- **Dimension 1.2** — DONE — An idle poll with nothing rung costs exactly one Dragonfly command and zero Postgres statements in the scheduler (`PollCost`, published as `agentsfleet_lease_poll_dragonfly_commands_total`). A rung poll costs two Dragonfly commands, sent together, plus the candidate query; ringing again costs none, because the bit is the process's own; a claim adds its stream reads, and its mark clear when the fleet is drained. The request's authentication read is the HTTP guard's, outside the tally, and stays in `scaling.md`'s budget → Test `test_idle_poll_cost_is_bounded`
- **Dimension 1.3** — DONE — A ring that lands while a poll runs is kept: a take clears only the bit it returns, and `fetch_and` decides which of two racing takes won it, so a newer ring stays for the next poll and one ring never sends two polls to one peek → Test `test_a_ring_after_a_take_is_a_new_ring`
- **Dimension 1.4** — DONE — A lost ring costs only latency: the rotation peek, made on every poll, visits the fleet's partition within one full turn of this daemon's cursor; a gap or a lag on any channel is counted under `kind="gap"` and rings nothing, and with no hub nothing rings and the rotation alone serves → Test `test_a_lost_ring_falls_back_to_rotation`
- **Dimension 1.5** — DONE — Two fleets marked in different partitions are both leased within one poll interval by two different workers: each takes the first rung partition at or after its cursor, so the second finds the ring the first left → Test `test_two_marked_fleets_lease_within_one_interval`
- **Dimension 1.6** — DONE — A rung partition is rung again only when the candidate scan offered a fleet from it that the poll never tried, because it leased one ahead of it; a fleet the scan left out, running or not this runner's labels, never holds the bell. Every other exit leaves it clear, a faulted poll's included, so one broken fleet cannot keep drawing every poll, and a poll dropped before it settles gives its ring back. A partition whose only marked fleet is mid-run is left clear after a poll that peeked it, the next mark there rings it again, and a fleet marked in another partition is reached by its own ring or, with that ring lost, by the rotation peek every poll makes → Test `test_a_busy_rung_partition_does_not_starve_rotation`
- **Dimension 1.7** — DONE — With several partitions rung, a poll takes the first at or after its cursor, so consecutive polls take different ones and each ring is answered once → Test `test_consecutive_polls_take_different_rung_partitions`
- **Dimension 1.8** — DONE — Every replica hears every ring: a mark publishes on its partition's channel after the mark lands, carrying the mark's token, and each replica's hub subscribes all sixteen at boot; a ring that fails to publish is logged with its partition and counted under `kind="ring"`, and the admission succeeds; a rung peek that fails is counted under `kind="peek"` and leaves the bit clear, and the poll keeps the rotation's fleets; the ring and the rung peek give up at `READY_WAKE_DEADLINE_MS`; a primary that refuses resubscribes is redialled on a climbing wait → Test `test_a_frame_rings_its_partition`

### §2 — Every terminal lease line logs its segments

- **Dimension 2.1** — DONE — `lease_completed` and `lease_failed` carry `bundle_ms`, `memory_ms`, `sandbox_ms`, `model_ms`, `tools_ms`, `report_ms` and `first_token_ms`. The six durations have boundaries that do not overlap: bundle fetch; memory hydrate; sandbox take-or-prepare and land; model calls; tool calls; hold-or-destroy, memory push, report spool and report post. `first_token_ms` is the first answer chunk measured from the first model call, inside `model_ms`, never from the sink's birth (today's sink starts before hydrate, so its first-chunk duration includes setup); the report's `time_to_first_token_ms`, which product analytics records, reads the same clock, so it drops by the setup time at upgrade, as the field's description and the changelog say. The six sum within 5 ms of the `wall_ms` the terminal line itself carries, taken when it is logged, after the report post; `tools_ms` is present and zero for a run that called no tool; the report body's own `wall_ms`, captured before the post, is unchanged → Test `test_lease_completed_logs_its_segments`
- **Dimension 2.2** — DONE — A lease that fails before a segment starts logs that segment as absent, not zero, on `lease_failed` → Test `test_unstarted_segment_is_absent`
- **Dimension 2.3** — DONE — `runner_lease_issued` on the daemon carries `queue_wait_ms`, the admission's `created_at` to the issue instant, both read on the daemon's clock, for an entry read new only: a reclaim or an owed entry, a parked event a person answered say, logs none, and replica clock skew reads as zero, never below. The runner never computes it: the event's `created_at` is stamped on Fly's clock and the runner host keeps its own, so a runner-side subtraction would carry the clock skew between the two → Test `test_runner_lease_issued_logs_queue_wait`

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
Dragonfly: fleet:ready:{p} (unchanged) + sixteen sharded channels fleet:ready:{p}:wake, payload the mark's token, named in runner_fleet.md
Wire: LeaseResponse unchanged; retry_after_ms unchanged (1,000); the activity POST unchanged
Constants: worker offset = n × NO_WORK_RETRY_AFTER_MS / N; READY_WAKE_DEADLINE_MS = 100 (afd_core timing.rs); one doorbell bit per partition (READY_PARTITIONS = u16::BITS)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| A ring fails to publish | Dragonfly error, or `READY_WAKE_DEADLINE_MS` passes, on the admission path | warned with the partition, counted under `kind="ring"`; the admission succeeds within the deadline; the rotation finds the fleet within one turn |
| A ring is missed | the hub's subscription dropped and came back, or a reader lagged | counted under `kind="gap"`; nothing is rung; the rotation reaches what was missed |
| No hub, or a primary down | the subscription connection would not open at boot, or a primary refuses the hub's resubscribes | nothing rings, or that primary's partitions stop ringing; polls find fresh work through the rotation alone, and the hub's redial wait climbs to its ceiling instead of restarting each lap |
| Stale ring | the fleet was drained by another runner | the rung peek finds nothing; the take already cleared the bit |
| A peek fails | the rung partition, or the rotation's, is on a sick node | a failed or late rung peek counts under `kind="peek"` and leaves the bit clear; either peek's fleets are still offered when the other fails; the poll fails only when both do |
| Busy partition | its marked fleets are mid-run, or not this runner's labels | the bit is left clear after one poll that could not lease from it; the next mark there rings again; a label-restricted fleet whose ring another runner took, and a steer admitted while its fleet ran, are found by the held-sandbox read or the rotation, today's speed |
| A partition holds more marks than one peek samples | thousands ready in one partition | the fleet is leased within a few visits; the ring shortens the wait to the visit, not to one sample |

## Invariants

1. The readiness index and the doorbell are hints; the streams are the record — enforced by the reclaim sweeper re-deriving readiness, already proven, and by Dimensions 1.4 and 1.6.
2. An idle poll's scheduler touches no Postgres — enforced by the `PollCost` tally asserted in Dimension 1.2. The request's authentication read belongs to the HTTP guard (`afd_http/src/auth/guard.rs`), is counted in `scaling.md`, and is why the idle back-off stays at 1,000 ms.
3. The rotation peek is made on every poll, rung or not — enforced by Dimension 1.6.
4. No single slot carries every mark and every poll: each ring publishes on its own partition's slot, and a poll reads its rung partitions from process memory — enforced by Dimension 1.8 and `test_a_wake_channel_shares_its_partitions_slot`.
5. The early flush changes when a frame is posted, never whether it is counted — enforced by Dimension 3.3.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `lease_completed`, `lease_failed` (new fields) | ops | a runner settles or fails a lease | the six segment durations, `first_token_ms`, lease identifier | durations only — no prompt, reply or tool content | `test_lease_completed_logs_its_segments` |
| ready wake failure counter | ops | a ring fails, a rung peek fails or runs late, or a replica's subscription has a gap | kind (ring, peek, gap), partition | no fleet content | `test_a_rung_peek_that_fails_leaves_the_ring_clear` |
| `runner_lease_issued` (new field) | ops | the daemon issues a lease | `queue_wait_ms`, lease, fleet and event identifiers | a duration and identifiers only | `test_runner_lease_issued_logs_queue_wait` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_marked_fleet_is_leased_on_the_next_poll` | a fleet outside partition 0 marked and its ring heard, fresh cursor at 0 → the first poll leases it at 4 Dragonfly commands, and its partition is left clear; `test_the_cursor_ids_are_cast_once_per_scan`: under a forced generic plan the sort key reads the cursor's ids from a once-per-scan subquery, never `$8` itself; `test_the_cursor_partition_leases_before_the_rung_one`: partition 5 rung, its fleet sticky to the runner, the cursor at the other's partition → the first poll leases the cursor's fleet, partition 5 is rung again, a later poll leases the rung fleet; `test_skewed_workload_preserves_discovery_and_delivery_fairness` → one rotation leases every placeable fleet |
| 1.2 | integration | `test_idle_poll_cost_is_bounded` | nothing rung → 1 Dragonfly command, 0 Postgres round trips; a ring over a partition since emptied → 2 and 0, and the ring is taken |
| 1.3 | unit | `test_a_ring_after_a_take_is_a_new_ring` | partition 6 taken, then rung again → the next take returns 6; `test_racing_takes_win_a_ring_once`: eight threads take one rung bit, a thousand times → exactly one wins each time; `test_a_take_that_loses_a_race_takes_the_next_ring`: two rung bits, two racers → both taken |
| 1.4 | integration | `test_a_lost_ring_falls_back_to_rotation` | a quiet mark that rang nothing → leased within 16 polls; unit `test_a_gap_rings_nothing_and_is_counted`, `test_a_lag_rings_nothing_and_is_counted`: a confirmed-twice subscription, or an overflowed one → nothing rung, `kind="gap"` counted |
| 1.5 | integration | `test_two_marked_fleets_lease_within_one_interval` | two fleets in two partitions rung, each leasable only by its own runners → each fleet's runner leases it on its first poll |
| 1.6 | integration | `test_a_busy_rung_partition_does_not_starve_rotation` | fleet A leased and running; A re-marked → one dry poll leaves A's partition clear; fleet B then marked in another partition → leased on the next poll; `test_a_running_fleets_ring_is_not_rung_again`: A running and rung, a poll leasing from its rotation → A's partition clear; `test_a_rung_claim_that_faults_leaves_the_ring_clear`; unit `test_a_dropped_poll_gives_its_ring_back` |
| 1.7 | integration | `test_consecutive_polls_take_different_rung_partitions` | three fleets in three partitions rung, each leasable only by its own runners → three consecutive polls lease all three; unit `test_a_take_starts_at_the_cursor_and_wraps`: rings at 2, 9 and 14 taken from cursors 9, 10 and 15 → 9, 14, 2 |
| 1.8 | unit | `test_a_frame_rings_its_partition` | a frame on partition 11's channel → partition 11 rung and no other; `test_a_rung_peek_that_fails_leaves_the_ring_clear`: a string where the rung partition's hash should be → 2 Dragonfly commands, `assign_ready_wake_peek_failed` names the partition, `kind="peek"` counted, the bit clear; `test_an_unrung_mark_is_reported_with_its_partition` → `ready_wake_ring_failed` with the partition, `kind="ring"` counted; `test_the_daemon_doorbell_hears_its_hub` → the daemon's doorbell listens; `test_a_cluster_refusing_resubscribes_is_redialled_on_a_climbing_wait` → at most 12 redials a second, 32 with the old flat wait; sweeper `an_entry_a_dead_consumer_left_idle_is_claimed_and_its_fleet_marked` and `reclaim_restores_only_a_fleet_with_deliverable_work` → every sweeper re-mark is marked and rings nothing |
| 2.1 | unit | `test_lease_completed_logs_its_segments` | a fake daemon lease → seven fields present; the six durations sum within 5 ms of the wall time at the log line |
| 2.2 | unit | `test_unstarted_segment_is_absent` | a lease with no bundle whose memory hydrate is refused → `lease_failed` with `memory_ms` and `report_ms` present; `bundle_ms`, `sandbox_ms`, `model_ms`, `tools_ms` and `first_token_ms` absent |
| 2.3 | integration | `test_runner_lease_issued_logs_queue_wait` | an admission created at T, polled with `now` at T+350 ms → `queue_wait_ms` from 350 to 350 plus the poll's own time; `test_an_owed_entry_is_leased_as_redelivered`: an entry another consumer read and never acknowledged → leased as redelivered; unit `test_a_redelivered_entry_logs_no_wait`, `test_a_reclaimed_lease_logs_no_wait`, `test_clock_skew_never_logs_a_negative_wait` → none, none, 0 |
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
| R2 | Idle poll scheduler cost bounded at one Dragonfly read and no Postgres (§1) | `make test-integration-rustd` | `test_idle_poll_cost_is_bounded` passed | P0 | |
| R3 | A busy rung partition cannot starve the rotation (§1) | `make test-integration-rustd` | `test_a_busy_rung_partition_does_not_starve_rotation` passed | P0 | |
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
- Parking an idle poll in the daemon until its doorbell rings: it holds a request per worker, so the lease route would need its own in-flight cap beside the API's 256 (`afd_http/src/admission/mod.rs`); the doorbell meets §1 without it, and §4's numbers decide whether pickup is worth that cost.

## Product Clarity (authoring record)

1. **Successful user moment** — A fleet owner sends "how are you" and the reply starts in about the time the model takes, with no dead pause first.
2. **Preserved user behaviour** — Every steer, webhook and schedule is still leased once and settled once; the wire does not change.
3. **Optimal-way check** — A long poll would cut pickup to one round trip (about 60 ms); the doorbell gets to the next poll's arrival with no extra read and no held requests, which Indy chose over the directory it replaced.
4. **Rebuild-vs-iterate** — Iterate: the readiness index stays; a channel per partition rings it.
5. **What we build** — A doorbell rung per partition and read from process memory, a poll that peeks the rung partition beside the rotation it keeps, a first frame that posts at once, staggered workers, seven segment fields on both terminal lines.
6. **What we do NOT build** — A long poll; runner OTLP on dev; model or provider changes.
7. **Fit with existing features** — Idle poll cost stays at one Dragonfly read and the one Postgres authentication read it already pays; the doorbell rides the live tail's one subscription connection; `docs/architecture/scaling.md` restates the budget in the same Pull Request.
8. **Surface order** — N/A — no user surface; operators read the journal.
9. **Dashboard restraint** — N/A — no dashboard change.
10. **Confused-user next step** — An operator greps the lease's `lease_completed` or `lease_failed` line and reads which segment was slow.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the doorbell, the runner's early flush and stagger, and the segment timings ship together, because §4's proof reads its numbers from §2's fields and §3 is what makes §1's gain visible on screen.
- **Alternatives considered:** a server-held long poll (rejected by Indy for its held requests; parking a poll on the doorbell stays out of scope); the one-slot readiness directory this branch first built (replaced: it put back the hot slot partitioning removed); fewer partitions (rejected: it shortens the rotation but keeps the wait proportional to it); a shorter idle back-off (rejected: every poll authenticates against Postgres, so it multiplies that read across every runner); a dirty set with an empty-check-and-clear script (rejected: the partition hash and the set sit on different slots, so the script is `CROSSSLOT`).
- **Patch-vs-refactor verdict:** this is a **patch** because the readiness index and the poll keep their shape and gain one channel per partition. The larger refactor, one work queue in Postgres, is specced on its own as M226_001.

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
- **Decisions after review (Oct 13, 2026)** — Indy answered the five open review items "1A 2A 3A 4A", then for the fifth: "I select this - My call is A: keep the start-only stagger and let §4's ten dev steers measure pickup p95." (a) Every poll was sent to the same oldest dirty partition and the rig's wasted claim rate rose 20%: a poll now takes one of the three oldest entries in turn by its cursor (Dimension 1.7). (b) The directory slot has a deadline and no breaker: the chaos run that freezes its node measures what a poll pays, and that number decides. (c) A steer admitted mid-run loses its entry to the next dry poll and is found by the holder's held-first read the moment the run ends, or by the rotation: no re-note; §4's dev steers show whether the rotation path happens. (d) Oldest-first trusts each replica's clock: documented in `directory.rs` and `runner_fleet.md`, since skew reorders entries and loses none. (e) The worker stagger sets only the first poll: kept; §4 measures pickup p95. (f) Greptile on #750: a fleet left out of a shared 64-fleet scan can lose its hint under heavy load, `queue_wait_ms` read the poll's starting clock, and the Dragonfly tally skipped the claim's stream commands. Indy chose "Document, reply (Recommended)", "Fix in this PR (Recommended)" and "Count every command": documented on `interleave` and Dimension 1.1, the queue wait now ends at the lease's write, and every claim-path command joins the tally. (g) With the chaos run's numbers (a frozen directory node costs a poll 100 ms and a mark about 200 ms, nothing fails), Indy chose "Breaker on the directory slot: no breaker", and "Shipping: This depends on pickup bench." (h) (f)'s documented case broke the earlier milestone's promise that one rotation of 16 polls leases every placeable fleet (`test_skewed_workload_preserves_discovery_and_delivery_fairness`): a poll leases one fleet, and when the hint's fleet won the scan's random order the rotation's visit to a partition with work went unused. Indy answered "A" (the cursor's partition first): both peeks reach the scan whole, up to 128 ids for at most 64 candidates, and `SELECT_READY_CANDIDATES` orders the cursor's ids first; with the lane's reset before each run the fleet suite passed 5 of 5 on this branch (216 passed) and 5 of 5 on main (200 passed). (i) Indy answered "Bar for shipping yes": at every load step of the pickup bench, this branch's pickup p95 is at most main's and its Postgres statements per lease at most main's plus 10%; the bench waits on Indy: "donot build it now, i am trying to understand first." A review of A found one case it leaves, documented as (f) chose: a cursor partition that fills the scan with fleets none of which leases cuts the hinted ones untried, and the dry poll spends the entry.
- **Open for Indy** — (a) Does M225 also point dev's runner at a collector, or do the segment fields stay journal-only until the "runner collector built later" note in `docs/architecture/observability.md` lands? (b) Does the refused-write retry cap (make `request_policy_not_allowed` and `secret_not_found` terminal for a request shape, cap identical refusals per lease) fold into M225 or get its own spec?
- **Skill-chain outcomes** — second gstack `/review` (Oct 13, 2026, on `60503d02f`): seven specialists, an adversarial pass and a red team. Critical, found by both the adversarial pass and the red team: a poll whose claim faulted returned before it spent its hint, so one broken fleet could send every poll back to its fault; fixed in `518fb55ea`, which spends the hint on every exit and an empty hinted peek wherever the poll leased. Indy ticked the rest of the fixes: `queue_wait_ms` for an entry read new only, never negative (`f08f3dfbf`); the cursor's ids cast once per scan (`2d2d2f3e3`); the reclaim sweeper's re-mark unhinted (`c2e752117`); race-free hint counts, a poll-level deadline proof and the two over-length tests split (`5e6853b08`); the report's `time_to_first_token_ms` documented as counting from the run's start (`9ef061c2d`); a wrong field of a secret named with the fields it has (`b0d9e2454`); the model tables' badge markup, label sort, neutral badge, column-sized names and the Model library ending at its rows (`7da9e39b3`); and the advisory clean-ups (`d085f0205`). Left as they are, by Indy's choice: Dimension 2.1 keeps the activity drain and a delegated child's model time outside the segments it names, no dashboard panel shows the two new families, and the poll tally keeps counting commands beside each call.
- **Doorbell swap (Oct 13, 2026)** — An adversarial review of the pickup path found the directory put back the hot slot partitioning removed: `datastore_scaling.md` rejected one readiness hash for "one slot, one node, every poll and every mark", and the directory was one hash on one slot that every poll read and every mark wrote. The doorbell replaced it: a ring per partition on that partition's slot, sixteen bits per replica, no token, no deadline, no delete. Idle poll cost returns to one Dragonfly command. This supersedes the Oct 12 rejection of a pub/sub wake, because the doorbell feeds bits and holds no request; parking a poll stays out of scope. The second re-review's items (a), (b) and (d) went with the directory; (c), one fleet's claim fault ending the whole poll, predates this branch and stays open.

> Indy (2026-10-13): "Swap in #750 (Recommended)" — context: replace the readiness directory with a per-partition doorbell before #750 merges, and spec the one-queue refactor separately.

- **REVIEW round 3 (Oct 14, 2026, gstack `/review` on `39c4f3415`, delta from `b25be9c72`)** — five specialists, a red team and an adversarial pass; 15 issues, 1 critical. Fixed: a dead primary looped every replica's hub, because the doorbell holds subscriptions on every primary and the wait after a refused resubscribe restarted each lap (it now climbs: 32 redials a second before, at most 12 after, against the half-working server `hub_socket_faults.rs` already uses, since the shared lane forbids killing a primary, `support/fleet_queue.rs`); the ring and the rung peek lost the directory's 100 ms bound; a gap rang all sixteen and hid real rings for a turn; a running fleet's partition stayed rung; the sweeper rang for live runs its autoclaim took; a dropped poll lost its ring; tests for the daemon's wiring, the ring counter, a lost take race and two flaky waits; stale docs, and the renamed `data_flow.md` heading restored. Deferred to M225_002: the bench lanes building `Leases` without a doorbell, a hub retry at boot, a counter for deaf channels, rings lost to tag-restricted fleets or a full peek sample, ringing only on new marks, and the per-replica herd.

> Indy (2026-10-14): "Fix + fault test (Recommended)" and "Defer all to M225_002 (Recommended)" — context: the hub's redial wait, and the review's doorbell design gaps held for the pickup bench spec.

- **Deferrals** — §4 (Dimensions 4.1, 4.2) to after the merge: it needs this branch on dev, and dev deploys only from `main` (`.github/workflows/deploy-dev.yml` `on: push: branches: main`), so §4 stays IN_PROGRESS through the Pull Request.

> Indy (2026-10-12): "Defer §4 to after merge (Recommended)" — context: the ten dev steers and the two-workspace check wait for the merge's dev deploy.
