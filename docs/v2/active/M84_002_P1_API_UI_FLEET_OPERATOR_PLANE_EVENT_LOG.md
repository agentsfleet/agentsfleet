# M84_002: Fleet operator plane (cordon/drain/revoke) + runner event log

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 002
**Date:** Jun 04, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operators can't take a host out of rotation or audit fleet history; the read-only list (M84_001) shows *now* but nothing past or actionable.
**Categories:** API, UI
**Batch:** B2 — after M84_001 (the read list + derived liveness it builds on must land first).
**Branch:** feat/m84-fleet-operator-plane-fresh
**Depends on:** M84_001 (the `GET /v1/fleet/runners` read + derived liveness + dashboard surface this extends). Composes with M85_001 (eligibility filter narrows the reassignment re-lease set) but does not require it.
**Provenance:** agent-generated (Indy CTO consult, Jun 04 2026 — authored as a design artifact in PR `feat/m84-dashboard-runner-enrollment`; **not implemented there**). Realises the operator plane + reassignment deferred from M80_006 §1/§2 after its design study.

**Canonical architecture:** `docs/architecture/runner_fleet.md` (the runner-state model — `admin_state` intent vs derived liveness vs `runner_events` history; token rotation/revocation) + `docs/architecture/roadmap.md` (the "Fleet operator plane + proactive reassignment" deferral this builds). The no-JSONB-status decision (CTO-cross-validated Jun 04 2026) is canonical: intent is a typed `admin_state` column, history is an event table, runtime liveness is derived.

---

## Implementing agent — read these first

