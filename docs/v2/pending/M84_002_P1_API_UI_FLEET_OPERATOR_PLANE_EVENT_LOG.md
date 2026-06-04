# M84_002: Fleet operator plane (cordon/drain/revoke) + runner event log

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 002
**Date:** Jun 04, 2026
**Status:** PENDING
**Priority:** P1 — operators can't take a host out of rotation or audit fleet history; the read-only list (M84_001) shows *now* but nothing past or actionable.
**Categories:** API, UI
**Batch:** B2 — after M84_001 (the read list + derived liveness it builds on must land first).
**Branch:** {feat/m84-fleet-operator-plane — added when work begins}
**Depends on:** M84_001 (the `GET /v1/fleet/runners` read + derived liveness + dashboard surface this extends). Composes with M85_001 (eligibility filter narrows the reassignment re-lease set) but does not require it.
**Provenance:** agent-generated (Indy CTO consult, Jun 04 2026 — authored as a design artifact in PR `feat/m84-dashboard-runner-enrollment`; **not implemented there**). Realises the operator plane + reassignment deferred from M80_006 §1/§2 after its design study.

**Canonical architecture:** `docs/architecture/runner_fleet.md` (the runner-state model — `admin_state` intent vs derived liveness vs `runner_events` history; token rotation/revocation) + `docs/architecture/roadmap.md` (the "Fleet operator plane + proactive reassignment" deferral this builds). The no-JSONB-status decision (CTO-cross-validated Jun 04 2026) is canonical: intent is a typed `admin_state` column, history is an event table, runtime liveness is derived.

---

## Implementing agent — read these first

