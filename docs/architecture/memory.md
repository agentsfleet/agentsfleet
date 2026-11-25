---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Memory: what a fleet remembers across runs

> Read this when you ask what a fleet remembers, who can read it and when it is lost. Parent: [`README.md`](./README.md) · User-facing: [docs.agentsfleet.net/memory](https://docs.agentsfleet.net/memory).

## What it is

Fleet memory is what a fleet chose to keep from earlier runs, so it behaves like a teammate who has been here before.
The model writes it on purpose with `memory_store`; nothing is saved behind its back.
Each entry is a key, its content and a category, stored as one row in `memory.entries` in Postgres.

| Fact | Value | Source |
|---|---|---|
| Owner of a row | One fleet, its writer, inside that fleet's workspace | `20261108124700_memory_entries.sql` |
| Unique key | `(fleet_id, key)`; a store under an existing key replaces it | `uq_entries_fleet_id_key` |
| Database role | `memory_runtime`, with no grant outside the `memory` schema | `20261108120100_roles_and_privileges.sql` |
| Load window | 256 KiB of the fleet's own entries (`LOAD_WINDOW_BYTES`) | `af_wire::memory` |
| Shared window | A quarter of that, 64 KiB, of other fleets' shared entries (`LOAD_SHARED_BYTES`) | `af_wire::memory` |
| Capture cadence | Every five completed tool calls, plus the final deltas in the run's report | `afd_gate` context policy |
| `daily` retention | 72 hours, removed on the fleet's next capture | `afd_memory::window` |
| Entry count cap | None | `afd_memory::window` |

## Why it exists

A run is short-lived compute, and the runner keeps nothing between runs.
A fleet still has to remember its owner, its deploy target and its standing rules.
So memory lives behind `agentsfleetd`, and the runner loads it into each run and sends changes back.

## How it behaves

### Scope and sharing

Every row belongs to one fleet and that fleet's workspace, so its identity is `(workspace_id, fleet_id, key)`.
A row is private to its writer unless the writer holds the publish grant and its `memory_store` call named `visibility: shared`.
A fleet holding the read grant then loads and recalls that row, which names its writer.

Both grants are workspace settings on the fleet, `memory_reads_workspace` and `memory_publishes_workspace` on `fleet.fleets`, false by default.
A person sets them with `PATCH F/memory-access` under `fleet:write`, where `F` is `/v1/workspaces/{workspace_id}/fleets/{fleet_id}`.
No fleet overwrites or forgets another fleet's row, because the writer is part of the key.

The daemon derives `fleet_id` from the run it issued, never from the client.
`afd_fleet`'s `RunService::load` and `RunService::capture` refuse a runner that holds no live run on that fleet.

### Isolation

Memory sits in its own `memory` schema behind the `memory_runtime` Postgres role.
The daemon's `api_runtime` role takes it with `SET LOCAL ROLE memory_runtime` inside each memory transaction, and Postgres restores the role at commit or rollback.
The role isolates storage; workspace access and the sharing grants decide who reads what.

`fleet_id` references `fleet.fleets` with `ON DELETE CASCADE`, so deleting a fleet erases its memory.
Postgres checks that cascade with the table owner's authority, which grants `memory_runtime` nothing on `fleet.fleets`.

### Categories and the load window

| Category | Loaded how | Expires |
|---|---|---|
| `core` | First, newest first, and pinned into every prompt | Never; a person removes it |
| `note` | The default when a call names no category; windowed by recency | Never; replaced by a store under its key |
| `daily` | Windowed by recency | After 72 hours, on the next capture |
| `conversation` and any custom name | Windowed by recency | Never |

Loading is a pure function of the rows and the byte budget.
Every `core` entry that fits loads first, then the newest other entries fill what remains.
Each tier takes a prefix of its own order, so an older small entry never slips in behind a rejected newer one.

What does not fit stays in Postgres and is not loaded.
A fleet that pins everything as `core` crowds its own prompt, and its oldest `core` entries then stay unloaded.
A non-empty fleet always loads at least one entry, even when that entry alone passes the budget.

### The four tools

The memory tools run in the runner's supervisor, never in the sandbox.

| Tool | What it does |
|---|---|
| `memory_store` | Writes a key under a category, `note` by default; an existing key is replaced |
| `memory_recall` | Finds entries by a case-insensitive substring of key or content, key matches first |
| `memory_list` | Lists the loaded entries, optionally one category |
| `memory_forget` | Hides a key for the rest of the current run only |

A recall the loaded window cannot answer asks `POST /v1/runners/me/memory/{fleet_id}/recall`, at most three times per run (`RECALL_MISS_CAP`).
There is no vector search and no scoring.

### How memory travels with a run

```text
assign run ──► run payload carries the fleet's window (run.memory)
                  │  (absent when the read failed: the runner calls
                  │   GET /v1/runners/me/memory/{fleet_id} itself)
                  ▼
supervisor seeds afr_memory::Loaded (in memory, no disk file)
                  │
every 5 tool calls ──► POST /v1/runners/me/memory/{fleet_id}   (fenced)
                  │
report ──► carries the final deltas ──► daemon captures them,
                                        then commits the settle
```

The daemon captures a report's deltas under the run's own fencing token, in the memory role's transaction, before it commits the settle.
The settle frees the thread, so no other run of the fleet can start between capture and commit.
A failed capture is logged as `memory_capture_failed`, and the report still settles.

Capture locks the fleet, rechecks the run's fence, reads the grants, then upserts and removes expired `daily` rows in one transaction.
A final database-clock check refuses a capture that outlived its run while waiting.
A share from a fleet without the publish grant is refused by `memory_store` and skipped at capture.

### Why one fleet's memory has one writer at a time

The fleet's thread row admits one live run, and capture checks that run's fencing token.
A slow old run can briefly coexist with its replacement, but only the current holder's writes land.
A runner's concurrent runs are always different fleets, so they write different namespaces.

### What reaches the model

The prompt builder writes the [fixed prompt text](./capabilities.md#the-fixed-prompt), then every loaded `core` entry of the fleet's own under one heading.
With no pinned entry, the heading is absent.
Other categories and other fleets' shared entries stay recall-only; the model reads them as tool output.

The context planner counts the pinned block against the model's window.
A pinned entry stays in every later prompt until a person removes it, from the fleet page's Remembers list or with `DELETE F/memories/{key}`.
The command line reads memory with `agentsfleet fleet memory list` and `search`; it cannot remove an entry.

### Slack threads

Memory belongs to a fleet, never to a Slack thread.
A mention routed to a fleet through its `mention` trigger runs with that fleet's memory, like any other turn.
An unconfigured channel never creates a fallback fleet that owns memory ([Slack routing](./slack_mentions.md#how-a-mention-is-routed)).

## Limits

- A new fleet is a new `fleet_id` with empty memory; continuity needs the same fleet, or the read grant on shared memory.
- The only scopes are fleet-private and workspace-shared; there is no account-wide memory.
- Postgres is the only store; no vendor memory store is built.
- A runner crash loses at most the memory stored since the last push, five tool calls back.
- Shared memory lets one fleet plant a fact another trusts; the recorded writer keeps each shared entry traceable.

Habits that make the window work for a fleet:

- Pin standing guidance as `core` only when a person asks.
- Reuse stable keys: `deploy_target` beats a dated `deploy_target_jun12`.
- Keep entries small; the budget counts bytes, so one large entry crowds out many small ones.

## Related

- [`context_lifecycle.md`](./context_lifecycle.md): conversation compaction, which is separate from memory.
- [`thread_model.md`](./thread_model.md): the turns a chat reads back on every run.
- [`checkpoints.md`](./checkpoints.md): how a replacement run resumes a turn.
- [`capabilities.md`](./capabilities.md): why pinned memory is advisory, never enforcement.
- [`runner_fleet.md`](./runner_fleet.md): the runner plane memory travels over.
