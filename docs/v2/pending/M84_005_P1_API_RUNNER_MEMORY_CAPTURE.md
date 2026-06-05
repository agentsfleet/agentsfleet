# M84_005: Per-run agent memory capture — push to `/v1/runners/me/memory`, control plane persists

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 005
**Date:** Jun 05, 2026
**Status:** PENDING
**Priority:** P1 — operator/customer-facing capability (agents remember across runs) **with** a security boundary: the capture path must not put a credential, a control-plane URL, or a database connection inside the untrusted sandboxed agent.
**Categories:** API
**Batch:** B1 — runs **in parallel** with M84_003 (security launch slice); disjoint trees (`src/zombied/`+contract+runner memory wiring here vs `src/runner/` process-boundary there), two shared touchpoints to coordinate (`build_runner.zig`, `make/test-integration.mk` — second to land rebases).
**Branch:** {feat/m84-runner-memory-capture — added at CHORE(open)}
**Depends on:** **M84_003 (sandbox env/fd/cap hardening)** — the no-token-leak guarantee requires `ZOMBIE_RUNNER_TOKEN` to be absent from the child (M84_003 §1). Relates to **M84_004 (egress)** — this design needs **no** child egress (the daemon pushes), so it does not depend on the egress allowlist.
**Provenance:** agent-generated — Indy asked to "ensure the memory of the agent is captured during every run" (Jun 05, 2026); code-grounded in the Orly Chief Technology Officer (CTO) review, decisions locked by Indy (Jun 05, 2026).

> **Provenance is load-bearing.** Verified against `protocol.zig` (report shape + `/v1/runners` paths), `runner/report.zig` + `fleet/service_report.zig` (the `zrn_`-authenticated runner-plane handler with fencing), `memory/handler.zig` + `memory/helpers.zig` (the `memory.memory_entries` write + Row-Level Security (RLS) + Insecure-Direct-Object-Reference (IDOR) guard), `runner/engine/runner.zig` + `runner/engine/zombie_memory.zig` (the inert in-child direct-Postgres path; the SQLite-default fallback), and `build_runner.zig` (`base,sqlite`). Re-confirm at PLAN.

**Canonical architecture:** [`docs/architecture/runner_fleet.md`](../../architecture/runner_fleet.md) §Egress model ("durable memory rides the trusted plane, never the agent") and §Datastore role model. The control-plane (`zombied`) memory write + RLS model are in `src/zombied/http/handlers/memory/`.

---

> **IN FOR LAUNCH — full implementation (Indy, Jun 05, 2026).** The marketing site already sells durable memory (`memory_store`, "memory checkpoints", "durable memory"), so shipping it makes an existing product claim true rather than adding net-new scope. Full scope ships: runner push to `/v1/runners/me/memory` as the write path, server-side persist to `memory.memory_entries`, sunset of the tenant POST/DELETE + the SQLite default, tenant GET kept. Pairs with M84_003 in the launch milestone (M84_004 egress stays deferred).

## Implementing agent — read these first

