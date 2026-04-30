# M47_001: Approval Inbox — Dashboard Surface for Pending Gate Actions

**Prototype:** v2.0.0
**Milestone:** M47
**Workstream:** 001
**Date:** Apr 29, 2026 (amended from Apr 25, 2026 draft)
**Status:** DONE
**Priority:** P1 — without this, the dashboard misleads (zombies stalled at approval gates appear "Active"). Not strictly launch-blocking for the platform-ops wedge (which is read-only diagnostics in v1) but adjacent enough that shipping the runtime without it leaves a known UX hole. Approval gates can still fire on platform-ops (e.g., tool-call cost overruns, agent proposing a write action) and any future zombie that performs gated destructive operations.
**Categories:** UI, API
**Batch:** B2 — after M40-M45 substrate, parallel with M46/M48/M49.
**Branch:** feat/m47-approval-inbox
**Depends on:** M42_001 (events stream + history — pending gates write `status='gate_blocked'` on `core.zombie_events`). M4_001 (existing approval gate — DONE; this builds on it).

**Canonical architecture:** `docs/architecture/` §10 (capabilities — approval gating row), §13 (path to bastion — per-audience approval).

---

## Amendment notes (Apr 29, 2026)

The Apr 25 draft assumed a table and column shape that doesn't match the codebase. This amendment reconciles spec ↔ code before EXECUTE so the Files-Changed table, schema work, and dedup story are all grounded in real artifacts. Key corrections:

| Apr 25 draft | Reality | Amended decision |
|---|---|---|
| Table `core.approval_gates` | `core.zombie_approval_gates` (schema/010) | Use real name everywhere. |
| Columns `gate_kind`, `proposed_action`, `evidence`, `blast_radius`, `timeout_at`, `resolved_at`, `resolved_by`, `outcome` | None exist. Real cols: `id`, `zombie_id`, `workspace_id`, `action_id`, `tool_name`, `action_name`, `status`, `detail`, `requested_at`, `updated_at`, `created_at`. | Pre-v2.0 in-place schema edit (Schema Removal Guard mandates teardown-rebuild, not ALTER). Add `gate_kind`, `proposed_action`, `evidence`, `blast_radius`, `timeout_at`, `resolved_by` as new columns. Keep `status` as the state machine; terminal values (`approved`/`denied`/`timed_out`) ARE the `outcome` — no separate column. Use existing `updated_at` as resolved-at on terminal transitions — no separate column. |
| URL path `/{gate_id}` ambiguous (action_id? row id?) | DB row pk is UUID v7 `id`; `action_id` is a separate id used as Redis key in `requestApproval` | Public URL identifier is the row `id` (call it `gate_id` in the API surface). Internally the resolve code looks up `action_id` from the row to wake the worker via Redis. |
| Append-only trigger ignored | `WHERE OLD.status != 'pending'` blocks UPDATE, which IS our dedup precondition | Keep the trigger; it gives atomic at-most-one-resolution for free. |
| `src/state/approval_gate.zig` | Real path: `src/zombie/approval_gate.zig` (+ `_db`, `_slack`, `_test`) | Files-Changed updated. |
| Slack handler refactor scope undefined | `src/http/handlers/webhooks/approval.zig` already does Redis+DB resolve | Extract a single channel-agnostic `approval_gate.resolve(action_id, outcome, by, reason)` and repoint both Slack webhook + new dashboard handler at it. Centralizes dedup. |
| Auto-timeout vague | No `timeout_at` and no sweeper exist | Add `timeout_at` column populated at INSERT (24h default). Add a background sweeper thread in the worker process that flips expired rows to `timed_out` and writes the Redis decision key with deny semantics. Required for the user's "no corner cutting" bar. |

Orphan sweep: nothing removed. Existing approval surface is extended in place.

---

## Implementing agent — read these first

