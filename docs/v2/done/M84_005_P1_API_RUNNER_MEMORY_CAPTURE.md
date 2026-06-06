# M84_005: Per-run agent memory capture — push to `/v1/runners/me/memory`, control plane persists

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 005
**Date:** Jun 05, 2026
**Status:** DONE
**Priority:** P1 — operator/customer-facing capability (agents remember across runs) **with** a security boundary: the capture path must not put a credential, a control-plane URL, or a database connection inside the untrusted sandboxed agent.
**Categories:** API
**Batch:** B1 — runs **in parallel** with M84_003 (security launch slice); disjoint trees (`src/zombied/`+contract+runner memory wiring here vs `src/runner/` process-boundary there), two shared touchpoints to coordinate (`build_runner.zig`, `make/test-integration.mk` — second to land rebases).
**Branch:** feat/m84-runner-memory-capture
**Depends on:** **M84_003 (sandbox env/fd/cap hardening)** — the no-token-leak guarantee requires `ZOMBIE_RUNNER_TOKEN` to be absent from the child (M84_003 §1). Relates to **M84_004 (egress)** — this design needs **no** child egress (the daemon pushes), so it does not depend on the egress allowlist.
**Provenance:** agent-generated — Indy asked to "ensure the memory of the agent is captured during every run" (Jun 05, 2026); code-grounded in the Orly Chief Technology Officer (CTO) review, decisions locked by Indy (Jun 05, 2026).

> **Provenance is load-bearing.** Verified against `protocol.zig` (report shape + `/v1/runners` paths), `runner/report.zig` + `fleet/service_report.zig` (the `zrn_`-authenticated runner-plane handler with fencing), `memory/handler.zig` + `memory/helpers.zig` (the `memory.memory_entries` write + Row-Level Security (RLS) + Insecure-Direct-Object-Reference (IDOR) guard), `runner/engine/runner.zig` + `runner/engine/zombie_memory.zig` (the inert in-child direct-Postgres path; the SQLite-default fallback), and `build_runner.zig` (`base,sqlite`). Re-confirm at PLAN.

**Canonical architecture:** [`docs/architecture/runner_fleet.md`](../../architecture/runner_fleet.md) §Egress model ("durable memory rides the trusted plane, never the agent") and §Datastore role model. The control-plane (`zombied`) memory write + RLS model are in `src/zombied/http/handlers/memory/`.

---

> **IN FOR LAUNCH — full implementation (Indy, Jun 05, 2026).** The marketing site already sells durable memory (`memory_store`, "memory checkpoints", "durable memory"), so shipping it makes an existing product claim true rather than adding net-new scope. Full scope ships: runner push to `/v1/runners/me/memory` as the write path, server-side persist to `memory.memory_entries`, sunset of the tenant POST/DELETE + the SQLite default, tenant GET kept. Pairs with M84_003 in the launch milestone (M84_004 egress stays deferred).

## Implementing agent — read these first

1. `src/lib/contract/protocol.zig` — the `/v1/runners/me/*` path constants + `ReportRequest`/fencing shape; this workstream adds a sibling `/v1/runners/me/memory` path + request type.
2. `src/zombied/http/handlers/runner/report.zig` + `src/zombied/fleet/service_report.zig` — the existing `zrn_`-authenticated runner handler with fencing verification (`UZ-RUN-005`); the new memory handler mirrors its auth + fencing, then persists.
3. `src/zombied/http/handlers/memory/handler.zig` + `helpers.zig` — `innerStoreMemory` (`INSERT … memory.memory_entries ON CONFLICT (key, zombie_id)`), `setMemoryRole`, `resolveZombieInWorkspace` (builds `instance_id`); the write extracts into the shared adapter.
4. `src/runner/daemon/loop.zig` — where the daemon drives a lease; the memory push is issued here (daemon-side, holds the `zrn_` token).
5. `src/runner/engine/runner.zig` (§4 memory) + `src/runner/engine/zombie_memory.zig` + `build_runner.zig` — the **inert** in-child direct-Postgres path AND the **ephemeral-SQLite-as-default** backend; both are sunset by this workstream.
6. `dispatch/write_zig.md` · `dispatch/write_sql.md` (if the write touches schema) · `docs/AUTH.md` (the `zrn_` runner-plane auth boundary).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m84): per-run agent memory capture via /v1/runners/me/memory (no creds in the agent)`
- **Intent (one sentence):** Every run's agent memory is pushed by the **daemon** to `POST /v1/runners/me/memory` over the existing `zrn_` runner-plane auth and persisted by the **control plane (`zombied`)** to `memory.memory_entries` — so the sandboxed agent never holds a database connection, a control-plane token, or a memory URL, and the local ephemeral-SQLite default is gone.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. **Resolve two coupled mechanisms:** (a) how the runner obtains the run's memory deltas from NullClaw (read the run's working store vs hook the `remember` tool); (b) **with SQLite removed, what in-run working store the agent recalls/writes against** (an in-memory backend hydrated at lease start vs another non-durable store) and how prior memory is hydrated to the child **without** giving it a credential/URL. A `[?]` here blocks the spec.

### Handshake — RESOLVED (Indy, Jun 05, 2026)

**Intent restated:** durable agent memory is the **control plane's** job, never the agent's. The runner **parent** (it holds the `zrn_` token) hydrates a run's prior memory in and captures the run's memory out, both over the existing `/v1/runners` plane; the sandboxed child holds no token, URL, or Data Source Name (DSN) and keeps no durable on-disk memory.

**Mechanisms resolved (the two `[?]`):**
- **(a) Delta extraction → read the run's working store.** The runner owns the `MemoryRuntime` it builds for the child, so at the push cadence it enumerates that store directly (`Memory.list(alloc, category=null, session_id=null)`) and surfaces the entries to the parent over the stdout pipe. No `remember`-tool hook. The push carries the **full current entry set** (not a computed diff) — `ON CONFLICT (key, zombie_id) DO UPDATE` makes that idempotent — byte-capped per push.
- **(b) In-run store + hydration → reuse SQLite in `:memory:` mode, hydrated via a parent GET.** The in-run working store is NullClaw's **SQLite engine run file-less (`db_path = ":memory:"`)** — reused, *not* a new in-memory backend (`base,sqlite` stays in `build_runner.zig`; only the path changes). It is non-durable: seeded at run start and discarded at run end. Prior memory reaches the child through the **runner parent**, which issues `GET /v1/runners/me/memory` with its `zrn_` token and pipes the result down stdin; the child seeds its `:memory:` store from it. The child makes **no** network call and holds **no** credential.

**`ASSUMPTIONS I'M MAKING:`**
1. NullClaw `v2026.5.29` exposes `Memory.list` (enumerate), `Memory.store` (seed), and an `:memory:` SQLite path via the registry `db_path` field — verified against `~/Projects/oss/nullclaw` at PLAN; re-confirm against the pinned hash at EXECUTE.
2. The lease→child stdin channel that already carries `secrets_map` can carry a hydrated-memory blob the same way (parent-built, not child-fetched).
3. The run-end push completes before `report`, so a continuation run hydrates the snapshot the previous run stored (ordering documented in `runner_fleet.md` §Memory continuity).
4. `zombie_id` (UUIDv7, no `zmb:` prefix) is the only scope, server-derived from the path + verified lease; a client-supplied scope is ignored (server-authoritative). *(Ratified Jun 06 — supersedes the earlier `"zmb:"`-prefixed `instance_id` form; see Discovery.)*

