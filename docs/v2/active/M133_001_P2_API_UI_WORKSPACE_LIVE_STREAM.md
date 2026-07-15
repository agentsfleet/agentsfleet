<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M133_001: The workspace-multiplexed live stream

**Prototype:** v2.0.0
**Milestone:** M133
**Workstream:** 001
**Date:** Jul 14, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — an optimization of the already-shipped-and-working Live Wall (M132), not a blocker; the wall streams correctly today via N per-fleet connections, this collapses them to one.
**Categories:** API, UI
**Batch:** B1
**Branch:** feat/m133-workspace-stream
**Test Baseline:** unit=2642 integration=334
**Depends on:** M132_001 (ships the Live Wall whose N-per-tile stream mode this replaces and deletes)
**Provenance:** Large Language Model (LLM)-drafted (claude-opus-4-8, Jul 14, 2026) — authored from the frozen variant-F design (`designs/fleet-dashboard-20260714/FREEZE.md` §2 G4, §3, §5) and its route/handler/schema vetting matrix.
**Canonical architecture:** `docs/architecture/data_flow.md` §"Two streams + one pub/sub channel", §"Connection topology"; `docs/architecture/concurrency.md` §"Channel inventory"

---

## Overview

**Goal (testable):** One Server-Sent Events (SSE) connection — `GET /v1/workspaces/{ws}/events/stream` — fans in every readable fleet's `fleet:{id}:activity` pub/sub channel for that workspace, each frame tagged with its originating `fleet_id`, respecting workspace authorization (a subscriber never receives a frame from a fleet it cannot read), the instance stream cap, per-fleet backpressure, and reconnect gap-backfill (`since` floor + `next_cursor` follow); the Live Wall consumes this one stream and the N-per-tile stream mode is deleted from the client with zero surviving references.

