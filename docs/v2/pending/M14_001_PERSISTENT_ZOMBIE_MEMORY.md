# M14_001: Persistent Zombie Memory — Agent Memory Survives Crashes and Restarts

**Prototype:** v2
**Milestone:** M14
**Workstream:** 001
**Date:** Apr 10, 2026
**Status:** PENDING
**Priority:** P1 — Zombies lose all learned context on crash/restart; every restart is a cold start
**Batch:** B1
**Depends on:** M5_001 (dynamic skill architecture, tool_bridge)

---

## Overview

**Goal (testable):** Zombie agent memory persists across workspace destruction, process crashes, and zombie restarts — verified by storing a fact in run N and recalling it in run N+1 after workspace cleanup.

**Problem:** NullClaw's memory tools (`memory_store`, `memory_recall`, `memory_list`, `memory_forget`) default to SQLite at `<workspace_dir>/memory.db`. The executor workspace is a temporary worktree deleted after each run. On crash, the workspace is destroyed. On restart, a fresh workspace is created. Result: every zombie run starts with zero memory. The agent cannot learn from prior interactions, remember user preferences, or accumulate context across conversations. For long-running zombies that handle ongoing work (bug triage, lead response, incident management), this is a serious capability gap.

**Solution summary:** Evaluate durable storage backends that survive workspace lifecycle. NullClaw already supports pluggable memory backends (Postgres, Redis, API gateway, markdown files). The work is: (1) evaluate alternatives against UseZombie's deployment topology (Fly.io workers, Cloudflare edge, bare-metal), (2) select and configure the backend, (3) wire the executor to pass memory backend config per-zombie, (4) verify cross-run persistence with integration tests.

---

## 1.0 Storage Backend Evaluation

**Status:** PENDING

Evaluate each candidate against these criteria: durability (survives crash), latency (memory recall during agent conversation), operational complexity (what we run/pay for), multi-zombie isolation (zombies don't read each other's memory), and deployment topology fit (Fly workers, Cloudflare, bare-metal).

NullClaw v2026.4.9 supports these memory backend profiles (from `config_types.zig`): `hybrid_keyword`, `local_keyword`, `markdown_only`, `postgres_keyword`, `postgres_hybrid`, `minimal_none`, plus custom backends (Redis, ClickHouse, LanceDB, API gateway).

**Dimensions (test blueprints):**

- 1.1 PENDING — Postgres (existing infrastructure)
  - target: evaluation document
  - input: UseZombie already runs Postgres for `core.*` tables. Memory tables would be per-zombie namespace in the same cluster.
  - expected: Evaluation covering — latency impact on shared DB, isolation model (schema per workspace vs row-level), backup/retention, vector search support (`pgvector` extension availability on Fly Postgres), connection pool pressure from N concurrent zombies doing memory ops
  - test_type: contract

- 1.2 PENDING — Cloudflare R2 / Fly Volumes / shared filesystem
  - target: evaluation document
  - input: R2 is S3-compatible object storage on Cloudflare edge. Fly Volumes are persistent NVMe attached to Fly machines. Evaluate both as durable backing for SQLite (SQLite on a persistent volume that outlives the workspace).
  - expected: Evaluation covering — R2 latency from Fly workers (cross-network hop?), Fly Volume attachment model (one volume per machine, not shareable across machines), SQLite on network filesystem limitations (WAL mode fails on NFS/9p — NullClaw already has a fallback to DELETE journal mode), Dragonfly/KeyDB as Redis-compatible alternatives with persistence
  - test_type: contract

- 1.3 PENDING — Dragonfly / KeyDB / Valkey (Redis-compatible with persistence)
  - target: evaluation document
  - input: UseZombie already runs Redis for event streams (XREADGROUP). Dragonfly is a Redis-compatible engine with native persistence (snapshots + AOF), multi-threaded, lower memory footprint. KeyDB is Redis fork with multi-threading. Valkey is the Redis OSS successor.
  - expected: Evaluation covering — can memory ops coexist on the same Redis instance as event streams without latency interference, persistence guarantees (Dragonfly snapshot interval vs AOF), key namespace isolation per zombie, memory data model fit (NullClaw memory is key-value with categories — maps naturally to Redis hashes), vector search support (RediSearch module for hybrid recall)
  - test_type: contract

- 1.4 PENDING — Decision matrix and recommendation
  - target: evaluation document
  - input: Results from 1.1–1.3
  - expected: Scored matrix (durability, latency, ops cost, isolation, topology fit) with clear recommendation and fallback. Document trade-offs explicitly. If Postgres wins, note the connection pool concern. If Dragonfly wins, note the new dependency cost.
  - test_type: contract

---

## 2.0 Memory Backend Configuration Wiring

**Status:** PENDING

Wire the selected backend through the executor config path. NullClaw's `MemoryRuntime` is initialized from `config_types.zig` `MemoryConfig`. The executor must pass per-zombie memory config so each zombie gets its own isolated memory namespace.

**Dimensions (test blueprints):**

