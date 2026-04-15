# M25_001: zBench Micro-Benchmark Catalog — Tier-1 Code-Level Perf Gates

**Prototype:** v2
**Milestone:** M25
**Workstream:** 001
**Date:** Apr 16, 2026
**Status:** PENDING
**Priority:** P2 — Operator/maintainer tooling; regression detection on hot paths
**Batch:** B7+ — follows M24_001 (bench tooling migration to hey + dummy zbench stub)
**Branch:** feat/m25-zbench-micro-catalog
**Depends on:** M24_001 (bench tooling in place)

---

## Overview

**Goal (testable):** Replace the no-op placeholder in `src/tools/zbench_micro.zig` with a catalog of real micro-benchmarks covering the hot paths of zombied. `make bench` Tier-1 reports timings for each; CI stores the artifact and tracks drift over time.

**Problem:** M24_001 decoupled `make bench` into two tiers:

- **Tier-1** — zbench code micro-benchmarks (`src/tools/zbench_micro.zig`) — currently one dummy `noop` function.
- **Tier-2** — hey HTTP loadgen — covers end-to-end request throughput.

Tier-2 catches system-level regressions. Tier-1 is the tool that catches *code-level* regressions — a 3× slowdown in JSON encoding, a 10× regression in route matching, a quadratic blowup in cursor parsing. Without Tier-1 populated, we only see the composite number and miss the where.

**Solution summary:** Identify the hot paths in zombied (informed by real workload: webhook receive → zombie execute → activity write → LLM token billing → response). For each, write a zbench benchmark in `zbench_micro.zig`. Start with the top 5-7 most load-bearing; future additions happen as contributors reach for them when touching the code.

---

## 1.0 Catalog — Benchmarks To Write

**Status:** PENDING

Each entry below is a target micro-benchmark. Each should live as a `bench_<name>` function in `zbench_micro.zig` registered via `bench.add(...)`.

### 1.1 `route_match` — router dispatch

**Hot path:** every HTTP request hits `src/http/router.zig::match(path)`. This is a chain of `std.mem.eql` / prefix / suffix checks. With ~40 routes, worst case is ~40 string comparisons. Regression surface: re-adding removed flat routes, bad ordering that makes longer paths fall through, or introducing a loop where a string match would suffice.

**Benchmark shape:**
- Input: slice of representative paths (healthz, workspace-scoped zombie CRUD, webhooks, memory, billing, activity).
- For each iteration, call `match(path)` over the fixture set.
- Report: ns per `match` call.
- Gate: p99 < 2 µs per match.

**Spec dimension:** `1.1 PENDING — target zbench_micro.zig::bench_route_match — input: 12 paths covering every Route arm — expected: p99 < 2 µs`

### 1.2 `error_registry_lookup` — error code → entry

**Hot path:** every `hx.fail(ec.ERR_xxx, msg)` goes through `error_registry.lookup(code)`. Post-M16_001, this is a `StaticStringMap` so lookup is O(1) hash. Benchmark confirms the hash stays O(1) and no regression re-introduces the linear scan.

**Benchmark shape:**
- Input: slice of 30 error codes (mix of real ones + 2 unknowns for `UNKNOWN` path).
- Per iteration: look up each code.
- Report: ns per lookup.
- Gate: p99 < 100 ns per lookup.

### 1.3 `activity_cursor_roundtrip` — cursor encode/decode

**Hot path:** `zombie activity` endpoint uses keyset-cursor pagination (RULE KYS). Every `GET /v1/workspaces/{ws}/zombies/{id}/activity?cursor=X` decodes + validates the cursor. Composite key: `(created_at_ms, id::uuid)` base64-encoded. Regression surface: allocator churn in the decode path, URL-decode bugs, or accidental substring parsing.

**Benchmark shape:**
- Input: 100 fixture cursors from real activity data (or synthesized).
- Per iteration: decode → validate → re-encode each.
- Report: ns per round-trip.
- Gate: p99 < 1 µs per round-trip.

### 1.4 `json_encode_response` — hx.ok response shape

**Hot path:** every successful response goes through `Hx.ok(status, body)` which serializes the body struct to JSON. Uses `std.json.Stringify`. Regression surface: comptime reflection changes, anonymous struct growth, or accidental heap allocation per field.

**Benchmark shape:**
- Input: a fixture struct roughly the size of `GET /v1/workspaces/{ws}/zombies` response (10 zombies × 6 fields each).
- Per iteration: stringify to a caller-owned buffer.
- Report: ns per encode.
- Gate: p99 < 50 µs for the 10-zombie fixture.

### 1.5 `pg_query_wrapper_overhead` — PgQuery.from(...) + deinit

**Hot path:** `PgQuery.from(conn.query(...))` wraps every pg query result. Introduced in M10_004. Overhead should be ~0 vs raw pg.Query. Benchmark pins that claim. Uses a mocked connection (no real DB).