1. `src/lib/contract/protocol.zig` — the `/v1/runners/me/*` path constants + `ReportRequest`/fencing shape; this workstream adds a sibling `/v1/runners/me/memory` path + request type.
2. `src/zombied/http/handlers/runner/report.zig` + `src/zombied/fleet/service_report.zig` — the existing `zrn_`-authenticated runner handler with fencing verification (`UZ-RUN-005`); the new memory handler mirrors its auth + fencing, then persists.
3. `src/zombied/http/handlers/memory/handler.zig` + `helpers.zig` — `innerStoreMemory` (`INSERT … memory.memory_entries ON CONFLICT (key, instance_id)`), `setMemoryRole`, `resolveZombieInWorkspace` (builds `instance_id`); the write extracts into the shared adapter.
4. `src/runner/daemon/loop.zig` — where the daemon drives a lease; the memory push is issued here (daemon-side, holds the `zrn_` token).
5. `src/runner/engine/runner.zig` (§4 memory) + `src/runner/engine/zombie_memory.zig` + `build_runner.zig` — the **inert** in-child direct-Postgres path AND the **ephemeral-SQLite-as-default** backend; both are sunset by this workstream.
6. `dispatch/write_zig.md` · `dispatch/write_sql.md` (if the write touches schema) · `docs/AUTH.md` (the `zrn_` runner-plane auth boundary).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m84): per-run agent memory capture via /v1/runners/me/memory (no creds in the agent)`
- **Intent (one sentence):** Every run's agent memory is pushed by the **daemon** to `POST /v1/runners/me/memory` over the existing `zrn_` runner-plane auth and persisted by the **control plane (`zombied`)** to `memory.memory_entries` — so the sandboxed agent never holds a database connection, a control-plane token, or a memory URL, and the local ephemeral-SQLite default is gone.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. **Resolve two coupled mechanisms:** (a) how the runner obtains the run's memory deltas from NullClaw (read the run's working store vs hook the `remember` tool); (b) **with SQLite removed, what in-run working store the agent recalls/writes against** (an in-memory backend hydrated at lease start vs another non-durable store) and how prior memory is hydrated to the child **without** giving it a credential/URL. A `[?]` here blocks the spec.

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
| LENGTH | **maybe** — new handler + adapter | Memory persist lives in the shared adapter (`src/memory/zombie_memory.zig`), not inline in the handler. |
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
- **The tenant memory write** (`memory/handler.zig:innerStoreMemory` + `helpers.zig`: `INSERT … ON CONFLICT (key, instance_id)` under `SET ROLE memory_runtime` with the IDOR-guarded `instance_id`) is the known-good SQL. Extract it into a shared `src/memory/zombie_memory.zig` adapter used by **both** the tenant handler and the new runner-memory handler — don't fork the SQL.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/lib/contract/protocol.zig` | EDIT | Add `PATH_RUNNER_MEMORY` (`/v1/runners/me/memory`) + a `MemoryPushRequest { lease_id, memory: []MemoryDelta }` type (UFS field names). |
| `src/zombied/http/handlers/runner/memory.zig` | CREATE | New `zrn_` handler: verify fencing for `lease_id` (like `/reports`), persist deltas via the shared adapter with server-derived `instance_id`. |
| `src/zombied/http/{router,route_table,route_table_invoke}.zig` | EDIT | Wire `POST /v1/runners/me/memory` → the new handler (runner-plane middleware, not bearer). |
| `src/memory/zombie_memory.zig` | CREATE | Shared write adapter (`setMemoryRole` + `instance_id` + `INSERT … ON CONFLICT`) reused by tenant handler + runner-memory handler. |
| `src/zombied/http/handlers/memory/handler.zig` | EDIT | **Remove** `innerStoreMemory` (POST) + `innerDeleteMemory` (DELETE); keep `innerListMemories` (GET). The runner-memory handler is the only writer (via the shared adapter). |
| `src/zombied/http/{router,route_table,route_table_invoke,route_matchers,routes}.zig` | EDIT | Drop the tenant memory **POST** dispatch and the **`workspace_zombie_memory`** by-key (DELETE) route + `matchWorkspaceZombieMemoryByKey`; keep the collection **GET**. Retired verbs 404/405 (pre-v2, no shim). |
| `src/runner/engine/runner.zig` (+ result frame) | EDIT | Surface the run's memory deltas to the parent over the pipe; **remove the SQLite-default + direct-Postgres memory branch**; use a non-durable in-run working store (hydration resolved at PLAN). |
| `src/runner/daemon/loop.zig` | EDIT | Push the surfaced deltas to `/v1/runners/me/memory` (daemon-side; cadence: run-end, optionally mid-run on `memory_checkpoint_every`). |
| `src/runner/engine/zombie_memory.zig` | DELETE | Retire the inert in-child direct-Postgres adapter (RULE NDC). |
| `build_runner.zig` | EDIT | Drop the SQLite-default memory engine reliance per the in-run-store decision (PLAN-confirmed engine set). |
| `docs/architecture/runner_fleet.md` | EDIT (small) | Document the memory-push path under §Egress model / §Datastore role model. |

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

The child surfaces deltas over the stdout pipe; the **daemon** pushes them to `/v1/runners/me/memory` using the `zrn_` token it holds. Cadence: at run end (mandatory), and optionally mid-run on `memory_checkpoint_every`.

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
- **Dimension 4.2** — the runner no longer defaults to an ephemeral on-disk SQLite memory file; no durable memory artifact is left in the workspace → Test `test_no_default_sqlite_memory_file`
- **Dimension 4.3 (locked, Indy Jun 05)** — the tenant **POST** (`innerStoreMemory`) and **DELETE** (`innerDeleteMemory`, with its `workspace_zombie_memory` by-key route + `matchWorkspaceZombieMemoryByKey` matcher) are removed; pre-v2 they 404/405 with no compat shim. Tenant **GET** (`innerListMemories`) stays. → Test `test_tenant_memory_write_verbs_retired`
- **Dimension 4.4** — no `zombiectl` memory write command exists (verified — nothing to remove); if a read command is later wanted it is GET-only → recorded in Discovery

### §5 — In-run working store + hydration (the recall path without SQLite)

With SQLite removed, the agent still needs to recall/remember **during** the run. The working set is a **non-durable** in-run store (in-memory), hydrated at lease start from the control plane **without** handing the child a credential/URL (the daemon feeds prior memory over the pipe / on the lease), and flushed via the §2 push.

