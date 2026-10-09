# Memory — scope, isolation, and durability

> Parent: [`README.md`](./README.md) · Transport mechanics: [`runner_fleet.md`](./runner_fleet.md) §"Memory continuity" · In-run tools + lifecycle: [`capabilities.md`](./capabilities.md) §4 · User-facing: [docs.agentsfleet.net/memory](https://docs.agentsfleet.net/memory).

What a fleet *learned* from prior events, so it behaves like a teammate who's been here before. This file is the canonical answer to **"what is memory keyed by, what's isolated, and what survives"** — the facts other docs and specs cite. The deep hydrate/capture transport lives in `runner_fleet.md`; the categories/selection/tools live in `capabilities.md` and the user docs.

---

## 1. Scope — written by one fleet, shared with the workspace only by grant

Every memory row belongs to **one fleet**, its writer, and to that fleet's **workspace**: the identity is `(workspace_id, fleet_id, key)`, held by the unique `(key, fleet_id)` because a fleet has one workspace. A row is fleet-visible unless its writer held the publish grant and stored it with `visibility: workspace`; a fleet holding the read grant then hydrates and recalls it, naming the writer. No fleet overwrites or forgets another's row. Both grants are a workspace setting on the fleet, false by default ([`runner_fleet.md`](./runner_fleet.md) §"Memory backends and scope").

| Fact | Where it's enforced |
|---|---|
| Store columns are `fleet_id`, `workspace_id` and `workspace_visible`; the upsert key is the unique index `(key, fleet_id)` | `schema/820_memory_entries.sql`, `schema/926_memory_entries_workspace_scope.sql` |
| Every read and write goes through one trait, scoped `WHERE fleet_id = $1`, plus the workspace's shared rows only for a granted reader | `rustd/crates/afd_memory/` — `MemoryStore`, the Postgres store, the grants, and the flip |
| The grants are read and set as the api role on `core.fleets`, never by the store | `schema/927_fleet_memory_access.sql`, `rustd/crates/afd_memory/src/access.rs` |
| `fleet_id` is **server-derived from the lease**, never client-supplied (Insecure-Direct-Object-Reference guard) | `rustd/crates/afd_api_runner/src/handler/runner/memory.rs` (`lease.fleet_id == {fleet_id}`) |
| Two fleets never share a namespace — Fleet A cannot read Fleet B's memory | role isolation (§2) + the `(key, fleet_id)` key |

> [!NOTE]
> **Terminology.** The scope column is `fleet_id`, and the column, the wire path and the code use it end to end. A doc or spec that says `instance_id` or `zombie_id` is stale.

## 2. Isolation — a Postgres role, not the workspace

Memory lives in its own `memory` schema behind the **`memory_runtime`** Postgres role, which holds **zero grants on `core.*`** (RULE CTX). `api_runtime` does `SET ROLE memory_runtime` only inside a memory request, then `RESET`. `fleet_id` **REFERENCES `core.fleets` ON DELETE CASCADE** (`schema/820`), so memory is **erased with the fleet it belongs to** and an erased account keeps none of it. That edge does not weaken the isolation: PostgreSQL evaluates both the check and the cascade with the table owner's authority, so `memory_runtime` gains no `core` grant and still cannot name `core.fleets` at all. The role boundary remains the isolation, not a workspace column. The workspace is only the *authorization* boundary above this: a tenant must own the fleet to read or forget its memory (API reference › Fleet memories).

## 3. Durable store vs ephemeral compute — why "ephemeral fleets" lose memory

Two layers, deliberately split:

- **Durable** — the `fleet_id`-keyed rows in `memory.memory_entries` (Postgres), the default backend. This is what persists. A workspace can later be flipped to another store, which migrates its memory ([`runner_fleet.md`](./runner_fleet.md) §"Memory backends and scope").
- **Ephemeral** — the *compute*. Each run holds its memory in the runner's supervisor, seeded at lease start and pushed, fenced, before the report; it is gone when the run ends ([Runner execution](./runner_execution.md#crates)).

Continuity is the hydrate/capture loop bridging the two: `GET /v1/runners/me/memory/{fleet_id}` seeds the run at its start; `POST` captures deltas back at run end (fencing-verified, like `/reports`). Transport detail: [`runner_fleet.md`](./runner_fleet.md) §"Memory continuity".

**The load-bearing consequence:** because the writer is in the key, **a new fleet = a new `fleet_id` = an empty namespace of its own.** Spinning a *new ephemeral fleet per event* gives each one nothing of its own to hydrate. Memory continuity **requires reusing the same `fleet_id`** across events, or granting the new fleet read on the workspace's shared memory, which seeds it with what publishing fleets learned.

## 4. The M106 channel pattern

Because memory is `fleet_id`-keyed, **per-channel memory = a per-channel fleet.** The Slack-resident bot (M106) gives each channel a **durable resident fleet**. `agentsfleetd` installs the resident beside fleets that subscribe to a channel (M206_002, [`scenarios/slack-incident-responder.md`](./scenarios/slack-incident-responder.md) §4). The boundary itself needs no new code: a runner hydrates or captures only a fleet it holds a live lease on (`rustd/crates/afd_fleet/src/lease/memory.rs:45-50`). Every mention in any thread of that channel routes to the same `fleet_id`, so memory persists thread to thread. The thread is a delivery surface, not a memory key. Per-thread would forget across threads; a resident stays private by holding no shared-memory grant. Spec: `docs/v2/done/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md`; scenario: [`scenarios/slack-channel-resident.md`](./scenarios/slack-channel-resident.md).

## 5. Categories, selection, tools — see the topic docs

The four tools (`memory_store` / `memory_recall` / `memory_list` / `memory_forget`), the categories (`core` pinned, `daily` 72h auto-prune, `conversation` windowed), the byte-budget category-pinned hydration window, and cap eviction all live in [`capabilities.md`](./capabilities.md) §4 and the user-facing memory doc. No vector search, no scoring — recall is a case-insensitive substring match on `key` and content, key matches first (`rustd/crates/afr_memory/src/hydrated.rs`, `rustd/crates/afd_memory/src/sql.rs`).

## Code pointers

| Concern | Path |
|---|---|
| Schema (table, `(key, fleet_id)` index, role grants, `fleet_id` foreign key + cascade) | `schema/820_memory_entries.sql` |
| The only write/read adapter (`WHERE fleet_id = $1`, `ON CONFLICT (key, fleet_id)`) | `rustd/crates/afd_memory/src/postgres/`, its statements in `src/sql.rs` |
| Runner hydrate/capture endpoints (lease-derived `fleet_id`, fencing) | `rustd/crates/afd_api_runner/src/handler/runner/memory.rs` |
| Tenant read and forget (ownership-gated) | `rustd/crates/afd_api_tenant/src/handler/fleet/memory.rs` |
| A run's memory, seeded at lease start and pushed before the report | `rustd/crates/afr_memory/src/hydrated.rs` |
