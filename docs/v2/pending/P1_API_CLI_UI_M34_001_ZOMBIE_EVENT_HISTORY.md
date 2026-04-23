# M34_001: Zombie Event History — Persisted Per-Event Log with Actor Provenance

**Prototype:** v2.0.0
**Milestone:** M34
**Workstream:** 001
**Date:** Apr 23, 2026
**Status:** PENDING
**Priority:** P1 — the "chat send + response" loop only completes if operators can read responses back. Today nothing reads. M37_001 §2.2 (chat end-to-end), M33_001's chat CLI, and M36_001's live watch all depend on a queryable per-event history. Also gives "who's going rogue" visibility for multi-tenant ops.
**Batch:** B1 — parallel with M33_001 (control stream) and M35_001 (executor policy). Blocks chat UI, live watch, and debugging.
**Branch:** feat/m34-zombie-event-history (to be created)
**Depends on:** nothing structural — the write path hooks into existing `processNext` and can land independently of M34 (though M34's `actor` propagation makes the provenance column meaningful from day one).

**Canonical architecture:** `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` §2 (core.zombie_events ownership), §5 (`processNext` write path), §8 invariants 3 and 4.

---

## Overview

**Goal (testable):** Every event that arrives on `zombie:{id}:events` creates one row in `core.zombie_events` marked `status='received'` before any gate check runs. When the event completes, the row is updated with `response_text`, `token_count`, `wall_ms`, and terminal status. Operators can query the history via `zombiectl zombie events {id}` or `GET /v1/workspaces/{ws}/zombies/{id}/events` (cursor-paginated, filterable by `actor`). The same endpoint powers the chat CLI's history replay, the UI Events tab, and the "which zombies in this workspace are burning tokens" admin query.

**Problem:** `core.zombie_sessions` holds only the latest checkpoint (rolling context summary). Every event's request/response is thrown away the moment the next event starts. Debugging means grepping logs. "Did this zombie cost $200 last month?" has no answer. Chat CLI can't show history because there is none to show. Replay after crash works (XAUTOCLAIM + in-memory context) but there's no audit trail.

**Solution summary:** One new table, one write path change, two new GET endpoints, one CLI subcommand, one UI tab. Schema: `core.zombie_events` with `UNIQUE(zombie_id, event_id)` making replay idempotent. Write path: INSERT at top of `processNext` (status='received'), UPDATE at bottom (status='processed' or 'agent_error' with `response_text`, tokens, timings, `failure_label`, `gate_outcome`). The `actor` column captures provenance — `steer:<user>`, `webhook`, `slack:<user_id>`, `svix`, `cron:<schedule>`, `api:<api_key_name>`. GET endpoints: per-zombie (cursor page) and per-workspace (for "who's rogue" roll-ups). Retention policy: deferred to a later workstream; default forever until someone asks.

---

## Files Changed (blast radius)

### Schema

| File | Action | Why |
|---|---|---|
| `schema/NNN_zombie_events.sql` | CREATE (pre-v2.0 teardown) | New table with indices on `(zombie_id, created_at DESC)`, `(workspace_id, created_at DESC)`, `(actor, created_at DESC)`. Schema Guard required at EXECUTE. |

Schema DDL:

```sql
CREATE TABLE core.zombie_events (
  id              uuid PRIMARY KEY,
  zombie_id       uuid NOT NULL REFERENCES core.zombies(id) ON DELETE CASCADE,
  workspace_id    uuid NOT NULL REFERENCES core.workspaces(id),
  event_id        text NOT NULL,
  event_type      text NOT NULL,
  source          text NOT NULL,    -- webhook|slack|svix|steer|cron|api
  actor           text NOT NULL,    -- "steer:<user>" | "webhook" | "slack:<uid>" | "svix" | "cron:<sched>" | "api:<key>"
  request_json    jsonb NOT NULL,
  response_text   text,
  token_count     bigint,
  wall_ms         bigint,
  ttft_ms         bigint,
  status          text NOT NULL,    -- received | processed | agent_error | gate_blocked | cancelled | deliver_error
  gate_outcome    text,             -- passed | approval_denied | approval_timeout | gate_unavailable | auto_killed_anomaly | auto_killed_policy
  execution_id    text,
  failure_label   text,
  created_at      bigint NOT NULL,
  completed_at    bigint,
  UNIQUE (zombie_id, event_id)
);
CREATE INDEX zombie_events_by_zombie    ON core.zombie_events (zombie_id, created_at DESC);
CREATE INDEX zombie_events_by_workspace ON core.zombie_events (workspace_id, created_at DESC);
CREATE INDEX zombie_events_by_actor     ON core.zombie_events (actor, created_at DESC);
```

### Worker (Zig)

| File | Action | Why |
|---|---|---|
| `src/zombie/zombie_events.zig` | CREATE | `insertReceived(conn, zombie_id, ws_id, event_id, event_type, source, actor, request_json) -> id`. `updateCompleted(conn, zombie_id, event_id, response_text, tokens, wall_ms, ttft_ms, status, failure_label, gate_outcome, execution_id)`. |
| `src/zombie/event_loop.zig` | MODIFY | `processEvent` calls `insertReceived` before gate checks; `updateCompleted` after `deliverEvent` returns (both happy + failure paths). XACK only after UPDATE succeeds. |
| `src/queue/redis_zombie.zig` | MODIFY | `ZombieEvent` gets `actor` field (already planned in M34 steer); also picked up from webhook/slack/svix XADDs via M34. |

### HTTP (Zig)

| File | Action | Why |
|---|---|---|
| `src/http/handlers/zombies/events.zig` | CREATE | `GET /v1/workspaces/{ws}/zombies/{id}/events` (cursor page, actor filter, status filter) + `GET /v1/workspaces/{ws}/zombies/{id}/events/{event_id}` (single-event detail). |
| `src/http/handlers/workspaces/events.zig` | CREATE | `GET /v1/workspaces/{ws}/events` — cross-zombie, same query shape, for admin "who's going rogue" roll-ups. Pagination: cursor. Filters: actor, zombie_id, status. |
| `src/http/router.zig` + `route_table.zig` + `route_manifest.zig` | MODIFY | Register routes. |
| `public/openapi/paths/zombie-events.yaml` (new fragment) | CREATE | OpenAPI spec for the new endpoints; bundled by the existing split/bundle process (M28_002). |

### CLI (JavaScript)

| File | Action | Why |
|---|---|---|
| `zombiectl/src/commands/zombie_events.js` | CREATE | `zombiectl zombie events {id} [--limit N] [--actor X] [--status Y] [--cursor C]`. Default: table (event_id, actor, status, tokens, wall_ms, created_at). `--json` for machine output. `--follow` polls every 1s (same mechanism chat uses). |
| `zombiectl/src/commands/events.js` | CREATE | `zombiectl events --workspace {ws}` — cross-zombie. |
| `zombiectl/src/commands/zombie.js` | MODIFY | Register `events` subcommand. |

### UI (TypeScript/React)

| File | Action | Why |
|---|---|---|
| `ui/packages/app/lib/api/zombie-events.ts` | CREATE | `getEvents(wsId, zId, {limit, actor, status, cursor})`, `getEvent(wsId, zId, eventId)`. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/EventsTab.tsx` | CREATE | Table of events with actor badges; click to expand response_text + request_json. Infinite scroll via cursor. Filter dropdown for actor/status. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/page.tsx` | MODIFY | Add Events tab alongside existing tabs. |
| `ui/packages/app/app/(dashboard)/workspaces/[id]/events/page.tsx` | CREATE | Workspace-level events page (cross-zombie) for admin. |

### Tests

| File | Action | Why |
|---|---|---|
| `src/zombie/zombie_events_test.zig` | CREATE | Unit: insert + update; UNIQUE on replay. |
| `src/zombie/zombie_events_integration_test.zig` | CREATE | Integration with real pg: write path end-to-end via `processEvent`. |
| `src/http/handlers/zombies/events_test.zig` | CREATE | Pagination, filters, auth, cross-workspace 403. |
| `zombiectl/test/zombie-events.unit.test.js` | CREATE | CLI flags + output shapes. |
| `ui/packages/app/tests/events-tab.test.tsx` | CREATE | Table rendering, filter interactions, expand/collapse. |

---

## Applicable Rules

**ZIG-DRAIN**, **TST-NAM**, **FLL** (on .zig files — `zombie_events.zig` write path ≤250 lines; handlers ≤250; tests ≤300). **Schema Guard** applies to schema file (print guard output at EXECUTE).

---

## Sections

### §1 — Schema + write path

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | Schema | `make db-migrate` | `core.zombie_events` exists with PK, FKs, indices, UNIQUE constraint | integration |
| 1.2 | PENDING | `processEvent` | event lands on `zombie:{id}:events` | row inserted with `status='received'`, `actor` set, `request_json` non-null, `created_at` present, `completed_at` NULL | integration |
| 1.3 | PENDING | `processEvent` happy path | event completes via executor | row updated with `status='processed'`, `response_text`, `token_count`, `wall_ms`, `completed_at` | integration |
| 1.4 | PENDING | `processEvent` agent_error path | agent returns exit_ok=false | row updated with `status='agent_error'` + `failure_label` | integration |
| 1.5 | PENDING | `processEvent` gate_blocked path | gate denies | `status='gate_blocked'` + `gate_outcome` | integration |
| 1.6 | PENDING | replay (XAUTOCLAIM + redelivery) | same event_id twice | UNIQUE violation on second INSERT; UPDATE path taken to re-mark status='received' then proceed; exactly one final row | integration |

### §2 — HTTP GET endpoints

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `GET /v1/.../zombies/{id}/events?limit=20` | populated table | 200 + cursor-paginated list sorted DESC on created_at | integration |
| 2.2 | PENDING | same with `?actor=steer:kishore` | filter | only matching rows | integration |
| 2.3 | PENDING | same with `?cursor=<opaque>` | paginate | next page; invalid cursor → 400 | integration |
| 2.4 | PENDING | `GET .../zombies/{id}/events/{event_id}` | detail | full request_json + response_text + all columns | integration |
| 2.5 | PENDING | `GET /v1/workspaces/{ws}/events` | cross-zombie | same shape, rows across all zombies in workspace | integration |
| 2.6 | PENDING | unauth cross-workspace | caller not member of workspace | 403 | integration |

### §3 — CLI

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `zombiectl zombie events {id}` | populated | table output with event_id/actor/status/tokens/wall_ms/created_at | unit |
| 3.2 | PENDING | same `--actor steer:kishore --limit 5` | filter + limit | respects both | unit |
| 3.3 | PENDING | same `--json` | machine-readable | emits one JSON array | unit |
| 3.4 | PENDING | same `--follow` | polls | prints new rows as they appear (used by chat CLI internally) | unit (fake timers) |
| 3.5 | PENDING | `zombiectl events --workspace {ws}` | cross-zombie | workspace-scoped | unit |

### §4 — UI

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | `EventsTab.tsx` on zombie detail | render | fetches /events; shows table with actor badges | unit (MSW) |
| 4.2 | PENDING | click row | detail expand | renders request_json + response_text | unit |
| 4.3 | PENDING | filter by actor | dropdown | refetches with query params | unit |
| 4.4 | PENDING | workspace events page | render | cross-zombie roll-up; zombie_id column visible | unit |

---

## Interfaces

**Produced:**

- `GET /v1/workspaces/{ws}/zombies/{id}/events` — cursor list, filters `actor`, `status`, optional `from`/`to` timestamps.
- `GET /v1/workspaces/{ws}/zombies/{id}/events/{event_id}` — single event.
- `GET /v1/workspaces/{ws}/events` — cross-zombie roll-up.
- Zig: `src/zombie/zombie_events.zig` `insertReceived`, `updateCompleted`.

**Consumed:**

- `processEvent` call chain (existing, modified here).
- `actor` field on the XADD payload (M33_001 introduces it).

---

## Failure Modes

| Failure | Trigger | Behavior | Observed |
|---|---|---|---|
| Schema migration fails on live DB | constraint conflict with existing data | migration rolls back; worker startup fails fast | CI blocks the deploy |
| INSERT fails mid-processEvent | pg hiccup | `processEvent` does NOT XACK; event replays; UNIQUE makes second attempt idempotent (UPDATE path) | brief delay; no loss |
| UPDATE-on-complete fails | pg hiccup | XACK skipped; event replays; final state converges on next attempt | brief delay; no loss |
| Cursor opaque tamper | adversarial user | signature/validation fails → 400 `ERR_INVALID_CURSOR` | structured error |
| Huge response_text (10MB) | bug / prompt weirdness | truncated at INSERT to 256KB; `response_truncated_at` column set (future) — for MVP, just truncate silently + log warn | operator sees truncated response |

---

## Invariants

| # | Invariant | Enforcement |
|---|---|---|
| 1 | `UNIQUE(zombie_id, event_id)` — one row per event, replay idempotent | schema constraint + §1.6 integration |
| 2 | Every row has non-null actor | schema NOT NULL + M34 steer/webhook/slack/svix ingestor compliance |
| 3 | Write path must not XACK before UPDATE succeeds (for at-least-once delivery) | §1.3 tracing assertion |
| 4 | `GET` endpoints are workspace-scoped + auth-enforced | §2.6 negative test |
| 5 | Schema is pre-v2.0 teardown-compatible (no ALTER/DROP plumbing in this spec) | Schema Guard invariants |

---

## Test Specification

Per sections above. Covers unit (insert/update, UNIQUE replay, pagination helpers, CLI flag parsing, UI component rendering) + integration (real pg write path end-to-end, cross-workspace auth, cursor correctness) + manual smoke (UI events tab loads for the platform-ops zombie post-dogfood).

### Regression

Zombie event processing in steady state stays fast — the INSERT + UPDATE adds one round-trip each, acceptable for non-hot paths. Bench watchpoint: event wall_ms p99 should not regress by >50ms vs pre-M35 baseline.

---

## Execution Plan (Ordered)

| Step | Action | Verify |
|---|---|---|
| 1 | CHORE(open): worktree. | `git worktree list` |
| 2 | Schema + Schema Guard output. | migration runs clean |
| 3 | Zig `zombie_events.zig` + `event_loop` wire-up. | §1 integration |
| 4 | HTTP handlers + OpenAPI fragment. | §2 integration |
| 5 | CLI subcommands. | §3 unit |
| 6 | UI Events tab + workspace events page. | §4 unit |
| 7 | Full gates: lint, test, memleak, cross-compile, bench (latency regression check). | green |
| 8 | CHORE(close): spec → done/, Ripley log, release-doc `<Update>`. | PR green |

---

## Acceptance Criteria

- [ ] `core.zombie_events` table exists with correct schema + indices.
- [ ] Every processed event produces exactly one row (UNIQUE replay-idempotent).
- [ ] GET endpoints return correctly paginated + filtered results.
- [ ] `zombiectl zombie events` and `zombiectl events` work as documented.
- [ ] UI Events tab + workspace events page render with real data from platform-ops dogfood.
- [ ] Latency regression <50ms on event p99.
- [ ] Schema Guard output recorded at EXECUTE.

---

## Eval Commands

```bash
# schema sanity
psql $DATABASE_URL -c '\d core.zombie_events'

# write path
make test-integration -- -Dtest-filter=zombie_events

# pagination smoke
curl -s "$API/v1/workspaces/$WS/zombies/$ZID/events?limit=5" | jq '.items | length'

# replay-idempotent
# scripted: XADD same event_id twice → assert single row

# no FLL on md (linter — this spec lives as .md so no grep gate)
```

---

## Discovery (fills during EXECUTE)

---

## Out of Scope

- Retention policy (TTL-based delete). Add when disk pressure shows.
- Full-text search on request_json/response_text. Add when ops asks.
- Aggregate billing roll-ups (tokens/$ per workspace). Data is here; dashboards are M37+ territory.
- Argument-level redaction of sensitive fields in request_json. For MVP the ingestors are assumed to already redact (see M35_001 credential templating — raw secrets never reach request_json because they're already placeholders from the agent's perspective).
