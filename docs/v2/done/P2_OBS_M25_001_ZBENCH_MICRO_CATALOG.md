# M25_001: zBench Micro-Benchmark Catalog — Tier-1 Code-Level Perf Gates

**Prototype:** v2
**Milestone:** M25
**Workstream:** 001
**Date:** Apr 16, 2026
**Status:** DONE
**Priority:** P2 — Operator/maintainer tooling; regression detection on hot paths
**Batch:** B7+ — follows M24_001 (bench tooling migration to hey + dummy zbench stub)
**Branch:** feat/m25-zbench-micro-catalog
**Depends on:** M24_001 (bench tooling in place)

---

## Overview

**Goal (testable):** Replace the no-op placeholder in `src/zbench_micro.zig` with a catalog of real micro-benchmarks covering the hot paths of zombied. `make bench` Tier-1 reports timings for each; CI stores the artifact and tracks drift over time. (Entry files moved from `src/tools/` to `src/` in this workstream — Zig's module-root confines `@import` within the entry file's directory, and the bench needs to reach across `src/http`, `src/errors`, `src/zombie`, `src/types`.)

**Problem:** M24_001 decoupled `make bench` into two tiers:

- **Tier-1** — zbench code micro-benchmarks (`src/zbench_micro.zig`) — before this workstream, one dummy `noop` function.
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
- For each iteration, sweep all 12 paths calling `match(path)`.
- Report: ns per full sweep (div by 12 for per-match).
- Gate: p99 < 2 µs per match (measured baseline ~59 ns/match on macOS M-series, ReleaseFast).

**Spec dimension:** `1.1 DONE — target zbench_micro.zig::benchRouteMatch — input: 12 paths covering every Route arm — observed p99 ~709 ns/sweep (~59 ns/match)`

### 1.2 `error_registry_lookup` — error code → entry

**Hot path:** every `hx.fail(ec.ERR_xxx, msg)` goes through `error_registry.lookup(code)`. Post-M16_001, this is a `StaticStringMap` so lookup is O(1) hash. Benchmark confirms the hash stays O(1) and no regression re-introduces the linear scan.

**Benchmark shape:**
- Input: slice of 19 error codes (mix of real ones + 2 unknowns for `UNKNOWN` path).
- Per iteration: sweep all codes through `lookup`.
- Report: ns per sweep (div by 19 for per-lookup).
- Gate: p99 < 100 ns per lookup (measured baseline ~64 ns/lookup on macOS M-series, ReleaseFast).

**Spec dimension:** `1.2 DONE — target zbench_micro.zig::benchErrorRegistryLookup — observed p99 ~1.21 µs/sweep (~64 ns/lookup)`

### 1.3 `activity_cursor_roundtrip` — cursor encode/decode

**Hot path:** `zombie activity` endpoint uses keyset-cursor pagination (RULE KYS). Every `GET /v1/workspaces/{ws}/zombies/{id}/activity?cursor=X` decodes + validates the cursor. Composite key: `{created_at_ms}:{uuid}` plaintext (no base64). Regression surface: allocator churn in the encode path, parse bugs, or accidental substring scanning. Parse/format factored into `src/zombie/activity_cursor.zig` so the bench exercises the same code the request path does.

**Benchmark shape:**
- Input: 100 synthesized cursors covering a range of timestamps and uuid suffixes.
- Per iteration: sweep all 100: `parse` → `format` → `free`.
- Report: ns per sweep (div by 100 for per-roundtrip).
- Gate: p99 < 50 µs per roundtrip. Round-trip dominated by GPA alloc/free churn, not the parse itself — an arena-backed caller would see sub-µs. Gate set with 2-3× margin over measured baseline.

**Spec dimension:** `1.3 DONE — target zbench_micro.zig::benchActivityCursorRoundtrip — observed p99 ~1.997 ms/sweep (~20 µs/roundtrip)`

### 1.4 `json_encode_response` — hx.ok response shape

**Hot path:** every successful response goes through `Hx.ok(status, body)` which serializes the body struct to JSON. Uses `std.json.Stringify`. Regression surface: comptime reflection changes, anonymous struct growth, or accidental heap allocation per field.

**Benchmark shape:**
- Input: a fixture struct roughly the size of `GET /v1/workspaces/{ws}/zombies` response (10 zombies × 6 fields each).
- Per iteration: stringify to a caller-owned buffer via `std.json.Stringify.valueAlloc`.
- Report: ns per encode.
- Gate: p99 < 100 µs for the 10-zombie fixture (measured baseline ~57.5 µs on macOS M-series, ReleaseFast — GPA-dominated; 2× margin).

**Spec dimension:** `1.4 DONE — target zbench_micro.zig::benchJsonEncodeResponse — observed p99 ~57.5 µs per encode`

### 1.5 `pg_query_wrapper_overhead` — PgQuery.from(...) + deinit

**Status:** DEFERRED (moved to §6 Out of Scope).

**Why deferred:** `PgQuery` is a 23-line, single-pointer delegation wrapper (`src/db/pg_query.zig`). `from()` is a struct literal; `next`/`drain`/`deinit` forward directly to `pg.Result`. Overhead is provably zero by inspection — there is nothing for the bench to measure that isn't the underlying `pg.Result` implementation. Faithfully mocking `*pg.Result` (opaque external struct from `karlseguin/pg.zig`) would require either vendoring a test-only mock that duplicates its interface (large scope, drift risk) or running against a real DB (violates §3 constraint "No network, no DB, no filesystem"). Added tests in `src/db/pg_query_test.zig` already pin the API surface; a size-assertion `comptime { std.debug.assert(@sizeOf(PgQuery) == 8); }` in `pg_query.zig` pins the wrapper overhead structurally.