**Problem:** Streams are strictly per-fleet today (`fleets/{id}/events/stream`). The Live Wall (M132, per Indy's "stream everything, no cap" override) opens one SSE connection per live tile — a wall of L live fleets viewed by V viewers costs L×V connections and L×V registry slots, each a separate `EventSource`. On HTTP/1.1 (dev) it exhausts the per-origin connection budget and half the tiles degrade to snapshot fallback; on HTTP/2 it multiplexes but still burns one stream-registry slot per tile per viewer against `sse_max_streams`. There is no workspace-multiplexed stream to collapse them into.

**Solution summary:** A new multiplexed SSE handler enumerates the fleets in the authorized workspace, subscribes to each `fleet:{id}:activity` channel through the process's existing SubscriptionHub (refcounted, one shared Redis connection — no new dials), and multiplexes their frames into one connection, splicing the originating `fleet_id` into each frame so the client demultiplexes to the right tile. It claims one stream-registry slot for the whole connection. The wall swaps its N `EventSource`s for one, and the per-tile stream registry usage is deleted. The per-fleet route survives unchanged — the Fleet Console (M131) keeps its single-fleet thread. An optional server-side 7-day rollup endpoint is scoped but built only if client paging cost is shown to hurt.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m133): one workspace stream replaces the wall's per-tile SSE fan-out
- **Intent (one sentence):** The Live Wall watches an entire workspace over a single authorized SSE connection instead of one per live tile, and the old N-connection mode is gone.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/fleets/events_stream.zig` — the per-fleet SSE handler this multiplexes: the registry-slot claim (line ~93, `sse_max_streams`), the detached-thread handoff (`startStreamThread`/`streamThreadMain`), the `StreamJob` single-owner lifecycle, `streamLoop`'s pop→writeFrame→heartbeat cadence, and `extractKind`'s anchored-prefix parse. Mirror this shape; the multiplexed job owns a *set* of subscriptions, not one.
2. `src/agentsfleetd/events/subscription_hub.zig` + `subscription.zig` — `subscribe`/`unsubscribe` are refcounted per channel on ONE shared connection; M channels cost M map entries, never M dials. Read the two-lock model first. This spec extends the hub ADDITIVELY (shared-consumer variant, §1); behavioral changes to the existing per-fleet paths are out of scope.
3. `src/agentsfleetd/http/handlers/workspaces/events.zig` — the workspace-scoped event LIST already exists (`authorizeWorkspaceAndSetTenantContext`, RLS-scoped `listForWorkspace`); reuse its authorization pattern and it is the backfill source the client pages on reconnect.
4. `ui/packages/app/lib/streaming/fleet-stream-registry.ts` — the per-fleet client stream registry (per-`EventSource` reconnect/backfill/optimistic model); the wall's new single-stream client mirrors its reconnect+backfill discipline against `GET /v1/workspaces/{ws}/events`.
5. `docs/architecture/data_flow.md` §"Two streams + one pub/sub channel" and §"Connection topology" — the pub/sub loss semantics, refcounted SUBSCRIBE, and client gap-recovery (M122) this stream inherits verbatim.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/workspaces/events_stream.zig` | CREATE | The multiplexed SSE handler: authorize workspace → enumerate readable fleet ids inline from `core.fleets` (mirrors `fleets/list.zig:176`; no store helper) → subscribe each channel via the hub → multiplex, tagging each frame with `fleet_id`. |
| `src/agentsfleetd/http/routes.zig` | EDIT | New route variant `workspace_events_stream` (sibling of `workspace_events`, routes.zig:96). |
| `src/agentsfleetd/http/router.zig` | EDIT | Match `GET /v1/workspaces/{ws}/events/stream` (deepest-shape-first, beside the `workspace_events` suffix match at router.zig:145). |
| `src/agentsfleetd/http/route_matchers.zig` | EDIT (conditional) | ONLY if needed: `matchWorkspaceSuffix` is 3-segment; `events/stream` is 4 segments, so a two-segment-suffix matcher is added unless an existing shape fits. |
| `src/agentsfleetd/http/route_scopes.zig` | EDIT | Scope row: `workspace_events_stream` → `FLEET_READ` (beside `workspace_events`, route_scopes.zig:152). |
| `src/agentsfleetd/http/route_table.zig` | EDIT | `classFor` → `.stream`; middleware + invoke row (route_table.zig:87, 214 pattern). |
| `src/agentsfleetd/http/route_table_invoke_events.zig` | EDIT | Register `invokeWorkspaceEventsStream` alongside the existing fleet/workspace event invokes. |
| `src/agentsfleetd/http/route_scopes_test.zig` | EDIT | Exhaustive scope test gains the new variant. |
| `src/agentsfleetd/http/router_test.zig` | EDIT | Exhaustive route-match test gains the new path. |
| `src/agentsfleetd/events/subscription_hub.zig` | EDIT | ADDITIVE shared-consumer variant: N channel attachments feed ONE consumer queue/epoch (§1); per-fleet paths untouched. Plus refcounted subscriber handles so the reader can fan out with the map mutex released (§6). |
| `src/agentsfleetd/events/subscription.zig` | EDIT | Shared (tagged) consumer + `ref`/`unref` ownership; the lines 41-44 budget comment updated — the fan-in shares ONE ring, so the bound is per-connection (§3, §6). |
| `src/agentsfleetd/events/subscription_hub_reader.zig` | EDIT | *(Amended at PLAN)* Tagged dispatch for shared consumers; snapshot-under-lock → push-unlocked, with a reader-confined reusable scratch buffer (§6). |
| `src/agentsfleetd/events/subscription_hub_test.zig` | EDIT | Shared-consumer fan-in, wake, teardown, refcount-during-dispatch, and lock-discipline tests. |
| `src/agentsfleetd/events/activity_channel.zig` | CREATE | *(Amended at PLAN — RULE UFS/NDC)* The `fleet:{id}:activity` name formatted and parsed in ONE place; both SSE handlers share it instead of duplicating the literals. |
| `src/agentsfleetd/events/fleet_set_cache.zig` | CREATE | *(§6)* Process-wide refcounted per-workspace fleet-set cache: ONE RLS-scoped enumeration per workspace per cadence, versioned so an unchanged tick is a no-op for every viewer. |
| `src/agentsfleetd/events/fleet_set_cache_test.zig` | CREATE | *(§6)* Enumerate-once-for-many-viewers, version no-op tick, refcounted eviction, allocation-failure unwind. |
| `src/agentsfleetd/http/handlers/sse_frame.zig` | CREATE | *(Amended at PLAN — RULE NDC)* SSE frame writing shared by both handlers: `extractKind`, `writeFrame`, and the multiplexed `writeTaggedFrame` (fleet-id splice), lifted verbatim from the per-fleet handler so one tested copy exists. |
| `src/agentsfleetd/http/handlers/workspaces/events_stream_fanin.zig` | CREATE | *(§1, §6)* The `FanIn` type: the owned principal copy the re-authorization tick uses, the attached channel set, and the diff against the shared fleet-set cache. |
| `src/agentsfleetd/http/handlers/workspaces/workspace_stream_soak_test.zig` | CREATE | *(§7)* Chaos/soak: ≥100 concurrent streams, Redis killed mid-stream, fleet churn under load, RSS bound on the production allocator, bounded shutdown under load. |
| `src/agentsfleetd/http/handlers/workspaces/workspace_events_stream_integration_test.zig` | CREATE | Real-stack SSE integration: authorization boundary, cap, backpressure, reconnect, fleet-appears/disappears, Redis-down. |
| `src/agentsfleetd/http/handlers/workspaces/workspace_stream_test_fixtures.zig` | CREATE | Fixture rows (two workspaces, fleets in each) for the cross-workspace isolation and fan-in tests (RULE ITF). *(Amended at PLAN: fixtures live sibling to the integration suite per `dispatch/write_zig.md` §HTTP Integration Tests, not `src/db/`.)* |
| `src/agentsfleetd/http/handlers/fleets/events_stream.zig` | EDIT (non-behavioral) | *(Amended at PLAN, RULE UFS)* its file-local `fleet:`/`:activity` literals become references to the hub's new shared `pub` channel-name constants; behavior untouched (Invariant 5 regression suite pins it). |
| `docs/REST_API_DESIGN_GUIDELINES.md` | EDIT | §7 route-registration facts: add the new path so `check-route-registration-doc` stays green. |
| `public/openapi/paths/workspaces.yaml` + `public/openapi.json` | EDIT | *(Amended at PLAN)* §7 step 6: document `GET /v1/workspaces/{ws}/events/stream`; regenerated bundle — the served-vs-documented route-coverage gate blocks otherwise. |
| `ui/packages/app/lib/api/events.ts` | EDIT | *(Amended at PLAN)* `streamWorkspaceEventsUrl` / `backfillWorkspaceEventsUrl` helpers beside the per-fleet ones. |
| `ui/packages/app/app/backend/v1/workspaces/[workspaceId]/events/route.ts` | CREATE | *(Amended at PLAN)* same-origin proxy for the workspace events LIST — the reconnect backfill source; did not exist. |
| `ui/packages/app/lib/streaming/workspace-stream.ts` | CREATE | One-connection-per-workspace client: opens the multiplexed `EventSource`, demultiplexes by `fleet_id`, reconnect + workspace-scoped backfill. |
| `ui/packages/app/lib/streaming/workspace-stream.test.ts` | CREATE | Demultiplex-by-fleet, reconnect/backfill, malformed-frame-drop tests. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetsList.tsx` | EDIT | The wall subscribes ONCE to the workspace stream and routes frames to tiles; the per-tile stream wiring is removed. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.tsx` | EDIT | The tile (M132-created) stops opening its own `EventSource` and takes frames from the workspace stream. |
| `ui/packages/app/app/backend/v1/workspaces/[workspaceId]/events/stream/route.ts` | CREATE | Same-origin token-minting proxy for the multiplexed stream (mirror of the per-fleet SSE proxy). |
| `ui/packages/app/tests/dashboard-fleets-wall.test.ts` | EDIT | Wall opens exactly one stream; tiles update from tagged frames; the N-stream path is asserted gone. |

<!-- G3-v2 rollup endpoint files are intentionally NOT listed here — that Section
is optional (§5) and its blast radius is added only if its trigger fires. -->

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NDC** + **ORP** (the wall's N-per-tile stream mode is DELETED, not stranded — every removed symbol swept, see Dead Code Sweep), **NLR** (touch-it-fix-it on the wall client), **STR** (streaming test must verify the transport frames over the wire, not just a parser), **KYS** (reconnect backfill is a `since` floor + `next_cursor` follow against the workspace events list's composite `(created_at, event_id)` keyset — the client never fabricates a cursor from an event id), **ECL** (Redis-down / hub-stopped is a distinct transient class from client disconnect — 503 vs quiet-close), **HLP** (the fleet enumeration is an inline handler query mirroring `fleets/list.zig:176` — no orphan store helper), **OBS** (cap rejection, Redis-unavailable, and fleet-set changes each get a log/metric), **ITF** (integration tests use real schema fixtures), **TSC/TSJ** (every `.ts` touched), **FLS** (the fleet-id enumeration query drains before `deinit`), **FLL** (handler ≤350 / fn ≤50).
- **`dispatch/write_zig.md`** — ZIG/PUB/LIFECYCLE: tagged-union results, `errdefer` on the multi-subscription job, cross-compile both linux targets, `conn.query().drain()` before `deinit()`.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** §7 — new streaming route registration facts kept fresh.
- **`dispatch/write_ts_adhere_bun.md`** — UI Substitution / const discipline on the wall client.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — new `.zig` handler + store edit | `zig build -Dtarget=x86_64-linux && -Dtarget=aarch64-linux`; `make check-pg-drain` for the fleet-id query. |
| PUB / Struct-Shape | yes — new handler pub surface + the multiplexed job struct | Shape verdict: `innerWorkspaceEventsStream` mirrors the per-fleet `innerEventsStream` signature; the job is a single-owner struct like `StreamJob`. |
| File & Function Length (≤350/≤50/≤70) | yes | The handler stays under cap by splitting the fan-in-set management into helpers, mirroring `events_stream.zig`'s function split. |
| UFS (repeated/semantic literals) | yes | Channel prefix/suffix, the `fleet_id` splice key, the re-enumeration cadence, and the registry-entry fleet-id sentinel (§3) are named constants; the `fleet:`/`:activity` literals are shared verbatim with the per-fleet handler's existing constants. |
| UI Substitution / DESIGN TOKEN | no | No new visual surface; the wall's tiles already exist (M132). Client swaps the data source only. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes (LOGGING, LIFECYCLE, ERROR) / no (SCHEMA) | Reuse `ERR_SSE_STREAM_CAP`, `ERR_FORBIDDEN`, `ERR_INVALID_REQUEST`; LIFECYCLE on the multi-subscription job teardown; no migration (read-only fan-in over existing tables). |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/http/handlers/fleets/events_stream.zig` — the per-fleet SSE handler; the multiplexed handler is its generalization from one channel to a workspace-scoped set. Divergence: the job owns N subscriptions and a re-enumeration cadence; the slot claim is one-per-connection, not one-per-fleet.
- **Reference:** `src/agentsfleetd/http/handlers/workspaces/events.zig` — the authorization + RLS scoping pattern for a workspace path, and the backfill list the client pages.
- **Reference:** `ui/packages/app/lib/streaming/fleet-stream-registry.ts` — the client reconnect/backfill/idle-release model the single-stream client mirrors.

## Sections (implementation slices)

### §1 — The multiplexed handler (fan-in, not pattern-subscribe)

`GET /v1/workspaces/{ws}/events/stream` authorizes the workspace, enumerates the fleet ids the caller can read, and subscribes to each `fleet:{id}:activity` through the hub, multiplexing into one SSE connection.

**Implementation default:** fleet enumeration reads **`core.fleets`** (existence, NOT event history) — a fleet with no events yet must be subscribed BEFORE its first event or that event's frames are systematically missed (`data_flow.md` §C: the first PUBLISH follows the first insert by milliseconds). The query is inline in the handler, mirroring `fleets/list.zig:176`'s `core.fleets` query; `fleet_events_store.zig` is the wrong home (it scopes event rows, not fleet existence).

**Implementation default (multi-subscription wait):** `Subscription.pop(timeout)` is a single-queue futex wait; no N-queue wait primitive exists. The hub gains an ADDITIVE shared-consumer variant: N channel attachments feed ONE consumer queue/epoch, so the stream thread does one futex wait for the whole fan-in and the heartbeat cadence falls out of `pop`'s timeout exactly as in the per-fleet loop. **Rejected alternative:** a `pop(0)` sweep across N subscriptions on a named poll cadence — trades latency and idle CPU for zero hub change, and the heartbeat cadence no longer falls out naturally. Existing per-fleet behavior is untouched.

**Implementation default:** **fan-in over per-fleet channels, NOT Redis `PSUBSCRIBE fleet:*:activity`** — a pattern subscribe is process-global and crosses every tenant's channels, so it would require post-hoc filtering of another workspace's frames off a shared firehose (a security boundary enforced by discipline, not construction — rejected). Fan-in subscribes only to the authorized fleet set, so a frame the caller may not see is never delivered to this connection at all, and it reuses the hub's proven refcounted one-connection model unchanged.

**Implementation default:** each multiplexed frame carries its originating `fleet_id` (the activity payload does not include it — `data_flow.md` §EXECUTE shows `{kind, event_id, actor}`); splice it in cheaply from the channel name the frame arrived on, mirroring `extractKind`'s anchored-prefix approach rather than a full JSON re-parse. **Ordering matters:** extract `kind` from the ORIGINAL payload first, THEN splice `fleet_id` — `extractKind` anchors on the leading field, and a splice-first frame would break that anchor.

- **Dimension 1.1** — an authorized subscriber receives multiplexed frames from every readable fleet in the workspace, each tagged with the correct `fleet_id` → Test `test_workspace_stream_fans_in_all_readable_fleets`
- **Dimension 1.2** — the fan-in subscribes per-fleet channel via the hub, never `PSUBSCRIBE` (no cross-tenant firehose) → Test `test_workspace_stream_uses_scoped_fanin_not_pattern`
- **Dimension 1.3** — the whole connection claims exactly one stream-registry slot regardless of fleet count → Test `test_workspace_stream_claims_single_registry_slot`

### §2 — The authorization boundary (a subscriber sees only its workspace)

Workspace scoping is a security boundary. The fleet-id enumeration runs under the same tenant context / Row-Level-Security (RLS) as the workspace events list; a subscriber can never receive a frame from a fleet outside a workspace it can read.

- **Dimension 2.1** — a caller authorized for workspace A never receives any frame published on a workspace-B fleet's channel, even under concurrent publishes → Test `test_workspace_stream_never_leaks_other_workspace_frames`
- **Dimension 2.2** — a caller with no read access to the path workspace gets `ERR_FORBIDDEN` before any subscribe → Test `test_workspace_stream_forbidden_for_unauthorized_workspace`
- **Dimension 2.3** — membership revoked mid-stream: each re-enumeration tick re-runs the RLS-scoped query under fresh authorization, so a caller who loses access is unsubscribed from the lost fleets by the next tick → Test `test_workspace_stream_membership_revocation_unsubscribes`

### §3 — Cap, backpressure, reconnect, and a changing fleet set

The connection respects `sse_max_streams` (one slot; the registry entry carries a named sentinel constant as its fleet id — a workspace stream has no single fleet), inherits the per-attachment bounded-ring backpressure (oldest-frame-drop), heartbeats to detect a vanished client, resets its per-connection sequence to 0 on connect, ignores `Last-Event-ID`, and adjusts its subscription set as fleets appear or disappear.

**Implementation default:** **fleet-set refresh by periodic re-enumeration** diffed against the subscribed set (subscribe the new, unsubscribe the gone) on a named cadence — simplest, reuses the hub's add/remove; a workspace-level control channel is the heavier alternative rejected until a latency need is shown. **Amended at PLAN (§6):** the enumeration itself is SHARED per workspace, not per connection — see §6.

**Memory bound (stated, not inherited):** `subscription.zig:41-44` budgets 64 frames ≈ 64 KiB per stalled consumer, "bounded overall by the SSE stream cap" — fan-in silently turns that into cap × fleets × 64 KiB (e.g. 200 streams × 64 fleets ≈ 800 MiB worst case). **Resolved by construction:** the fan-in shares ONE consumer ring across every attached channel (the shared-consumer variant, §1), so the budget is per-CONNECTION — `sse_max_streams × QUEUE_CAPACITY` — and is fleet-count-independent. The `subscription.zig` budget comment is updated in the same diff and `test "subscription: the shared queue's memory bound is per-consumer, not per-channel"` pins it.

- **Dimension 3.1** — at cap (or draining) the connection is refused with `ERR_SSE_STREAM_CAP` + Retry-After, before any subscribe → Test `test_workspace_stream_refused_at_cap`
- **Dimension 3.2** — a slow consumer drops oldest frames per-fleet (bounded ring) and the connection survives; a fully vanished client is torn down at the next heartbeat → Test `test_workspace_stream_slow_consumer_drops_oldest`
- **Dimension 3.3** — a fleet created mid-stream is picked up on the next re-enumeration and its frames begin arriving; a fleet deleted mid-stream is unsubscribed with no error to the client → Test `test_workspace_stream_picks_up_and_drops_fleets_midstream`
- **Dimension 3.4** — reconnect resumes from sequence 0; `Last-Event-ID` request headers are ignored; the client recovers the gap with a `since` floor (last confirmed `created_at` minus overlap) then follows `next_cursor` pages, merging by event id — the discipline `fleet-stream-backfill.ts` already implements (a bare event id is NOT a cursor: `parseCursor` decodes the composite `(created_at, event_id)` keyset and rejects anything else as 400 `InvalidCursor`) → Test `test_workspace_stream_reconnect_resets_seq_client_backfills`
- **Dimension 3.5** — hub or Redis unavailable at connect time is refused with a transient 503 (`internalDbUnavailable`, the per-fleet `startStreamThread` shape), never a hang or a half-open stream → Test `test_workspace_stream_503_when_hub_unavailable`

### §4 — The wall consumes one stream; the N-connection mode dies

The Live Wall subscribes once to the workspace stream and routes each tagged frame to its tile. The per-tile `EventSource` fan-out M132 shipped is DELETED — no dormant N-stream code path survives (RULE NDC/ORP). The Redis-unavailable / hub-stopped path degrades the whole wall to the snapshot the tiles already render, never a crash.

- **Dimension 4.1** — the wall opens exactly one `EventSource` for the workspace; tiles update from `fleet_id`-tagged frames → Test `test_wall_opens_single_workspace_stream`
- **Dimension 4.2** — the workspace stream unavailable (503 / Redis down) leaves every tile on its last-event snapshot, no dead tiles, one reconnect loop → Test `test_wall_degrades_to_snapshot_when_stream_unavailable`
- **Dimension 4.3** — a malformed or untagged frame is dropped, never routed to a wrong tile or crashing the wall → Test `test_workspace_stream_drops_malformed_frame`
- **Dimension 4.4** *(added at PLAN — Indy, Jul 15, 2026: "make the user experience better")* — a burst of frames across many tiles coalesces into ONE render per animation frame, not one render per frame; a 60-fleet burst must not cost 60 React renders → Test `test_wall_coalesces_burst_into_one_render`
- **Dimension 4.5** *(added at PLAN)* — the stream's first frame carries the fanned-in fleet set, so a quiet tile renders as *quiet* rather than indistinguishable from a dead one; a tile absent from the live set shows its snapshot eyebrow → Test `test_wall_marks_tiles_live_from_hello_frame`
- **Dimension 4.6** *(added at PLAN)* — when the server drops frames against a stalled consumer, the wall surfaces a "catching up" state rather than silently showing stale tiles → Test `test_wall_surfaces_catching_up_on_drops`

### §6 — The fan-in must not reintroduce per-viewer cost *(added at PLAN — Indy, Jul 15, 2026: "code optimized performance, concurrency … robust stability")*

This milestone exists to delete the wall's per-viewer L×V connection cost. Two shapes in the naive fan-in would quietly reintroduce per-viewer cost, so they are fixed here rather than filed as follow-ups.

**Shared fleet-set, per-caller authorization.** A per-connection re-enumeration costs V full `core.fleets` scans per workspace per tick — trading V connections for V queries. The enumeration is workspace-scoped and identical for every viewer, so it is SHARED: a refcounted process-wide `FleetSetCache` runs ONE RLS-scoped enumeration per workspace per cadence and bumps a `version` only when the set actually changes; a connection whose version matches does no query, no diff, and no allocation on its tick. **Authorization stays per-caller and is never shared** — each tick still re-checks *this* principal's workspace read with one indexed point lookup, because a revoked member must lose frames even when the workspace's fleet set has not changed (Dimension 2.3 holds by construction, not by cache).

**Fan-out outside the map mutex.** `subscription_hub_reader.dispatch` currently calls `sub.push()` — a `malloc`, a nested lock, and a `futexWake` per subscriber — while holding `hub.mutex`, the lock every `subscribe`/`unsubscribe`/`channelCount` needs. A busy wall makes that critical section O(viewers) of syscalls and stalls every stream open on the process. Fixed: the reader snapshots the subscriber pointers under the lock into a **reader-thread-confined scratch buffer** (C5; reused across frames, so zero per-frame allocation after warmup), releases the lock, and pushes unlocked. Pushing outside the lock means a concurrent `unsubscribe` could free a `Subscription` mid-push, so the snapshot is lifetime-guarded by **refcounting the `Subscription`** (`ref`/`unref` — the ownership shape `dispatch/write_zig.md` prescribes): the stream thread holds one ref for its lifetime, the reader takes a ref per snapshot and drops it after the push, and `unsubscribe` removes from the map then unrefs. Last unref destroys — a subscription can never be freed while a push that already snapshotted it is in flight.

- **Dimension 6.1** — V concurrent streams on one workspace cost ONE fleet enumeration per cadence, not V → Test `test_fleet_set_cache_enumerates_once_for_many_viewers`
- **Dimension 6.2** — a connection whose cached version is unchanged does no query and no re-subscribe work on its tick → Test `test_fleet_set_cache_version_unchanged_is_a_no_op_tick`
- **Dimension 6.3** — authorization is NOT shared: a caller revoked mid-stream is torn down on its next tick even though the workspace's fleet set never changed → Test `test_workspace_stream_revocation_is_per_caller_not_cached`
- **Dimension 6.4** — the hub reader holds `hub.mutex` only for the map lookup + snapshot; `subscribe`/`unsubscribe` proceed while a fan-out push is in flight → Test `test_dispatch_fanout_does_not_block_subscribe`
- **Dimension 6.5** — a `Subscription` unsubscribed concurrently with an in-flight fan-out push is not freed until the push completes (refcount) → Test `test_subscription_refcount_survives_unsubscribe_during_dispatch`

### §7 — Chaos and soak: the stability the fan-in claims *(added at PLAN — Indy, Jul 15, 2026: "concurrency tested with robust stability")*

The mandated bar (`/write-integration-test`: zero leaks including error paths, ≥100 concurrent connections proving no hidden global lock, a latency budget under load) is the floor, not the ceiling. This Section adds the failure-injection the fan-in's claims actually rest on.

- **Dimension 7.1** — ≥100 concurrent workspace streams across multiple workspaces sustain publishes with no leak, no wedge, and p95 publish→receive inside the existing 200 ms budget (no hidden global lock) → Test `test_workspace_streams_at_scale_no_global_lock`
- **Dimension 7.2** — Redis killed mid-stream: every live workspace stream survives on heartbeats, the hub redials, and delivery resumes on the SAME connections with no client reconnect → Test `test_workspace_streams_survive_redis_kill_midstream`
- **Dimension 7.3** — fleets churning in and out under load (create/delete while frames flow) leaves the attached channel set exactly equal to the readable fleet set, with no orphaned wire subscription → Test `test_fleet_churn_under_load_leaves_no_orphan_subscription`
- **Dimension 7.4** — process RSS over a long churn soak (streams opening/closing, fleets appearing/disappearing) stays bounded on the PRODUCTION allocator — the leak layer `testing.allocator` cannot see → Test `test_workspace_stream_churn_rss_bounded`
- **Dimension 7.5** — shutdown under full load: `drain → hub.stop → awaitEmpty` returns bounded with 100 live workspace streams; no stream thread wedges and nothing is freed under a live thread → Test `test_shutdown_bounded_under_live_workspace_streams`

### §5 — Optional: server-side 7-day rollup (G3-v2) — build ONLY if paging hurts

M131 aggregates the 7-day rollup client-side, PER FLEET, over `GET …/events?since=7d` pages. This Section adds a server rollup endpoint **only if** that aggregation is shown to exceed a stated budget — and to actually replace it, the endpoint accepts a `fleet_id` filter (mirroring the workspace events list's filter) so a single fleet's rollup is one call.

**Trigger condition (must be met before building):** the client-side rollup for a realistic fleet's 7-day window requires more than the workspace events list's max page (`LIMIT_MAX=200`) times a small constant of round-trips at p95 — i.e. measured paging cost on real data exceeds the budget recorded at PLAN. The budget is a named constant, named at PLAN. If the trigger is not met, this Section ships as `DEFERRED` with the measurement in Discovery; it is not mandated.

- **Dimension 5.1** *(conditional)* — the server rollup, filtered to one fleet, returns wakes · tokens · server-truth spend (`credit_deducted_nanos`) · failed count over a 7-day window in one call and equals M131's per-fleet client aggregate → Test `test_workspace_rollup_matches_client_aggregation`

## Interfaces

```
GET /v1/workspaces/{ws}/events/stream            (NEW — multiplexed SSE)
  Auth: Bearer (CLI/programmatic) + cookie (dashboard proxy), workspace-scoped.
  200 text/event-stream; frames:
    id: <per-connection seq, resets to 0 on connect>
    event: <kind>                                 (event_received | tool_call_* | event_complete | ...)
    data: {"fleet_id":"<uuid>", ...original activity payload...}
  Refusal: ERR_SSE_STREAM_CAP (429/503 + Retry-After) at cap/drain.
  ERR_FORBIDDEN when the caller cannot read {ws}. Last-Event-ID ignored;
  reconnect recovery: GET /v1/workspaces/{ws}/events?since=<floor> then follow
  next_cursor pages, merged by event id (fleet-stream-backfill.ts discipline).
  The cursor is the server-issued composite (created_at, event_id) keyset —
  a bare event id is a 400 InvalidCursor (parseCursor).

GET /v1/workspaces/{ws}/fleets/{id}/events/stream  (UNCHANGED — survives)
  The Fleet Console (M131) keeps this single-fleet thread. This spec does
  not touch its handler; only the WALL's use of N such streams is deleted.

GET /v1/workspaces/{ws}/events/rollup?window=7d&fleet_id=<id>  (§5 — OPTIONAL)
  { wakes, tokens, spend_nanos (credit_deducted_nanos), failed } — built only
  if §5's trigger fires; fleet_id filter mirrors the workspace events list so
  it can replace M131's per-fleet client aggregation; server-truth cost,
  never client token×rate math.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unauthorized workspace | Caller lacks read on `{ws}` | `ERR_FORBIDDEN` before any subscribe; no slot retained (the handler claims-then-releases like the per-fleet one — deliberate, so a tab-storm of unauthorized retries still pays the cap check first), no frame delivered. |
| Membership revoked mid-stream | Caller loses workspace read after connect | Each re-enumeration tick re-runs the RLS-scoped query under fresh authorization; lost fleets drop out of the result and are unsubscribed by the next tick. |
| Cross-workspace frame | Concurrent publish on another tenant's fleet | Never delivered — only the authorized fleet set is subscribed (construction, not filtering). |
| Stream refused / capped | `sse_max_streams` reached or shutdown drain | `ERR_SSE_STREAM_CAP` + Retry-After; slot never half-claimed; metric incremented. |
| Redis / hub unavailable | Pub/sub connection down or hub stopped | Transient 503 on connect (ECL: distinct from client disconnect); mid-stream loss follows documented pub/sub semantics — queue quiets, heartbeats hold, client backfills on reconnect. |
| Reconnect / resume | Client drops and reopens | Per-connection seq resets to 0; `Last-Event-ID` ignored; client re-primes a `since` floor and follows `next_cursor` pages, merging by event id (`fleet-stream-backfill.ts` discipline) — never a fabricated `cursor=<event_id>`, which `parseCursor` rejects as 400 `InvalidCursor`. |
| Fleet appears mid-stream | Fleet created after connect | Picked up on next re-enumeration; frames begin arriving; no reconnect needed. |
| Fleet disappears mid-stream | Fleet deleted after connect | Unsubscribed on next re-enumeration; no error surfaced to the client. |
| Slow consumer / backpressure | Client reads slower than publishes | Per-fleet bounded ring drops oldest (drop counter bumped); connection survives; a truly vanished client is torn down at the heartbeat write failure. |
| Malformed / untagged frame | Publisher shape drift | Dropped; never routed to a wrong tile; the wall does not crash. |

## Invariants

1. **Workspace isolation is enforced by construction** — the connection subscribes only to channels of fleets the caller can read (RLS-scoped enumeration); no frame from another workspace can enter the multiplex. Proven by `test_workspace_stream_never_leaks_other_workspace_frames`.
2. **One connection, one registry slot** — a workspace stream claims exactly one `sse_max_streams` slot regardless of fanned-in fleet count. Runtime-checked; proven by `test_workspace_stream_claims_single_registry_slot`.
3. **No pattern-subscribe** — impossible by construction: the subscriber client has no `PSUBSCRIBE` send and `redis_subscriber.zig` ignores `pmessage` shapes; the test proves the scoped fan-in mechanically (`channelCount()` rises by N).
4. **Cost is server truth** — any rollup (§5) reports `credit_deducted_nanos`, never client token×rate math. Enforced by the rollup test comparing against the charges source.
5. **The per-fleet stream is untouched** — the console's single-fleet route keeps working; the diff does not modify `fleets/events_stream.zig` behavior. Enforced by the unchanged per-fleet integration tests staying green.
6. **Single-owner job lifecycle** — the multi-subscription job is created on the request thread and destroyed exactly once by the stream thread (or the spawn-failure path), unsubscribing every channel; `errdefer`-enforced, memleak-proven.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `sse_backpressure_rejections` (existing) | ops | Workspace stream refused at cap | live count, max | no payload/identity | `test_workspace_stream_refused_at_cap` |
| workspace-stream open/close log | ops | Connection opens/tears down | workspace_id, fanned-in fleet count, drop total | no frame contents, no token | `test_workspace_stream_fans_in_all_readable_fleets` |
| fleet-set change log | ops | Re-enumeration adds/removes fleets | workspace_id, added/removed counts | fleet ids only, no payload | `test_workspace_stream_picks_up_and_drops_fleets_midstream` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_workspace_stream_fans_in_all_readable_fleets` | 3 fleets publish; one connection receives all 3, each frame carries its own `fleet_id`. |
| 1.2 | integration | `test_workspace_stream_uses_scoped_fanin_not_pattern` | hub `channelCount()` rises by N (one per readable fleet); `PSUBSCRIBE` is impossible by construction — the subscriber client exposes no `PSUBSCRIBE` send and `redis_subscriber.zig` treats `pmessage` as ignored. |
| 1.3 | integration | `test_workspace_stream_claims_single_registry_slot` | workspace of N fleets → registry `count()` rises by 1, not N. |
| 2.1 | integration | `test_workspace_stream_never_leaks_other_workspace_frames` | publish on a workspace-B fleet while subscribed to A → A connection receives 0 B-frames. |
| 2.2 | integration | `test_workspace_stream_forbidden_for_unauthorized_workspace` | caller without read on `{ws}` → `ERR_FORBIDDEN`, no slot retained. |
| 2.3 | integration | `test_workspace_stream_membership_revocation_unsubscribes` | revoke the caller's workspace read mid-stream → next re-enumeration tick unsubscribes; no further frames delivered. |
| 3.1 | integration | `test_workspace_stream_refused_at_cap` | registry at `sse_max_streams` → `ERR_SSE_STREAM_CAP` + Retry-After. |
| 3.2 | integration | `test_workspace_stream_slow_consumer_drops_oldest` | flood one fleet past ring cap → oldest dropped, drop counter up, connection alive. |
| 3.3 | integration | `test_workspace_stream_picks_up_and_drops_fleets_midstream` | create a fleet post-connect → its frames arrive after re-enumeration; delete one → its frames stop, no error. |
| 3.4 | integration | `test_workspace_stream_reconnect_resets_seq_client_backfills` | reopen → `id:` restarts at 0; `Last-Event-ID` header ignored; client issues a `since`-floor backfill and follows `next_cursor`, merged by event id. |
| 3.5 | integration | `test_workspace_stream_503_when_hub_unavailable` | hub stopped / Redis down at connect → transient 503 (`internalDbUnavailable`), no slot retained, no half-open stream. |
| 4.1 | e2e | `test_wall_opens_single_workspace_stream` | rendered wall opens exactly 1 `EventSource`; tagged frame updates the matching tile. |
| 4.2 | unit | `test_wall_degrades_to_snapshot_when_stream_unavailable` | stream 503 → every tile shows last snapshot, one reconnect loop, no dead tile. |
| 4.3 | unit | `test_workspace_stream_drops_malformed_frame` | untagged/garbled frame → dropped, no tile mutated, no throw. |
| 5.1 | integration | `test_workspace_rollup_matches_client_aggregation` *(conditional on §5 trigger)* | server rollup with `fleet_id=<id>` equals M131's per-fleet client-side 7-day aggregate for that fleet; spend from `credit_deducted_nanos`. |
| regression | integration | per-fleet stream suite unchanged | `fleets/events_stream` integration tests stay green — console thread not regressed. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Workspace isolation holds — isolation, forbidden, revocation covered (§2) | `grep -cE "never delivers a frame from a fleet outside\|forbidden for a workspace the caller cannot read\|membership revoked mid-stream" src/agentsfleetd/http/handlers/workspaces/workspace_events_stream_integration_test.zig` | `3` (pass/fail via S3) | P0 | |
| R2 | Fan-in + single slot + no pattern covered (§1) | `grep -cE "fans in every readable fleet\|claims exactly one registry slot\|one wire channel per fleet .scoped" src/agentsfleetd/http/handlers/workspaces/workspace_events_stream_integration_test.zig` | `3` (pass/fail via S3) | P0 | |
| R3 | Cap / reconnect / changing set / hub-down covered (§3) | `grep -cE "refused at the SSE cap\|created mid-stream is picked up\|reconnect restarts the per-connection\|refused with a transient error when the hub" src/agentsfleetd/http/handlers/workspaces/workspace_events_stream_integration_test.zig` | `4` (pass/fail via S3) | P0 | |
| R4 | Wall opens one stream, N-mode gone (§4) | `cd ui/packages/app && bunx vitest run tests/dashboard-fleets-wall.test.ts` | exit 0 | P0 | |
| R5 | Per-fleet console stream not regressed | `make test-integration 2>&1 \| grep -c "fleet_events_stream.*passed"` | ≥1, 0 failed | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean (incl. route-registration doc) | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes (HTTP/Redis/schema touched) | `make test-integration` | exit 0 | P0 | |
| S4 | e2e walks the wall path | `make acceptance-e2e` | exit 0 | P0 | |
| S5 | No leaks (multi-subscription job wiring) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -vE '\.md$\|_test\.(zig\|ts\|tsx)$\|\.test\.(ts\|tsx)$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep (N-stream mode gone) | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. R4's `bunx vitest` run is focused evidence only — package-scoped runners are not verification; S1's `make test-unit-all` is the gate. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| The wall's per-tile stream module M132 shipped for the N-connection mode (name confirmed against the M132 branch at CHORE(open); if the per-tile fan-out lives inside `FleetsList.tsx` it is deleted in-file, not as a file) | `test ! -f <the M132 per-tile stream module>` OR documented in-file removal |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| the wall's per-tile `EventSource`/registry fan-out (M132 symbol) | `git grep -nE "perTile\|perFleetTileStream\|N-stream" -- "ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets"` | 0 matches |
| the tile's own `EventSource`/registry use (M132-created; console keeps its own) | `git grep -n "fleet-stream-registry\|EventSource" -- "ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.tsx"` | 0 matches (the tile takes frames from the workspace stream) |

<!-- The exact M132 symbol names are resolvable only against the M132 branch this
depends on; the executing agent pins them at CHORE(open) from that diff and
completes both grep rows with the real identifiers. -->

## Out of Scope

- Cron schedule create/update/delete — that is **M105_001** (pending); this stream renders trigger activity read-only like the rest of the design.
- Interrupting a running fleet — the composer stays disabled mid-run; interrupting a working fleet is a backend capability that does not exist and is not smuggled in here.
- Any change to the per-fleet Fleet Console stream (M131) beyond leaving it working — its route and handler survive untouched.
- The G3-v2 server rollup unless §5's trigger fires — otherwise it ships `DEFERRED` with the measurement recorded in Discovery.
- Per-user UI prefs (G6) — lands in M132, not here.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator opens the Fleets Wall on a busy workspace; every live tile pulses in real time from a single connection, and nothing degrades to a stale snapshot the way it did when each tile fought for its own socket on HTTP/1.1.
2. **Preserved user behaviour** — The wall looks and behaves identically (tiles, pulses, snapshot fallback); the Fleet Console's per-fleet thread is unchanged; no route the console or CLI depends on is removed.
3. **Optimal-way check** — Fan-in over the existing refcounted hub is the most direct shape: it reuses the one shared pub/sub connection and the stream registry as-is. The unconstrained optimum (a single workspace-level Redis channel the publisher writes to directly) would need a publisher change and a new channel in the substrate; the gap is acceptable because fan-in delivers the same one-connection client outcome with zero substrate change.
4. **Rebuild-vs-iterate** — Iterate. The substrate (hub, registry, per-fleet handler) is right; this generalizes the per-fleet handler to a workspace set. No determinism traded away.
5. **What we build** — One multiplexed SSE handler, a workspace fleet-id enumeration query, a single-stream wall client + proxy route, and the deletion of the N-connection mode.
6. **What we do NOT build** — A publisher-side workspace channel (rejected: substrate change for no client-visible gain); `PSUBSCRIBE` filtering (rejected: security boundary by discipline); the server rollup unless measured to be needed.
7. **Fit with existing features** — Compounds with M132's wall (its data source) and M122's client gap-recovery (same backfill discipline); must not destabilize the per-fleet console stream, pinned by the unchanged regression suite.
8. **Surface order** — API-first (the multiplexed route is the enabling capability), then the UI swap. Justified: the wall cannot consume a stream that does not exist.
9. **Dashboard restraint** — No new controls or claims; the wall shows exactly what it showed before, sourced more cheaply. The optional rollup surfaces only counters already backed by server-truth spend.
10. **Confused-user next step** — A refused stream returns `ERR_SSE_STREAM_CAP` with Retry-After and the wall self-heals to snapshots; a degraded tile carries the `snapshot` eyebrow the operator already knows from M132.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Four required Sections (handler, authorization, resilience, wall swap) plus one explicitly optional Section (§5 rollup) gated on a measured trigger. The split follows the boundary of concern — capability, security, resilience, consumption — so each is independently testable.
- **Alternatives considered:** (a) `PSUBSCRIBE fleet:*:activity` with post-hoc workspace filtering — rejected: makes tenant isolation a filtering discipline instead of a construction guarantee; (b) a new publisher-side workspace channel — rejected: substrate change (publisher + channel inventory) for no client-visible benefit over fan-in; (c) keeping the N-connection wall and just raising `sse_max_streams` — rejected: does not fix the HTTP/1.1 dev degradation and scales connections with L×V.
- **Patch-vs-refactor verdict:** **patch** — it generalizes an existing handler, reuses the registry unchanged, and extends the hub additively (shared-consumer variant; per-fleet paths untouched). The one deletion (the wall's N-stream mode) is scoped cleanup, not a refactor of the substrate.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage:
  - CHORE(open) Jul 15, 2026: M132_001 (dependency) is in-flight on `feat/m132-live-wall-getting-started` with spec-only commits — no wall code exists yet, so the Dead Code Sweep's M132 symbol names cannot be pinned at open. Sequencing: §1–§3 + the client stream module/proxy land API-first off `main`; §4 (wall swap + N-stream deletion) and the sweep's symbol pinning fold in once M132_001 merges. Surfaced to Indy at start.
  - PLAN Jul 15, 2026 — Files Changed amended (rules beat spec): OpenAPI paths YAML + bundle added (REST guide §7 is six steps; route-coverage gate enforces), workspace events LIST proxy route added (backfill source did not exist), `lib/api/events.ts` URL helpers added, fixtures moved sibling to the integration suite per `dispatch/write_zig.md`, per-fleet handler touched non-behaviorally to share the hub's channel-name constants (RULE UFS).
  - PLAN Jul 15, 2026 — §5 rollup budget constant named `ROLLUP_PAGING_BUDGET_ROUND_TRIPS = 3` (× the workspace events list `LIMIT_MAX = 200`). The trigger cannot be measured yet: M131's client-side aggregation is unmerged, so there is no real paging cost to measure. §5 tracks toward `DEFERRED` unless a measurement lands before CHORE(close).
- **Performance decisions (Indy-directed, "make it performant, not an explanation"):**
  - **Per-tick authorization is a point lookup, not the enumeration.** Each stream re-authorizes its own caller every refresh tick (10 s prod) with `authorizeWorkspace` — the authorize-ONLY variant (one `SELECT 1 … workspace_id AND tenant_id`, `Index Scan using workspaces_workspace_id_key`, no `set_config`). The RLS context the enumeration needs is set by the cache's own `enumerate`, so the handler's tick pays one indexed point lookup, not two round-trips. Revocation latency = one tick (Indy chose 10 s over tightening — negligible auth load, acceptable eyeballs-surface latency).
  - **Enumeration query, EXPLAIN-verified.** `SELECT id::text AS fleet_id FROM core.fleets WHERE workspace_id=$1 ORDER BY id LIMIT $2`. `Index Scan using uq_fleets_workspace_id_name` (workspace_id prefix) + a bounded top-N **native-uuid** Sort. The `AS fleet_id` alias is load-bearing: without it `ORDER BY id` binds to the `id::text` projection and sorts 36-char strings under the DB collation (slower AND locale-dependent, which would make the stable-set order collation-sensitive). No status filter — a paused fleet mid-run still emits, so the partial `_active` index is deliberately not used. Runs ≤0.1×/sec per **workspace** (shared via the cache), never per connection.
  - **Fan-out is unlocked (§6).** The hub reader snapshots subscribers under `hub.mutex` into a reader-confined reusable scratch buffer, then pushes with the lock released — one `malloc`+`futexWake` per subscriber no longer runs under the lock every `subscribe`/`unsubscribe` needs. Lifetime-guarded by refcounting the `Subscription` (`ref`/`unref`), so a racing `unsubscribe` can't free a handle mid-push.
  - **Shared fleet-set cache (§6).** V viewers of a workspace cost ONE enumeration per cadence, not V — versioned so an unchanged tick is a no-op (no query, no diff, no alloc). Authorization stays per-caller and is never cached.
  - **Buffered SSE writer.** The multiplexed writer buffers one frame's six pieces and flushes once — one `write()` syscall per frame instead of six, identical latency (flush fires the instant the frame completes). A 60-fleet burst: ~420 → ~60 syscalls.
  - **Consumer memory is per-connection, not cap × fleets.** The fan-in shares ONE bounded ring across every attached channel, so a stalled workspace stream costs `QUEUE_CAPACITY` frames total regardless of fleet count — the 800 MiB worst case the spec flagged cannot occur.
- **Real behavioral bug caught by integration tests:** `createSharedConsumer` did not check the hub's stopped flag, so a connect against a stopped hub 200-ed into a dead stream instead of refusing (unlike the per-fleet path). Fixed to return `HubStopped` → 503 at connect.
- **Test-isolation decision:** fleet-set-cardinality tests are driven at the `FanIn` seam against a workspace UNIQUE to each test (constructed principal, no shared token) — the shared fixture workspace is nondeterministic under the parallel runner (siblings seed fleets into it), and every persona token is pinned to that one workspace. Connection-lifecycle tests (cap, forbidden, hub-down, one-slot, reconnect-seq) stay at HTTP against the shared workspace since they assert on the connection, not the fleet set.
- **Metrics review** — reused `sse_backpressure_rejections` / `sse_dropped_frames_total`; workspace-stream open/close + fleet-set-change logs added (no new metric names). No token/payload contents logged.
- **Skill-chain outcomes** — (pending CHORE(close): `/write-unit-test`, `/review`, `kishore-babysit-prs`).
- **Deferrals** — §4 wall swap (FleetsList/FleetTile consuming the workspace stream, the N-stream deletion, and the Dead Code Sweep symbol pinning) is BLOCKED on M132_001 merging — the wall code does not exist yet on `main`. The client (`workspace-stream.ts`) + proxy routes + URL helpers land now; the wall wiring + burst-coalescing/hello-frame/catching-up UX (Dims 4.4–4.6) fold in once M132 merges. §5 rollup ships `DEFERRED` (trigger not measurable — M131 client aggregation unmerged).
