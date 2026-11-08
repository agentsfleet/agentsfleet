# Memory: scope, isolation, and durability

> Parent: [`README.md`](./README.md) · Transport mechanics: [`runner_fleet.md`](./runner_fleet.md) §"Memory continuity" · In-run tools + lifecycle: [`capabilities.md`](./capabilities.md) §4 · User-facing: [docs.agentsfleet.net/memory](https://docs.agentsfleet.net/memory).

What a fleet *learned* from prior events, so it behaves like a teammate who's been here before. This file is the canonical answer to **"what is memory keyed by, what's isolated, and what survives"**. Other docs and specs cite these facts.

The deep load/capture transport lives in `runner_fleet.md`; the categories, selection and tools live in `capabilities.md` and the user docs.

---

## 1. Scope: written by one fleet, shared with the workspace only by grant

Every memory row belongs to **one fleet**, its writer, and to that fleet's **workspace**. The identity is `(workspace_id, fleet_id, key)`, held by the unique `(key, fleet_id)` because a fleet has one workspace. A row is fleet-visible unless its writer held the publish grant and stored it with `visibility: workspace`.

A fleet holding the read grant then loads and recalls such a row, naming the writer. No fleet overwrites or forgets another's row. Both grants are a workspace setting on the fleet, false by default ([`runner_fleet.md`](./runner_fleet.md) §"Memory backends and scope").

| Fact | Where it's enforced |
|---|---|
| Store columns are `fleet_id`, `workspace_id` and `workspace_visible`; the upsert key is the unique constraint `(fleet_id, key)` | `schema/2100_memory_entries.sql` |
| Every read and write goes through one store, scoped `WHERE fleet_id = $1`, plus the workspace's shared rows only for a granted reader | `afd_memory`: `Memories`, the Postgres store (`PgStore`), and the grants |
| The grants are read and set as the api role on `fleet.fleets`, never by the store | `schema/1500_fleet_fleets.sql` and `afd_memory`'s access module |
| `fleet_id` is **server-derived from the lease**, never client-supplied (Insecure-Direct-Object-Reference guard) | `afd_api_runner`'s runner memory handler hands the path's fleet to `afd_fleet`'s `Plane::hydrate` / `Plane::capture`, which refuse a runner holding no live lease on that fleet |
| Private rows remain writer-scoped; granted readers can read other fleets' workspace-visible rows, but cannot overwrite or forget them | shared-memory grants + workspace predicate + the `(key, fleet_id)` key |

> [!NOTE]
> **Terminology.** The scope column is `fleet_id`, and the column, the wire path and the code use it end to end. A doc or spec that says `instance_id` or `zombie_id` is stale.

## 2. Isolation: a Postgres role, not the workspace

Memory lives in its own `memory` schema behind the `memory_runtime` Postgres role, with zero grants outside that schema (`schema/1010_roles_and_privileges.sql`).
`api_runtime` takes it with `SET LOCAL ROLE memory_runtime` inside each memory transaction.
Postgres restores the role at commit or rollback (`afd_memory`'s Postgres store).

`fleet_id` references `fleet.fleets` with `ON DELETE CASCADE`, so deleting a fleet erases its memory (`schema/2100_memory_entries.sql`).
Postgres checks the foreign key and cascade with the table owner's authority.
That grants `memory_runtime` no access to `fleet.fleets`.

The database role isolates storage; workspace authorization and shared-memory grants decide access above it.
Tenant reads and forgetting require workspace access to the fleet (API reference › Fleet memories).

## 3. Durable store vs ephemeral compute: why "ephemeral fleets" lose memory

Two layers, deliberately split:

- **Durable** — the `fleet_id`-keyed rows in `memory.entries` (Postgres), the default backend. This is what survives, and Postgres is the only store. A second store, and the flip that migrates a workspace onto it, come with the first vendor store ([`runner_fleet.md`](./runner_fleet.md) §"Memory backends and scope").

- **Ephemeral** — the *compute*. Each run holds its memory in the runner's supervisor, seeded at lease start and pushed, fenced, before the report. It is gone when the run ends ([`runner_execution.md`](./runner_execution.md#crates)).

Continuity is the load/capture loop bridging the two. `GET /v1/runners/me/memory/{fleet_id}` seeds the run at its start; `POST` captures deltas back at run end (fencing-verified, like `/reports`). Transport detail: [`runner_fleet.md`](./runner_fleet.md) §"Memory continuity".

**The load-bearing consequence**. The writer is in the key, so a new fleet is a new `fleet_id` with an empty namespace of its own. Spinning a *new ephemeral fleet per event* gives each one nothing of its own to load.

Memory continuity **requires reusing the same `fleet_id`** across events. The alternative is granting the new fleet read on the workspace's shared memory, which seeds it with what publishing fleets learned.

## 4. Slack threads and fleet memory

Memory belongs to a configured fleet and its permitted scope, rather than a Slack thread.
Mentions routed to that fleet can use saved facts only when its tools and grants permit memory capture and recall.
A shared fleet identifier alone proves neither operation.

The runner reads or saves memory only under the current lease fence.
[Slack routing](scenarios/slack-incident-responder.md#4-routing-a-mention) selects configured fleets; an unconfigured channel never creates a memory-owning fallback fleet.
Historical stored fleets and their memory remain unchanged.

## 5. Categories, selection, tools: see the topic docs

The four tools (`memory_store` / `memory_recall` / `memory_list` / `memory_forget`) and the categories live in [`capabilities.md`](./capabilities.md) §4 and the user-facing memory doc. The categories are `core` pinned, `daily` 72h auto-prune and `conversation` windowed. The byte-budget category-pinned loading window and cap eviction live there too.

No vector search, no scoring. Recall is a case-insensitive substring match on `key` and content. Key matches come first (`afr_memory`'s in-run store and `afd_memory`'s search statements).

## 6. What reaches the model

At run start, the supervisor loads the fleet's saved entries into the runner's memory tool store.
Workspace entries join that store only when the reader holds the shared-memory grant.
The prompt builder does not insert the entire store into each model request.

The model reads entries through its allowed `memory_recall` and `memory_list` tools; returned content enters the conversation as tool output.

The implemented scopes are fleet-private and workspace-shared. There is no account-wide memory scope in `memory.entries` or the runner's loading path.
Private setup chat uses its own conversation history and checkpoint; its authoring tool set does not expose the runtime memory tools.

## 7. Two checkpoint mechanisms

| Mechanism | Trigger and content | Next run or request |
|---|---|---|
| Semantic memory capture | After every five completed tool calls by default, push pending `memory_store` changes; push again before the final report | A later run of the same fleet loads saved entries into its tool store; granted readers also load workspace-shared entries |
| Conversation compaction | Near the model's working-window limit, a metered model call summarizes selected conversation groups into `memory.context_checkpoints` | Preparation uses the owned draft or live runtime branch checkpoint, original source checks and retained conversation suffix |

Semantic capture skips empty changes and makes no summary model call.
A failed mid-run push is logged; final capture retries before reporting, as described in [runner transport](runner_fleet.md#memory-continuity-durable-fleet-memory-rides-the-trusted-plane).
Child loops do not run the periodic capture counter.

Conversation summaries remain separate from saved semantic entries and filesystem state.
See [conversation compaction](context_lifecycle.md) for summary generation, token measurement, publication checks and the current progress-event gap.

## Code pointers

| Concern | Owner |
|---|---|
| Schema (table, `(fleet_id, key)` unique constraint, role grants, `fleet_id` foreign key + cascade) | `schema/2100_memory_entries.sql` |
| The only write/read adapter (`WHERE fleet_id = $1`, `ON CONFLICT (key, fleet_id)`) | `afd_memory`'s Postgres store and its SQL statements |
| Runner load/capture endpoints (lease-derived `fleet_id`, fencing) | `afd_api_runner`'s runner memory handler |
| Tenant read and forget (ownership-gated) | `afd_api_tenant`'s fleet memory handler |
| A run's memory, seeded at lease start and pushed before the report | `afr_memory`'s `Hydrated` store |