1. `docs/architecture/roadmap.md` (the "Fleet operator plane + proactive reassignment" section) — the design study that carved this out: the all-runners-down hold, the reassignment-eligibility problem, why `RUNNER_STATUS_{cordoned,revoked}` + `UZ-RUN-009` were left **unbuilt** so the design wasn't foreclosed. This spec builds them.
2. `src/zombied/cmd/serve_runner_lookup.zig` — the runner-auth lookup that gates on `admin_state == 'active'`; adding `cordoned`/`revoked`/`draining`/`drained` makes this the revoke mechanism (`admin_state != 'active'` → 401).
3. `src/zombied/http/handlers/runner/{register,heartbeat,lease,report}.zig` + `src/zombied/fleet/{assign,reclaim}.zig` — the existing **writes** the event log hooks (registered / lease_acquired / lease_released / reclaim) and the affinity slot the sweeper expires for reassignment.
4. `docs/v2/done/M84_001_*` (the prior enrollment spec it builds on) — the derived-liveness model (`registered/online/busy/offline`) + `GET /v1/fleet/runners` this extends with mutation + history; `last_seen_at=0` sentinel.
5. `docs/REST_API_DESIGN_GUIDELINES.md` + `ui/packages/app/app/(dashboard)/settings/api-keys/components/RevokeConfirm.tsx` — the `PATCH` route conventions + the destructive-confirm UI to mirror for cordon/revoke.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Fleet operator plane — cordon/drain/revoke runners + immutable event history
- **Intent (one sentence):** Let a platform admin take a runner out of rotation (cordon → drain → revoke) from the dashboard, and answer "what has this runner done / when was it last busy / how long offline" from an append-only event log — without bloating the current-state model into a Kubernetes-style status object.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`. Key assumptions: (1) `status`→`admin_state` (typed enum, **not** JSONB); (2) liveness stays **derived** (M84_001), never stored; (3) history is `fleet.runner_events` (append-only), not a status field; (4) the sweeper is audit-first for heartbeat lapse and drives admin-state reassignment where no heartbeat timeout exists. A mismatch → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NLR/NLG (the `status`→`admin_state` rename is a clean break pre-2.0; no legacy alias), UFS (`admin_state` values + `event_type` values are named consts shared verbatim Zig↔TS), ORP (sweep every `status`/`RUNNER_STATUS_ACTIVE` call site after the rename), NDC.
- **`docs/ZIG_RULES.md`** — pg-drain on the new reads (event-log query, sweeper scan), tagged-union results, the reassignment write must be atomic under fencing, cross-compile.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — `PATCH /v1/fleet/runners/{id}` (cordon/drain/revoke) + `GET /v1/fleet/runners/{id}/events`: idempotent PATCH semantics, route registration, error envelope.
- **`docs/SCHEMA_CONVENTIONS.md`** — the `status`→`admin_state` rename migration and the `fleet.runner_events` table (app-enforced enums, RULE STS; single-concern migrations).
- **`docs/AUTH.md`** — `admin_state != 'active'` extends the runner-auth gate; the operator plane is `platformAdmin()`-gated (Layer-1 authz).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile; the reassignment write stays atomic under the existing fence. |
| SCHEMA | yes | pre-v2.0 teardown-rebuild (VERSION 0.x — no ALTER, enforced by `check-schema-gate`): rename `status`→`admin_state` **in place** in `021_fleet_runners.sql`; add the `fleet.runner_leases (runner_id, status)` index **in place** in `022_fleet_runner_leases.sql` (sweeper lookup + derived `busy`/`active` counts, unindexed today); add new `025_fleet_runner_events.sql` (with the offline-event partial unique index for cross-replica sweeper single-flight) + register it in `schema/embed.zig` + the migration array. |
| ERROR REGISTRY | yes | wire `UZ-RUN-009` (runner revoked → 401 on the runner plane); `UZ-RUN-014` for missing runner rows; `UZ-AUTH-021` reused for the platform-admin gate. |
| LIFECYCLE | yes | event-log + sweeper reads drain before release; the sweeper job's lifecycle (start/stop) is owned like the existing background workers. |
| LOGGING | yes | state transitions logged via the logfmt envelope; never log a `zrn_`/`token_hash`. |
| UFS | yes | `admin_state` + `event_type` value sets single-sourced; cross-runtime identical. |
| UI Substitution / DESIGN TOKEN | yes | cordon/revoke = `ConfirmDialog` (mirror `RevokeConfirm`); history = design-system primitives + theme tokens. |
| File & Function Length | yes | the sweeper + reassignment factor into helpers (≤50-line fns). |

---

## Overview

**Goal (testable):** A platform admin cordons a runner (`PATCH …/{id}` → `admin_state=cordoned`); because runner auth admits only `active`, the runner's next plane call gets `401 UZ-RUN-009`, and any active leases stay fenced until normal lease expiry or §4's admin-driven reassignment expires their affinity. Draining/drained/revoked are explicit operator intents on the same non-active gate; every transition (registered / online / offline / lease_acquired / lease_released / cordoned / drained / revoked) lands an immutable `fleet.runner_events` row answerable by `GET …/{id}/events`.

**Problem:** After M84_001 an operator can *see* the fleet but can't *act* on it (no way to cordon a misbehaving host, drain it, or revoke a leaked `zrn_`) and can't *audit* it (the derived snapshot can't answer "when was it last busy", "how many runs this period", "how long offline"). A dead runner's work also waits on the lease TTL backstop rather than being proactively reassigned.

**Solution summary:** Three clean, separately-typed concerns (the CTO-validated model): **intent** → a typed `admin_state` column (rename of the overloaded `status`) driving cordon/drain/revoke and the runner-auth gate; **runtime** → liveness stays *derived* (M84_001), never stored; **history** → an append-only `fleet.runner_events` log emitted on the writes the system already does. A single background **liveness sweeper** marks stale runners offline (emitting events) and expires their affinity so work re-leases (closing the M80_006 §2 reassignment deferral). "Busy" stays **derived** from `fleet.runner_leases` — under M88_002's worker pool a runner holds 0..N active leases, so there is no singular live-lease column to drift; `busy = EXISTS(active lease)` and `active = COUNT(active)` both derive server-side, and reassignment targets a specific lease row, not a runner column. (Capacity-aware scheduling — `available = worker_count − active` — is **out of scope here**: it needs a runner-reported `worker_count` that no spec transports yet, and there is no scheduler in this workstream; it lands with M85_001.) **No JSONB status object** — that complexity is imported only if many independent subsystems ever write runner conditions (they don't, yet).

**Visual model.** Three separately-typed concerns; a linear operator state machine that gates runner-auth; one sweeper that detects offline *and* drives reassignment.

```
                    ┌─────────────────── RUNNER STATE ───────────────────┐
                    │                                                     │
        ┌───────────┴──────────┐  ┌──────────────────┐  ┌────────────────┴───────┐
        │   INTENT             │  │   RUNTIME        │  │   HISTORY              │
        │   admin_state        │  │   liveness       │  │   fleet.runner_events  │
        │   (typed enum col)   │  │   (DERIVED)      │  │   (append-only table)  │
        ├──────────────────────┤  ├──────────────────┤  ├────────────────────────┤
        │ active               │  │ registered       │  │ runner_registered      │
        │ cordoned             │  │ online           │  │ runner_online          │
        │ draining             │  │ busy             │  │ runner_offline         │
        │ drained              │  │ offline          │  │ lease_acquired         │
        │ revoked              │  │                  │  │ lease_released         │
        │                      │  │ = f(last_seen_at,│  │ runner_cordoned …      │
        │ operator writes it   │  │     leases)      │  │ runner_revoked         │
        │ gates runner-auth    │  │ NEVER stored     │  │                        │
        └──────────────────────┘  └──────────────────┘  └────────────────────────┘
              orthogonal ───────────────┘                         ▲
                                                          emitted in same txn
                                                          as the state write
        ✗ REJECTED: one JSONB {phase, conditions[], history} k8s-style blob
```

Operator lifecycle — `PATCH /v1/fleet/runners/{id}` (platformAdmin · idempotent):

```
      ┌────────┐  action:cordon   ┌──────────┐  action:drain   ┌─────────┐
      │ active │ ───────────────► │ cordoned │ ──────────────► │draining │
      └────────┘                  └──────────┘                 └─────────┘
          ▲                       no runner-plane calls;       admin-driven
          │                       active leases stay fenced    reassignment
          │                       until expiry / §4                 │
          │                                                           ▼
          │                                                   ┌─────────┐
          │                                                   │ drained │
          │                                                   └─────────┘
          │                                        action:revoke │
          │  re-enroll = new runner                              ▼
          │  (no un-revoke)                                ┌──────────┐
          └────────────────────────────────────────────  │ revoked  │
                                                           └──────────┘
                                          admin_state != active │
                                                                ▼
                                       runner's next authed call: 401 UZ-RUN-009