1. `docs/architecture/` §10 (capabilities — approval gating row) — what the platform guarantees about gate semantics.
2. M4_001 spec (`docs/v2/done/M4_001_APPROVAL_GATE.md`) — existing gate state machine. Its acceptance criteria define the legal `status` values (`pending`, `approved`, `denied`, `timed_out`).
3. M42_001 spec — the `core.zombie_events.status='gate_blocked'` mechanism that surfaces a stalled zombie. The events row links to the gate via the same `event_id`/`action_id` join key.
4. `docs/REST_API_DESIGN_GUIDELINES.md` §1–§7 — URL/method/body/response/error/OpenAPI/5-place route registration.
5. `docs/AUTH.md` — the Next Route Handler proxy pattern. Dashboard mutations on `/approvals` go through a Route Handler that mints API-audience JWTs server-side; the browser never holds the JWT.
6. `ui/packages/app/app/backend/v1/workspaces/[workspaceId]/zombies/[zombieId]/events/stream/route.ts` — the M42 SSE proxy. Mirror this pattern for any approvals SSE.
7. Existing approval source files — read all of these before editing:
   - `src/zombie/approval_gate.zig` (Redis-side request/wait/resolve + ActionDetail)
   - `src/zombie/approval_gate_db.zig` (DB-side recordGatePending + resolveGateDecision)
   - `src/zombie/approval_gate_slack.zig` (Slack message builder)
   - `src/http/handlers/webhooks/approval.zig` (Slack callback resolution path)
   - `schema/010_core_zombie_approval_gates.sql`

---

## Overview

**Goal (testable):** An operator opens the dashboard with a zombie stalled at an approval gate and sees:
1. A badge on the zombie's status card showing "N pending approvals".
2. A `/approvals` page listing all pending gate actions across the workspace with full action details (proposed action, evidence gathered, blast-radius assessment, age, timeout countdown).
3. Approve and Deny buttons that resolve the gate immediately from the browser.
4. The zombie resuming (or halting via `denied` semantics) within 2s of the click.

Same flow accessible programmatically:
- `GET /v1/workspaces/{ws}/approvals` — list.
- `POST /v1/workspaces/{ws}/approvals/{gate_id}:approve` and `:deny` — resolve.

Multi-channel dedup: Slack click and dashboard click race against the same `WHERE status='pending'` UPDATE; first one wins, the loser gets `409 already_resolved` with the original outcome and resolver attribution.

**Problem:** Approval gate interactions today flow only through Slack DMs (`src/http/handlers/webhooks/approval.zig`). If a zombie hits a gate while the operator is looking at the dashboard, the zombie shows "Active" — no indication it's stalled. For platform-ops, gates fire on tool-call cost overruns or write-action proposals; the operator needs a dashboard view of pending gates so they're not invisible behind a "healthy zombie" status pill. The mechanism generalizes to any future zombie that performs gated destructive work.

