# M18_001: Zombie Execution Telemetry — Per-Delivery Metrics Store and Dual API

**Prototype:** v0.11.0
**Milestone:** M18
**Workstream:** 001
**Date:** Apr 12, 2026
**Status:** PENDING
**Priority:** P1 — UseZombie blind to per-execution latency and token costs; customers cannot self-serve their own usage data
**Batch:** B1
**Branch:** feat/m18-zombie-execution-telemetry
**Depends on:** M15_001 (credit metering path — `recordZombieDelivery` call site), M15_002 (PENDING — `ZombieEventProps` struct extended here in parallel)

---

## Overview

**Goal (testable):** After each zombie event delivery, `zombie_execution_telemetry` receives
one row carrying `token_count`, `time_to_first_token_ms`, `epoch_wall_time_ms`, and
`wall_seconds`; `GET /v1/workspaces/{ws}/zombies/{id}/telemetry` returns the last N rows
for that zombie; `GET /internal/v1/telemetry` returns rows filterable by workspace and zombie
across all tenants.

**Problem (observable):**
- `src/executor/client.zig:StageResult` carries `token_count` and `wall_seconds` but not
  `time_to_first_token_ms` (TTFT). There is no epoch timestamp for when a delivery began.
- `src/zombie/metering.zig:ExecutionUsage` has `token_count` as a reserved field with a
  comment "reserved for M15_002" — it is never read or stored.
- There is no `zombie_execution_telemetry` table. The only per-delivery record is the
  `CREDIT_DEDUCTED` audit row in `workspace_credit_audit`, which carries billing fields only.
- Customers cannot query their own execution latency or token spend history beyond the
  aggregate credit balance. UseZombie has no cross-workspace query path for these fields.

