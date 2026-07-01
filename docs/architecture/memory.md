# Memory — scope, isolation, and durability

> Parent: [`README.md`](./README.md) · Transport mechanics: [`runner_fleet.md`](./runner_fleet.md) §Memory continuity · In-run tools + lifecycle: [`capabilities.md`](./capabilities.md) §4 · User-facing: `docs.agentsfleet.net` → Memory.

What a fleet *learned* from prior events, so it behaves like a teammate who's been here before. This file is the canonical answer to **"what is memory keyed by, what's isolated, and what survives"** — the facts other docs and specs cite. The deep hydrate/capture transport lives in `runner_fleet.md`; the categories/selection/tools live in `capabilities.md` and the user docs.

---

## 1. Scope — keyed by `fleet_id`, never workspace

Every memory row belongs to **one fleet**, keyed by the column **`fleet_id`** (UUID). There is **no `workspace_id` column** in the memory store, and a fresh `fleet_id` starts with an empty namespace.

| Fact | Where it's enforced |
|---|---|
| Store column is `fleet_id`; the upsert key is the unique index `(key, fleet_id)` | `schema/013_memory_entries.sql` (`fleet_id UUID NOT NULL`; `idx_memory_entries_key_fleet_id`) |
| Every read/write scopes `WHERE fleet_id = $1` (never a fetch-all + in-memory filter) | `src/agentsfleetd/memory/fleet_memory.zig` — the only `INSERT`/cap/sweep/list path |
| `fleet_id` is **server-derived from the lease**, never client-supplied (Insecure-Direct-Object-Reference guard) | `src/agentsfleetd/http/handlers/runner/memory.zig` (`lease.fleet_id == {fleet_id}`) |
| Two fleets never share a namespace — Fleet A cannot read Fleet B's memory | role isolation (§2) + the `(key, fleet_id)` key |

> **Terminology.** The scope column is `fleet_id`. The legacy NullClaw name **`instance_id`** (and the interim `zombie_id`) are **retired** — `schema/013` says so explicitly (*"no legacy instance_id prefix"*). Any doc or spec that still says `instance_id` is stale; the column, the wire path, and the code are `fleet_id` end to end.

## 2. Isolation — a Postgres role, not the workspace

Memory lives in its own `memory` schema behind the **`memory_runtime`** Postgres role, which holds **zero grants on `core.*`** (RULE CTX). `api_runtime` does `SET ROLE memory_runtime` only inside a memory request, then `RESET`. The table carries **no foreign key to `core.fleets`** and **survives workspace destruction** (`schema/013` lines 2, 5–6) — the role boundary is the isolation, not a workspace column. The workspace is only the *authorization* boundary above this: a tenant must own the fleet to read its memory via the tenant read (`GET /v1/workspaces/{ws}/fleets/{id}/memories`, scope `fleet:read`).

## 3. Durable store vs ephemeral compute — why "ephemeral fleets" lose memory

Two layers, deliberately split:

- **Durable** — the `fleet_id`-keyed rows in `memory.memory_entries` (Postgres). This is what persists.
- **Ephemeral** — the *compute*. Each run forks a fresh sandboxed child whose in-run store is **SQLite `:memory:`** (no disk file); it vanishes on child exit.

Continuity is the hydrate/capture loop bridging the two: `GET /v1/runners/me/memory/{fleet_id}` seeds the child at run start; `POST` captures deltas back at run end (fencing-verified, like `/reports`). Transport detail: `runner_fleet.md` §Memory continuity.

**The load-bearing consequence:** because the durable key is `fleet_id`, **a new fleet = a new `fleet_id` = an empty namespace.** Spinning a *new ephemeral fleet per event* gives each one nothing to hydrate — zero continuity. Memory continuity **requires reusing the same `fleet_id`** across events. The fleet (and its memory) is durable; only the run is ephemeral. "Workspace-shared memory across ephemeral fleets" is not possible — there is no workspace key.

## 4. The M106 channel pattern

Because memory is `fleet_id`-keyed, **per-channel memory = a per-channel fleet.** The Slack-resident bot (M106) gives each channel a **durable resident fleet**; every mention in any thread of that channel routes to the same `fleet_id`, so memory persists thread→thread (the thread is a delivery surface, not a memory key). Per-thread would forget across threads; per-workspace can't exist (no workspace key). Spec: `docs/v2/done/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md`; scenario: [`scenarios/slack-channel-resident.md`](./scenarios/slack-channel-resident.md).

## 5. Categories, selection, tools — see the topic docs

The four tools (`memory_store` / `memory_recall` / `memory_list` / `memory_forget`), the categories (`core` pinned, `daily` 72h auto-prune, `conversation` windowed), the byte-budget category-pinned hydration window, and cap eviction all live in [`capabilities.md`](./capabilities.md) §4 and the user-facing memory doc. No vector search, no scoring — a substring filter on `key` is the ceiling (`direction.md`).

## Code pointers

| Concern | Path |
|---|---|
| Schema (table, `(key, fleet_id)` index, role grants, `survives workspace destruction`) | `schema/013_memory_entries.sql` |
| The only write/read adapter (`WHERE fleet_id = $1`, `ON CONFLICT (key, fleet_id)`) | `src/agentsfleetd/memory/fleet_memory.zig` |
| Runner hydrate/capture endpoints (lease-derived `fleet_id`, fencing) | `src/agentsfleetd/http/handlers/runner/memory.zig` |
| Tenant read (`fleet:read`, ownership-gated) | `src/agentsfleetd/http/handlers/memory/handler.zig` |
| In-run store seeding (`:memory:` SQLite) | `src/runner/engine/inrun_memory.zig` |