**Solution summary:** New API endpoints over the existing `core.zombie_approval_gates` audit table. Schema extended in place (pre-v2.0 teardown-rebuild rule) with the operator-visible fields (`gate_kind`, `proposed_action`, `evidence`, `blast_radius`, `timeout_at`, `resolved_by`). Single channel-agnostic resolve core called by both Slack webhook and dashboard handler. Background sweeper transitions expired rows to `timed_out` and writes deny-semantic decision keys. Dashboard surfaces: (a) badge on zombie cards, (b) `/approvals` page with table + inline detail/approve flow, (c) "Pending" tab on zombie detail page.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `schema/010_core_zombie_approval_gates.sql` | EDIT (in-place, pre-v2.0 teardown rule) | Add columns: `gate_kind TEXT`, `proposed_action TEXT`, `evidence JSONB`, `blast_radius TEXT`, `timeout_at BIGINT`, `resolved_by TEXT`. Keep append-only trigger. Add index on `(workspace_id, status, requested_at)` for inbox list queries. |
| `src/zombie/approval_gate.zig` | EDIT | Extend `ActionDetail` with `gate_kind`/`proposed_action`/`evidence`/`blast_radius`/`timeout_ms`. Add `pub fn resolve(pool, redis, action_id, outcome, by, reason) !ResolveOutcome` — the channel-agnostic core. Re-export. |
| `src/zombie/approval_gate_db.zig` | EDIT | `recordGatePending` accepts and writes the new columns. New `pub fn listPending(pool, alloc, workspace_id, filters, cursor, limit) !ListResult` for the inbox. New `pub fn resolveAtomic(pool, alloc, action_id, outcome, by) !ResolveDbOutcome` returning either `{updated, row}` or `{already, terminal_row}`. |
| `src/zombie/approval_gate_sweeper.zig` | NEW | Background thread polling `WHERE status='pending' AND timeout_at <= NOW()` every 60s; transitions to `timed_out` via the shared resolve core; writes deny-semantic Redis key. ≤200 lines, single file. |
| `src/http/handlers/webhooks/approval.zig` | EDIT | Repoint Slack callback path at `approval_gate.resolve(...)`. No behavior change for Slack users; centralizes dedup. |
| `src/http/handlers/approvals/list.zig` | NEW | `GET /v1/workspaces/{ws}/approvals` handler. `Hx` contract per REST guide §8. |
| `src/http/handlers/approvals/resolve.zig` | NEW | `POST /v1/workspaces/{ws}/approvals/{gate_id}:approve` and `:deny`. Calls `approval_gate.resolve(...)`. |
| `src/http/router.zig` | EDIT | Add 3 route variants: `approvals_list`, `approvals_approve`, `approvals_deny`. |
| `src/http/route_table.zig` | EDIT | Register middleware policy (workspace member auth) for the 3 routes. |
| `src/http/route_table_invoke.zig` | EDIT | Wire dispatch entries for the 3 routes. |
| `src/http/path_matchers.zig` (or equivalent matcher module) | EDIT | Matcher for `/approvals` and `/approvals/{gate_id}:approve|:deny`. |
| `public/openapi/paths/approvals.yaml` | NEW | OpenAPI spec for the 3 endpoints. Loose ≤400-line advisory. |
| `public/openapi.json` | REGENERATE | `make openapi`. |
| `src/cmd/serve.zig` | EDIT | Start the sweeper thread on worker boot; clean shutdown on signal. |
| `ui/packages/app/lib/approvals.ts` | NEW | Client helpers + SWR hooks (`usePendingApprovals`, `useApproval(gate_id)`, `approveAction`, `denyAction`). |
| `ui/packages/app/app/backend/v1/workspaces/[workspaceId]/approvals/route.ts` | NEW | Next Route Handler — list proxy. Mints API-audience JWT. |
| `ui/packages/app/app/backend/v1/workspaces/[workspaceId]/approvals/[gateId]/route.ts` | NEW | Next Route Handler — detail GET (single-row read). |
| `ui/packages/app/app/backend/v1/workspaces/[workspaceId]/approvals/[gateId]/approve/route.ts` | NEW | Next Route Handler — POST `:approve` proxy. |
| `ui/packages/app/app/backend/v1/workspaces/[workspaceId]/approvals/[gateId]/deny/route.ts` | NEW | Next Route Handler — POST `:deny` proxy. |
| `ui/packages/app/app/(dashboard)/approvals/page.tsx` | NEW | Workspace-wide list page. |
| `ui/packages/app/app/(dashboard)/approvals/[gateId]/page.tsx` | NEW | Detail page with Approve/Deny. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/approvals/page.tsx` | NEW | Per-zombie tab. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/layout.tsx` (or nav component) | EDIT | Add "Pending (N)" tab to zombie detail nav. |
| `ui/packages/app/components/<ZombieCard>.tsx` | EDIT | Pending-approvals badge. (Locate actual component path during PLAN — likely under `app/(dashboard)/zombies/components/`.) |
| `tests/integration/approval_inbox_test.zig` (or `src/http/handlers/approvals/*_integration_test.zig`) | NEW | E2E: gate fires → appears in inbox → approve via API → zombie resumes within 2s. |
| `samples/fixtures/m47-gate-fixture/SKILL.md` | NEW | Synthetic test fixture firing a gate. Under `samples/fixtures/` (NOT a public sample). |