- 2.1 PENDING — Executor config accepts memory backend settings
  - target: `src/executor/types.zig:ExecutorConfig` or new `memory_config` field
  - input: Memory backend type (postgres/redis/dragonfly), connection string, zombie-scoped namespace key
  - expected: Config struct extended, validated at startup, passed through to NullClaw `Config.memory`
  - test_type: unit

- 2.2 PENDING — Per-zombie memory isolation
  - target: `src/executor/runner.zig:executeInner` or memory init path
  - input: Two zombies (zombie_id=A, zombie_id=B) running concurrently
  - expected: Zombie A's `memory_store("user_pref", "dark mode")` is NOT visible to zombie B's `memory_recall("user_pref")`. Isolation via key prefix (`zmb:{zombie_id}:`) or schema separation.
  - test_type: integration

- 2.3 PENDING — Workspace-scoped vs zombie-scoped memory
  - target: memory namespace design
  - input: NullClaw memory has `session_id` scoping (null = global, explicit = session). Map zombie_id to global scope, individual run_id to session scope.
  - expected: `memory_store` with `category: "core"` persists across runs (zombie-scoped). `memory_store` with `category: "conversation"` scoped to current run only.
  - test_type: integration

---

## 3.0 Cross-Run Persistence Verification

**Status:** PENDING

Prove that memory survives the workspace lifecycle: store in run N, destroy workspace, create new workspace for run N+1, recall succeeds.

**Dimensions (test blueprints):**

- 3.1 PENDING — Store-destroy-recall integration test
  - target: integration test harness
  - input: (1) Start zombie run, call `memory_store("learned_fact", "user prefers terse responses")`, (2) complete run, workspace destroyed, (3) start new run for same zombie_id, call `memory_recall("user prefers")`
  - expected: Recall returns the stored fact with content match. Zero data loss.
  - test_type: integration

- 3.2 PENDING — Crash recovery test
  - target: integration test harness
  - input: (1) Start zombie run, store memory, (2) kill executor process (SIGKILL — no graceful shutdown), (3) restart zombie, recall memory
  - expected: All committed memory_store calls before crash are recoverable. In-flight stores may be lost (acceptable — document the guarantee).
  - test_type: integration

- 3.3 PENDING — Memory size and retention bounds
  - target: configuration + enforcement
  - input: Zombie stores 10,000 facts over weeks of operation
  - expected: Memory has a configurable retention policy (max entries, max age, or max bytes). Old `daily` category entries are auto-pruned. `core` category entries are never pruned unless explicitly forgotten. Document the defaults.
  - test_type: unit

---

## 4.0 Interfaces

**Status:** PENDING

### 4.1 Executor Config Extension

```zig
/// Memory backend configuration passed per-zombie.
pub const MemoryBackendConfig = struct {
    /// Backend type: "postgres", "redis", "dragonfly", "sqlite_volume"
    backend: []const u8,
    /// Connection string or file path (depends on backend)
    connection: []const u8,
    /// Namespace prefix for zombie isolation (e.g., "zmb:{zombie_id}")
    namespace: []const u8,
    /// Max entries before pruning (0 = unlimited)
    max_entries: u32 = 0,
    /// Max age in hours for "daily" category (0 = no auto-prune)
    daily_retention_hours: u32 = 72,
};
```

### 4.2 Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| backend | string | one of: postgres, redis, dragonfly, sqlite_volume | "postgres" |
| connection | string | valid URI or file path | "postgresql://..." |
| namespace | string | non-empty, alphanumeric + colon + hyphen | "zmb:zom_01JQ..." |
| max_entries | u32 | >= 0 | 5000 |
| daily_retention_hours | u32 | >= 0 | 72 |

### 4.3 Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| memory initialized | log line | executor startup | `memory.backend_ready backend=postgres namespace=zmb:zom_01JQ...` |
| memory unavailable | log warning | connection failure | `memory.backend_unavailable err=ConnectionRefused — falling back to ephemeral` |

### 4.4 Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Backend unreachable at startup | Log warning, fall back to ephemeral SQLite (current behavior) | Agent runs with no persistent memory; activity log records the degradation |
| Backend unreachable mid-run | Memory ops return error to agent; agent continues without memory | Agent sees tool error, can retry or skip |
| Namespace collision | Prevented by zombie_id uniqueness (ULIDs) | N/A — cannot happen with correct namespace generation |
| Storage quota exceeded | Prune oldest `daily` entries; if still over, reject new stores | Agent sees "memory full" tool error |

---

## 5.0 Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Backend down at zombie start | Postgres/Redis unreachable | Fall back to ephemeral workspace SQLite, log degradation to activity stream | `zombiectl logs` shows "memory degraded — ephemeral only" |
| Backend down mid-conversation | Network partition | memory_store/recall return error to agent | Agent may mention "I can't access my memory right now" |
| Slow backend (> 500ms) | Overloaded Postgres/Redis | Agent conversation latency increases | Slower responses; activity log shows memory latency |
| Data corruption | Disk failure, bad migration | Memory ops fail consistently | Zombie restarts with empty memory (same as today — no worse) |