**Benchmark shape:**
- Input: a pre-built pg.Query-like mock that yields 100 rows.
- Per iteration: `var q = PgQuery.from(mock); defer q.deinit();` then iterate.
- Report: ns per 100-row iteration.
- Gate: p99 within 5% of raw mock iteration.

### 1.6 `uuid_v7_generate` — ID minting

**Hot path:** every new zombie, workspace, activity event, session mints a UUIDv7 via `id_format.generateWorkspaceId` (and family). Uses `std.crypto.random.bytes` + `std.fmt.bytesToHex`. Regression surface: allocator swap, formatter change, or RNG swap.

**Benchmark shape:**
- Per iteration: generate one UUIDv7 string.
- Report: ns per mint.
- Gate: p99 < 2 µs per generation.

### 1.7 `webhook_signature_verify` — HMAC verification

**Hot path:** every incoming webhook (`/v1/webhooks/{zombie_id}`, Slack, grant approval) verifies an HMAC-SHA256 signature before the handler runs. Regression surface: constant-time comparison discipline (RULE CTM / CTE), allocator thrash on the digest, or a rewrite that breaks vectorization.

**Benchmark shape:**
- Input: 1 KB payload + fixture key + fixture signature.
- Per iteration: full verify (compute + constant-time compare).
- Report: ns per verify.
- Gate: p99 < 10 µs per verify.

---

## 2.0 Surface Area Checklist

- [ ] **OpenAPI** — no. Tier-1 benchmarks don't touch HTTP surface.
- [ ] **zombiectl** — no.
- [ ] **User docs** — no. Internal tool.
- [ ] **Release notes** — yes, when the catalog lands. Patch bump.
- [ ] **Schema** — no.
- [ ] **Schema teardown guard** — N/A.
- [ ] **Spec-vs-rules conflict** — none.

---

## 3.0 Implementation Constraints

| Constraint | Verify |
|---|---|
| Each benchmark fn ≤ 30 lines | grep + review |
| `zbench_micro.zig` stays ≤ 350 lines (RULE FLL); split into `bench/` subdir if it grows past that | `wc -l src/tools/zbench_micro.zig` |
| Fixtures live in `src/tools/zbench_fixtures.zig`, not inline | code review |
| Each benchmark has a spec-declared gate (p99 or time/run target) | spec §1 entries |
| `make bench` total runtime ≤ 30s on macOS dev machine | time-boxed |
| No network, no DB, no filesystem — pure CPU/memory | code review |

---

## 4.0 Execution Plan

1. Land the first two benchmarks (1.1 route_match, 1.2 error_registry_lookup) — prove the pattern.
2. Add fixtures file `src/tools/zbench_fixtures.zig` with inputs shared across benchmarks.
3. Land the remaining five in 1.3-1.7 order; each with a spec-declared gate.
4. Add CI artifact preservation — upload the stdout stats to the workflow run so trend can be eyeballed PR-over-PR.
5. Follow-up: gate CI on gate failure (today gates print but the job doesn't fail on p99 exceeded — that requires capturing zbench output and parsing).

---

## 5.0 Acceptance Criteria

- [ ] All 7 benchmarks from §1.1-1.7 registered in `zbench_micro.zig`
- [ ] `make bench` Tier-1 completes in ≤ 30s
- [ ] Each benchmark has a documented gate (spec §1)
- [ ] Fixtures are in a dedicated file, not inline
- [ ] Release notes updated with the new bench coverage
- [ ] No unit tests regress (catalog is tooling, not production code)

---

## 6.0 Out of Scope

- **Upstream zbench overflow fix** — the `nvar += sd * sd` u64 overflow in zbench's `Statistics` exists on every branch (zig-0.15.1, zig-0.16.0, main). Not triggered by code micro-benchmarks today (noop has tight variance; real benchmarks of the items above are nanosecond-scale, too). File an upstream PR separately; don't block this catalog on it.
- **Alternate loadgen tools** — wrk2, vegeta. Tier-2 is committed to `hey`; revisit only if `hey` proves inadequate.
- **RSS growth gate** — M24_001's original `api_bench_runner.zig` tracked RSS. hey doesn't. Reintroducing RSS tracking is a separate concern; use `make memleak` for leak detection today.
- **Per-route latency gates** — the existing single p95 gate on `/healthz` is a system smoke test, not a per-route SLO. SLO enforcement lives in the Grafana dashboards (production), not the bench gate.

---

## 7.0 Applicable Rules

- RULE FLL — 350-line file gate (per-file and per-function)
- RULE NDC — no dead code (dead benchmarks removed when their hot path goes cold)

## 8.0 Eval Commands

```bash
# Tier-1 only (quick):
zig build -Dwith-bench-tools=true bench-micro

# Full bench (Tier-1 + Tier-2):
make bench

# One-off: run a specific benchmark via zbench's filter (once implemented):
# zig build -Dwith-bench-tools=true bench-micro -- --test-filter "route_match"
```