```

Liveness sweeper + reassignment (§4) — one periodic job:

```
   every tick → scan runners where last_seen_at is stale (> threshold)
        │
        ├─ not stale → skip
        │
        └─ stale → emit runner_offline · expire affinity slot (per-zombie)
                        │
                        ▼  work needs a home
                   eligible healthy runner?
                        │                 │
                   yes  ▼                 ▼  no
              re-lease (fence:        HOLD — unclaimed, no error/thrash,
              one winner)            until capacity returns  (§4.2)
```

---

## Prior-Art / Reference Implementations

- **API** → `src/zombied/http/handlers/runner/*` + `route_table*` (mirror M84_001's `GET /v1/fleet/runners` wiring for `PATCH …/{id}` + `GET …/{id}/events`); `src/zombied/fleet/reclaim.zig` (the existing lease-expiry reclaim the sweeper generalises).
- **Schema** → `schema/021_fleet_runners.sql` (the `status` column being renamed) + the nearest event/audit table; `docs/SCHEMA_CONVENTIONS.md`.
- **UI** → `ui/packages/app/app/(dashboard)/admin/runners/*` (M84_001's surface, extended with row actions) + `settings/api-keys/components/RevokeConfirm.tsx` (the destructive `ConfirmDialog` to mirror).
- **Background job** → the existing zombied background worker lifecycle (the deferred-metrics refresher / reclaim cadence) the sweeper joins.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/021_fleet_runners.sql` | EDIT | In-place rename `status` → `admin_state` (pre-v2.0 teardown-rebuild — no ALTER migration); values active\|cordoned\|draining\|drained\|revoked, app-enforced. |
| `schema/022_fleet_runner_leases.sql` | EDIT | Add the `(runner_id, status)` index in place (pre-v2.0 teardown — no separate migration file): the sweeper's "find this runner's active leases" query + the derived `busy`/`active` counts scan by `runner_id`, unindexed today. |
| `schema/025_fleet_runner_events.sql` | CREATE | Append-only `fleet.runner_events` with canonical `uid` generated from `id`, `runner_id` FK, `event_type`, `occurred_at`, metadata JSONB, `dedup_key` BIGINT NULL + a partial unique index `(runner_id, dedup_key) WHERE event_type='runner_offline'` — the offline-event idempotency key (stale `last_seen_at`) for cross-replica sweeper single-flight. |
| `schema/embed.zig` + migration array | EDIT | Register the new `025_fleet_runner_events.sql` (021 + 022 are edited in place — already registered). |
| `src/zombied/cmd/serve_runner_lookup.zig` | EDIT | Gate on `admin_state == 'active'`; non-active → `401 UZ-RUN-009`. |
| `src/zombied/http/handlers/fleet/runner_patch.zig` | CREATE | `PATCH /v1/fleet/runners/{id}` cordon/drain/revoke; platform-admin gated; emits events. |
| `src/zombied/http/handlers/fleet/runner_events.zig` | CREATE | `GET /v1/fleet/runners/{id}/events` (paginated history). |
| `src/zombied/fleet/runner_events.zig` | CREATE | The append helper called from existing write paths. |
| `src/zombied/http/handlers/runner/{register,lease,report}.zig` + `fleet/{assign,reclaim}.zig` | EDIT | Emit events on the writes already happening. |
| `src/zombied/fleet/liveness_sweeper.zig` | CREATE | Periodic: stale → offline event + expire affinity (reassignment). |
| `src/zombied/http/router.zig` + `route_matchers.zig` + `route_table_invoke.zig` + `auth/middleware/mod.zig` | EDIT | Register the two new fleet routes under `platformAdmin()`. |
| `src/zombied/errors/error_entries.zig` | EDIT | Wire `UZ-RUN-009` (runner revoked). |
| `src/lib/contract/protocol.zig` | EDIT | `AdminState` + `RunnerEvent`/event-type enums. |
| `ui/packages/app/app/(dashboard)/admin/runners/*` | EDIT | Row actions (cordon/drain/revoke via ConfirmDialog) + an activity/history view. |
| `ui/packages/app/lib/api/runners.ts` | EDIT | `updateRunnerAdminState` + `listRunnerEvents`. |
| `docs/architecture/runner_fleet.md` + `roadmap.md` | EDIT | Document the realised operator plane + event model; clear the M80_006 §1/§2 deferral. |
| `docs/AUTH.md` | EDIT | Update the runner-auth gate prose (DOCUMENT stage): the lookup selects `admin_state` (renamed from `status`); `admin_state='active'` admits, else 401. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, five Sections — admin_state intent (§1), operator mutation (§2), event log (§3), sweeper + reassignment (§4), dashboard actions + history (§5). Each maps to one of the three CTO-validated state categories (intent / history / runtime) plus the surfaces that drive them.
- **Alternatives considered:** (a) a single JSONB `status` object holding phase + conditions + history — **rejected** (CTO-cross-validated): source-of-truth + drift problems, and we have one intent dimension, not the k8s controller-explosion that justifies conditions. (b) store liveness — **rejected**: it's a pure function of `last_seen_at` + leases; storing it reintroduces drift. (c) split events and operator-plane into two specs — **rejected by Indy** (one "second spec" in this PR): they share the sweeper and the same admin surface.
- **Patch-vs-refactor verdict:** **small refactor + feature** — the `status`→`admin_state` rename is a contained refactor of one auth-gating column; everything else is additive (event table, two routes, one job, UI actions).

---

## Sections (implementation slices)

### §1 — `admin_state` (operator intent), the typed enum

Rename the overloaded `status` to `admin_state` and expand its values (active|cordoned|draining|drained|revoked, app-enforced). The runner-auth lookup gates on `admin_state == 'active'`, so non-active becomes the revoke/cordon mechanism. **Implementation default:** rename in place (pre-2.0 clean break, no alias).

- **Dimension 1.1** ✅ DONE — `admin_state` replaces `status`; mint writes `active`; the runner-auth lookup admits only `active` → Test `runner auth admits an active admin_state and rejects a revoked one` (integration, `runner_enrollment_integration_test.zig`).
- **Dimension 1.2** ✅ DONE — every old `status`/`RUNNER_STATUS_ACTIVE` reference is migrated (orphan sweep zero) → verified by the `RUNNER_STATUS_ACTIVE` + `sandbox_tier, status` greps (Eval E4): 3 production sites + 12 test seeds swept, `RUNNER_STATUS_ACTIVE` 0 refs.

### §2 — Operator-plane mutation (`PATCH /v1/fleet/runners/{id}`)

Platform-admin-gated cordon → drain → revoke. Any non-active state blocks the runner plane via `401 UZ-RUN-009`; active leases remain fenced and are picked up by normal lease expiry or §4's admin-driven reassignment. **Implementation default:** idempotent PATCH (re-cordoning a cordoned runner is a no-op success).

- **Dimension 2.1** ✅ DONE — cordon → no new runner-plane calls for that runner; active lease rows stay fenced for expiry / §4 reassignment → Test `fleet runner PATCH cordons idempotently then drains`.
- **Dimension 2.2** ✅ DONE — revoke → runner's next authed call returns `401 UZ-RUN-009` → Test `fleet runner PATCH revoke makes the next runner-plane call unauthorized`.
- **Dimension 2.3** ✅ DONE — the mutation is platform-admin-gated; tenant admin / `zmb_t_` → `403 UZ-AUTH-021` → Test `fleet runner PATCH is platform-admin gated`.
- **Dimension 2.4** ✅ DONE — malformed action rejects before a DB write and a missing runner returns `404 UZ-RUN-014` → Test `fleet runner PATCH rejects malformed actions and missing runners`.

### §3 — Immutable event log (`fleet.runner_events`)

Append-only history emitted on writes the system already performs (registered / lease_acquired / lease_released / cordoned / drained / revoked). Read via `GET …/{id}/events`. **Implementation default:** for **single-statement** writes (register, the `affinity.claim` lease-acquire, the lease-settle status flip, the `PATCH` admin_state transitions) the event INSERT joins the same statement/transaction, so history can't diverge from state. The **report finalize** path is explicitly **non-atomic by design** (`service_report.zig` — `loadLease` / `claimReportAndSettle` / `markTerminal` / checkpoint each acquire a separate connection, "best-effort and logged on failure"), so its `lease_released` event is **best-effort** (logged on failure), not transactional. The spec does **not** refactor `service_report` into one txn just to host the event.

- **Dimension 3.1** ✅ DONE — minting, leasing, reporting, and a cordon each append exactly one typed event with `occurred_at` → Tests `state writes append runner events and history route lists them`, `lease and report append acquire and release events`.
- **Dimension 3.2** ✅ DONE — `GET …/{id}/events` returns paginated history and supports `event_type` + `since`/`until` millisecond filters for last-busy reads and window counts → Test `lease and report append acquire and release events`.

### §4 — Liveness sweeper + reassignment

One periodic job: a runner whose `last_seen_at` is stale beyond the threshold gets an `offline` event and its affinity slot expired so its work re-leases to a healthy host (closing the M80_006 §2 reassignment deferral). "Busy" stays **derived** from `fleet.runner_leases` (no singular live-lease column — a pooled runner holds 0..N leases), and the sweeper frees the **per-zombie** affinity slot, not a runner-level column. **Invariant:** if no healthy runner exists, work **holds** (no thrash/fail) until capacity returns.

**Drain completion is automatic.** `PATCH … { action: drain }` sets operator intent to `draining`; the sweeper flips it to `drained` and emits `runner_drained` once the active lease count is zero. If the runner is already idle, that completion happens on the next sweeper tick — there is no separate manual "finish drain" action.

**Reassignment latency reality (the sweeper is audit-first, reclaim-second).** Demand-driven reclaim already re-leases a dead runner's work at the affinity slot's `leased_until` expiry (`LEASE_TTL_MS` = 30 s): the next healthy poller wins the zombie and `reclaim.reclaimPriorActive` fences it — *faster* than this sweeper's stale threshold (`RUNNER_OFFLINE_AFTER_MS` = 90 s). So the sweeper does **not** make heartbeat-lapse reassignment work (the TTL already does); its deliverables are (1) the `runner_offline` **audit event**, (2) reassignment for the **admin-driven** path — cordon/drain/revoke has no TTL lapse to trigger reclaim — and (3) the all-runners-down hold (§4.2). To expire a dead/cordoned runner's slots it enumerates that runner's active leases: `SELECT zombie_id FROM fleet.runner_leases WHERE runner_id = $id AND status = 'active'` (**0..N** rows under the pool), which needs an index on `fleet.runner_leases (runner_id, status)` — `runner_id` is unindexed today.

**Cross-replica single-flight — a unique constraint, not an advisory lock.** Every `zombied` replica runs this sweeper, so a stale runner is detected by all of them on the same tick. The `runner_offline` event carries an idempotency key — the stale `last_seen_at` snapshot — under a partial unique index on `fleet.runner_events`; each replica `INSERT … ON CONFLICT DO NOTHING RETURNING`s, so exactly one wins and emits the event + drives the (already lease-fenced) reassignment, and the rest no-op. An advisory lock is **rejected** (it serializes the sweeper across replicas, defeating the horizontal scale replicas exist for); a stored `offline_notified_at` CAS column is **rejected** (it reintroduces the runtime-shadow the `current_lease_id` drop just removed, and is discipline-enforced, not DB-enforced). The key is immutable while the runner is dead (no heartbeat updates `last_seen_at`) and distinct across episodes (revival requires a heartbeat that bumps it).

- **Dimension 4.1** ✅ DONE — a runner gone stale is swept → `offline` event + affinity expired; its zombie re-leases to a live runner → Test `stale runner swept and work reassigned`.
- **Dimension 4.2** ✅ DONE — all-runners-down: a swept runner's work holds (stays unclaimed, no error) until a live runner returns → Test `reassignment holds when no eligible target`.
- **Dimension 4.3** ✅ DONE — "busy"/"active" derive from the `fleet.runner_leases` active-lease set (`busy = EXISTS(active lease)`, `active = COUNT(active)`); a pooled runner reports 0/1/N active leases correctly with **no** runner-level lease column → Test `liveness derives active lease set without singular column`. (`available = worker_count − active` is out of scope — no `worker_count` source exists; see Interfaces.)
- **Dimension 4.4** ✅ DONE — N replicas sweeping the same stale runner concurrently emit exactly one `runner_offline` event (the partial unique index admits one INSERT; the rest no-op) → Test `concurrent sweepers emit one offline event`.
- **Dimension 4.5** ✅ DONE — idle `draining` runners are still swept and become `drained` without waiting on an active-lease selector → Test `idle draining runner becomes drained on the next sweep`.

### §5 — Dashboard: row actions + activity history

The M84_001 runners surface gains per-row cordon/drain/revoke (destructive `ConfirmDialog`, mirror `RevokeConfirm`) and a per-runner activity view reading the event log. **Invariant:** actions + history are platform-admin-only (server 403 + UI not rendered for non-admins).

- **Dimension 5.1** ✅ DONE — a platform admin cordons/revokes a runner from the list; the badge reflects the new `admin_state` → Tests `cordons a runner from the row and updates the admin-state badge`, `revokes a runner from the row and updates the badge`, `updateRunnerAdminStateAction forwards the runner state change through withToken when admin`.
- **Dimension 5.2** ✅ DONE — the activity view renders the event timeline for a runner → Tests `opens runner activity and renders the event timeline`, `pages runner activity without reloading the runner list`, `listRunnerEventsAction forwards activity-history paging through withToken when admin`.

---

## Interfaces

```
fleet.runners.admin_state : TEXT (active|cordoned|draining|drained|revoked), app-enforced. Renamed from `status`.
fleet.runner_events : (uid generated from id, id, runner_id FK, event_type, occurred_at BIGINT, metadata JSONB, dedup_key BIGINT NULL) — append-only.
  Partial unique: (runner_id, dedup_key) WHERE event_type='runner_offline' — one offline event per
  offline episode across replicas; dedup_key = the stale last_seen_at snapshot the sweeper read.

Liveness (DERIVED from fleet.runner_leases — NO singular column on fleet.runners):
  busy   = EXISTS(active lease for runner)     active = COUNT(active lease for runner)
  Reassignment targets a specific fleet.runner_leases row, never a runner-level lease pointer.
  OUT OF SCOPE: capacity-aware scheduling (available = worker_count − active). No spec transports a
  runner-reported worker_count yet (heartbeat body is empty); it lands with the scheduler (M85_001).
  event_type ∈ {runner_registered, runner_online, runner_offline, lease_acquired, lease_released,
                runner_cordoned, runner_draining, runner_drained, runner_revoked}.

PATCH /v1/fleet/runners/{id}   platformAdmin; body { action: cordon|drain|revoke }; idempotent.
                               → 200 { id, admin_state }   (tenant admin / zmb_t_ → 403 UZ-AUTH-021)
GET   /v1/fleet/runners/{id}/events  platformAdmin; optional event_type/since/until filters;
                                      paginated { items, total, page, page_size }.
Runner plane: a revoked/cordoned runner's authed call → 401 UZ-RUN-009.
Liveness (derived, M84_001) is UNCHANGED — admin_state and liveness are orthogonal.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Revoke a runner mid-lease | operator revokes a busy host | the next authed runner-plane call → `401 UZ-RUN-009`; the active lease stays fenced until normal expiry or §4 reassignment, and a stale holder's late report is still rejected by the fence. |
| Cordon then host keeps heartbeating | host unaware | heartbeat is rejected by the non-active auth gate; liveness stops advancing and later derives offline, while `admin_state` stays cordoned. |
| All runners down during sweep | no healthy target | work **holds** (unclaimed, no error/dead-letter) until capacity returns (§4.2). |
| Event write fails | DB error mid-transaction | the state write + its event share a transaction → both roll back; no half-written history. |
| Non-platform-admin mutates | wrong role | `403 UZ-AUTH-021`; nothing changes; UI action not rendered (§2.3/§5). |
| Double-cordon / double-revoke | retried PATCH | idempotent no-op success; one event, not duplicates. |
| Sweeper races reclaim | concurrent expiry | the existing fencing token admits one winner; reassignment never double-frees a slot. |
| Two replicas sweep one stale runner | every `zombied` replica runs the sweeper | the offline event's partial-unique idempotency key admits one `INSERT`; the rest `ON CONFLICT DO NOTHING` → exactly one event + one reassignment trigger → Test 4.4 |

---

## Invariants

1. **Liveness stays derived, never stored** — `admin_state` is intent, `runner_events` is history; no runtime-state column, and **no singular `current_lease_id`** — a pooled runner (M88_002) holds 0..N active leases, so `busy = EXISTS(active lease)` and `active = COUNT(active)` derive from `fleet.runner_leases`. Enforced by review + the absence of an `online/offline` column and of any runner-level lease pointer. (`available = worker_count − active` is **not** in scope — no `worker_count` is transported; capacity-aware scheduling is M85_001.)
2. **`admin_state != 'active'` ⇒ runner-auth rejects** (`401 UZ-RUN-009`) — enforced by §1.1/§2.2 + the lookup gate.
3. **Event ⇄ state consistency** — for **single-statement** state changes (register, lease-acquire claim, lease-settle, the admin_state `PATCH`es) the event is written in the same statement/transaction — enforced by §3.1 + the integration test injecting a mid-write failure. The non-atomic **report finalize** path (`service_report.zig`, by design) emits its `lease_released` event **best-effort** (logged on failure); reconstructing a missed release event is possible from the lease row's terminal state.
4. **Operator plane is platform-admin-only** (Layer-1 authz, never a Postgres GRANT) — enforced by §2.3.
5. **Reassignment holds, never thrashes/fails** when no eligible target — enforced by §4.2.
6. **`runner_events` is append-only** (no UPDATE/DELETE grant) — enforced by the migration's GRANTs + review.
7. **No JSONB status object** — runner state is `admin_state` (typed) + derived liveness + `runner_events`; conditions-JSONB is out — enforced by review against this Invariant.
8. **≤1 `runner_offline` event per offline episode, across all replicas** — the offline event's idempotency key (stale `last_seen_at`) under a partial unique index on `fleet.runner_events` makes N racing sweepers' duplicate INSERTs no-op — enforced by the DB constraint (not an advisory lock, not review discipline) + Dimension 4.4.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `runner auth admits only active admin state` | active → 200; cordoned/revoked → 401. |
| 1.2 | regression | `no orphaned status references` | grep `\.status`/`RUNNER_STATUS_ACTIVE` in fleet paths → 0 stale. |
| 2.1 | integration | `fleet runner PATCH cordons idempotently then drains` | cordon → `admin_state=cordoned`; repeated cordon leaves `updated_at` unchanged; drain → `admin_state=draining`. |
| 2.2 | integration | `fleet runner PATCH revoke makes the next runner-plane call unauthorized` | revoke → next runner `/me` call `401 UZ-RUN-009`. |
| 2.3 | integration | `fleet runner PATCH is platform-admin gated` | tenant admin / `zmb_t_` PATCH → `403 UZ-AUTH-021`. |
| 2.4 | integration | `fleet runner PATCH rejects malformed actions and missing runners` | malformed action → `400 UZ-REQ-001`; missing runner → `404 UZ-RUN-014`. |
| 3.1 | integration | `state writes append runner events and history route lists them`; `lease and report append acquire and release events` | register/lease-acquire/cordon → one typed event each in the same statement/txn; report → best-effort `lease_released` (logged on failure, non-atomic by design). |
| 3.2 | integration | `lease and report append acquire and release events` | `GET …/events?event_type=lease_acquired&page_size=1` → latest busy event; `until=0` window → `total=0`; unfiltered history returns acquire + release. |
| 4.1 | integration | `stale runner swept and work reassigned` | stale `last_seen` → offline event + affinity expired → re-leased. |
| 4.2 | integration | `reassignment holds when no eligible target` | no live runner → work unclaimed, no error; returns → claimed. |
| 4.3 | integration | `liveness derives active lease set without singular column` | runner with 0/1/N active leases → `busy`/`active` correct; no runner-level lease column exists. |
| 4.4 | integration | `concurrent sweepers emit one offline event` | N replicas sweep the same stale runner → exactly one `runner_offline` row (others `ON CONFLICT DO NOTHING`). |
| 4.5 | integration | `idle draining runner becomes drained on the next sweep` | `admin_state=draining` and zero active leases → next sweep writes `drained` + one `runner_drained` event. |
| 5.1 | component/server-action | `cordons a runner from the row and updates the admin-state badge`; `revokes a runner from the row and updates the badge`; `updateRunnerAdminStateAction forwards the runner state change through withToken when admin` | admin cordons/revokes → badge reflects `admin_state`; server action stays platform-admin-gated. |
| 5.2 | component/server-action | `opens runner activity and renders the event timeline`; `pages runner activity without reloading the runner list`; `listRunnerEventsAction forwards activity-history paging through withToken when admin` | event timeline renders for a runner; pagination requests the expected page; server action stays platform-admin-gated. |

**Regression:** the existing lease/fence/reclaim + M84_001 derived-liveness suites stay green. **Idempotency:** PATCH cordon/drain/revoke are idempotent (re-applying yields one event, success).

---

## Acceptance Criteria

- [x] `admin_state` rename + auth gate; revoke → `401 UZ-RUN-009` — verify: `make test-integration` + `zig build test-auth`
- [x] `PATCH /v1/fleet/runners/{id}` cordon/drain/revoke, platform-admin-gated — verify: `make test-integration`
- [x] `fleet.runner_events` append-only; emitted on state writes; `GET …/events` reads — verify: `make test-integration`
- [x] Sweeper marks offline + reassigns; holds when no target — verify: `make test-integration-db`
- [x] Dashboard cordon/revoke + activity view, platform-admin-only — verify: `bun run test:coverage --no-file-parallelism` + focused runner coverage slice
- [x] `make lint-zig` clean · app package tests pass · cross-compile both linux targets
- [x] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: operator plane + event log + sweeper
make test-integration 2>&1 | grep -iE "admin state|cordon|revoke|event|reassign|sweep" | tail -15
# E2: revoke gate
zig build test-auth 2>&1 | tail -5
# E3: Build + cross-compile
zig build && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo PASS || echo FAIL
# E4: no orphaned `status` after rename
grep -rn "RUNNER_STATUS_ACTIVE\|\.status" src/zombied/fleet src/zombied/cmd/serve_runner_lookup.zig | head
# E5: Gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.** N/A — no files deleted (the `status` column is renamed, not dropped to a new file).

**2. Orphaned references — zero remaining.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `RUNNER_STATUS_ACTIVE` (renamed to admin_state const) | `grep -rn "RUNNER_STATUS_ACTIVE" src/` | 0 (replaced by the `admin_state` const) |
| `fleet.runners.status` column refs | `grep -rn "runners.*status\b" src/ schema/` | 0 stale (all → `admin_state`) |

---

## Discovery (consult log)

> **Empty at creation.** Append as work surfaces consults/decisions.

- **Provenance (Jun 04 2026)** — authored in PR `feat/m84-dashboard-runner-enrollment` after Indy's CTO consult on runner state. Indy: *"Yes author the event-log + operator-plane as second pending in this PR."* The no-JSONB-status model was cross-validated (Indy stress-tested it against another model; both agreed: typed `admin_state` + derived liveness + `runner_events`, conditions-JSONB only if many independent subsystems ever write runner state).
- **Builds the M80_006 deferral** — `roadmap.md`'s "Fleet operator plane + proactive reassignment" (the cordon/drain/revoke surface, `RUNNER_STATUS_{cordoned,revoked}`, `UZ-RUN-009`, and heartbeat-lapse reassignment) was deferred after a design study; this is that spec.
- **Re-scope (Jun 08 2026) — `current_lease_id` dropped; "busy" stays derived.** The original draft proposed a singular `fleet.runners.current_lease_id` column as a cheap busy-marker + reassignment target. M88_002's worker pool makes a runner hold **0..N** concurrent leases, so a singular column is fundamentally wrong (there is no single "current" lease). Ratified with Indy (ChatGPT + CTO review): `runner_leases` is the sole assignment truth; `busy = EXISTS(active lease)` and `active = COUNT(active)` **derive** from it — no column, no counter, no drift, no migration tear-out when M88_002 lands. The capacity predicate `available = worker_count − active` is the *direction* (a capacity-based scheduler) but is **out of scope here**: no spec transports a runner-reported `worker_count` (the heartbeat body is empty, M88_002), and this workstream has no scheduler — capacity-aware placement is M85_001. A materialized active-count is likewise deferred (for scheduler scale only, if ever). This removed one migration (`schema/0NN_runner_current_lease.sql`) and Dimension 4.3's column-tracking test. (Adversarial review Jun 09 caught the dangling `worker_count` reference — fixed by scoping capacity out, keeping only the two derivable signals.)
- **Sweeper single-flight — X (unique constraint) chosen over Y (CAS column) + advisory lock (Jun 08 2026, Orly CTO review, ratified Indy).** Under N `zombied` replicas every replica runs the liveness sweeper, so the `runner_offline` audit event must be exactly-once per offline episode (the reassignment side-effect is already lease-fenced via `reclaim.zig`). **Chosen (X):** a partial unique index on `fleet.runner_events` keyed by the stale `last_seen_at` (the offline-episode idempotency key) — `INSERT … ON CONFLICT DO NOTHING RETURNING`; the winning replica emits + drives reassignment. **Rejected:** an advisory lock (serializes the sweeper across replicas, defeating horizontal scale) and a CAS on a stored `fleet.runners.offline_notified_at` column (reintroduces the runtime-shadow the `current_lease_id` drop removed; discipline-enforced, not DB-enforced; `approval_gate`'s CAS flips *real state*, this would be pure dedup bookkeeping). The idempotency key lives on the append-only event table — where idempotency keys belong — leaving `fleet.runners` column-free. Key correctness: immutable while dead (no heartbeat), distinct across episodes (revival bumps `last_seen_at`).
- **§1 implemented (Jun 09 2026) — teardown-rebuild rename, not an ALTER migration.** VERSION is 0.37.0 (major 0 < 2), so `check-schema-gate` forbids `ALTER`/`DROP` — the `status`→`admin_state` rename is an **in-place edit of `021_fleet_runners.sql`** (and §4's sweeper index lands in-place in `022`), not new migration files. The ORP sweep was wider than first scoped: **12 test seeds** (not 3) plus 3 production sites (register insert, runnerBearer lookup, `GET /me`) referenced `fleet.runners.status`; all swept, `RUNNER_STATUS_ACTIVE` removed (0 refs). `SelfResponse.status` wire field **kept** (sourced from the renamed column) — renaming it ripples cross-binary to the runner daemon, out of this spec's blast radius; `docs/AUTH.md` gate prose updates at DOCUMENT stage. `AdminState` is a typed enum; `ADMIN_STATE_ACTIVE` derives from it via `@tagName` (UFS). Commit `bd222fae` added the Error Registry entry for `UZ-RUN-009`; this pickup reran `make harness-verify`, `zig build test-auth`, and the pre-commit gates clean.
- **Pre-close review fix (Jun 09 2026) — PATCH event metadata now reads the locked row state.** The adversarial review found a race where concurrent non-revoked transitions could update the runner correctly but write stale `from_admin_state` metadata from the earlier caller-side read. `runner_patch.zig` now locks the current row in the update statement, uses that locked `from_admin_state` for the event metadata, and routes idempotent repeats through the locked no-op path. Verification reran `make test-integration-db`, `make lint-zig`, `zig build test`, and both Linux cross-builds.
- **Deferrals** — populate during implementation; none at authoring.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Coverage vs the operator-plane + event + sweeper matrix (esp. event⇄state txn, revoke gate, reassignment hold). | Clean; count in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial review vs spec, ZIG_RULES, AUTH.md, the append-only + no-JSONB invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Operator plane + events | `make test-integration-db` | `✓ [zombied] database-backed integration tests passed` | yes |
| Backend unit graph | `zig build test` | exited 0 (known negative-test diagnostics only) | yes |
| Sweeper + reassignment | `make test-integration-db` | stale/offline/reassignment/drain integration tests passed in suite | yes |
| Dashboard changed-surface coverage | focused `vitest ... --coverage --coverage.thresholds.100 --coverage.thresholds.perFile` | 50 tests; statements 100% (149/149), branches 100% (94/94), functions 100% (59/59), lines 100% (129/129) | yes |
| Dashboard package coverage | `bun run test:coverage --no-file-parallelism` | 91 files, 856 tests; statements 100% (2139/2139), branches 100% (1306/1306), functions 100% (675/675), lines 100% (1901/1901) | yes |
| Repository coverage gate | `make test-coverage-all` | app 100%, website 100%, design-system 100% (301/301 statements, 285/285 branches, 118/118 functions, 278/278 lines); `zombiectl` 1097 pass / 2 skip and package gate passed | yes |
| Cross-compile | `zig build -Dtarget=x86_64-linux`; `zig build -Dtarget=aarch64-linux` | both exited 0 | yes |
| Staged harness | `make harness-verify` | all staged gates green | yes |
| Frontend lint | `make lint-apps-ds-ctl` | app, design-system, and `zombiectl` lint/type checks passed | yes |
| Secret scan | `gitleaks detect` | no leaks found | yes |

---

## Punch List

- **Deployment follow-up (requested by Indy, Jun 10 2026: 12:05 AM Indian Standard Time (IST))** — `deploy-dev` failed while syncing the Cloudflare tunnel token because the secret update API returned `502 Bad Gateway`; the log does not indicate an expired token. `deploy-dev-worker` then wrote `/etc/default/zombie-runner` but validation found `ZOMBIE_API_URL`, `ZOMBIE_RUNNER_TOKEN`, and `RUNNER_HOST_ID` absent. This is outside this runner-operator workstream's file scope; investigate secret-update retry/backoff and runner environment propagation in a deployment follow-up.

---

## Out of Scope

- **Tag/label placement (the scheduler)** — that is M85_001; this spec's reassignment re-leases to any eligible runner and composes with M85_001's eligibility filter when it lands.
- **Capacity / fairness / autoscale** — out (the non-goals fence holds).
- **`conditions JSONB` / health probes / maintenance windows / hardware inventory** — explicitly deferred; adopt the `phase + conditions JSONB` split **only** when multiple independent subsystems write runner state (not now).
- **Runner-initiated self-cordon / graceful self-drain on shutdown** — future; this plane is operator-initiated.
