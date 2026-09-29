<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M207_004: The event path and the live tail do work in proportion to real traffic, proven by lane

**Prototype:** v2.0.0
**Milestone:** M207
**Workstream:** 004
**Date:** Sep 28, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — an idle deployment spends Postgres writes on fleets that drained long ago, a refused or waiting event holds its fleet, a node blip freezes the live tail silently, and a long reply re-parses its whole answer on every flush
**Categories:** API, UI
**Batch:** B1 — folded into M207_003's branch and Pull Request by Indy's decision; split into its own file only for the spec line cap
**Branch:** feat/m207-003-every-send-settles
**Folded-into:** `M207_003`
**Baseline revision:** a9424e37029c5557db9471a9356e1d0888122c12 — M207_003's Fix-First tip, the review boundary
**Test Baseline:** unit `make test-unit-all` exit 0 — Rust 2,760 passed / 0 failed / 666 ignored (156 binaries), app 3,216 (347 files, 100% coverage), website 142, cli 1,777 passed / 16 skipped, design-system 640; integration `make test-integration-rustd` exit 0 — 646 passed / 0 failed over 136 binaries
**Baseline evidence:** local runs of the tree committed as `a9424e370`, macOS, compose Postgres + four-process Dragonfly, Sep 29, 2026; counts summed from each lane's summary lines. A first unit run failed `afd_outbound --test lanes` under load from the concurrent integration lane; it passed 5/5 alone and the full rerun above is green
**Depends on:** M207_003 — shares its branch; its Fix-First tip is this workstream's review boundary
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 28, 2026) from four read-only performance audits and an adversarial Chief Technology Officer (CTO) review; the load-bearing findings were re-read by the author at the lines cited
**Canonical architecture:** `docs/architecture/scaling.md` §Where the next ceiling actually lives, §Measured ceilings; `docs/architecture/runner_fleet.md` §Readiness index; `docs/architecture/data_flow.md` §D. WATCH

---

## Overview

**Goal (testable):** after every fleet drains, a runner poll issues zero Postgres statements and strands no event on any replica; a stopped admission frees its fleet at once; an acknowledgement reads the entries it trims and no more; one Dragonfly node's loss costs only that node's channels and tells their viewers to catch up; and a streamed reply's per-flush render cost does not grow with the answer's length.
**Problem:** the lease never clears a readiness mark (`ReadyIndex::clear_if_unchanged` has no production caller), so every poll re-claims drained fleets with autocommit writes and overwrites their sticky-runner hint; the published "0 Postgres per idle poll" exists only because the lease lane force-clears marks first. A refusal, Retry or Await exits after a won claim without releasing it, so the fleet's next event waits out the claim. Past 1,000 stream entries every acknowledgement reads 1,000 entries back to find one id. On the live tail, any reconnect loses frames with no signal to the viewer, one node's blip redials the whole cluster connection, a subscribe holds every frame on the replica, each viewer copies each frame, and 64 streams per replica is the hard ceiling. In the browser, the whole answer is re-parsed as markdown per flush, and the wall re-renders tiles for chunks it never shows.
**Solution summary:** measure first with a statement counter and a lease lane that drains through the real report path; mint a unique token inside every mark and clear on a group-empty poll after taking over any orphaned pending entry; release the claim on every stop; trim from the floor; give every lost subscription a gap, repair one node without redialling the cluster, split dispatch from subscribe control, and share one payload across viewers under a measured ceiling; render only the open markdown block, project the wall, and keep the thread shell out of each flush.

## PR Intent & comprehension handshake