---

## Sections (implementation slices)

### §1 — Schema extension (pre-v2.0 in-place edit)

Edit `schema/010_core_zombie_approval_gates.sql` directly. VERSION=0.30.0; no ALTER migrations. DB is wiped on rebuild.

```sql
CREATE TABLE IF NOT EXISTS core.zombie_approval_gates (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_zombie_approval_gates_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    zombie_id       UUID NOT NULL REFERENCES core.zombies(id),
    workspace_id    UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    action_id       TEXT NOT NULL,
    tool_name       TEXT NOT NULL,
    action_name     TEXT NOT NULL,
    -- new (M47):
    gate_kind       TEXT NOT NULL DEFAULT '',     -- "destructive_action" | "cost_overrun" | "external_call" | ""
    proposed_action TEXT NOT NULL DEFAULT '',     -- human-readable summary
    evidence        JSONB NOT NULL DEFAULT '{}',  -- agent-gathered context
    blast_radius    TEXT NOT NULL DEFAULT '',     -- agent's assessment
    timeout_at      BIGINT NOT NULL,              -- ms epoch; sweeper auto-denies after this
    resolved_by     TEXT NOT NULL DEFAULT '',     -- "user:<email>" | "slack:<user_id>" | "api:<key_id>" | "system:timeout"
    -- existing:
    status          TEXT NOT NULL DEFAULT 'pending',
    detail          TEXT NOT NULL DEFAULT '',
    requested_at    BIGINT NOT NULL,
    updated_at      BIGINT,                        -- doubles as resolved_at on terminal status
    created_at      BIGINT NOT NULL
);

-- M47 inbox list: pending rows by workspace, oldest first.
CREATE INDEX IF NOT EXISTS idx_zombie_approval_gates_workspace_status_requested
    ON core.zombie_approval_gates (workspace_id, status, requested_at);

-- existing indexes + append-only trigger preserved
```

The MILESTONE-ID gate forbids `M47` in source; the SQL comment header for the new column block describes the purpose without referencing the milestone.

Note for the implementer: bare `M47` in column-block comments will trip the gate. Use `-- inbox-visible fields` etc. instead.

### §2 — Channel-agnostic resolve core

Add to `src/zombie/approval_gate.zig`:

```zig
pub const ResolveOutcome = union(enum) {
    resolved: struct { gate_id: []const u8, outcome: GateStatus, resolved_at: i64 },
    already_resolved: struct { gate_id: []const u8, outcome: GateStatus, resolved_at: i64, resolved_by: []const u8 },
};

/// Single dedup point. Atomic UPDATE on `WHERE status='pending'` plus Redis decision write.
/// Slack webhook AND dashboard handler call this. First caller wins; second gets `already_resolved`.
pub fn resolve(
    pool: *pg.Pool,
    redis: *queue_redis.Client,
    alloc: Allocator,
    action_id: []const u8,
    outcome: GateStatus,        // .approved | .denied | .timed_out
    by: []const u8,             // "user:<email>" | "slack:<user_id>" | "api:<key_id>" | "system:timeout"
    reason: []const u8,         // optional, may be ""
) !ResolveOutcome
```

Implementation: `approval_gate_db.resolveAtomic` runs `UPDATE ... SET status=$1, detail=$2, resolved_by=$3, updated_at=$4 WHERE action_id=$5 AND status='pending' RETURNING id, action_id`. If 0 rows → `SELECT ... WHERE action_id=$1` for the terminal row → `.already_resolved`. If 1 row → write Redis decision key (existing `resolveApproval`) and return `.resolved`. The append-only trigger guarantees the precondition is enforced even under concurrent writers from different processes.