- **Dimension 5.1** — at run start the agent can recall prior memory hydrated through the trusted plane (no child network call) → Test `test_prior_memory_hydrated_to_child`
- **Dimension 5.2** — recalled memory is treated as untrusted input (never auto-executed); poisoning is bounded to the agent's own `instance_id` → recorded in Discovery + Failure Modes

---

## Interfaces

> **Illustrative — exact shapes verified at PLAN.** Contract, not implementation.

```
# NEW runner-plane endpoint (zrn_ auth, like /reports):
#   POST /v1/runners/me/memory
#   MemoryPushRequest { lease_id, memory: []MemoryDelta }
#   MemoryDelta       { key, content, category }      # NO instance_id from client
# Server-side persist (control plane / zombied):
#   verify the runner holds lease_id (fencing) else UZ-RUN-005
#   instance_id := derive("zmb:" + lease.zombie_id)   # server-derived, NOT client-supplied
#   SET ROLE memory_runtime; INSERT memory.memory_entries … ON CONFLICT (key, instance_id) DO UPDATE
# The sandboxed child:
#   - holds NO zrn_ token, NO control-plane URL, NO DSN, NO durable on-disk memory
#   - recall/remember operate on a NON-DURABLE in-run store, hydrated via the daemon, flushed via the push
```

Contract: the tenant memory API (`/v1/workspaces/.../memories`) is observably unchanged (it delegates its write to the shared adapter); run-capture is server-authoritative for `instance_id`.

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
| Duplicate push | push retried | `ON CONFLICT (key, instance_id) DO UPDATE` makes the write idempotent |

---

## Invariants

1. **No credential, URL, or DB in the agent** — the child holds no `zrn_` token, no control-plane URL, no DSN, and opens no DB socket; it cannot be prompt-injected into "reach your memory endpoint" because none exists in it. Enforced by `test_no_token_or_url_in_child` + `test_child_holds_no_dsn` (+ M84_003 §1).
2. **Control plane is the source of truth** — durable memory lives only in `zombied`'s `memory.memory_entries`; the runner keeps no durable on-disk memory (no SQLite default). Enforced by `test_no_default_sqlite_memory_file` + Dead Code Sweep.
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
| 2.2 | integration | `test_no_token_or_url_in_child` | child env/argv/inputs contain no `zrn_`/`ZOMBIE_RUNNER_TOKEN`/control-plane URL |
| 3.1 | integration | `test_pushed_memory_persisted` | deltas land under the server-derived `instance_id` |
| 3.2 | integration | `test_memory_push_cross_zombie_isolation` | a spoofed `instance_id` writes only the lease's zombie scope |
| 3.3 | unit | `test_single_write_adapter` | tenant handler + runner handler both call the shared adapter |
| 4.1 | unit | `test_child_holds_no_dsn` | in-child direct-Postgres branch removed; no DSN reachable |
| 4.2 | integration | `test_no_default_sqlite_memory_file` | after a run, no durable SQLite memory file remains in the workspace |
| 4.3 | integration | `test_tenant_memory_write_verbs_retired` | tenant `POST /memories` + `DELETE /memories/{key}` → 404/405 (no shim); `GET` still 200 |
| 5.1 | integration | `test_prior_memory_hydrated_to_child` | agent recalls prior memory hydrated via the daemon (no child network call) |
| — | integration | `test_memory_push_idempotent` | the same push twice → one row per `(key, instance_id)` |

- **Regression:** the tenant memory API (`/v1/workspaces/.../memories` POST/GET/DELETE) behaves identically after the adapter extraction; `make test` + `make test-integration` pass.
- **Idempotency/replay:** `test_memory_push_idempotent`.

---

## Acceptance Criteria