### 1.6 `uuid_v7_generate` — ID minting

**Hot path:** every new zombie, workspace, activity event, session mints a UUIDv7 via `id_format.generateWorkspaceId` (and family). Uses `std.crypto.random.bytes` + `std.fmt.bytesToHex`. Regression surface: allocator swap, formatter change, or RNG swap.

**Benchmark shape:**
- Per iteration: generate one UUIDv7 string, free.
- Report: ns per mint.
- Gate: p99 < 50 µs per generation (measured baseline ~32.7 µs on macOS M-series, ReleaseFast). Dominated by the `getrandom`/`CCRandomGenerateBytes` syscall — earlier spec target of 2 µs was not achievable with `std.crypto.random` on any OS that round-trips to kernel randomness. Gate set to catch regressions such as an accidental switch to `std.fmt.allocPrint` inside a hot loop or double-allocation in the formatter.

**Spec dimension:** `1.6 DONE — target zbench_micro.zig::benchUuidV7Generate — observed p99 ~32.7 µs per mint`

### 1.7 `webhook_signature_verify` — HMAC verification

**Hot path:** every incoming webhook (`/v1/webhooks/{zombie_id}`, Slack, grant approval) verifies an HMAC-SHA256 signature before the handler runs. Regression surface: constant-time comparison discipline (RULE CTM / CTE), allocator thrash on the digest, or a rewrite that breaks vectorization.

**Benchmark shape:**
- Input: 1 KB payload + fixture key + precomputed GITHUB-style signature (comptime-computed in `zbench_fixtures.zig`).
- Per iteration: full verify (hex-decode, HMAC-SHA256 compute, constant-time compare).
- Report: ns per verify.
- Gate: p99 < 10 µs per verify (measured baseline ~1.04 µs on macOS M-series, ReleaseFast).

**Spec dimension:** `1.7 DONE — target zbench_micro.zig::benchWebhookSignatureVerify — observed p99 ~1.04 µs per verify`

---

## 2.0 Surface Area Checklist

- [x] **OpenAPI** — no. Tier-1 benchmarks don't touch HTTP surface.
- [x] **zombiectl** — no.
- [x] **User docs** — no. Internal tool.
- [x] **Release notes** — yes, patch bump. Updated in `/Users/kishore/Projects/docs/changelog.mdx`.
- [x] **Schema** — no.
- [x] **Schema teardown guard** — N/A.
- [x] **Spec-vs-rules conflict** — none. Catalog is tooling; the `src/tools/` directory is empty after this workstream so bench entry files moved to `src/zbench_micro.zig` + `src/zbench_fixtures.zig` (module-root requirement: `root_source_file` directory bounds the @import tree; the bench needs to import across `src/http`, `src/errors`, `src/zombie`, `src/types`).

---

## 3.0 Implementation Constraints

| Constraint | Verify | Status |
|---|---|---|
| Each benchmark fn ≤ 30 lines | `wc -l` of `bench*` fns | DONE (longest 8 lines) |
| `zbench_micro.zig` stays ≤ 350 lines (RULE FLL) | `wc -l src/zbench_micro.zig` | DONE (103 lines) |
| Fixtures live in `src/zbench_fixtures.zig`, not inline | code review | DONE |
| Each benchmark has a spec-declared gate (p99 or time/run target) | spec §1 entries | DONE |
| `make bench` Tier-1 runtime ≤ 30s on macOS dev machine | time-boxed | DONE (~11 s wall incl. build) |
| No network, no DB, no filesystem — pure CPU/memory | code review | DONE |

---

## 4.0 Execution Plan

1. Land the first two benchmarks (1.1 route_match, 1.2 error_registry_lookup) — prove the pattern. — DONE
2. Add fixtures file `src/zbench_fixtures.zig` with inputs shared across benchmarks. — DONE
3. Land the remaining four in 1.3, 1.4, 1.6, 1.7 order; each with a spec-declared gate. (1.5 deferred — see §1.5.) — DONE
4. Follow-up: CI artifact preservation — upload zbench stdout to the workflow run for PR-over-PR trend.
5. Follow-up: gate CI on gate failure (today gates print but the job doesn't fail on p99 exceeded — requires capturing zbench output and parsing).

---

## 5.0 Acceptance Criteria

- [x] Benchmarks 1.1, 1.2, 1.3, 1.4, 1.6, 1.7 registered in `src/zbench_micro.zig` (1.5 deferred — see §1.5).
- [x] `make bench` Tier-1 completes in ≤ 30 s (~11 s measured including ReleaseFast build).
- [x] Each benchmark has a documented gate (spec §1 entries show target vs. observed).
- [x] Fixtures are in a dedicated file (`src/zbench_fixtures.zig`), not inline.
- [x] Release notes updated with the new bench coverage (patch bump to 0.9.1 in `/Users/kishore/Projects/docs/changelog.mdx`).
- [x] No unit tests regress — `make test` passes. Cursor parse/format was extracted into `src/zombie/activity_cursor.zig` and covered by new unit tests; callers in `src/zombie/activity_stream.zig` refactored to use it.

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
# Tier-1 only (ReleaseFast — mirrors what `make bench` runs):
zig build -Dwith-bench-tools=true -Doptimize=ReleaseFast bench-micro

# Full bench (Tier-1 + Tier-2):
make bench

# One-off: run a specific benchmark via zbench's filter (once implemented):
# zig build -Dwith-bench-tools=true -Doptimize=ReleaseFast bench-micro -- --test-filter "route_match"
```