### §3 — Read endpoint: `GET /approvals`

```
GET /v1/workspaces/{ws}/approvals?status=pending&zombie_id=&gate_kind=&limit=50&cursor=
  → 200 {
    items: [
      {
        gate_id: string,           # row id (UUID v7)
        zombie_id: uuid,
        zombie_name: string,
        gate_kind: string,
        tool_name: string,
        action_name: string,
        proposed_action: string,
        evidence: object,
        blast_radius: string,
        status: "pending" | "approved" | "denied" | "timed_out",
        requested_at: int64_ms,
        timeout_at: int64_ms,
        updated_at: int64_ms | null,    # resolved_at when terminal
        resolved_by: string | null,
      }
    ],
    next_cursor: string | null,
  }
```

Default `status=pending`, ordered by `requested_at ASC` (oldest most urgent). Cursor encodes `(requested_at, id)`. `zombie_name` joined from `core.zombies` for display convenience.

### §4 — Resolve endpoints: approve / deny

```
POST /v1/workspaces/{ws}/approvals/{gate_id}:approve
  body: { reason?: string ≤ 4096 chars }
  → 200 { gate_id, action_id, outcome: "approved", resolved_at, resolved_by }
  → 409 { error: "already_resolved", gate_id, outcome, resolved_at, resolved_by }
  → 404 { error: "not_found" }    # gate_id doesn't exist OR not in this workspace

POST /v1/workspaces/{ws}/approvals/{gate_id}:deny
  same shape
```

Path-segment colon syntax matches REST guide §1 examples. Handler resolves `gate_id` → `action_id` (single SELECT) then calls `approval_gate.resolve(...)`. Idempotency: a retry with the same `gate_id` and a previously-successful outcome still returns 200 if the resolved-by matches; otherwise 409.

`by` derivation: `user:<email>` from the JWT subject claim for dashboard; `api:<key_id>` for direct API key callers; `slack:<user_id>` for Slack webhook (existing path); `system:timeout` for sweeper.

### §5 — Worker wake mechanism (unchanged)

The existing `waitForDecision` loop in `approval_gate.zig` polls Redis every 2s. After this milestone, the resolve core writes the same Redis key on the same path, so worker wake latency stays at the existing ≤2s ceiling. No pubsub added — keeps the change small. Sweeper-driven timeouts also write the Redis key (`deny`), so worker observes timeout via the same path.

### §6 — Auto-timeout sweeper

`src/zombie/approval_gate_sweeper.zig` — single background thread started by `src/cmd/serve.zig`:

```zig
pub fn run(pool: *pg.Pool, redis: *queue_redis.Client, alloc: Allocator, shutdown: *std.atomic.Value(bool)) void;
```

Loop: every 60s, `SELECT id, action_id FROM core.zombie_approval_gates WHERE status='pending' AND timeout_at <= $1` → for each, call `approval_gate.resolve(action_id, .timed_out, "system:timeout", "")`. Clean shutdown on `shutdown.load() == true`. Default timeout is 24h (`timeout_ms_default = 24 * 60 * 60 * 1000`, set at INSERT in `recordGatePending`); SKILL.md prose can override per-zombie via the gate-firing call site. Worker side treats `timed_out` as `denied` for destructive ops (safe default — invariant 3 below).

### §7 — Slack handler refactor

`src/http/handlers/webhooks/approval.zig` currently calls `approval_gate.resolveApproval` (Redis-only) then `approval_gate.resolveGateDecision` (DB-only) sequentially. Replace with one call to `approval_gate.resolve(pool, redis, alloc, action_id, outcome, "slack:<user_id>", reason)`. Slack message-patching hook (already in `approval_gate_slack.zig`) stays.

This is a behavior-preserving refactor for Slack users — but it is the moment dedup becomes correct. Without the unified core, Slack and dashboard would race on Redis-then-DB ordering and could both succeed.