- **PR title (eventual):** carried by M207_003's Pull Request (fold)
- **Intent (one sentence):** agentsfleet's cost tracks the work it does, not the fleets that ever ran or the viewers that happen to watch, and no viewer ever misses frames without being told.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_fleet/src/lease/assign.rs` — the candidate loop, the empty arm, and the per-process consumer name the pending read uses.
2. `rustd/crates/afd_dragonfly/src/ready.rs` — `mark`, `peek`, `clear_if_unchanged`: the readiness index and the ingress race its token exists for.
3. `rustd/crates/afd_fleet/src/lease/pull.rs` — the admission steps whose Stop arms return without a release.
4. `rustd/crates/afd_dragonfly/src/hub/pump.rs` — the pump that awaits subscribes inside its dispatch loop and redials on any node's disconnection.
5. `rustd/crates/afd_bench/src/lane/lease.rs` — how a lane seeds, drives and counts; `quiesce()` is why today's idle number is clean.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_db/src/**` | EDIT | A counter of executed statements and transactions the lanes read |
| `rustd/crates/afd_bench/src/**`, `rustd/crates/afd_bench/src/bin/tail.rs`, `make/bench.mk`, `bench/baselines/*.rig.json` | EDIT / CREATE | Re-baselined lanes; the lease lane drains through the report path; the tail lane |
| `rustd/crates/afd_dragonfly/src/{ready,ready/**,streams,streams/**}.rs` | EDIT | Tokens minted inside `mark`; the orphan takeover; trim from the floor |
| every `ReadyIndex::mark` caller: `rustd/crates/{afd_admission,afd_approval,afd_runner,afd_fleet,afd_bench}/src/**`, `rustd/crates/{afd_dragonfly,afd_fleet,afd_events,afd_approval,agentsfleetd}/tests/**` | EDIT | `mark` no longer takes a token |
| `rustd/crates/afd_dragonfly/{Cargo.toml,src/lib.rs,src/error.rs,src/error/**}`, `rustd/crates/afd_observability/src/{producers,metrics/declared}/fleet.rs`, `rustd/crates/afd_approval/src/inbox/{sweep,resolve}.rs` | EDIT / CREATE | Token minting through `afd_crypto`; the over-cap error file split; the claim-empty counter; the expiry wake |
| `rustd/crates/afd_fleet/src/lease/{assign,restore,pull,pull/**,affinity,issue,envelope,store,deliver,admit/**,sql/**}.rs` | EDIT | Group-empty clear; release on every stop; the held-slot filter; tenant read once; meter reset folded into the lease insert |
| `rustd/crates/afd_events/src/{history/statement,history/mod,lib}.rs`, `rustd/crates/afd_events/tests/{events_suite,integration_list_plans}.rs` | EDIT / CREATE | List and thread reads split by scope and cursor so a generic plan keeps the index |
| `schema/922_fleet_events_scope_statistics.sql`, `rustd/crates/afd_db/src/migration.rs` | CREATE / EDIT | The dependency statistics object 4.2's fleet-scoped plans need, analysed once at apply (Indy's SCHEMA GUARD approval) |
| `rustd/crates/afd_dragonfly/tests/integration_retention{,/floor}.rs`, `rustd/crates/afd_outbound/tests/integration_producer_outage.rs` | EDIT / CREATE | Retention tests above the new slack; the floor's own proofs |
| `rustd/crates/afd_dragonfly/src/{hub,hub/**,topology,transport,transport/**,test_util}.rs`, `rustd/crates/{afd_dragonfly,afd_sse,afd_api_tenant}/Cargo.toml`, `rustd/crates/afd_dragonfly/tests/**` | EDIT / CREATE | Gap arm; replay-attributed repair; dispatch and control tasks; shared payload; the fault fakes split under the cap |
| `rustd/crates/afd_api_tenant/src/handler/stream/body.rs`, `rustd/crates/{afd_gate,afd_approval,afd_fleet,agentsfleetd}/tests/**`, `public/openapi.json` | EDIT / CREATE | A shared-payload SSE body; consumers of the gap arm; the `catching_up` description |
| `ui/packages/app/lib/streaming/{fleet-stream-registry,fleet-stream-entry,workspace-stream}.ts` (+ tests) | EDIT | The chat and the wall backfill on `catching_up`, and a gap during a walk queues one walk after it |
| `rustd/crates/afd_sse/src/{tail,frame,fanin,ceiling}.rs`, `rustd/crates/afd_api_tenant/src/handler/stream{,/wall}.rs`, `rustd/crates/agentsfleetd/src/preflight/knobs.rs` | EDIT | Gap as `catching_up`; one rendered frame per replica; coalesced wall counters; the measured ceiling |
| `ui/packages/app/components/domain/{FleetReplyBody,FleetMarkdown,FleetThread,FleetThreadViewport}.tsx` | EDIT | Block-memoised markdown; a stable adapter; one announcement per reply |
| `ui/packages/app/lib/streaming/{workspace-store,workspace-tile,fleet-stream-backfill}.ts`, `ui/packages/app/components/domain/fleetMarkdownBlocks.ts`, `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.tsx` | EDIT / CREATE | Wall projection split from the store; streaming block boundaries; one notify per backfill walk |
| `ui/packages/app/tests/{helpers/*,bench/fleet-markdown-stream.bench.tsx,dashboard-fleets-wall.test.tsx,fleet-thread/malformed-metadata.test.ts}` | EDIT / CREATE | Shared wall and store harnesses split under the cap; the streaming corpus and bench; the log's `aria-live` pin |
| `docs/architecture/{scaling,runner_fleet,data_flow,datastore_scaling,concurrency}.md` | EDIT | Measured rows; the readiness token and clear; sharded pub/sub wording; the hub's gap |
| `rustd/crates/afd_sse/src/{lib,frame/**}.rs`, `rustd/crates/afd_sse/tests/integration_sequencing.rs`, `rustd/crates/agentsfleetd/src/preflight{.rs,/ceiling_tests.rs}`, `docker-compose.yml`, `docs/metrics.census.tsv` | EDIT / CREATE | The shared-payload frame data; the test that pins `SSE_MAX_STREAMS` to the tail baseline; `pg_stat_statements` preloaded for the statement counter; the claim-empty counter's census row |
| `deploy/fly/agentsfleetd-{prod,dev}/fly.toml` | EDIT | Comment only: `SSE_MAX_STREAMS` defaults to 256 (Indy approved the deploy-config touch) |
| `rustd/crates/afd_api/tests/{integration_fleet_streams.rs,support/fleet_stream_transport_fixture.rs}` | EDIT | Test readers buffer an SSE event to its blank line, since the shared-payload body writes one event as several chunks |
| `playbooks/operations/observability/observability_test{,_support}.sh` | DELETE | M197's observability self-test suite and its stubs, removed on Indy's call ("nuke that observability_test.sh"); its bench-baselines guard failed every later branch that re-baselined |
| `rustd/crates/afd_db/src/test_util{.rs,/**}`, `rustd/crates/afd_db/tests/{db_suite,integration_platform_default}.rs`, `rustd/crates/afd_credential/tests/integration_rotation{,/**}.rs`, `rustd/crates/afd_api/tests/{harness/stubs_tenant,integration_admin,integration_wall_ticks,tenant_plane_suite,webhook_fleet_route,webhook_fleet_route/unsupported,support/wall_stream_fixture}.rs`, `rustd/crates/afd_admission/{Cargo.toml,tests/receipt_faults{,/**}.rs}`, `rustd/crates/afd_cron/{Cargo.toml,src/fire.rs,tests/integration_fire.rs}`, `rustd/crates/afd_runner/{Cargo.toml,tests/**}`, `rustd/crates/afd_dragonfly/tests/{datastore_suite,group_create_faults,hub_gap_faults,hub_repair_faults,retention_faults,support/fake_redis{,/**}}.rs`, `rustd/Cargo.lock` | EDIT / CREATE | The coverage bar: every changed Rust file outside `afd_bench` above 99% line coverage. A platform default a failing holder created is removed as it unwinds |
| Tests beside each file above | CREATE / EDIT | One test per Dimension |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (retention slack, ladder steps, the stream ceiling as named consts), NDC and ORP (`quiesce`'s force-clear and the token parameter leave no caller), FLL, LOG, NSQ/STS/SGR/ITF (every rewritten statement), TSC, TSJ, UIS.
- `docs/RUST_ERROR_STANDARD.md` — the hub's gap arm and every changed fallible signature keep their error kinds.
- `docs/LOGGING_STANDARD.md` — the gap, claim-empty and depth signals.
- `docs/architecture/concurrency.md` §The five invariants — the hub's second task joins the thread map and the channel inventory.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes — `assign.rs`, `pump.rs`, `workspace-store.ts` grow | split by concern before a file passes its headroom |
| UFS | yes | slack, ladder and ceiling figures are named consts |
| LOGGING | yes | gap and depth events carry counts and channel ids, never payloads |
| UI GATE / DESIGN TOKEN GATE | yes — reply and wall components | no raw elements or bracket utilities added |
| SCHEMA GUARD | yes — one additive file, approved by Indy (Discovery) | a dependency statistics object on `core.fleet_events (workspace_id, fleet_id)` and its `migration.rs` entry; no DROP or ALTER; the held-slot filter and the meter fold stay query text |

## Prior-Art / Reference Implementations

- **Reference:** redis-rs 1.7.0 cluster `resubscribe_node` — the per-node repair §5 stops overriding; `afd_runner/src/sweep/reclaim.rs` — `XAUTOCLAIM` into the lease consumer, the takeover §2 performs on a won claim; `afd_fleet_lifecycle/src/live_set.rs` — a per-replica cache with a tick, reused for the wall's counters; `INSERT_LEASE_WITH_EVENT` — the one statement §4 folds the meter reset into.

## Sections (implementation slices)

### §1 — Measure first

No path changes until its lane runs at HEAD. **Implementation default:** count executed statements and committed transactions, not pool acquires, because a report is one acquire for several statements and an acquire count cannot see a rewrite.

- **Dimension 1.1** — every executed statement and every commit increments counters the lanes read → Test `test_statement_counter_counts_each_statement` — DONE (`test_statement_counter_counts_each_statement`; counts come from Postgres's own `pg_stat_statements` and `xact_commit`, with the compose Postgres preloading it)
- **Dimension 1.2** — the four existing lanes re-run at HEAD before any §2–§5 code, and their baselines land in a measurement-only commit → Test `bench_rebaseline_at_head` — DONE (four lanes re-run at `774f99cbd` on a reset rig: lease 412.98/s, 11.7 round trips per lease; steer 1,638.8/s, 1.94 Postgres transactions per steer; outbound 49.6 jobs/s; cardinality 4,641 B per fleet at 10,000)
- **Dimension 1.3** — the lease lane drains through the real lease and report path without force-clearing, asserting each event terminal once, two ledger rows per event and a readiness depth of zero → Test `bench_lease_drains_through_report` — DONE (`bench_lease_drains_through_report`: `lanes_lease` 7 passed; 200 events processed once, 2 ledger rows each, `drain_ready_depth=0`, 47.93 statements and 25.56 commits per lease)
- **Dimension 1.4** — a tail lane reports allocations, CPU and latency per delivered frame over a viewer ladder, and memory per stream over a stream ladder; a node's loss is proven by §5's integration test, because killing one process of a shared compose container is not a stable bench → Test `bench_tail_reports_fanout_and_faults` — DONE (`bench_tail_reports_fanout_and_faults`: `make bench-tail PROFILE=rig` exit 0, `frames_undelivered=0 lag_notices=0 streams_unreached=0`, per-rung `stream_receive_p95_ms` 0.131–0.184)

### §2 — A poll touches only fleets with work, and strands nothing

A drained fleet's mark is cleared, and only after the whole group has nothing pending. **Implementation default:** `mark` mints a Universally Unique Identifier version 7 (UUIDv7) token itself and callers pass none, because two of today's mark sites append nothing and the fleet id as token makes a compare-and-clear unconditional; a won claim first takes over the group's oldest pending entry, because the empty read sees only this process's own pending list and a won claim proves no live lease holds the fleet.

- **Dimension 2.1** — two marks of one fleet mint distinct tokens → Test `test_marks_mint_distinct_tokens` — DONE (`test_marks_mint_distinct_tokens`: `afd_dragonfly` lib 51 passed, 0 failed)
- **Dimension 2.2** — a group-empty poll clears the mark with the token it peeked; a mark written between the read and the clear survives and is leased next poll → Test `test_mark_written_during_poll_survives` — DONE (`test_mark_written_during_poll_survives`, 24 looped races, and it asserts `agentsfleet_lease_claims_empty_total` moves: ok)
- **Dimension 2.3** — an entry pending under another replica's consumer is delivered by the next won claim, never stranded behind a cleared mark → Test `test_entry_pending_elsewhere_is_delivered` — DONE (`test_entry_pending_elsewhere_is_delivered`: ok)
- **Dimension 2.4** — the candidate query skips slots a live runner holds, and breaks ties at random; a claim that leases nothing clears the fleet's last-runner hint, so one fleet that keeps stopping cannot starve its partition for a runner → Test `test_candidates_skip_held_slots` — DONE (`test_candidates_skip_held_slots`: ok; the poll costs 1 round trip, with no losing claim)
- **Dimension 2.5** — after every fleet drains, an idle poll issues zero Postgres statements → Test `bench_idle_poll_after_drain` — DONE (`bench_idle_poll_after_drain`, `bench_lease_drains_through_report`: `lanes_lease` 7 passed; `make bench-lease PROFILE=rig` prints `idle_statements_per_poll=0 idle_commits_per_poll=0 … drain_ready_depth=0`, against 47.5 before)

### §3 — A stopped admission frees its fleet

- **Dimension 3.1** — a refusal, Retry, Await or error after a won claim releases the claim through one helper, so the fleet's next event leases on the next poll → Test `test_stop_releases_the_claim` — DONE (`test_stop_releases_the_claim`: ok)
- **Dimension 3.2** — an approval resolution is leased on the next poll → Test `test_approval_resolution_leases_next_poll` — DONE (`test_approval_resolution_leases_next_poll`: ok)
- **Dimension 3.3** — an approval that expires unanswered re-marks its fleet, because a park cleared the mark → Test `an_expired_gate_wakes_its_fleet` — DONE (`an_expired_gate_wakes_its_fleet`: approval_suite 58 passed, 0 failed)

### §4 — Acknowledgements and reads stay small

- **Dimension 4.1** — the trim after an acknowledgement reads at most the entries it removes plus one and never more than `TRIM_READ_MAX`, bounds its read at the oldest owed entry, runs only past the keep count plus `TRIM_SLACK`, and never removes a pending or undelivered entry → Test `test_trim_reads_only_the_floor` — DONE (`floor::test_trim_reads_only_the_floor`: `integration_retention` 3 passed, 0 failed; 1,520 appended, 1,500 acknowledged, 10 pending → removed 520, read 521; `a_trim_keeps` 1 passed)
- **Dimension 4.2** — the fleet, workspace and thread reads keep their scope, cursor and `since` bound as index conditions under a generic plan, and walk their index in order without a Sort: a dependency statistics object stops the planner multiplying workspace and fleet odds, and actor-filtered reads get their own texts → Test `test_event_list_plans_use_the_index` — DONE (`test_event_list_plans_use_the_index`: `1 passed; 0 failed` at spreads (20, 5, 40) and (50, 10, 200); scope, cursor and `since` in Index Cond, no Sort on the unfiltered texts; red with the statistics object dropped)
- **Dimension 4.3** — a lease reads its tenant once, and its meter reset rides the lease insert → Test `test_issue_reads_the_tenant_once` — DONE (`test_issue_reads_the_tenant_once`, `test_the_lease_row_resets_the_meter_only_when_fresh`: both ok)

### §5 — The live tail degrades by channel, not by replica

**Implementation default:** a dispatch task owns pushes and a control task owns subscribe commands, because a subscribe round trip must never hold a frame. redis-rs 1.7.0 forwards a node's `Disconnection` without its address, and its own replay cannot restore a node's channels: it packs every sharded channel into one `SSUBSCRIBE` routed by the first channel's slot, dropped unless that slot is on the repaired node and refused `CROSSSLOT` by Dragonfly when it is (`subscription_tracker.rs:105-127`, `cluster_handling/async_connection/mod.rs:1173-1186`). So on any `Disconnection` the hub re-sends one `SSUBSCRIBE` per live channel on its existing connection, and each routes to its own slot's node; any confirmation after a channel's first is a gap, so a node loss gaps every channel. A disconnect whose re-subscribes do not confirm within `NODE_REPAIR_WINDOW`, or two at once, falls back to a full redial with a gap on every channel. The hub's driver repairs a node without an attempt cap. A gap is written as `catching_up` with `dropped: 0`, and the SSE body shares one payload across viewers, because axum's `Event` copies per viewer.

- **Dimension 5.1** — every hub reconnect sends each live channel a gap, and its viewers receive `catching_up` → Test `test_reconnect_sends_a_gap` — DONE (`hub_gap_faults::test_reconnect_sends_a_gap`: pass; live, the kill-every-socket test asserts the gap)
- **Dimension 5.2** — one primary's socket loss re-subscribes every live channel, one command each, restores the lost node's channels, gives each channel a gap, and leaves other nodes' frames flowing, with no new connection opened → Test `test_node_loss_is_a_gap_not_a_reconnect` — DONE (`test_node_loss_is_a_gap_not_a_reconnect` on the four-node cluster: `2 passed; 0 failed`, three runs; the hub waits for the owner's `CLUSTER MYID` before re-subscribing)
- **Dimension 5.3** — frames keep flowing while a subscribe is slow → Test `test_dispatch_continues_during_slow_subscribe` — DONE (`test_dispatch_continues_during_slow_subscribe`: a subscribe held 2 s, another channel's frame inside 500 ms: pass)
- **Dimension 5.4** — a frame's payload and rendered event are shared across viewers, not copied per viewer → Test `test_fanout_shares_one_payload` — DONE (`test_fanout_shares_one_payload`, `test_the_body_shares_one_payload_across_viewers`, `the_body_writes_what_axum_would`: pass; 1.05 allocations per delivered frame at 1,024 viewers, against 3.05)
- **Dimension 5.5** — a lagging wall viewer refreshes counters at most once per tick → Test `test_wall_lag_reads_counters_once_per_tick` — DONE (`test_wall_lag_reads_counters_once_per_tick`: 50 lags in one beat cost 1 read: pass)
- **Dimension 5.6** — `SSE_MAX_STREAMS`'s default is the largest stream-ladder rung at which every frame is delivered, p95 publish-to-receive stays under 250 ms and the streams' resident memory stays under 256 MiB, cited beside the const → Test `bench_stream_ceiling_ladder` — DONE (`bench_stream_ceiling_ladder`: `1 passed`; default 256, where the 256 rung measures p95 0.143 ms, 13,494 B per stream, 0 undelivered; the socket budget binds, not the ladder)
- **Dimension 5.7** — the chat backfills on any `catching_up`, lag or gap → Test `test_chat_backfills_on_catching_up` — DONE (`fleet-stream-registry-backfill.test.ts`: a gap backfills once, and a gap during an in-flight walk queues exactly one walk after it)
- **Dimension 5.8** — the wall shows catching up on any `catching_up`, including a gap's `dropped: 0` → Test `test_wall_shows_a_gap` — DONE (`workspace-stream-backfill.test.ts`, `dashboard-fleets-wall.test.tsx`: a `dropped: 0` gap shows catching up; one or three gaps during a walk queue one follow-up; a torn-down connection queues none)

### §6 — A flush renders one leaf

- **Dimension 6.1** — a streaming answer re-parses only its open block, finished blocks keep identity, and the settled render equals a single parse on a corpus with fences split across chunks → Test `test_streaming_markdown_reparses_only_the_open_block` — DONE (400 flushes of a 20 KB answer take about 400 ms against 10,517 ms for the whole-prefix path; the settled HTML is byte-identical to a single parse; the last two blocks stay open because a list item arriving a chunk later joins its list)
- **Dimension 6.2** — a wall tile does not re-render on chunk or tool frames → Test `test_wall_tile_ignores_chunks` — DONE (100 tiles and 2,000 chunk and tool frames → 0 tile commits, `FleetTile.stream.test.tsx`)
- **Dimension 6.3** — a text flush re-renders neither the thread viewport nor the composer → Test `test_flush_leaves_the_shell_alone` — DONE (`FleetThread.flush.test.tsx`: text flushes render neither the viewport nor the composer)
- **Dimension 6.4** — a backfill walk notifies once, and the transcript announces a settled reply once → Test `test_backfill_walk_notifies_once` — DONE (`fleet-stream-backfill.test.ts`: a three-page walk notifies its thread once; `FleetThread.flush.test.tsx`: a settled reply is announced once and never while it streams)

## Interfaces

```
ReadyIndex::mark(fleet_id) -> Result<ReadyToken>   a UUIDv7 token minted inside; callers pass none
ReadyIndex::clear_if_unchanged(fleet_id, token)    called by the lease on a group-empty poll or a park
FleetStreams::take_over_oldest(fleet, consumer)     XAUTOCLAIM min-idle 0 COUNT 1, never FORCE
hub::Received { Message(Arc<Message>), Lagged(u64), Gap }; SSE writes Gap as `catching_up` with `dropped: 0`
SSE_MAX_STREAMS default = the tail lane's measured value (knobs.rs), overridable as today
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Ingress marks during a clear | append lands between the poll's read and its clear | the newer token survives the compare; the event is leased next poll |
| Clear fails | Dragonfly error on the clear | the poll answers as today; the mark stays and costs one more empty claim |
| Entry orphaned in another consumer | a replica died between its stream read and its lease | the next won claim takes it over and delivers it |
| Stop after a won claim | refusal, Retry or Await | the claim is released; the next event is not held |
| Parked gate expires unanswered | nobody answers the approval | the inbox expiry sweep re-marks each swept fleet |
| Trim races an append | an append lands during the trim | the floor is the minimum of delivered, pending and kept; nothing pending is removed |
| Primary socket lost | one Dragonfly primary drops | the driver repairs it; the hub re-subscribes every channel one command each; every channel gets a gap, so the other nodes' viewers backfill needlessly once |
| Loss the hub cannot attribute | a replica drops, two nodes drop within `NODE_REPAIR_WINDOW`, or the driver's replay is lost | full redial; every channel gets a gap |
| Slot migration | the driver replays every channel | every channel gets a gap; some viewers backfill needlessly, none miss frames silently |
| Whole connection lost | every node unreachable | redial and resubscribe as today, and every channel gets a gap |
| Subscribe storm | every tab reconnects after a deploy | frames keep dispatching while subscribes queue |

## Invariants

1. A mark is cleared only by the token that wrote it — `clear_if_unchanged` compares inside Dragonfly; `test_mark_written_during_poll_survives`.
2. A cleared mark never strands an event — the clear runs only on a group-empty read after the orphan takeover, or on a park whose answer re-marks (continuation admission, runless wake, expiry sweep); `test_entry_pending_elsewhere_is_delivered`, `an_expired_gate_wakes_its_fleet`.
3. A viewer never misses frames silently — every lost subscription yields a gap, and a gap may over-report (a slot migration gaps every channel) but never under-report; `test_node_loss_is_a_gap_not_a_reconnect`, `test_reconnect_sends_a_gap`.
4. A trim never removes a pending or undelivered entry — its read ends at the oldest owed entry, and last-delivered only moves forward; a concurrent trim can shorten acknowledged history, never cross owed work; `test_trim_reads_only_the_floor`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `hub_channel_gap` (warn log, one per 1 s burst) | ops | channels lose their subscription | channel count, cause (`node_repaired`, `slot_moved`, `reconnected`, `resubscribed`), `error_code` | no channel names, no payloads | `test_reconnect_sends_a_gap` |
| `agentsfleet_lease_claims_empty_total` (counter) | ops | a won claim finds nothing deliverable | none | no fleet body | `test_mark_written_during_poll_survives` |
| `agentsfleet_fleet_ready_depth` (gauge, existing) | ops | each poll's peek | depth | none needed | `bench_idle_poll_after_drain` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_statement_counter_counts_each_statement` | three statements in one transaction → statements 3, commits 1 |
| 1.2 | bench | `bench_rebaseline_at_head` | four lanes at HEAD → baselines committed with revision and parameters |
| 1.3 | bench | `bench_lease_drains_through_report` | 200 fleets, 8 runners, no force-clear → every event terminal once, depth 0 |
| 1.4 | bench | `bench_tail_reports_fanout_and_faults` | viewers 1/64/256/1024 × 200 B/4 KiB/64 KiB → every frame delivered, allocations, CPU and p95 per frame; 64–4096 streams → memory per stream |
| 2.1 | unit | `test_marks_mint_distinct_tokens` | mark twice → two different tokens |
| 2.2 | integration | `test_mark_written_during_poll_survives` | re-mark between the empty read and the clear, looped → mark present, event leased next poll |
| 2.3 | integration | `test_entry_pending_elsewhere_is_delivered` | entry pending under consumer A, poll through consumer B → delivered, mark cleared only after |
| 2.4 | integration | `test_candidates_skip_held_slots` | a live claim on fleet X → X absent from another runner's candidates |
| 2.5 | bench | `bench_idle_poll_after_drain` | drain every fleet, then idle polls → 0 Postgres statements per poll |
| 3.1 | integration | `test_stop_releases_the_claim` | refusal then a new send → leased on the next poll, no claim-length wait |
| 3.2 | integration | `test_approval_resolution_leases_next_poll` | Await, then resolve → leased on the next poll |
| 3.3 | integration | `an_expired_gate_wakes_its_fleet` | mark force-cleared, gate expires → the fleet is marked again |
| 4.1 | integration | `test_trim_reads_only_the_floor` | 1,520 appended, 1,500 acked, 10 pending → removed 520, read 521, pending and undelivered kept; one owed entry below the window → read stops at it |
| 4.2 | integration | `test_event_list_plans_use_the_index` | a deep-cursor page per shape through `History`, then `EXPLAIN (GENERIC_PLAN)` per text → the expected index, scope and cursor or `since` in Index Cond, no Sort |
| 4.3 | integration | `test_issue_reads_the_tenant_once` | one lease → one tenant read, meters reset in the lease insert |
| 5.1 | integration | `test_reconnect_sends_a_gap` | force a hub reconnect → each live channel's viewer gets `catching_up` |
| 5.2 | integration | `test_node_loss_is_a_gap_not_a_reconnect` | drop one node's socket → its channels resubscribe and get a gap, other nodes' frames flow (their channels may also gap), connections opened unchanged |
| 5.3 | integration | `test_dispatch_continues_during_slow_subscribe` | a subscribe held 2 s while another channel publishes → frames delivered within the hold |
| 5.4 | unit | `test_fanout_shares_one_payload` | one message, 3 receivers → one payload allocation |
| 5.5 | unit | `test_wall_lag_reads_counters_once_per_tick` | 50 lag events in a tick → one counters read |
| 5.6 | bench | `bench_stream_ceiling_ladder` | 64/256/1024/4096 streams → memory and CPU per stream; the const cites the result |
| 5.7 | unit | `test_chat_backfills_on_catching_up` | open chat stream, `catching_up` with `dropped: 0` → one backfill read |
| 5.8 | unit | `test_wall_shows_a_gap` | wall stream, `catching_up` with `dropped: 0` → catching-up state shown |
| 6.1 | unit | `test_streaming_markdown_reparses_only_the_open_block` | 400 flushes of a 20 KB answer with split fences → finished blocks keep identity; settled output equals a single parse |
| 6.2 | unit | `test_wall_tile_ignores_chunks` | 100 tiles, 1,000 chunk frames → 0 tile renders |
| 6.3 | unit | `test_flush_leaves_the_shell_alone` | 100 flushes → viewport and composer render 0 times |
| 6.4 | unit | `test_backfill_walk_notifies_once` | three-page walk → one notify; settled reply → one announcement |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Baselines re-measured before code (§1) | `git log --format=%s -- bench/baselines/ \| head -1` | substring `bench` | P0 | |
| R2 | An idle poll after drain costs no Postgres (§2) | `make bench-lease PROFILE=rig \| grep -cE 'idle_statements_per_poll=0( \|$)'` | 1 (at HEAD before §2: `idle_statements_per_poll=47.5`) | P0 | |
| R3 | A stop frees its fleet (§3) | `make test-integration-rustd` | substring `test_stop_releases_the_claim ... ok` | P0 | |
| R4 | A node blip is a gap, not a reconnect (§5) | `make test-integration-rustd` | substring `test_node_loss_is_a_gap_not_a_reconnect ... ok` | P0 | |
| R5 | Flush cost is flat in answer length (§6) | `cd ui/packages/app && bunx vitest run components/domain/FleetReplyBody.test.tsx` | exit 0 | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the two specs' Files Changed tables | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration lane green (live Postgres + Dragonfly) | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Versions in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** every declared `conform` and `verify.*` command from `.oracle/orly.json` appears above verbatim. **Grading protocol (VERIFY):** Graded = ✅/❌ + one decisive output line; repository-command rows point at the final `orly gate pr` in M207_003's Session Notes. A P1 ❌ needs an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `force_clear` in the lease lane's idle window | `git grep -n "force_clear" -- rustd/crates/afd_bench/src/lane/lease.rs` | 0 matches |
| the token argument to `mark` | `git grep -nE "\.mark\([^,)]+, " -- rustd/crates` | 0 matches |

## Out of Scope

- Awaiting Indy's scope call (Discovery), not deferred: the one-statement claim, issue and report rewrites; the wallet lock span; the budget read's index; the reply text side-store; assistant-ui eviction of capped messages; the server-side wall filter; a bounded push channel; status folded into the admission insert.
- Caching runner authentication or the lease target on the activity request: a security boundary that trades revocation lag for load.
- One publish per activity batch: needs every replica to read both shapes before any publisher switches, which is two releases.
- Routing viewers to replicas by fleet: at most a replica-count gain at two to four replicas.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator leaves the dashboard open on a long run: the reply streams without stutter, the wall stays calm, and a Dragonfly node restart shows a brief "catching up" instead of a silently frozen tail.
2. **Preserved user behaviour** — every frame, order, reply text, Resend and backfill behaves as today; the workspace stream's default output is unchanged.
3. **Optimal-way check** — the optimal shape is a scheduler that only ever sees ready work; a cleared, token-guarded readiness index that never strands an entry is that shape on the existing layout.
4. **Rebuild-vs-iterate** — iterate on each path's existing layout; no new subsystem, datastore or service.
5. **What we build** — the counters and lanes, minted mark tokens with an orphan-safe clear, release on every stop, the floor trim, the hub's gap, per-node repair, task split and shared frame under a measured ceiling, the block-memoised reply, the wall projection and a still thread shell.
6. **What we do NOT build** — the money-path statement rewrites until Indy decides; auth caching, per-batch publishing, fleet-affine routing (Out of Scope).
7. **Fit with existing features** — compounds with M207_003's settle reads and `call_id`; must not destabilise at-least-once delivery, fencing or single-charge billing.
8. **Surface order** — daemon first: §1–§5 are server costs; §6 is the browser's.
9. **Dashboard restraint** — no new controls; only the existing `catching_up` state appears on a node loss.
10. **Confused-user next step** — a "catching up" notice resolves itself through backfill; nothing to press.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six Sections in dependency order — measurement first, because every later Dimension proves itself against a lane, then the scheduler, the stop paths, acknowledgements and reads, the tail, the browser.
- **Alternatives considered:** a separate milestone and Pull Request per layer (recommended at REVIEW, overridden by Indy for one Pull Request); one-statement claim, issue and report rewrites (the CTO review found a live-lease expiry, a lock-order deadlock, a double-update of one `runner_affinity` row and a pre-charge budget on the completion frame — held for Indy's call); a scheduler rewrite (rejected: the readiness index is the right shape once it is cleared safely).
- **Patch-vs-refactor verdict:** this is a **refactor** of the readiness index's lifecycle and the hub's task structure, because the defects are structural (a mark nothing clears, one task doing two jobs, a node loss handled as a total loss), not local.

## Discovery (consult log)

- **Consults** — four read-only audits (daemon live tail, durable events, browser, plus the M207_003 adversarial review) and an adversarial Chief Technology Officer (CTO) review of the first plan (verdict: rework). The author re-read at source: marks never cleared (`ready.rs:259` has no production caller); the empty read is per consumer (`lease/restore.rs:83-90`, consumer `agentsfleetd-{pid}` at `assign.rs:271-273`), so a naive clear strands another replica's pending entry; Stop arms return without a release (`pull.rs:150-153`, `:262-272`); data-modifying Common Table Expressions (CTEs) run whether read or not (`sql/lease.rs:239-242`). Dragonfly v2.0.0 cluster mode rejects global `PUBLISH` and routes sharded pub/sub by slot (the pinned tag's `docs/cluster-mode.md`, read by the live-tail audit).
- **Browser limits recorded, not fixed** — the settled-reply announcement is proven by its unit test, not yet heard through a screen reader in a browser (check on DEV with Dimension 6.7 of M207_003); a reference-style link or footnote whose definition arrives later renders unresolved while streaming and resolves at settle.
- **Open scope call** — asked Sep 29, 2026 with no answer inside the question window: adopt the reviewed core plus the stream work, holding the money-path rewrites, wallet lock span, budget index, reply side-store, eviction, server filter, bounded push channel and status fold. The agent built only the agreed scope; nothing in Out of Scope's first bullet is deferred until Indy's quote lands here.
- **Metrics review** — one warn log (`hub_channel_gap`) and one counter (`agentsfleet_lease_claims_empty_total`) added; no analytics or funnel change. A subscribe issued while its primary is still reconnecting can land on another node and be confirmed (redis-rs `mod.rs:944-969`): §5 closes it for every re-subscribe after a node loss, which waits for the owner to answer `CLUSTER MYID`; three paths stay exposed — a first `Subscribe` for a new channel whose owner is mid-repair, the `Resubscribe` after a slot move to a reconnecting owner, and a redial's generation-start re-subscribe when the fresh connection missed a node.
- **Build-time decisions (Sep 29, 2026)** — 4.2: the planner multiplies workspace and fleet odds and collapses the actor guard to about one row, so a fleet page sorted its whole fleet; options were statistics, a once-per-query workspace check, or accepting the Sort. 5.2: the driver's replay never restores a node's channels on a multi-primary hub (live: `reconnected` 1 ms after the kill, no replay in 5 s; raw probe `-CROSSSLOT`); options were hub-side re-subscribe (A), a patched redis-rs (D), or the 5 s redial (C). An upstream redis-rs issue is drafted, filed only on Indy's word. 5.6: 4096 passes speed and memory; the socket limit binds; Indy chose 256 and declined a follow-up.

> Indy (2026-09-29): "Statistics (Recommended)" — context: 4.2's Sort; approves the additive `schema/` statistics file under SCHEMA GUARD.
> Indy (2026-09-29): "A: hub resubscribes (Recommended)" — context: 5.2's node loss; every channel gaps on any node loss.

- **Skill-chain outcomes** — pending.
- **Deferrals** — the packaging decisions:

> Indy (2026-09-28, before 20:43): "Fold into M207_003" — context: where the performance refactor lands; the recommendation was a separate milestone M208 with four workstreams.
> Indy (2026-09-28, before 20:43): "Fold anyway (override)" — context: `dispatch/write_spec.md:84` gives backend-heavy and billing work its own spec and Pull Request; Indy overrode it for this refactor.