1. `docs/architecture/roadmap.md` (the "Fleet operator plane + proactive reassignment" section) — the design study that carved this out: the all-runners-down hold, the reassignment-eligibility problem, why `RUNNER_STATUS_{cordoned,revoked}` + `UZ-RUN-009` were left **unbuilt** so the design wasn't foreclosed. This spec builds them.
2. `src/zombied/cmd/serve_runner_lookup.zig` — the runner-auth lookup that gates on `status == 'active'`; renaming to `admin_state` + adding `cordoned`/`revoked`/`draining`/`drained` makes this the revoke mechanism (`admin_state != 'active'` → 401).
3. `src/zombied/http/handlers/runner/{register,heartbeat,lease,report}.zig` + `src/zombied/fleet/{assign,reclaim}.zig` — the existing **writes** the event log hooks (registered / lease_acquired / lease_released / reclaim) and the affinity slot the sweeper expires for reassignment.
4. `docs/v2/done/M84_001_*` (the prior enrollment spec it builds on) — the derived-liveness model (`registered/online/busy/offline`) + `GET /v1/fleet/runners` this extends with mutation + history; `last_seen_at=0` sentinel.
5. `docs/REST_API_DESIGN_GUIDELINES.md` + `ui/packages/app/app/(dashboard)/settings/api-keys/components/RevokeConfirm.tsx` — the `PATCH` route conventions + the destructive-confirm UI to mirror for cordon/revoke.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Fleet operator plane — cordon/drain/revoke runners + immutable event history
- **Intent (one sentence):** Let a platform admin take a runner out of rotation (cordon → drain → revoke) from the dashboard, and answer "what has this runner done / when was it last busy / how long offline" from an append-only event log — without bloating the current-state model into a Kubernetes-style status object.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`. Key assumptions: (1) `status`→`admin_state` (typed enum, **not** JSONB); (2) liveness stays **derived** (M84_001), never stored; (3) history is `fleet.runner_events` (append-only), not a status field; (4) the sweeper both emits offline events and drives heartbeat-lapse reassignment (one job, not two). A mismatch → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NLR/NLG (the `status`→`admin_state` rename is a clean break pre-2.0; no legacy alias), UFS (`admin_state` values + `event_type` values are named consts shared verbatim Zig↔TS), ORP (sweep every `status`/`RUNNER_STATUS_ACTIVE` call site after the rename), NDC.
- **`docs/ZIG_RULES.md`** — pg-drain on the new reads (event-log query, sweeper scan), tagged-union results, the reassignment write must be atomic under fencing, cross-compile.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — `PATCH /v1/fleet/runners/{id}` (cordon/drain/revoke) + `GET /v1/fleet/runners/{id}/events`: idempotent PATCH semantics, route registration, error envelope.
- **`docs/SCHEMA_CONVENTIONS.md`** — the `status`→`admin_state` rename migration, the `fleet.runner_events` table, the `current_lease_id` column (app-enforced enums, RULE STS; single-concern migrations).
- **`docs/AUTH.md`** — `admin_state != 'active'` extends the runner-auth gate; the operator plane is `platformAdmin()`-gated (Layer-1 authz).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile; the reassignment write stays atomic under the existing fence. |
| SCHEMA | yes | three migrations: rename `status`→`admin_state` (+ expand values, app-enforced), add `fleet.runner_events`, add `runners.current_lease_id`. Update `schema/embed.zig` + array. |
| ERROR REGISTRY | yes | wire `UZ-RUN-009` (runner revoked → 401 on the runner plane); `UZ-AUTH-021` reused for the platform-admin gate. |
| LIFECYCLE | yes | event-log + sweeper reads drain before release; the sweeper job's lifecycle (start/stop) is owned like the existing background workers. |
| LOGGING | yes | state transitions logged via the logfmt envelope; never log a `zrn_`/`token_hash`. |
| UFS | yes | `admin_state` + `event_type` value sets single-sourced; cross-runtime identical. |
| UI Substitution / DESIGN TOKEN | yes | cordon/revoke = `ConfirmDialog` (mirror `RevokeConfirm`); history = design-system primitives + theme tokens. |
| File & Function Length | yes | the sweeper + reassignment factor into helpers (≤50-line fns). |

---

## Overview

**Goal (testable):** A platform admin cordons a runner (`PATCH …/{id}` → `admin_state=cordoned`); it stops receiving new leases but finishes in-flight work; draining then revoking sets `admin_state=revoked` so the runner's next call gets `401 UZ-RUN-009`; every transition (registered / online / offline / lease_acquired / lease_released / cordoned / drained / revoked) lands an immutable `fleet.runner_events` row answerable by `GET …/{id}/events`; a runner whose heartbeat lapses is swept offline and its affinity expired so its work re-leases to a healthy host.

**Problem:** After M84_001 an operator can *see* the fleet but can't *act* on it (no way to cordon a misbehaving host, drain it, or revoke a leaked `zrn_`) and can't *audit* it (the derived snapshot can't answer "when was it last busy", "how many runs this period", "how long offline"). A dead runner's work also waits on the lease TTL backstop rather than being proactively reassigned.

**Solution summary:** Three clean, separately-typed concerns (the CTO-validated model): **intent** → a typed `admin_state` column (rename of the overloaded `status`) driving cordon/drain/revoke and the runner-auth gate; **runtime** → liveness stays *derived* (M84_001), never stored; **history** → an append-only `fleet.runner_events` log emitted on the writes the system already does. A single background **liveness sweeper** marks stale runners offline (emitting events) and expires their affinity so work re-leases (closing the M80_006 §2 reassignment deferral), with `current_lease_id` making "busy" a column read and naming the reassignment target. **No JSONB status object** — that complexity is imported only if many independent subsystems ever write runner conditions (they don't, yet).

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
| `schema/0NN_runner_admin_state.sql` | CREATE | Rename `fleet.runners.status` → `admin_state`; values active\|cordoned\|draining\|drained\|revoked (app-enforced). |
| `schema/0NN_fleet_runner_events.sql` | CREATE | Append-only `fleet.runner_events` (id, runner_id FK, event_type, occurred_at, metadata JSONB). |
| `schema/0NN_runner_current_lease.sql` | CREATE | `fleet.runners.current_lease_id` (nullable FK) — cheap "busy" + reassignment target. |
| `schema/embed.zig` + migration array | EDIT | Register the three migrations. |
| `src/zombied/cmd/serve_runner_lookup.zig` | EDIT | Gate on `admin_state == 'active'`; non-active → `401 UZ-RUN-009`. |
| `src/zombied/http/handlers/fleet/runner_patch.zig` | CREATE | `PATCH /v1/fleet/runners/{id}` cordon/drain/revoke; platform-admin gated; emits events. |
| `src/zombied/http/handlers/fleet/runner_events.zig` | CREATE | `GET /v1/fleet/runners/{id}/events` (paginated history). |
| `src/zombied/fleet/runner_events.zig` | CREATE | The append helper called from existing write paths. |
| `src/zombied/http/handlers/runner/{register,lease,report}.zig` + `fleet/{assign,reclaim}.zig` | EDIT | Emit events on the writes already happening; set/clear `current_lease_id`. |
| `src/zombied/fleet/liveness_sweeper.zig` | CREATE | Periodic: stale → offline event + expire affinity (reassignment). |
| `src/zombied/http/router.zig` + `route_matchers.zig` + `route_table_invoke.zig` + `auth/middleware/mod.zig` | EDIT | Register the two new fleet routes under `platformAdmin()`. |
| `src/zombied/errors/error_entries.zig` | EDIT | Wire `UZ-RUN-009` (runner revoked). |
| `src/lib/contract/protocol.zig` | EDIT | `AdminState` + `RunnerEvent`/event-type enums; `current_lease_id` on the self/list shapes. |
| `ui/packages/app/app/(dashboard)/admin/runners/*` | EDIT | Row actions (cordon/drain/revoke via ConfirmDialog) + an activity/history view. |
| `ui/packages/app/lib/api/runners.ts` | EDIT | `patchRunner` + `listRunnerEvents`. |
| `docs/architecture/runner_fleet.md` + `roadmap.md` | EDIT | Document the realised operator plane + event model; clear the M80_006 §1/§2 deferral. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, five Sections — admin_state intent (§1), operator mutation (§2), event log (§3), sweeper + reassignment (§4), dashboard actions + history (§5). Each maps to one of the three CTO-validated state categories (intent / history / runtime) plus the surfaces that drive them.
- **Alternatives considered:** (a) a single JSONB `status` object holding phase + conditions + history — **rejected** (CTO-cross-validated): source-of-truth + drift problems, and we have one intent dimension, not the k8s controller-explosion that justifies conditions. (b) store liveness — **rejected**: it's a pure function of `last_seen_at` + leases; storing it reintroduces drift. (c) split events and operator-plane into two specs — **rejected by Indy** (one "second spec" in this PR): they share the sweeper and the same admin surface.
- **Patch-vs-refactor verdict:** **small refactor + feature** — the `status`→`admin_state` rename is a contained refactor of one auth-gating column; everything else is additive (event table, two routes, one job, UI actions).

---

## Sections (implementation slices)

### §1 — `admin_state` (operator intent), the typed enum

Rename the overloaded `status` to `admin_state` and expand its values (active|cordoned|draining|drained|revoked, app-enforced). The runner-auth lookup gates on `admin_state == 'active'`, so non-active becomes the revoke/cordon mechanism. **Implementation default:** rename in place (pre-2.0 clean break, no alias).

- **Dimension 1.1** — `admin_state` replaces `status`; mint writes `active`; the runner-auth lookup admits only `active` → Test `runner auth admits only active admin state`.
- **Dimension 1.2** — every old `status`/`RUNNER_STATUS_ACTIVE` reference is migrated (orphan sweep zero) → Test `no orphaned status references`.

### §2 — Operator-plane mutation (`PATCH /v1/fleet/runners/{id}`)

Platform-admin-gated cordon → drain → revoke. Cordon stops new leases; in-flight work finishes; revoke sets `admin_state=revoked` → the runner's next call is `401 UZ-RUN-009`. **Implementation default:** idempotent PATCH (re-cordoning a cordoned runner is a no-op success).

- **Dimension 2.1** — cordon → no new lease claims for that runner; in-flight lease unaffected → Test `cordon stops new leases keeps in-flight`.
- **Dimension 2.2** — revoke → runner's next authed call returns `401 UZ-RUN-009` → Test `revoke 401s the runner plane`.
- **Dimension 2.3** — the mutation is platform-admin-gated; tenant admin / `zmb_t_` → `403 UZ-AUTH-021` → Test `operator mutation is platform-admin-gated`.

### §3 — Immutable event log (`fleet.runner_events`)

Append-only history emitted on writes the system already performs (registered / lease_acquired / lease_released / cordoned / drained / revoked). Read via `GET …/{id}/events`. **Implementation default:** events are emitted in the same transaction as the state write so history can't diverge from state.

- **Dimension 3.1** — minting, leasing, reporting, and a cordon each append exactly one typed event with `occurred_at` → Test `state writes append events`.
- **Dimension 3.2** — `GET …/{id}/events` answers "last lease_acquired" / counts over a window → Test `event history answers last-busy and counts`.

### §4 — Liveness sweeper + reassignment

One periodic job: a runner whose `last_seen_at` is stale beyond the threshold gets an `offline` event and its affinity slot expired so its work re-leases to a healthy host (closing the M80_006 §2 reassignment deferral). `current_lease_id` makes "busy" a column read and names the slot to free. **Invariant:** if no healthy runner exists, work **holds** (no thrash/fail) until capacity returns.

- **Dimension 4.1** — a runner gone stale is swept → `offline` event + affinity expired; its zombie re-leases to a live runner → Test `stale runner swept and work reassigned`.
- **Dimension 4.2** — all-runners-down: a swept runner's work holds (stays unclaimed, no error) until a live runner returns → Test `reassignment holds when no eligible target`.
- **Dimension 4.3** — `current_lease_id` set on claim, cleared on report → "busy" derivable without a join → Test `current lease id tracks the live lease`.

### §5 — Dashboard: row actions + activity history

The M84_001 runners surface gains per-row cordon/drain/revoke (destructive `ConfirmDialog`, mirror `RevokeConfirm`) and a per-runner activity view reading the event log. **Invariant:** actions + history are platform-admin-only (server 403 + UI not rendered for non-admins).

- **Dimension 5.1** — a platform admin cordons/revokes a runner from the list; the badge reflects the new `admin_state` → Test `dashboard cordon revoke updates state` (e2e).
- **Dimension 5.2** — the activity view renders the event timeline for a runner → Test `dashboard shows runner activity` (e2e/component).

---

## Interfaces

```
fleet.runners.admin_state : TEXT (active|cordoned|draining|drained|revoked), app-enforced. Renamed from `status`.
fleet.runners.current_lease_id : UUID NULL — the live lease (cheap "busy"; reassignment target).
fleet.runner_events : (id, runner_id FK, event_type, occurred_at BIGINT, metadata JSONB) — append-only.
  event_type ∈ {runner_registered, runner_online, runner_offline, lease_acquired, lease_released,
                runner_cordoned, runner_draining, runner_drained, runner_revoked}.

PATCH /v1/fleet/runners/{id}   platformAdmin; body { action: cordon|drain|revoke }; idempotent.
                               → 200 { id, admin_state }   (tenant admin / zmb_t_ → 403 UZ-AUTH-021)
GET   /v1/fleet/runners/{id}/events  platformAdmin; paginated { items, total, page, page_size }.
Runner plane: a revoked/cordoned runner's authed call → 401 UZ-RUN-009.
Liveness (derived, M84_001) is UNCHANGED — admin_state and liveness are orthogonal.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Revoke a runner mid-lease | operator revokes a busy host | in-flight lease's fenced report still settles or is rejected by fence; the next authed call → `401 UZ-RUN-009`; work re-leases (§4). |
| Cordon then host keeps heartbeating | host unaware | heartbeats still bump `last_seen_at` (liveness), but no new lease is granted; admin_state stays cordoned. |
| All runners down during sweep | no healthy target | work **holds** (unclaimed, no error/dead-letter) until capacity returns (§4.2). |
| Event write fails | DB error mid-transaction | the state write + its event share a transaction → both roll back; no half-written history. |
| Non-platform-admin mutates | wrong role | `403 UZ-AUTH-021`; nothing changes; UI action not rendered (§2.3/§5). |
| Double-cordon / double-revoke | retried PATCH | idempotent no-op success; one event, not duplicates. |
| Sweeper races reclaim | concurrent expiry | the existing fencing token admits one winner; reassignment never double-frees a slot. |

---

## Invariants

1. **Liveness stays derived, never stored** — `admin_state` is intent, `runner_events` is history; no runtime-state column — enforced by review + the absence of an `online/offline` column.
2. **`admin_state != 'active'` ⇒ runner-auth rejects** (`401 UZ-RUN-009`) — enforced by §1.1/§2.2 + the lookup gate.
3. **Event ⇄ state consistency** — every state-change event is written in the same transaction as the state change — enforced by §3.1 + the integration test injecting a mid-write failure.
4. **Operator plane is platform-admin-only** (Layer-1 authz, never a Postgres GRANT) — enforced by §2.3.
5. **Reassignment holds, never thrashes/fails** when no eligible target — enforced by §4.2.
6. **`runner_events` is append-only** (no UPDATE/DELETE grant) — enforced by the migration's GRANTs + review.
7. **No JSONB status object** — runner state is `admin_state` (typed) + derived liveness + `runner_events`; conditions-JSONB is out — enforced by review against this Invariant.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `runner auth admits only active admin state` | active → 200; cordoned/revoked → 401. |
| 1.2 | regression | `no orphaned status references` | grep `\.status`/`RUNNER_STATUS_ACTIVE` in fleet paths → 0 stale. |
| 2.1 | integration | `cordon stops new leases keeps in-flight` | cordon → no new claim; existing lease still reports. |
| 2.2 | integration | `revoke 401s the runner plane` | revoke → next runner call `401 UZ-RUN-009`. |
| 2.3 | integration | `operator mutation is platform-admin-gated` | tenant admin / `zmb_t_` PATCH → `403 UZ-AUTH-021`. |
| 3.1 | integration | `state writes append events` | mint/lease/report/cordon → one typed event each, same txn. |
| 3.2 | integration | `event history answers last-busy and counts` | `GET …/events` → last `lease_acquired`, window count. |
| 4.1 | integration | `stale runner swept and work reassigned` | stale `last_seen` → offline event + affinity expired → re-leased. |
| 4.2 | integration | `reassignment holds when no eligible target` | no live runner → work unclaimed, no error; returns → claimed. |
| 4.3 | integration | `current lease id tracks the live lease` | set on claim, cleared on report. |
| 5.1 | e2e | `dashboard cordon revoke updates state` | admin cordons/revokes → badge reflects `admin_state`. |
| 5.2 | e2e/component | `dashboard shows runner activity` | event timeline renders for a runner. |

**Regression:** the existing lease/fence/reclaim + M84_001 derived-liveness suites stay green. **Idempotency:** PATCH cordon/drain/revoke are idempotent (re-applying yields one event, success).

---

## Acceptance Criteria

- [ ] `admin_state` rename + auth gate; revoke → `401 UZ-RUN-009` — verify: `make test-integration` + `zig build test-auth`
- [ ] `PATCH /v1/fleet/runners/{id}` cordon/drain/revoke, platform-admin-gated — verify: `make test-integration`
- [ ] `fleet.runner_events` append-only; emitted on state writes; `GET …/events` reads — verify: `make test-integration`
- [ ] Sweeper marks offline + reassigns; holds when no target — verify: `make test-integration`
- [ ] Dashboard cordon/revoke + activity view, platform-admin-only — verify: `make acceptance-e2e`
- [ ] `make lint` clean · `make test` passes · cross-compile both linux targets
- [ ] `gitleaks detect` clean · no file over 350 lines added

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
| Operator plane + events | `make test-integration` | {paste} | |
| Revoke gate | `zig build test-auth` | {paste} | |
| Sweeper + reassignment | `make test-integration` | {paste} | |
| Dashboard e2e | `make acceptance-e2e` | {paste} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {paste} | |

---

## Out of Scope

- **Tag/label placement (the scheduler)** — that is M85_001; this spec's reassignment re-leases to any eligible runner and composes with M85_001's eligibility filter when it lands.
- **Capacity / fairness / autoscale** — out (the non-goals fence holds).
- **`conditions JSONB` / health probes / maintenance windows / hardware inventory** — explicitly deferred; adopt the `phase + conditions JSONB` split **only** when multiple independent subsystems write runner state (not now).
- **Runner-initiated self-cordon / graceful self-drain on shutdown** — future; this plane is operator-initiated.