### §8 — Dashboard list page

`ui/packages/app/app/(dashboard)/approvals/page.tsx`:

- Table primitives from design-system (`Section`, `Card`, `Badge`, `Button`, `EmptyState`) — UI Substitution Gate applies.
- Columns: zombie name (link), gate kind badge, action name, proposed-action one-liner (truncated), age, timeout countdown, [Approve] [Deny] buttons.
- Filter chips: `zombie_id` (autocomplete), `gate_kind`.
- Empty state: "No pending approvals 🎉" — emoji approved by user request inline (rule allows when explicitly requested; this is part of the spec, so opt-in).
- Real-time strategy: SWR `refreshInterval=5000`. SSE for approvals deferred to follow-up — 5s poll meets the goal (zombie resumes within 2s of CLICK, not of dashboard refresh).
- Inline approve/deny: button click → POST → optimistic row removal + toast → revalidate. On 409, toast "Already resolved by …" and revalidate.

### §9 — Per-zombie tab

`ui/packages/app/app/(dashboard)/zombies/[id]/approvals/page.tsx` — same shape as global inbox, filtered to `zombie_id` from the route. Surfaces in the zombie detail page nav as "Pending (N)". Count comes from the same `usePendingApprovals` hook, scoped.

### §10 — ZombieCard badge

Edit the actual ZombieCard component (locate during PLAN — `git grep -l "ZombieCard\|zombie-card"` from `ui/packages/app/`). When `pending_approvals_count > 0`, render a `<Badge variant="warning">{n} pending</Badge>`. Click navigates to `/zombies/{id}/approvals`. Pending count comes from a single workspace-wide query at dashboard load + `revalidateOnFocus` — not per-card live polling (would thrash). Cache via SWR with a 5s deduper.

### §11 — Detail page

`ui/packages/app/app/(dashboard)/approvals/[gateId]/page.tsx`:

- Full `proposed_action` prose.
- `evidence` rendered as expandable JSON (`<JsonView>` from existing primitives) or kind-specific formatter when `gate_kind` matches a known shape.
- `blast_radius` callout (`<Card variant="alert">`).
- Approve button (green primary) + Deny button (red secondary).
- Optional reason text field (≤4096 chars).
- On 200 → redirect to `/approvals` with toast "Resolved as <outcome>".
- On 409 → render "Already resolved by <resolved_by> at <timestamp>", disable buttons, show toast.

### §12 — Test fixture

`samples/fixtures/m47-gate-fixture/SKILL.md`: synthetic SKILL.md exercising the gate path. Under `samples/fixtures/`, NOT `samples/`. Promoted to `samples/` only when a public zombie shape with gated destructive ops ships.

---

## Interfaces