- [ ] Run memory persists via `POST /v1/runners/me/memory` (default path) — verify: `test_runner_memory_push_persists`
- [ ] Push is fencing-verified — verify: `test_runner_memory_push_fencing_rejected`
- [ ] Agent holds no token/URL/DSN and no durable on-disk memory — verify: `test_no_token_or_url_in_child` + `test_child_holds_no_dsn` + `test_no_default_sqlite_memory_file`
- [ ] Cross-zombie isolation server-enforced — verify: `test_memory_push_cross_zombie_isolation`
- [ ] Prior memory hydrated to the agent without a child network call — verify: `test_prior_memory_hydrated_to_child`
- [ ] In-child direct-Postgres path + SQLite default removed — verify: Dead Code Sweep + `git grep -n 'zombie_memory' src/runner`
- [ ] Tenant memory write verbs (POST/DELETE) retired, GET kept — verify: `test_tenant_memory_write_verbs_retired`
- [ ] Single shared write adapter (tenant + runner) — verify: `test_single_write_adapter`; tenant API unchanged: `make test-integration`
- [ ] `make lint` clean · `make check-pg-drain` clean · cross-compile both linux targets
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: run memory lands via the runner push
make test-integration 2>&1 | grep -E "runner_memory_push_persists|cross_zombie_isolation|fencing_rejected|idempotent"
# E2: no credential/URL/DSN/durable-memory in the child
make test-unit-zigrunner 2>&1 | grep -E "no_token_or_url_in_child|holds_no_dsn|no_default_sqlite_memory_file"
# E3: sunset paths gone (in-child Postgres + SQLite default)
git grep -n 'zombie_memory' src/runner && echo "FAIL: in-child path remains" || echo "PASS"
grep -n 'engines' build_runner.zig   # SQLite-default reliance dropped per PLAN
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
| SQLite-default memory wiring | `git grep -n 'cfg.memory' src/runner/engine/runner.zig` | 0 (replaced by in-run store) |
| `innerStoreMemory` / `innerDeleteMemory` | `git grep -n 'innerStoreMemory\|innerDeleteMemory' src/zombied` | 0 matches |
| `matchWorkspaceZombieMemoryByKey` / `workspace_zombie_memory` route | `git grep -n 'matchWorkspaceZombieMemoryByKey\|workspace_zombie_memory\b' src/zombied` | 0 matches |

---

## Discovery (consult log)

- **Origin (Jun 05, 2026):** Indy — _"ensure the memory of the agent is captured during every run"_.
- **Code-grounded facts (Jun 05, 2026):**
  - `/v1/runners/me/reports` exists (`protocol.zig:40`), `zrn_`-authenticated, fencing-verified (`service_report.report`). The new `/v1/runners/me/memory` mirrors it.
  - DB write is `memory/handler.zig:innerStoreMemory` (`INSERT … memory.memory_entries ON CONFLICT (key, instance_id)`) + `helpers.zig` (`setMemoryRole`, IDOR `resolveZombieInWorkspace`) — the reusable adapter. **`src/memory/` is empty today** (the shared adapter is created here).
  - `{ws}`/`{zid}` come from the lease/event envelope (`event_envelope.zig:22-23`), set by `zombied` at lease issue — not child config. The collection POST has **no path `{key}`** (key rides the body).
  - Runner is `base,sqlite` (`build_runner.zig`); `runner/engine/zombie_memory.zig` inert; capture currently **only signalled** (`runner_progress.zig` logs `memory_checkpoint_due`). No UI/CLI calls `/v1/workspaces/.../memories` — it is an external-agent API (bearer).
- **Indy decisions (verbatim, Jun 05, 2026):**
  - _"the default is to push via the runners/me/memory url"_ → dedicated endpoint (not report-extend); §1/§2.
  - _"i dont want sqlite as default, so remove that"_ → sunset the ephemeral-SQLite-default backend; §4.2 + Invariant 2 + Dead Code Sweep.
  - _"create a M84_005 spec"_ + token-leak question → confirmed: the `zrn_` token stays in the daemon, never the child (report-/push-forwarded), so no leak; an agent cannot be prompted to reach a memory URL because none exists in it.
  - _"I feel get is needed."_ → tenant **GET (`innerListMemories`) is kept** (dashboard/operator read).
  - _"any zombiectl CLI if exists for memory must be removed (for POST and DELETE) just GET or show whatever"_ → **DECIDED (§4.3/4.4):** remove tenant **POST (`innerStoreMemory`)** + **DELETE (`innerDeleteMemory`** + by-key route/matcher); keep GET. **No `zombiectl` memory command exists today** (verified — nothing to remove on the CLI). Implication recorded: external agents can no longer **write** memory via the tenant API — all writes go through the runner push.
- **PLAN decisions to bank:** the delta extraction mechanism; the **in-run working store** that replaces SQLite (in-memory backend) + how prior memory is **hydrated** to the child without a credential; push cadence (run-end vs `memory_checkpoint_every`); per-push byte cap; the resulting `build_runner.zig` engine set.
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
| Runner unit | `make test-unit-zigrunner` | {paste snippet} | |
| Memory + runner-push integration | `make test-integration` | {paste snippet} | |
| pg-drain | `make check-pg-drain` | {paste snippet} | |
| Lint | `make lint` | {paste snippet} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |
| Dead code sweep | `git grep -n 'zombie_memory' src/runner` | {paste snippet} | |

---

## Out of Scope

- **The tenant memory API** (`/v1/workspaces/.../memories`) is now **read-only (GET)** — POST/DELETE are removed here (§4.3); `innerListMemories` is unchanged. It is the dashboard/external read surface, never the run-capture path.
- **Memory schema / retrieval-mode changes** (vector search, summarization) — capture-path only; reuses `memory.memory_entries` as-is.
- **Memory-content trust scanning** (detecting poisoned memory) — bounded here by per-zombie isolation + treating recalled memory as untrusted; content inspection is separate.