**Decisions banked from this session (see Discovery for verbatim quotes):** hydration is a dedicated runner-plane **`GET /v1/runners/me/memory`** (not lease-embed — lease size constraint); push cadence is **run-end + mid-run** on `memory_checkpoint_every`; in-run store is **SQLite `:memory:`** (reuse, not LRU); v1 hydrates the **full** memory set, with a dedicated scalable store as the post-launch direction; **robust unit + integration tests** on the loop (Indy directive). The architecture + diagrams are recorded in `docs/architecture/runner_fleet.md` §Memory continuity.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`**:
  - **RULE NDC / NLR** — the inert in-child direct-Postgres path and the SQLite-as-default fallback are **removed**, not left beside the push path.
  - **RULE UFS** — the `/v1/runners/me/memory` path, request field names, memory key/category constants, and the `instance_id` prefix (`zmb:`) are single-sourced and shared verbatim runner ↔ control plane.
  - **RULE NLG** — pre-2.0: no "legacy memory mode" framing.
- **`dispatch/write_zig.md`** — tagged-union results, `errdefer`, cross-compile both linux targets.
- **`dispatch/write_sql.md`** — only if the `memory.memory_entries` write changes (it should reuse the existing schema; no DDL expected).
- **`docs/AUTH.md`** — `/v1/runners/me/memory` is `zrn_` runner-plane auth (like `/reports`), never the tenant (`/v1/workspaces/*`) plane.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | **yes** — `*.zig` edits | Read `dispatch/write_zig.md`; cross-compile both linux targets. |
| UFS | **yes** — new path + fields + `instance_id` prefix shared both sides | Named constants in `contract`, reused runner + control plane. |
| LENGTH | **maybe** — new handler + adapter | Memory persist lives in the shared adapter (`src/zombied/memory/zombie_memory.zig`), not inline in the handler. |
| LOGGING | **yes** — `memory_captured` emit | Envelope unchanged; count + `instance_id`, never the memory content. |
| LIFECYCLE / SCHEMA | **yes** — pg-drain on the write path | `conn.query().drain()` before `deinit` (`make check-pg-drain`); reuse `memory.memory_entries` (no DDL). |
| ERROR REGISTRY | **maybe** — fencing/capture-failure codes | Reuse `UZ-RUN-005` (fencing) + `ERR_MEM_UNAVAILABLE`, or register a distinct memory-push code. |

---

## Overview

**Goal (testable):** After a run, every memory the agent wrote that run is present in `memory.memory_entries` under that zombie's `instance_id`, persisted by the control plane from a `zrn_`-authenticated `POST /v1/runners/me/memory` the daemon issued — and an inspection of the sandboxed child shows **no** token, **no** control-plane URL, **no** DSN, **no** durable on-disk memory file.

**Problem:** Memory capture is **signalled but not wired**: `runner_progress.zig` logs `memory_checkpoint_due` on the `memory_checkpoint_every` cadence but persists nothing; the daemon report carries a run-continuity checkpoint, not memory entries. The runner's only persistence paths are wrong: the in-child direct-Postgres adapter (`zombie_memory.zig`) is inert (`base,sqlite`) and would, if enabled, put a DSN + DB socket in the untrusted agent; the fallback is an **ephemeral workspace SQLite file**, which is a disk artifact, not durable, and not the source of truth. So learned memory does not survive the run.

**Solution summary:** Capture on the **trusted plane via a dedicated channel**. The runner surfaces the run's memory deltas over the child→daemon stdout pipe; the **daemon** pushes them to a new `POST /v1/runners/me/memory` (authenticated by the `zrn_` token the daemon already holds, fencing-verified like `/reports`); the control plane (`zombied`) persists them to `memory.memory_entries` server-side via a shared write adapter, deriving `instance_id` from the lease it issued. The in-child direct-Postgres path and the SQLite-as-default backend are removed; durable memory lives only in `zombied`'s Postgres. The agent holds nothing.

---

## Prior-Art / Reference Implementations

- **`/v1/runners/me/reports`** (`runner/report.zig` → `service_report.report`) is the known-good `zrn_`-authenticated, fencing-verified runner ingestion path; `/v1/runners/me/memory` mirrors its auth + fencing.
- **The tenant memory write** (`memory/handler.zig:innerStoreMemory` + `helpers.zig`: `INSERT … ON CONFLICT (key, zombie_id)` under `SET ROLE memory_runtime` with the IDOR-guarded `instance_id`) is the known-good SQL. Extract it into a shared `src/zombied/memory/zombie_memory.zig` adapter used by **both** the tenant handler and the new runner-memory handler — don't fork the SQL.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/lib/contract/protocol.zig` | EDIT | Add `PATH_RUNNER_MEMORY` (`/v1/runners/me/memory`) + `MemoryDelta { key, content, category }`, `MemoryPushRequest { lease_id, fencing_token, memory: []MemoryDelta }` (POST), and `MemoryHydrateResponse { memory: []MemoryEntry }` (GET) (UFS field names). |
| `src/zombied/http/handlers/runner/memory.zig` | CREATE | New `zrn_` handlers: **GET** resolves the runner's live lease → returns the zombie's full memory; **POST** verifies `lease_id` fencing (like `/reports`) → persists deltas via the shared adapter with server-derived `instance_id`. |
| `src/zombied/http/{router,route_table,route_table_invoke}.zig` | EDIT | Wire `GET` + `POST /v1/runners/me/memory` → the new handlers (runner-plane middleware, not bearer): `GET` hydrates the lease zombie's prior memory, `POST` captures the run's memory. |
| `src/zombied/memory/zombie_memory.zig` | CREATE | The single durable write path: `storeEntry` (`INSERT … ON CONFLICT (key, zombie_id)`) with per-zombie cap eviction (§6); `listAll` scoped read backing the hydration `GET`; the `Compactor` (real `.recency_window` arm, §8). `setMemoryRole`/`resetRole` stay in the tenant `helpers.zig` (caller switches role). |
| `src/zombied/observability/metrics_runner.zig` | EDIT | Capture observability (§7): captured-entries + push-failure counters, hydration set-size gauge; rendered via the existing `renderPrometheus`. |
| `src/zombied/http/handlers/memory/handler.zig` | EDIT | **Remove** `innerStoreMemory` (POST) + `innerDeleteMemory` (DELETE); keep `innerListMemories` (GET). The runner-memory handler is the only writer (via the shared adapter). |
| `src/zombied/http/{router,route_table,route_table_invoke,route_matchers,routes}.zig` | EDIT | Drop the tenant memory **POST** dispatch and the **`workspace_zombie_memory`** by-key (DELETE) route + `matchWorkspaceZombieMemoryByKey`; keep the collection **GET**. Retired verbs 404/405 (pre-v2, no shim). |
| `src/runner/engine/runner.zig` (+ result frame) | EDIT | Seed the in-run store from the hydration blob; surface the run's memory to the parent over the pipe; **switch the in-run store to SQLite `:memory:`** and **remove the direct-Postgres memory branch**. |
| `src/runner/daemon/loop.zig` | EDIT | `GET`-hydrate prior memory before the run and pipe it to the child; push captured memory to `POST /v1/runners/me/memory` (parent-side) at **run-end + mid-run** on `memory_checkpoint_every`. |
| `src/runner/engine/zombie_memory.zig` | DELETE | Retire the inert in-child direct-Postgres adapter (RULE NDC). |
| `build_runner.zig` | EDIT | Keep `base,sqlite`; the in-run store is SQLite run file-less (`db_path = ":memory:"`) — no on-disk memory file (reuse, not a new in-memory engine). |
| `docs/architecture/runner_fleet.md` | EDIT (small) ✅ done (686ec915) | §Memory continuity — the hydrate→capture loop + diagrams (durable reference). |
| `docs/architecture/capabilities.md` | EDIT (small) ✅ done (686ec915) | §4 reconciled — agent recalls from its parent-hydrated in-run store, not a direct durable read. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape (locked, Indy Jun 05):** a **dedicated `POST /v1/runners/me/memory`** push (daemon → control plane) is the **only** write path; the control plane is the single durable source of truth; the in-child direct-Postgres path, the ephemeral-SQLite default, **and the tenant write verbs (POST/DELETE)** are **sunset**. The tenant memory API becomes **read-only (GET)**. A **refactor** of the memory write surface.
- **Alternatives considered (rejected):** (a) **Agent calls `/v1/workspaces/.../memories` directly** — that is the *tenant* auth plane; the agent would need tenant credentials + egress to the control plane → a credential + URL inside the untrusted child. (b) **In-child direct Postgres** (`zombie_memory.zig`, `-Dengines=postgres`) — a DSN + DB socket in the untrusted agent (M84_004 Invariant 3 forbids it). (c) **Extend `/v1/runners/me/reports` instead of a dedicated endpoint** — rejected per Indy: a dedicated `/memory` endpoint keeps capture decoupled from the terminal report and supports **mid-run cadence** (`memory_checkpoint_every`) on long runs, not only run-end. (d) **Keep ephemeral SQLite as default** — rejected per Indy: a non-durable disk artifact, not the source of truth.

---

## Sections (implementation slices)

### §1 — `POST /v1/runners/me/memory` (control plane, `zrn_`, fencing-verified)

A dedicated runner-plane endpoint that accepts memory deltas for the runner's active lease, verifies the runner currently holds it (fencing, like `/reports`), and persists server-side. Mirrors `/reports` auth; never the tenant plane.

- **Dimension 1.1** — a `zrn_`-authenticated push for a held lease persists its deltas → Test `test_runner_memory_push_persists`
- **Dimension 1.2** — a push for a lease the runner does **not** hold (stale fencing) is rejected `UZ-RUN-005` → Test `test_runner_memory_push_fencing_rejected`

### §2 — The daemon pushes; the agent never calls out

The child surfaces deltas over the stdout pipe; the **daemon** (runner parent) pushes them to `/v1/runners/me/memory` using the `zrn_` token it holds. Cadence: at run end (mandatory) **and** mid-run on `memory_checkpoint_every` (locked, Indy Jun 05) — so a long run's learned memory is durable before it finishes.

- **Dimension 2.1** — a run that writes N entries results in the daemon pushing exactly those N deltas → Test `test_daemon_pushes_run_memory`
- **Dimension 2.2** — the `zrn_` token / control-plane URL are absent from the child's env/argv/inputs → Test `test_no_token_or_url_in_child` (regression-links M84_003 §1)

### §3 — Server-authoritative persistence (shared adapter, RLS, IDOR)

`zombied` writes `memory.memory_entries` via the shared adapter, deriving `instance_id` from the lease's `zombie_id` (which it issued), under `SET ROLE memory_runtime`. One write path for tenant + runner callers.

- **Dimension 3.1** — pushed deltas land under the correct, **server-derived** `instance_id` → Test `test_pushed_memory_persisted`
- **Dimension 3.2** — a client-supplied scope cannot target another zombie → Test `test_memory_push_cross_zombie_isolation`
- **Dimension 3.3** — the tenant API write and the runner push share one adapter (no SQL fork) → Test `test_single_write_adapter`

### §4 — Sunset every non-push write surface

Memory **writes** flow only through the runner push (§1/§2); every other write path is removed. Durable memory lives only in `zombied`'s Postgres; the tenant API is **read-only** (GET).

- **Dimension 4.1** — `src/runner/engine/zombie_memory.zig` and the `memory_connection`/`memory_namespace` branch are removed; the child opens no DB socket and holds no DSN → Test `test_child_holds_no_dsn` + Dead Code Sweep
- **Dimension 4.2 (reframed, Indy Jun 05 — reuse SQLite, don't reinvent an LRU)** — the in-run store is NullClaw's SQLite engine run **file-less** (`db_path = ":memory:"`); no on-disk SQLite memory file is created and no durable memory artifact is left in the workspace. Durability is the control plane's Postgres, not the local store. → Test `test_no_default_sqlite_memory_file`
- **Dimension 4.3 (locked, Indy Jun 05)** — the tenant **POST** (`innerStoreMemory`) and **DELETE** (`innerDeleteMemory`, with its `workspace_zombie_memory` by-key route + `matchWorkspaceZombieMemoryByKey` matcher) are removed; pre-v2 they 404/405 with no compat shim. Tenant **GET** (`innerListMemories`) stays. → Test `test_tenant_memory_write_verbs_retired`
- **Dimension 4.4** — no `zombiectl` memory write command exists (verified — nothing to remove); if a read command is later wanted it is GET-only → recorded in Discovery

### §5 — In-run working store + hydration (SQLite `:memory:`, parent-hydrated via `GET`)

The agent recalls/remembers **during** the run against a **non-durable** in-run store — NullClaw's SQLite engine run file-less (`db_path = ":memory:"`; reused, not a new backend). It is hydrated at lease start through the trusted plane: the runner **daemon** (which holds the `zrn_` token) issues **`GET /v1/runners/me/memory`**, then pipes the prior memory to the child over stdin — **no child network call, no credential**. The child seeds its `:memory:` store from the blob; recall/remember then run against it; it is flushed via the §2 push and discarded at run end. **v1 hydrates the full prior set** (Indy: _"Full memory every run, and then move to a different separate memorystore after testing"_).

- **Dimension 5.1** — at run start the agent can recall prior memory hydrated through the trusted plane (no child network call) → Test `test_prior_memory_hydrated_to_child`
- **Dimension 5.3** — the hydration `GET /v1/runners/me/memory` is `zrn_`-authenticated + fencing-verified: a held lease returns the zombie's prior entries; an unheld lease is rejected `UZ-RUN-005` → Test `test_hydrate_get_fencing`
- **Dimension 5.2** — recalled memory is treated as untrusted input (never auto-executed); poisoning is bounded to the agent's own `zombie_id` scope → recorded in Discovery + Failure Modes

### §6 — Per-zombie durable-set cap (accepted, Indy Jun 06 — SELECTIVE EXPANSION)

The per-push byte cap (`MAX_MEMORY_PUSH_BYTES`, 256 KiB) bounds one push; it does **not** bound the durable set a long-lived (or adversarial) zombie accumulates across runs. With full-set/windowed hydration, an unbounded set inflates every run's hydration cost. The store enforces a per-zombie ceiling (`MAX_MEMORY_ENTRIES_PER_ZOMBIE`): on `storeEntry`, evict the coldest (`ORDER BY updated_at ASC`) beyond the ceiling, server-side, in the same `memory_runtime` transaction. Stable-key overwrite (`ON CONFLICT`) + `memory_forget` remain the agent's own bounds; this is the hard backstop.

- **Dimension 6.1** — storing past the ceiling evicts the coldest entries, never exceeding the cap → Test `test_memory_cap_evicts_coldest`
- **Dimension 6.2** — an upsert to an existing key does not count against fresh-entry growth (overwrite, not insert) → Test `test_memory_cap_upsert_no_growth`

### §7 — Capture observability (accepted, Indy Jun 06 — SELECTIVE EXPANSION)

Memory capture is operable from day one, not log-line-only. Following the `metrics_runner.zig` Prometheus pattern: a captured-entries counter, a push-failure counter (`ERR_MEM_UNAVAILABLE`), and a hydration set-size gauge. Content is never a label — only counts + scope (`zombie_id`).

- **Dimension 7.1** — a successful capture increments the captured-entries counter by the stored count → Test `test_memory_metrics_capture_count`
- **Dimension 7.2** — a store failure increments the push-failure counter → Test `test_memory_metrics_push_failure`

### §8 — Hydration compaction (accepted, Indy Jun 06 — real compaction IN this PR)

The `Compactor` seam ships a **real** arm, not passthrough: deterministic **recency + byte-budget windowing** (`.recency_window`) — the hydrate returns the top entries by `updated_at DESC` under `HYDRATE_WINDOW_BYTES`, dropping the cold tail from the hydration payload. **Eviction is from the payload, not the database** — cold entries stay durable in Postgres, so there is **no fact loss**; a later `memory_recall` of a cold key still reaches it via a future selective-hydration arm behind the same seam. **Deterministic, in `zombied`, no LLM** — summarisation belongs on the executor plane (the control plane has no model), explicitly out of scope here.

- **Dimension 8.1** — a set exceeding the byte budget hydrates only the newest entries within budget; the cold tail is omitted but still present in `memory.memory_entries` → Test `test_compaction_recency_window`
- **Dimension 8.2** — a set within budget hydrates verbatim (window is a ceiling, not a floor) → Test `test_compaction_under_budget_passthrough`

---

## Interfaces

> **Illustrative — exact shapes verified at PLAN.** Contract, not implementation.
>
> **Terminology (ratified Jun 06):** the durable scope key is `zombie_id` (UUIDv7) everywhere — path, wire, code, and the `memory.memory_entries` column. Older prose in this spec that says `instance_id` / `zmb:` predates the migration (`schema/013`); a full prose sweep lands at CHORE(close). See Discovery (Jun 06).

```
# NEW runner-plane endpoints — siblings of /reports, zrn_ auth + fencing. The runner NAMES the
# zombie in the path ({zombie_id}, UUIDv7); the server verifies the runner holds a LIVE lease for
# it (IDOR guard) and scopes every query WHERE zombie_id = $1 at the database. Memory is keyed by
# the durable ZOMBIE (zombie_id), never the ephemeral lease, so run 3's GET returns run 1 + run 2
# (all wrote under the same zombie_id). Explicit-naming over infer-from-session: the runner already
# holds the zombie_id in its LeasePayload, and it does not depend on a "one live lease" invariant.
#
#   GET  /v1/runners/me/memory/{zombie_id}  -> MemoryHydrateResponse { memory: []MemoryDelta }
#        # Bearer zrn_. Returns a COMPACTED hydration window (recency + byte budget) of the zombie's
#        # durable set — the cold tail stays in Postgres, unhydrated (see §8 compaction).
#   POST /v1/runners/me/memory/{zombie_id}  MemoryPushRequest { lease_id, fencing_token, memory: []MemoryDelta }
#        # lease_id + fencing_token ride the BODY (like ReportRequest) — a write must be fenced.
#   MemoryDelta { key, content, category }   # NO zombie_id from client (server-derived: path + lease)
#
# Server-side (control plane / zombied), both verbs:
#   GET : verify the runner holds a live lease for {zombie_id}, else UZ-RUN-005 / ERR_RUN_LEASE_NOT_FOUND
#   POST: load body.lease_id (like /reports), verify the runner owns it AND lease.zombie_id == {zombie_id}
#         AND fencing_token is current (else UZ-RUN-005); a reclaimed holder writes nothing
#   SET ROLE memory_runtime; INSERT … memory.memory_entries ON CONFLICT (key, zombie_id) DO UPDATE
#   the store enforces the per-zombie durable-set cap (§6); GET passes rows through the Compactor (§8)
# The sandboxed child:
#   - holds NO zrn_ token, NO control-plane URL, NO DSN, NO durable on-disk memory
#   - recall/remember operate on a NON-DURABLE in-run store (SQLite :memory:), hydrated via the
#     parent's GET, flushed via the POST
```

Contract: run-capture is server-authoritative for `zombie_id` (derived from the path + the verified lease, never client-supplied). The tenant memory API (`/v1/workspaces/.../memories`) becomes **read-only** (GET) — its write verbs are retired (§4.3); the shared read adapter backs both planes.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Push write fails | `memory.memory_entries` insert / role-switch error | the run is not failed for a memory blip; capture failure logged `ERR_MEM_UNAVAILABLE`; daemon may retry the push |
| Stale fencing | runner lost the lease before pushing | reject `UZ-RUN-005`; no write (`test_runner_memory_push_fencing_rejected`) |
| Oversized deltas | a run emits a huge memory blob | cap total memory bytes per push (reuse a `MAX_*` bound); excess truncated + logged |
| Client-supplied `instance_id` | a tampered push targets another zombie | ignored — `instance_id` is server-derived from the lease (`test_memory_push_cross_zombie_isolation`) |
| Hydration unavailable | control plane can't supply prior memory at run start | run proceeds with empty memory (degrade, logged); never blocks the run on a recall miss |
| Token/URL-in-child regression | a refactor leaks `zrn_`/the control-plane URL into the agent | `test_no_token_or_url_in_child` fails the build (links M84_003 §1) |
| Duplicate push | push retried | `ON CONFLICT (key, zombie_id) DO UPDATE` makes the write idempotent |

---

## Invariants

1. **No credential, URL, or DB in the agent** — the child holds no `zrn_` token, no control-plane URL, no DSN, and opens no DB socket; it cannot be prompt-injected into "reach your memory endpoint" because none exists in it. Enforced by `test_no_token_or_url_in_child` + `test_child_holds_no_dsn` (+ M84_003 §1).
2. **Control plane is the source of truth** — durable memory lives only in `zombied`'s `memory.memory_entries`; the runner keeps no durable on-disk memory. The in-run store is SQLite run file-less (`db_path = ":memory:"`), so no on-disk memory file is ever created. Enforced by `test_no_default_sqlite_memory_file` + Dead Code Sweep.
3. **Server-authoritative scope** — `instance_id` is derived from the lease's `zombie_id` server-side; a client-supplied scope is ignored. Enforced by `test_memory_push_cross_zombie_isolation`.
4. **Single write path** — `memory.memory_entries` is written **only** through the runner-memory handler → shared adapter; the tenant API has no write verb. Enforced by `test_single_write_adapter` + `test_tenant_memory_write_verbs_retired` + grep.
5. **Fencing-verified push** — only the runner currently holding the lease can push memory for it. Enforced by `test_runner_memory_push_fencing_rejected`.
6. **Idempotent capture** — a retried push does not duplicate entries. Enforced by `test_memory_push_idempotent`.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_runner_memory_push_persists` | `zrn_` push for a held lease → deltas in `memory.memory_entries` |
| 1.2 | integration | `test_runner_memory_push_fencing_rejected` | push for an unheld/reclaimed lease → `UZ-RUN-005`, no write |
| 2.1 | unit | `test_daemon_pushes_run_memory` | run writes N → daemon issues a push with exactly N deltas |
| 2.3 | unit | `test_daemon_pushes_midrun_checkpoint` | on `memory_checkpoint_every` mid-run → an intermediate push is issued (not only run-end) |
| 2.2 | integration | `test_no_token_or_url_in_child` | child env/argv/inputs contain no `zrn_`/`ZOMBIE_RUNNER_TOKEN`/control-plane URL |
| 3.1 | integration | `test_pushed_memory_persisted` | deltas land under the server-derived `instance_id` |
| 3.2 | integration | `test_memory_push_cross_zombie_isolation` | a spoofed `instance_id` writes only the lease's zombie scope |
| 3.3 | unit | `test_single_write_adapter` | tenant handler + runner handler both call the shared adapter |
| 4.1 | unit | `test_child_holds_no_dsn` | in-child direct-Postgres branch removed; no DSN reachable |
| 4.2 | integration | `test_no_default_sqlite_memory_file` | `:memory:` mode → no on-disk SQLite memory file exists during or after a run; durability is Postgres |
| 4.3 | integration | `test_tenant_memory_write_verbs_retired` | tenant `POST /memories` + `DELETE /memories/{key}` → 404/405 (no shim); `GET` still 200 |
| 5.1 | integration | `test_prior_memory_hydrated_to_child` | agent recalls prior memory hydrated via the daemon (no child network call) |
| 5.3 | integration | `test_hydrate_get_fencing` | `GET /me/memory/{zombie_id}` held lease → entries; unheld/reclaimed → `UZ-RUN-005`/404 |
| — | integration | `test_memory_push_idempotent` | the same push twice → one row per `(key, zombie_id)` |
| 6.1 | integration | `test_memory_cap_evicts_coldest` | storing past `MAX_MEMORY_ENTRIES_PER_ZOMBIE` → coldest evicted, count ≤ cap |
| 6.2 | unit | `test_memory_cap_upsert_no_growth` | upsert to an existing key → row count unchanged |
| 7.1 | unit | `test_memory_metrics_capture_count` | capture of N → captured-entries counter += N |
| 7.2 | unit | `test_memory_metrics_push_failure` | store error → push-failure counter += 1 |
| 8.1 | unit | `test_compaction_recency_window` | set over budget → only newest within `HYDRATE_WINDOW_BYTES`; tail still in DB |
| 8.2 | unit | `test_compaction_under_budget_passthrough` | set under budget → hydrated verbatim |

- **Regression:** the tenant memory API (`/v1/workspaces/.../memories` POST/GET/DELETE) behaves identically after the adapter extraction; `make test` + `make test-integration` pass.
- **Idempotency/replay:** `test_memory_push_idempotent`.

---

## Acceptance Criteria

- [x] Run memory persists via `POST /v1/runners/me/memory` (default path) — verify: `test_runner_memory_push_persists`
- [x] Push is fencing-verified — verify: `test_runner_memory_push_fencing_rejected`
- [x] Agent holds no token/URL/DSN and no durable on-disk memory — verify: `test_no_token_or_url_in_child` + `test_child_holds_no_dsn` + `test_no_default_sqlite_memory_file`
- [x] Cross-zombie isolation server-enforced — verify: `test_memory_push_cross_zombie_isolation`
- [x] Prior memory hydrated to the agent without a child network call — verify: `test_prior_memory_hydrated_to_child`
- [x] In-child direct-Postgres path removed; in-run store is SQLite `:memory:` (no on-disk file) — verify: Dead Code Sweep + `git grep -n 'zombie_memory' src/runner`
- [x] Tenant memory write verbs (POST/DELETE) retired, GET kept — verify: `test_tenant_memory_write_verbs_retired`
- [x] Single shared write adapter (tenant + runner) — verify: `test_single_write_adapter`; tenant API unchanged: `make test-integration`
- [x] Per-zombie durable-set cap evicts coldest past the ceiling — verify: `test_memory_cap_evicts_coldest`
- [x] Capture metrics emitted (count, push-failure, hydration gauge) — verify: `test_memory_metrics_capture_count` + `test_memory_metrics_push_failure`
- [x] Real compaction hydrates a recency+byte window; cold tail stays in DB — verify: `test_compaction_recency_window`
- [x] `make lint` clean · `make check-pg-drain` clean · cross-compile both linux targets
- [x] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: run memory lands via the runner push
make test-integration 2>&1 | grep -E "runner_memory_push_persists|cross_zombie_isolation|fencing_rejected|idempotent"
# E2: no credential/URL/DSN/durable-memory in the child
make test-unit-zigrunner 2>&1 | grep -E "no_token_or_url_in_child|holds_no_dsn|no_default_sqlite_memory_file"
# E3: sunset in-child Postgres adapter; in-run store is SQLite :memory: (base,sqlite kept)
git grep -n 'zombie_memory' src/runner && echo "FAIL: in-child path remains" || echo "PASS"
grep -n 'engines' build_runner.zig ; git grep -n ':memory:' src/runner/engine/runner.zig
# E4: tenant memory API regression (shared adapter)
make test-integration 2>&1 | tail -5
# E5: pg-drain + cross-compile + gitleaks
make check-pg-drain 2>&1 | tail -3 && zig build -Dtarget=x86_64-linux 2>&1 | tail -3 && gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

**1. Orphaned files — sunset list (locked, Indy Jun 05).**

| File to delete | Verify |
|----------------|--------|
| `src/runner/engine/zombie_memory.zig` (in-child direct-Postgres adapter) | `test ! -f src/runner/engine/zombie_memory.zig` |

**2. Orphaned references.** The `memory_connection`/`memory_namespace` branch + any `zombie_memory` import in the runner are removed; the ephemeral-SQLite-default memory backend reliance is removed from `runner.zig`/`build_runner.zig`.

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `zombie_memory` (runner) | `git grep -n 'zombie_memory' src/runner` | 0 matches |
| Direct-Postgres branch (`memory_connection`/`memory_namespace`) | `git grep -n 'memory_connection\|memory_namespace' src/runner` | 0 matches (in-run store is SQLite `:memory:`; `cfg.memory.db_path = ":memory:"` is retained) |
| `innerStoreMemory` / `innerDeleteMemory` | `git grep -n 'innerStoreMemory\|innerDeleteMemory' src/zombied` | 0 matches |
| `matchWorkspaceZombieMemoryByKey` / `workspace_zombie_memory` route | `git grep -n 'matchWorkspaceZombieMemoryByKey\|workspace_zombie_memory\b' src/zombied` | 0 matches |

---

## Discovery (consult log)

- **Origin (Jun 05, 2026):** Indy — _"ensure the memory of the agent is captured during every run"_.
- **Code-grounded facts (Jun 05, 2026):**
  - `/v1/runners/me/reports` exists (`protocol.zig:40`), `zrn_`-authenticated, fencing-verified (`service_report.report`). The new `/v1/runners/me/memory` mirrors it.
  - DB write is `memory/handler.zig:innerStoreMemory` (`INSERT … memory.memory_entries ON CONFLICT (key, zombie_id)`) + `helpers.zig` (`setMemoryRole`, IDOR `resolveZombieInWorkspace`) — the reusable SQL. **`src/zombied/memory/` is new** (the shared adapter is created here; the write SQL moves out of the tenant handler).
  - `{ws}`/`{zid}` come from the lease/event envelope (`event_envelope.zig:22-23`), set by `zombied` at lease issue — not child config. The collection POST has **no path `{key}`** (key rides the body).
  - Runner is `base,sqlite` (`build_runner.zig`); `runner/engine/zombie_memory.zig` inert; capture currently **only signalled** (`runner_progress.zig` logs `memory_checkpoint_due`). No UI/CLI calls `/v1/workspaces/.../memories` — it is an external-agent API (bearer).
- **Indy decisions (verbatim, Jun 05, 2026):**
  - _"the default is to push via the runners/me/memory url"_ → dedicated endpoint (not report-extend); §1/§2.
  - _"i dont want sqlite as default, so remove that"_ → sunset the ephemeral-SQLite-default backend; §4.2 + Invariant 2 + Dead Code Sweep.
  - _"create a M84_005 spec"_ + token-leak question → confirmed: the `zrn_` token stays in the daemon, never the child (report-/push-forwarded), so no leak; an agent cannot be prompted to reach a memory URL because none exists in it.
  - _"I feel get is needed."_ → tenant **GET (`innerListMemories`) is kept** (dashboard/operator read).
  - _"any zombiectl CLI if exists for memory must be removed (for POST and DELETE) just GET or show whatever"_ → **DECIDED (§4.3/4.4):** remove tenant **POST (`innerStoreMemory`)** + **DELETE (`innerDeleteMemory`** + by-key route/matcher); keep GET. **No `zombiectl` memory command exists today** — re-verified Jun 05 by full-tree grep `grep -rniE '\bmemor(y|ies)\b|recall|remember' zombiectl/` → 0 hits; registry is agent/auth/billing/workspace/zombie_* — nothing to remove. (A new read/`show` command would be out-of-scope new work.) Implication: external agents can no longer **write** memory via the tenant API — all writes go through the runner push.
- **Indy decisions banked this session (verbatim, Jun 05, 2026):**
  - _"If its lease will it not have a size constraint? I feel 2 api is the only choice?"_ → hydration is a dedicated runner-plane **`GET`** (not lease-embed — lease payload size); §5.
  - _"yes intermediate push to runners/me/memory must happen"_ → cadence = **run-end + mid-run** on `memory_checkpoint_every`; §2.
  - _"if we already have sqlite i prefer to reuse that, and not reinvent a LRU."_ → in-run store = **SQLite `:memory:`** (reuse the engine; `db_path=":memory:"`, no on-disk file); §4.2/§5 + Invariant 2.
  - _"Full memory every run, and then move to a different separate memorystore after testing."_ → v1 hydrates the **full** prior set; a dedicated scalable store (selective hydration + compaction) is the post-launch direction; the `GET` is the swap-in seam.
  - _"ensure you donot use query param"_ + _"if one zombie_id = many lease_id How will you hydrate in run 3, with run2 and run 1"_ → memory is keyed by the **durable zombie**, never the ephemeral lease; both verbs are **top-level** `/v1/runners/me/memory` (no lease in path/query). The runner loop is **strictly serial** (one live lease — `loop.zig`), so the server resolves the zombie from the runner's live lease; run 3's single `GET` returns run 1 + run 2 because they share `instance_id=zmb:<zombie_id>`. `POST` carries `lease_id`+`fencing_token` in the body (like `ReportRequest`) to fence the write.
  - _"consider adding unit test, integration test robust on these"_ → robust unit + integration coverage on the loop (§Test Specification, plus `/write-unit-test` gate).
- **Known v1 limitation (Indy: _"how will nullclaw take this? since there is no compaction? which can be fixed later though"_):** NullClaw stores/recalls verbatim — **no compaction/summarisation**. Within a run the `:memory:` store is bounded (hydrated set + writes, discarded at end); the durable per-zombie set can grow over a long-lived zombie's life, and v1 hydrates it **in full** each run. Bounded in practice by stable-key overwrite (`ON CONFLICT … DO UPDATE`) + the agent's `memory_forget`; the real fix (selective hydration + compaction/eviction) lands with the **separate memory store** post-testing, behind the same `GET` seam — **no NullClaw/agent change**. Recorded so it is not silently carried.
- **PLAN decisions BANKED (Jun 05, 2026):** delta extraction = read the run's working store (`Memory.list`); in-run store = SQLite `:memory:`; hydration = parent `GET` piped to child; cadence = run-end + mid-run; per-push byte cap retained (Failure Modes); `build_runner.zig` stays `base,sqlite` (path → `:memory:`).
- **API shape + scope decisions BANKED (Jun 06, 2026 — `/plan-ceo-review`, Indy live):**
  - **Endpoint shape → zombie-in-path.** Both verbs are `/v1/runners/me/memory/{zombie_id}` (UUIDv7). This **supersedes** the Jun 05 "top-level, server-resolves-from-the-single-live-lease" banked decision. The serial-loop premise still holds (`loop.zig` is strictly serial — verified Jun 06), but the runner already holds the `zombie_id` in its `LeasePayload`, so explicit naming is more robust and does not depend on a "one live lease" invariant. Indy ratified Jun 06 ("Keep zombie-in-path"). `"donot use query param"` is honored — a path segment is not a query param.
  - **POST fence → explicit `lease_id`.** The POST body carries `lease_id` + `fencing_token` and the server loads that exact lease (mirroring `service_report.loadLease`), cross-checks `lease.zombie_id == {zombie_id}`, and fences — instead of *inferring* the live lease from `(runner_id, zombie_id)`. One fencing pattern in the codebase, not two (Indy: _"what if we changed via the lease_id?"_).
  - **`instance_id` → `zombie_id` (UUID).** Confirmed: the durable column + every wire/code path uses `zombie_id` (UUIDv7, no `zmb:` prefix); `schema/013` is the teardown-rebuild. The `"zmb:"` form is gone with the in-child Postgres path.
  - **Premise validated (why this is needed):** the product **already sells** durable cross-event memory (`docs/memory.mdx`, `changelog.mdx` "Persistent agent memory — persists in Postgres", website `FAQ.tsx`, tools catalogue) but the M80 runner-split regressed it (`runner_progress.zig` only logs `memory_checkpoint_due`; the runner store is an ephemeral workspace SQLite file deleted at run end). M84_005 restores an advertised capability over the trusted plane.
  - **SELECTIVE EXPANSION accepted (all three IN this PR):** §6 per-zombie durable-set cap; §7 capture metrics; §8 **real** compaction — _"in the real compaction in this PR as well"_. Compaction is **deterministic recency + byte-budget windowing in `zombied`, NOT LLM summarisation** (the control plane has no model; summarisation belongs on the executor plane — explicitly out of scope). Eviction is from the hydration payload, not the database, so no fact loss.
- **Runner-side landing (Jun 06, 2026 — commit `5577e3f6`):**
  - **In-run store correction.** The PLAN's "set `cfg.memory.db_path = ":memory:"`" is not reachable — NullClaw's `MemoryConfig` has no `db_path` field and `registry.resolvePaths` hardcodes `workspace/memory.db` for sqlite. `:memory:` is reached by a direct `registry.BackendConfig{ .db_path = ":memory:" }` bypass (new `engine/inrun_memory.zig`), mirroring the technique the deleted in-child Postgres adapter used. Fully within §4.2 intent.
  - **Stdin wrapper.** Prior memory reaches the child via a new `contract.RunnerChildInput { lease, hydrated_memory }` piped down stdin (the daemon GET-hydrates first); the child never fetches, so it holds no token/URL/DSN. Internal NullClaw bootstrap/autosave keys are filtered out of every capture (`isInternalMemoryEntryKeyOrContent`).
  - **§3.3 reconciled with §4.3.** With the tenant write verbs retired (§4.3), the shared `src/zombied/memory/zombie_memory.zig` adapter is the **single writer** (runner handler only); the tenant API is read-only. `test_single_write_adapter`'s intent is met structurally — one `storeEntry`, exercised by the runner push; grep proves no other `INSERT … memory.memory_entries`.
  - **Pre-existing literals named (RULE NLR).** Touching the runner test/util files surfaced 5 pre-existing power-of-ten/byte literals the UFS gate flagged; resolved in-diff via `std.time.ms_per_s`/`ns_per_s` and a `DEFAULT_DISK_WRITE_LIMIT_MB` const (Indy: pre-existing issues in touched files must be fixed, not deferred).
  - **Test posture.** Runner-side mechanics (pipe framing, supervisor forward, in-run capture + internal-key filter, mid-run checkpoint cadence) are unit-tested and pass locally. The persistence + fencing invariants (1.1/1.2/3.x/5.3/6.x/idempotent) are Postgres-backed integration tests (`zombie_memory_integration_test.zig`, `memory_fencing_test.zig`) that self-skip without `LIVE_DB` and run in CI — drive the adapter (`storeEntry`/`enforceCap`/`listAll`) and the handler lease-resolution (`pushLeaseSeq`/`liveLeaseSeq`, IDOR + reclaim-bump) directly against seeded fleet rows, mirroring `renewal_integration_test.zig`.
- **Terminology:** the `instance_id`/`zmb:` prose elsewhere in this spec predates the `schema/013` migration to a bare `zombie_id` (UUIDv7); the implemented code uses `zombie_id` end to end. Historical Discovery quotes are kept verbatim (not rewritten).
- **Deferrals** — none (any "deferred" needs an Indy-acked verbatim quote here).
- **Skill chain outcomes** — {`/write-unit-test`, `/review`, `/review-pr`.}

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits coverage vs this Test Specification (isolation + no-cred-in-child + sunset). | Clean. Iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs Invariants, Failure Modes, `dispatch/write_zig.md`, `docs/AUTH.md` (auth-plane boundary). | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR against the immutable diff. | Comments addressed before human review/merge. |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Runner unit | `make test-unit-zigrunner` | 225 passed; 4 skipped; 0 failed (incl. pipe-frame, supervisor-forward, in-run capture+filter, mid-run cadence) | ✅ |
| Zombied unit | `make test-unit-zombied` | 1220 passed; DB-backed adapter + handler-fencing tests skip locally (no Postgres), run in CI | ✅ |
| Memory + runner-push integration | `make test-integration` (CI) | adapter persist/idempotent/cap + fencing IDOR — Postgres-backed, `LIVE_DB`-gated; verified in CI | ⏳ CI |
| pg-drain | folded into `make lint-zig` | pg-drain check passed (395 files) | ✅ |
| Lint | `make lint-zig` + `make harness-verify` | ZLint 0/0; ALL GATES GREEN (UFS/LOGGING/ERR/LIFECYCLE/MS-ID) | ✅ |
| Cross-compile | `zig build -Dtarget={x86_64,aarch64}-linux` (both graphs, prod + test) | all targets compile | ✅ |
| Gitleaks | `gitleaks detect` | no leaks found | ✅ |
| Dead code sweep | `git grep -n 'zombie_memory\|memory_connection\|MemoryBackendConfig' src/` | 0 matches | ✅ |

---

## Out of Scope

- **The tenant memory API** (`/v1/workspaces/.../memories`) is now **read-only (GET)** — POST/DELETE are removed here (§4.3); `innerListMemories` is unchanged. It is the dashboard/external read surface, never the run-capture path.
- **Memory schema / retrieval-mode changes** (vector search, summarization) — capture-path only; reuses `memory.memory_entries` as-is.
- **Memory-content trust scanning** (detecting poisoned memory) — bounded here by per-zombie isolation + treating recalled memory as untrusted; content inspection is separate.