```
HTTP:
  GET    /v1/workspaces/{ws}/approvals?status=&zombie_id=&gate_kind=&cursor=&limit=
  POST   /v1/workspaces/{ws}/approvals/{gate_id}:approve  body: { reason? }
  POST   /v1/workspaces/{ws}/approvals/{gate_id}:deny     body: { reason? }
  GET    /v1/workspaces/{ws}/approvals/{gate_id}          # single row read (drives detail page)

Internal (Zig):
  approval_gate.resolve(pool, redis, alloc, action_id, outcome, by, reason) !ResolveOutcome
  approval_gate_db.listPending(pool, alloc, workspace_id, filters, cursor, limit) !ListResult
  approval_gate_db.getByGateId(pool, alloc, gate_id, workspace_id) !?GateRow
  approval_gate_db.resolveAtomic(pool, alloc, action_id, outcome, by, reason) !ResolveDbOutcome
  approval_gate_sweeper.run(pool, redis, alloc, shutdown)

Internal (TS):
  usePendingApprovals(workspaceId, filters?) → SWR<{items, nextCursor}>
  useApproval(workspaceId, gateId)            → SWR<GateRow>
  approveAction(workspaceId, gateId, reason?) → ResolveResult | AlreadyResolved
  denyAction(workspaceId, gateId, reason?)    → ResolveResult | AlreadyResolved

UI routes:
  /approvals                # workspace-wide list
  /approvals/{gate_id}      # detail + resolve
  /zombies/{id}/approvals   # per-zombie tab
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| Dashboard click races Slack click | Concurrent UPDATEs on the same row | DB precondition `WHERE status='pending'` + append-only trigger ensures one wins; loser gets 409 with original outcome + resolver attribution |
| Gate auto-times-out before user resolves | `timeout_at` reached | Sweeper transitions to `timed_out`; worker wait loop sees timeout via the Redis decision key with deny semantics for destructive ops |
| Worker doesn't wake within 2s | Redis poll interval 2s | Existing `waitForDecision` poll budget is the ceiling; documented and tested |
| Network blip during POST | Browser → server connection drops | Idempotent: client retries with same `gate_id` and outcome; server uses the precondition. If first attempt succeeded, retry returns 409 with same `resolved_by` — client treats as success |
| Sweeper crashes/restarts | Process dies | On boot, sweeper picks up where it left off — query is stateless. Operator visibility: log line per timeout transition |
| `evidence` JSON malformed at INSERT | Bug in gate-firing call site | `evidence JSONB DEFAULT '{}'` — empty object renders as "(no evidence)" in dashboard |
| Concurrent dashboard tabs | Two tabs both POST | Same dedup as Slack-vs-dashboard race |

---

## Invariants

1. **At most one resolution per gate.** State machine precondition `WHERE status='pending'` + append-only trigger.
2. **Resolution channel attribution preserved.** `resolved_by` records `user:<email>` (dashboard), `slack:<user_id>` (Slack), `api:<key_id>` (API key), or `system:timeout` (sweeper) distinctly.
3. **Auto-timeout = denied semantics for destructive ops.** Worker's `waitForDecision` returns `.timed_out`; the existing safe-default treatment (don't fire the destructive tool call) applies.
4. **No write actions execute before resolution.** Gate must transition `pending → approved` for the worker's tool call to fire.
5. **Single dedup point.** Both Slack and dashboard call `approval_gate.resolve(...)`; no other code path mutates terminal state.
6. **Pre-v2.0 schema discipline.** Edit `schema/010_core_zombie_approval_gates.sql` in place; no ALTER migrations.

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_pending_gate_appears_in_list` | Fire a gate → `GET /approvals` → item present with all spec fields populated |
| `test_approve_unblocks_zombie` | Fire → approve via API → assert worker wakes within 2s + tool call fires |
| `test_deny_halts_zombie` | Fire → deny → worker proceeds with denied path; tool call NOT fired |
| `test_dual_channel_dedup_slack_first` | Fire → resolve via Slack callback → resolve via dashboard handler → 409 with `outcome=approved` and `resolved_by=slack:*` |
| `test_dual_channel_dedup_dashboard_first` | Inverse of the above — dashboard wins, Slack callback gets 409 |
| `test_idempotent_approve_retry` | POST `:approve` twice with same body → second returns 409 with same `resolved_by` (client treats as success) |
| `test_timeout_to_denied_for_destructive` | Fire gate with `timeout_at = now+50ms` → wait 1.5 sweeper cycles → assert `status=timed_out`, `resolved_by=system:timeout`, worker treats as denied |
| `test_zombie_card_badge_count` | Fire 2 gates on one zombie → workspace pending-count query returns 2 |
| `test_per_zombie_filter` | 5 gates across 3 zombies → `GET /approvals?zombie_id=X` returns only that zombie's |
| `test_filter_by_gate_kind` | Mix of `destructive_action` and `cost_overrun` → filter returns only the requested kind |
| `test_cursor_pagination` | 51 pending gates → page size 50 → second page returns the 51st with no `next_cursor` |
| `test_404_on_unknown_gate_id` | POST `:approve` on a UUID that doesn't exist → 404 |
| `test_404_on_cross_workspace_gate_id` | POST `:approve` on a gate in workspace B from workspace A → 404 (not 403, no leak) |
| `test_evidence_jsonb_roundtrip` | Fire gate with evidence `{"files":["a","b"]}` → list response surfaces same JSON shape |