**Platform constraints:**
- SQLite WAL mode does not work on network filesystems (NFS, 9p, CIFS). NullClaw falls back to DELETE journal mode automatically (`sqlite.zig:37-48`). If Fly Volumes use 9p mounts, this applies.
- Postgres `pgvector` extension is required for hybrid recall (`postgres_hybrid` profile). Verify availability on Fly Postgres before selecting this profile.

---

## 6.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| No new Zig files > 400 lines | `wc -l` on all new/modified files |
| Memory config is optional — omitting it preserves current ephemeral behavior | Unit test: null memory config → ephemeral SQLite (no regression) |
| Cross-compiles on x86_64-linux, aarch64-linux | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| Zero impact on zombies that don't use memory tools | Benchmark: zombie without memory tools shows no latency change |
| Backend selection does not add compile-time dependencies | NullClaw already vendors all backends; no new build.zig.zon entries |

---

## 7.0 Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| memory_config_default_ephemeral | 2.1 | types.zig | null config | falls back to workspace SQLite |
| memory_config_validates_backend | 2.1 | types.zig | backend="invalid" | returns validation error |
| namespace_format | 2.2 | runner.zig | zombie_id="zom_01JQ..." | namespace="zmb:zom_01JQ..." |
| retention_prune_daily | 3.3 | memory config | 100 daily entries older than 72h | pruned to 0 |

### Integration Tests

| Test name | Dimension | Infra needed | Input | Expected |
|-----------|-----------|-------------|-------|----------|
| store_destroy_recall | 3.1 | selected backend | store → destroy workspace → recall | fact returned |
| crash_recovery | 3.2 | selected backend | store → SIGKILL → restart → recall | committed facts returned |
| zombie_isolation | 2.2 | selected backend | zombie A stores, zombie B recalls same key | zombie B gets nothing |

### Spec-Claim Tracing

| Spec claim | Test that proves it | Test type |
|-----------|-------------------|-----------|
| Memory survives workspace destruction | store_destroy_recall | integration |
| Memory survives process crash | crash_recovery | integration |
| Zombies cannot read each other's memory | zombie_isolation | integration |
| Missing config degrades gracefully | memory_config_default_ephemeral | unit |

---

## 8.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Complete storage backend evaluation (§1.0) — research, benchmark, score | Decision matrix document with recommendation |
| 2 | Design memory namespace schema for selected backend | Document reviewed |
| 3 | Extend executor config with MemoryBackendConfig | `zig build test` passes, null config = no regression |
| 4 | Wire NullClaw MemoryRuntime initialization in runner.zig | Executor starts with memory backend configured |
| 5 | Implement per-zombie namespace isolation | zombie_isolation integration test passes |
| 6 | Implement retention/pruning for daily category | retention_prune_daily unit test passes |
| 7 | Write store_destroy_recall and crash_recovery integration tests | All integration tests pass |
| 8 | Cross-compile check | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |

---

## 9.0 Acceptance Criteria

**Status:** PENDING

- [ ] Storage backend evaluation complete with scored decision matrix — verify: document exists in spec
- [ ] `memory_store` in run N → `memory_recall` in run N+1 returns stored fact — verify: `make test-integration` (store_destroy_recall)
- [ ] Process crash (SIGKILL) does not lose committed memory — verify: `make test-integration` (crash_recovery)
- [ ] Two concurrent zombies cannot read each other's memory — verify: `make test-integration` (zombie_isolation)
- [ ] Omitting memory config preserves current ephemeral behavior — verify: `make test` (memory_config_default_ephemeral)
- [ ] No new file exceeds 400 lines — verify: `wc -l`
- [ ] Cross-compile passes — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`

---

## 10.0 Applicable Rules

RULE XCC (cross-compile check), RULE FLL (full lint gate), RULE ORP (cross-layer orphan sweep), RULE DRN (drain before deinit), RULE 350L (line-length gate).

---

## 10.1 Invariants

N/A — no compile-time guardrails.

---

## 10.2 Eval Commands

```bash
# E1: Build
zig build 2>&1 | head -5; echo "build=$?"

# E2: Tests
make test 2>&1 | tail -5; echo "test=$?"

# E3: Lint
make lint 2>&1 | grep -E "✓|FAIL"

# E4: Gitleaks
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E5: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E6: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "xc_x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "xc_arm=$?"

# E7: Memory leak check
make check-pg-drain 2>&1 | tail -3; echo "drain=$?"
```

---

## 10.3 Dead Code Sweep

N/A — no files deleted.

---

## 10.4 Verification Evidence

**Status:** PENDING

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | | |
| Lint | `make lint` | | |
| 400L gate | `wc -l` | | |
| Backend evaluation | review §1.0 document | | |

---

## 11.0 Out of Scope

- Vector/semantic search optimization (use whatever NullClaw's selected profile provides out of the box)
- Cross-zombie shared memory (knowledge sharing between zombies in a workspace) — future milestone
- Memory migration tooling (export/import between backends) — not needed until multi-backend is real
- Memory UI in dashboard — separate milestone (M12 or later)
- Workspace-level shared facts (all zombies in a workspace see common context) — future milestone