**Solution summary:** Three-layer change. (1) Extend `StageResult` and `EventResult` with
`time_to_first_token_ms: u64` (read from executor JSON response) and capture
`epoch_wall_time_ms: i64` at the start of `deliverEvent()`. (2) Add schema table
`zombie_execution_telemetry` (one row per delivery, keyed on `event_id`) and write to it
from `recordZombieDelivery` in `metering.zig` via a new store function. (3) Add two HTTP
handlers: one workspace-scoped customer endpoint and one internal operator endpoint. No
changes to the credit audit path or billing logic.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/executor/client.zig` | MODIFY | Add `time_to_first_token_ms: u64 = 0` to `StageResult`; read from executor JSON |
| `src/zombie/event_loop_types.zig` | MODIFY | Add `time_to_first_token_ms: u64` and `epoch_wall_time_ms: i64` to `EventResult` |
| `src/zombie/event_loop.zig` | MODIFY | Capture `epoch_wall_time_ms` before `executeInSandbox`; pass new fields to `recordZombieDelivery` |
| `src/zombie/metering.zig` | MODIFY | Add `time_to_first_token_ms` and `epoch_wall_time_ms` to `ExecutionUsage`; update `recordZombieDelivery` signature; call new store write |
| `schema/NNN_zombie_execution_telemetry.sql` | CREATE | New table definition |
| `schema/embed.zig` | MODIFY | Add `@embedFile` constant for new SQL |
| `src/cmd/common.zig` | MODIFY | Add migration entry for new table |
| `src/state/zombie_telemetry_store.zig` | CREATE | `insertTelemetry`, `listTelemetryForZombie`, `listTelemetryAll` |
| `src/http/handlers/zombie_telemetry.zig` | CREATE | Customer and operator HTTP handlers |
| `src/http/router.zig` | MODIFY | Register new routes |
| `src/main.zig` | MODIFY | Add test import for new files |

## Applicable Rules

- RULE XCC — cross-compile before commit (always for Zig)
- RULE FLL — 350-line gate on every touched .zig file
- RULE FLS — drain all pg results (new store queries must use PgQuery)
- RULE ORP — cross-layer orphan sweep (new symbols added to metering.zig; verify no stale callers)
- Schema Table Removal Guard — fires on schema/NNN_zombie_execution_telemetry.sql creation (pre-v2.0 era)

---

## §1 — Executor and Event Loop Field Extensions

**Status:** PENDING

Extend the data pipeline from executor → event loop → metering with two new fields:
`time_to_first_token_ms` (reported by executor) and `epoch_wall_time_ms` (captured
locally before `executeInSandbox`). Neither field affects billing logic.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `src/executor/client.zig:StageResult` | N/A — struct field | `time_to_first_token_ms: u64 = 0` compiles; `getUsage` and `startStage` JSON paths read it via `json.getIntOrZero(result, "time_to_first_token_ms")` | unit (comptime + JSON mock) |
| 1.2 | PENDING | `src/zombie/event_loop_types.zig:EventResult` | N/A — struct extension | `time_to_first_token_ms: u64` and `epoch_wall_time_ms: i64` fields present; `@sizeOf` assertion updated | unit (comptime) |
| 1.3 | PENDING | `src/zombie/event_loop.zig:deliverEvent` | successful delivery, executor returns `time_to_first_token_ms=1200` | `EventResult.time_to_first_token_ms == 1200`; `EventResult.epoch_wall_time_ms > 0` (captured from `std.time.milliTimestamp()`) | unit (mock executor) |
| 1.4 | PENDING | `src/zombie/metering.zig:ExecutionUsage` | N/A — struct extension | `time_to_first_token_ms: u64` and `epoch_wall_time_ms: i64` fields; `recordZombieDelivery` accepts both; `deductZombieUsage` signature unchanged | unit (comptime) |

---

## §2 — Schema: `zombie_execution_telemetry` Table

**Status:** PENDING

New table persisting one row per zombie event delivery. Keyed on `event_id` (unique
constraint — idempotent on replay matching M15_001's dedup contract). All columns
non-nullable with sane defaults so schema forward-compat is not broken if executor
does not report TTFT yet (defaults to 0).

Schema (≤100 lines, single-concern per PLAN checklist rule):

```sql
CREATE TABLE zombie_execution_telemetry (
    id                       TEXT        NOT NULL PRIMARY KEY,
    zombie_id                TEXT        NOT NULL,
    workspace_id             TEXT        NOT NULL,
    event_id                 TEXT        NOT NULL,
    token_count              BIGINT      NOT NULL DEFAULT 0,
    time_to_first_token_ms   BIGINT      NOT NULL DEFAULT 0,
    epoch_wall_time_ms       BIGINT      NOT NULL DEFAULT 0,
    wall_seconds             BIGINT      NOT NULL DEFAULT 0,
    plan_tier                TEXT        NOT NULL DEFAULT 'free',
    credit_deducted_cents    BIGINT      NOT NULL DEFAULT 0,
    recorded_at              BIGINT      NOT NULL,
    CONSTRAINT uq_telemetry_event_id UNIQUE (event_id)
);
CREATE INDEX idx_telemetry_workspace_zombie ON zombie_execution_telemetry (workspace_id, zombie_id, recorded_at DESC);
CREATE INDEX idx_telemetry_workspace_time   ON zombie_execution_telemetry (workspace_id, recorded_at DESC);
```

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `schema/NNN_zombie_execution_telemetry.sql` | migration applied | table exists with all columns; UNIQUE on `event_id` enforced | integration (real DB) |
| 2.2 | PENDING | `src/state/zombie_telemetry_store.zig:insertTelemetry` | valid row | row inserted; second call with same `event_id` is a no-op (`ON CONFLICT DO NOTHING`) | integration (real DB) |
| 2.3 | PENDING | `src/state/zombie_telemetry_store.zig:insertTelemetry` | `time_to_first_token_ms=0` (executor did not report TTFT) | row inserted with `time_to_first_token_ms=0`; no error | integration (real DB) |
| 2.4 | PENDING | `src/zombie/metering.zig:recordZombieDelivery` | successful delivery with TTFT=800, epoch_wall_time_ms set | `zombie_execution_telemetry` has one row with matching fields after call | integration (real DB) |

---

## §3 — Customer Telemetry API

**Status:** PENDING

`GET /v1/workspaces/{ws}/zombies/{zombie_id}/telemetry?limit=50&cursor=`

Returns the last N deliveries for a single zombie in the caller's workspace.
Cursor-based pagination (opaque base64 cursor encoding `recorded_at + id`, same
pattern as `activity_stream.zig`). Auth: workspace JWT — tenancy enforced by
`workspace_id` filter in query. Response is JSON.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `src/http/handlers/zombie_telemetry.zig:handleCustomerTelemetry` | `GET /v1/workspaces/WS1/zombies/Z1/telemetry` with 5 seeded rows | JSON `{ "items": [...5 rows...], "cursor": null }`, each row has `token_count`, `time_to_first_token_ms`, `epoch_wall_time_ms`, `wall_seconds` | integration (DB) |
| 3.2 | PENDING | `src/http/handlers/zombie_telemetry.zig:handleCustomerTelemetry` | `limit=2`, 5 rows seeded | `items` has 2 rows; `cursor` non-null; second call with cursor returns next 2 | integration (DB) |
| 3.3 | PENDING | `src/http/handlers/zombie_telemetry.zig:handleCustomerTelemetry` | workspace_id in path != JWT workspace_id | HTTP 403, `UZ-WS-003` | unit |
| 3.4 | PENDING | `src/http/handlers/zombie_telemetry.zig:handleCustomerTelemetry` | `limit=0` or `limit=201` | HTTP 400, `UZ-TEL-001` — "limit must be 1–200" | unit |

---

## §4 — Operator Telemetry API

**Status:** PENDING

`GET /internal/v1/telemetry?workspace_id=&zombie_id=&after=&limit=100`

UseZombie-internal cross-workspace query endpoint. All params optional; `after` is
epoch ms. Auth: internal service token (same header pattern as existing `/internal/*`
routes). Returns rows newest-first. No cursor — internal consumers use `after` for
time-window queries.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | `src/http/handlers/zombie_telemetry.zig:handleOperatorTelemetry` | no filters, 10 rows across 3 workspaces | all 10 rows returned in `items`, newest first | integration (DB) |
| 4.2 | PENDING | `src/http/handlers/zombie_telemetry.zig:handleOperatorTelemetry` | `workspace_id=WS1` filter | only WS1 rows returned | integration (DB) |
| 4.3 | PENDING | `src/http/handlers/zombie_telemetry.zig:handleOperatorTelemetry` | `after=<epoch_ms>` | only rows where `recorded_at > after` returned | integration (DB) |
| 4.4 | PENDING | `src/http/handlers/zombie_telemetry.zig:handleOperatorTelemetry` | missing or invalid internal service token | HTTP 401 | unit |

---

## Interfaces

**Status:** PENDING

### Public Functions

```zig
// src/state/zombie_telemetry_store.zig

pub const TelemetryRow = struct {
    id:                    []const u8,  // owned
    zombie_id:             []const u8,  // owned
    workspace_id:          []const u8,  // owned
    event_id:              []const u8,  // owned
    token_count:           u64,
    time_to_first_token_ms: u64,
    epoch_wall_time_ms:    i64,
    wall_seconds:          u64,
    plan_tier:             []const u8,  // owned
    credit_deducted_cents: i64,
    recorded_at:           i64,

    pub fn deinit(self: *TelemetryRow, alloc: std.mem.Allocator) void;
};

pub const InsertTelemetryParams = struct {
    zombie_id:             []const u8,  // borrowed
    workspace_id:          []const u8,  // borrowed
    event_id:              []const u8,  // borrowed — idempotency key; ON CONFLICT DO NOTHING
    token_count:           u64,
    time_to_first_token_ms: u64,
    epoch_wall_time_ms:    i64,
    wall_seconds:          u64,
    plan_tier:             []const u8,  // borrowed
    credit_deducted_cents: i64,
    recorded_at:           i64,         // caller supplies std.time.milliTimestamp()
};

/// Non-fatal: on conflict (duplicate event_id) returns without error.
pub fn insertTelemetry(conn: *pg.Conn, alloc: std.mem.Allocator, params: InsertTelemetryParams) !void;

/// Customer query — workspace_id enforces tenancy. Returns newest-first.
/// cursor is opaque; pass null for first page.
pub fn listTelemetryForZombie(
    conn:         *pg.Conn,
    alloc:        std.mem.Allocator,
    workspace_id: []const u8,
    zombie_id:    []const u8,
    limit:        u32,
    cursor:       ?[]const u8,
) ![]TelemetryRow;

/// Operator query — cross-workspace. All params nullable.
pub fn listTelemetryAll(
    conn:         *pg.Conn,
    alloc:        std.mem.Allocator,
    workspace_id: ?[]const u8,
    zombie_id:    ?[]const u8,
    after_ms:     ?i64,
    limit:        u32,
) ![]TelemetryRow;
```

```zig
// src/zombie/metering.zig — updated ExecutionUsage (additions only)
pub const ExecutionUsage = struct {
    zombie_id:              []const u8,  // borrowed
    workspace_id:           []const u8,  // borrowed
    event_id:               []const u8,  // borrowed
    agent_seconds:          u64,
    token_count:            u64,
    time_to_first_token_ms: u64,         // NEW — 0 if executor did not report
    epoch_wall_time_ms:     i64,         // NEW — milliTimestamp() at delivery start
};
```

### API Endpoints

```
GET /v1/workspaces/{ws}/zombies/{zombie_id}/telemetry?limit={1-200}&cursor={opaque}
Authorization: Bearer <workspace-jwt>

GET /internal/v1/telemetry?workspace_id={ws}&zombie_id={id}&after={epoch_ms}&limit={1-500}
X-Internal-Token: <service-token>
```

### Output Contracts — Customer Response

```json
{
  "items": [
    {
      "event_id":              "019...",
      "zombie_id":             "zombie-abc",
      "token_count":           1420,
      "time_to_first_token_ms": 870,
      "epoch_wall_time_ms":    1744483200000,
      "wall_seconds":          14,
      "credit_deducted_cents": 14,
      "recorded_at":           1744483214000
    }
  ],
  "cursor": "base64opaque=="
}
```

### Error Contracts

| Error condition | Code | HTTP | Developer sees |
|----------------|------|------|---------------|
| JWT workspace ≠ path workspace | `UZ-WS-003` | 403 | "Workspace not found or access denied" |
| Invalid `limit` (< 1 or > 200) | `UZ-TEL-001` | 400 | "limit must be between 1 and 200" |
| Invalid `limit` for operator (> 500) | `UZ-TEL-002` | 400 | "limit must be between 1 and 500" |
| Invalid `cursor` encoding | `UZ-TEL-003` | 400 | "Invalid cursor format" |
| Missing or invalid internal token | `UZ-INT-001` | 401 | "Unauthorized" |

---

## Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| DB write fails in `recordZombieDelivery` | Postgres unavailable at telemetry write | `insertTelemetry` error logged; delivery continues (non-fatal, mirrors M15_001 pattern) | Delivery succeeds; telemetry row missing (gap in history) |
| Executor does not report TTFT | Older executor version; JSON field absent | `json.getIntOrZero` returns 0; `time_to_first_token_ms=0` stored | API returns `time_to_first_token_ms: 0` — distinguishable from real 0ms |
| Duplicate `event_id` on replay | XACK failure + redelivery (M15_001 scenario) | `ON CONFLICT DO NOTHING` on insert; no error | Original row retained; no double-entry |
| Cursor tampered or expired | Client sends malformed base64 | `UZ-TEL-003` HTTP 400 | Error message; retry from first page |

---

## Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| `zombie_execution_telemetry` schema file ≤ 100 lines, single-concern | `wc -l schema/NNN_zombie_execution_telemetry.sql` |
| `zombie_telemetry_store.zig` ≤ 350 lines | `wc -l src/state/zombie_telemetry_store.zig` |
| `zombie_telemetry.zig` (handler) ≤ 350 lines | `wc -l src/http/handlers/zombie_telemetry.zig` |
| No raw SQL strings in `metering.zig` — all queries via store | `grep -n "SELECT\|INSERT\|UPDATE" src/zombie/metering.zig` = 0 |
| `insertTelemetry` is non-fatal — never propagates error to `recordZombieDelivery` | code review + integration test 2.4 |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| pg-drain clean | `make check-pg-drain` |
| `deductZombieUsage` signature unchanged — telemetry is write-only side effect | `grep -n "deductZombieUsage" src/` — callers unchanged |

---

## Invariants (Hard Guardrails)

**Status:** PENDING

N/A — no compile-time guardrails for this workstream. Idempotency is enforced by
the DB `UNIQUE(event_id)` constraint, not a comptime assertion.

---

## Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `execution_usage_has_ttft_and_epoch_fields` | 1.4 | `metering.zig:ExecutionUsage` | N/A — comptime | fields exist; struct compiles |
| `event_result_has_new_fields` | 1.2 | `event_loop_types.zig:EventResult` | N/A — comptime | `time_to_first_token_ms` and `epoch_wall_time_ms` exist |
| `customer_endpoint_rejects_wrong_workspace` | 3.3 | `zombie_telemetry.zig:handleCustomerTelemetry` | JWT WS ≠ path WS | HTTP 403, `UZ-WS-003` |
| `customer_endpoint_rejects_invalid_limit` | 3.4 | `zombie_telemetry.zig:handleCustomerTelemetry` | `limit=0`, `limit=201` | HTTP 400, `UZ-TEL-001` |
| `operator_endpoint_rejects_missing_token` | 4.4 | `zombie_telemetry.zig:handleOperatorTelemetry` | no token | HTTP 401 |

### Integration Tests

| Test name | Dim | Infra needed | Input | Expected |
|-----------|-----|-------------|-------|----------|
| `schema_migration_creates_table` | 2.1 | DB | migration applied | table + indexes present |
| `insert_telemetry_idempotent` | 2.2 | DB | same `event_id` x2 | 1 row, no error |
| `insert_zero_ttft_allowed` | 2.3 | DB | `time_to_first_token_ms=0` | row inserted |
| `record_delivery_writes_telemetry` | 2.4 | DB | `recordZombieDelivery` called | matching row in `zombie_execution_telemetry` |
| `customer_api_returns_rows` | 3.1 | DB | 5 seeded rows for Z1 | 5 rows in response, correct fields |
| `customer_api_paginates` | 3.2 | DB | 5 rows, `limit=2` | page 1 has 2 rows + cursor; page 2 has next 2 |
| `operator_api_all_workspaces` | 4.1 | DB | 10 rows, 3 workspaces | all 10 returned unfiltered |
| `operator_api_workspace_filter` | 4.2 | DB | `workspace_id=WS1` | only WS1 rows |
| `operator_api_after_filter` | 4.3 | DB | `after=<epoch>` | only newer rows |

### Negative Tests

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|---------------|
| `customer_limit_zero` | 3.4 | `limit=0` | `UZ-TEL-001` HTTP 400 |
| `customer_limit_over_max` | 3.4 | `limit=201` | `UZ-TEL-001` HTTP 400 |
| `operator_missing_token` | 4.4 | no `X-Internal-Token` | `UZ-INT-001` HTTP 401 |
| `customer_cross_tenant` | 3.3 | JWT WS ≠ path WS | `UZ-WS-003` HTTP 403 |

### Edge Case Tests

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| `no_rows_returns_empty_array` | 3.1 | zombie with 0 deliveries | `{ "items": [], "cursor": null }` |
| `operator_all_filters_null` | 4.1 | no query params | all rows up to default limit |
| `ttft_zero_executor_old` | 2.3 | executor returns no `time_to_first_token_ms` key | row stored with 0, no error |

### Regression Tests

| Test name | What it guards | File |
|-----------|---------------|------|
| `deduct_zombie_usage_signature_stable` | `deductZombieUsage` signature unchanged — new `ExecutionUsage` fields must not break billing callers | `metering_test.zig` |
| `credit_audit_row_still_written` | M15_001 audit path still fires alongside new telemetry write | `metering_test.zig` |

### Leak Detection Tests

| Test name | Dim | What it proves |
|-----------|-----|---------------|
| `telemetry_row_deinit_no_leak` | 2.2 | `TelemetryRow.deinit` frees all owned slices; `std.testing.allocator` reports zero leaks |
| `list_telemetry_for_zombie_no_leak` | 3.1 | returned `[]TelemetryRow` slice is freed by caller; no leak on happy path |

### Spec-Claim Tracing

| Spec claim | Test | Type |
|-----------|------|------|
| One row per delivery, keyed on `event_id` | `insert_telemetry_idempotent` | integration |
| TTFT=0 stored when executor does not report it | `insert_zero_ttft_allowed` | integration |
| Customer sees only their workspace | `customer_cross_tenant` | negative |
| Operator sees all workspaces | `operator_api_all_workspaces` | integration |
| Non-fatal DB failure does not break delivery | `record_delivery_writes_telemetry` (inject conn fail variant) | integration |

---

## Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify (must pass before next step) |
|------|--------|--------------------------------------|
| 1 | Add `time_to_first_token_ms: u64 = 0` to `StageResult` in `executor/client.zig`; read from JSON | `zig build` |
| 2 | Add `time_to_first_token_ms` + `epoch_wall_time_ms` to `EventResult` in `event_loop_types.zig`; capture epoch before `executeInSandbox` in `deliverEvent` | `zig build` |
| 3 | Add new fields to `ExecutionUsage`; update `recordZombieDelivery` signature in `metering.zig` | `zig build` |
| 4 | Write `schema/NNN_zombie_execution_telemetry.sql`; add `@embedFile` to `schema/embed.zig`; add migration entry to `src/cmd/common.zig` | `zig build` + Schema Guard output |
| 5 | Write `src/state/zombie_telemetry_store.zig` (`insertTelemetry`, `listTelemetryForZombie`, `listTelemetryAll`, `TelemetryRow`) | `zig build` |
| 6 | Wire `insertTelemetry` call in `recordZombieDelivery` (non-fatal, parallel to credit deduction) | `zig build` |
| 7 | Write `src/http/handlers/zombie_telemetry.zig` (both handlers) | `zig build` |
| 8 | Register routes in `src/http/router.zig`; add test imports in `src/main.zig` | `zig build` |
| 9 | Write all unit + integration tests via `/write-unit-test` | `make test-integration-db` |
| 10 | Cross-compile + pg-drain + lint + gitleaks | `make lint && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |

---

## Acceptance Criteria

**Status:** PENDING

- [ ] `GET /v1/workspaces/{ws}/zombies/{id}/telemetry` returns rows with `token_count`, `time_to_first_token_ms`, `epoch_wall_time_ms`, `wall_seconds` — verify: integration test `customer_api_returns_rows`
- [ ] `GET /internal/v1/telemetry?workspace_id=X` returns only workspace X rows — verify: `operator_api_workspace_filter`
- [ ] Same `event_id` delivered twice inserts exactly one row — verify: `insert_telemetry_idempotent`
- [ ] DB write failure in telemetry path does not drop the event or fail the credit deduction — verify: integration test with injected conn failure in `insertTelemetry`
- [ ] Cross-tenant request returns HTTP 403 — verify: `customer_cross_tenant`
- [ ] `make check-pg-drain` passes — verify: `make check-pg-drain`
- [ ] All touched .zig files ≤ 350 lines — verify: `wc -l` gate in eval commands
- [ ] Cross-compiles on x86_64-linux and aarch64-linux — verify: E7

---

## Eval Commands (Post-Implementation Verification)

**Status:** PENDING

```bash
# E1: Schema guard — print before editing any schema file
cat VERSION
echo "SCHEMA GUARD: VERSION=$(cat VERSION) — full teardown branch (pre-v2.0.0)"

# E2: Build
zig build 2>&1 | head -5; echo "build=$?"

# E3: Unit tests
zig build test 2>&1 | tail -5; echo "unit_test=$?"

# E4: Integration tests
make test-integration-db 2>&1 | tail -10; echo "integration_test=$?"

# E5: Lint
make lint 2>&1 | grep -E "✓|FAIL"

# E6: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"

# E7: 350-line gate (exempts .md — RULE FLL)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines" }'

# E8: pg-drain
make check-pg-drain 2>&1 | tail -3; echo "drain=$?"

# E9: Gitleaks
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E10: Orphan sweep — verify deductZombieUsage callers unchanged
grep -rn "deductZombieUsage" src/ --include="*.zig"
echo "E10: callers above — verify signature unchanged"

# E11: No raw SQL in metering.zig
grep -n "SELECT\|INSERT\|UPDATE" src/zombie/metering.zig && echo "FAIL: raw SQL in metering.zig" || echo "PASS: no raw SQL"

# E12: Customer endpoint smoke (requires running server)
# curl -s -H "Authorization: Bearer <token>" \
#   "http://localhost:PORT/v1/workspaces/WS1/zombies/Z1/telemetry?limit=5" | jq .

# E13: Operator endpoint smoke (requires running server)
# curl -s -H "X-Internal-Token: <token>" \
#   "http://localhost:PORT/internal/v1/telemetry?limit=5" | jq .
```

---

## Dead Code Sweep

**Status:** PENDING

N/A — no files deleted. New symbols only.

Orphan check for renamed `recordZombieDelivery` signature:

| Changed symbol | Grep command | Expected |
|---------------|--------------|----------|
| `recordZombieDelivery` callers | `grep -rn "recordZombieDelivery" src/ --include="*.zig"` | Only `event_loop.zig` (1 caller) — verify arg count matches new signature |

---

## Verification Evidence

**Status:** PENDING — fill during VERIFY phase.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `zig build test` | | |
| Integration tests | `make test-integration-db` | | |
| Leak detection | `zig build test \| grep leak` | | |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | | |
| Lint | `make lint` | | |
| pg-drain | `make check-pg-drain` | | |
| 350L gate | `wc -l` (all touched .zig) | | |
| Gitleaks | `gitleaks detect` | | |

---

## Out of Scope

- Grafana dashboard for the telemetry data — ops concern; Prometheus/PostHog (M15_002) handles aggregate views
- Per-zombie TTFT trend charts in the app dashboard — M12 concern, depends on this API
- Retention policy / TTL on `zombie_execution_telemetry` rows — future ops milestone
- Streaming/WebSocket delivery of telemetry updates — polling API is sufficient for v1
- Token-level breakdown (input vs output tokens) — requires executor-side change beyond this scope
- `zombiectl telemetry` CLI subcommand — depends on this API; scoped to a future M18_002