---

## Acceptance Criteria

- [ ] `make test` passes (unit tier).
- [ ] `make test-integration` passes the 14 tests above (touches HTTP handlers + schema → tier 2 mandatory; tier 3 fresh-DB run before declaring ship-ready).
- [ ] `bun test` passes UI unit tests for the new components.
- [ ] Manual smoke (documented in PR Session Notes): install `samples/fixtures/m47-gate-fixture` → trigger gate → see badge → click Approve in dashboard → zombie resumes; repeat with Deny; repeat with Slack-first race.
- [ ] Slack + dashboard parity: a gate fires both channels; resolving in either resolves the other within 2s.
- [ ] No regressions on M4 Slack-only flow (existing tests pass).
- [ ] Cross-compile clean: `x86_64-linux` + `aarch64-linux`.
- [ ] `make memleak` clean (touches HTTP handlers + new sweeper thread).
- [ ] `make lint` clean.
- [ ] `make check-pg-drain` clean.
- [ ] Pub-surface audit clean per ZIG_RULES; new pub symbols are consumed externally.
- [ ] OpenAPI regenerated; REST §1 URL-shape gate (added in efa99744) passes.
- [ ] No milestone IDs (`M47`, `§…`, `T…`) in any non-doc source file.

---

## Discovery (consult log)

- **Apr 29, 2026** — Pre-EXECUTE consult: Apr 25 draft assumed table `core.approval_gates` and 8 non-existent columns. Schema Removal Guard (pre-v2.0) requires in-place edit, not ALTER. Auto-timeout has no existing mechanism. User decided: amend spec first (path A), then execute with full schema extension (path B) — no corner cutting on UX. Refactor scope: extract single channel-agnostic `resolve()` core; existing Slack handler repointed (no orphans). Sweeper added for auto-timeout. (See Amendment notes block above for the full table.)
- **Apr 29, 2026 — security follow-up** — Greptile flagged a cross-zombie gate resolution path on PR #265: `webhooks/approval.zig` (and `slack/interactions.zig`) parse both `zombie_id` and `action_id` from the same untrusted Slack/webhook payload, then called `resolve()` keyed only on `action_id`. Post-mutation `zombie_id` cross-check returned 404 to the caller while the DB row was already terminal. Fix lands the `zombie_id` filter in the SQL `WHERE` clause via a new `ResolveArgs` struct (Bun-style options struct + method-on-data), so a mismatched zombie returns `.not_found` without any write. Dashboard, sweeper, and worker timeout paths pass `""` (no filter) since they're already authz-scoped. Negative integration test in `inbox_integration_test.zig` asserts row stays pending under the cross-zombie attack and that the legitimate caller resolves cleanly afterward.

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal rules (RULE FLL 350-line gate, RULE TST-NAM, RULE ORP orphan sweep, RULE CHR changelog).
- `docs/ZIG_RULES.md` — Zig discipline (drain/dupe, cross-compile, errdefer, ownership, sentinel, pub audit).
- `docs/REST_API_DESIGN_GUIDELINES.md` — §1–§8, §10 pre-PR gates.
- `docs/AUTH.md` — Next Route Handler proxy pattern; never expose API JWT to browser.
- `docs/SCHEMA_CONVENTIONS.md` — schema file conventions; pre-v2.0 in-place edit rule.
- CLAUDE.md / AGENTS.md — Schema Table Removal Guard, Length Gate, Milestone-ID Gate, Pub Surface Gate, UI Component Substitution Gate, Verification Gate.
