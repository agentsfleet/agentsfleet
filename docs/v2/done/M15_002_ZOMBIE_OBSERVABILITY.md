# M15_002: Zombie Observability — PostHog + Prometheus

**Prototype:** v0.9.0
**Milestone:** M15
**Workstream:** 002
**Date:** Apr 11, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — Zombie execution is a Grafana and PostHog blind spot
**Batch:** B1 (parallel with M15_001)
**Branch:** feat/m15-zombie-observability
**Depends on:** M10_001 (pipeline v1 removal)

---

## Overview

**Goal:** Every zombie trigger and execution completion emits a PostHog event and increments
a Prometheus counter, making zombie throughput visible in Grafana and product analytics.

**Problem (observable):**
- PostHog dashboards show zero zombie events. `src/observability/posthog_events.zig` has
  `trackRunStarted`, `trackRunCompleted`, etc. — none called in zombie paths.
- `GET /metrics` response contains no `zombies_*` counters. `src/observability/metrics_counters.zig`
  `Snapshot` struct is all pipeline-era (`runs_created_total`, `agent_echo_calls_total`, etc.).
- `src/zombie/event_loop.zig` calls `logActivity()` only — zero PostHog, zero Prometheus.
- `src/http/handlers/webhooks.zig` enqueues to Redis but emits no analytics.

**V1 reference:** `src/observability/posthog_events.zig` — `[_]posthog.Property{...}` pattern,
`trackRunCompleted` (line 58) and `trackRunStarted` (line 10) for mirror reference.
`src/observability/metrics_counters.zig` — `Snapshot` struct (line 21), `DurationBuckets`
(line 6), `HistogramSnapshot` (line 7) for extension pattern.

**Solution:** Two additive changes to existing files (no new files):
1. Extend `Snapshot` struct with zombie counter fields + one histogram entry.
2. Add `ZombieEventProps` struct and two `trackZombie*` functions to `posthog_events.zig`.
Wire call sites in webhooks.zig (trigger) and event_loop.zig (completion).

---

---

## Amendments (Apr 12, 2026) — Spec-vs-Rules Reconciliation

Original spec references symbols and constraints that conflict with the current codebase and global rules. Authoritative overrides:

**A1. PostHog module path.** Spec references `src/observability/posthog_events.zig` — does not exist. Real modules:
- `src/observability/telemetry.zig` (Telemetry wrapper + PostHog client injection)
- `src/observability/telemetry_events.zig` (typed event structs with `kind`/`properties()` shape)

**A2. PostHog event pattern.** Replace `trackZombieTriggered(client, props) void` functions with typed event structs in `telemetry_events.zig`:
- `ZombieTriggered` struct — kind `.zombie_triggered`
- `ZombieCompleted` struct — kind `.zombie_completed`
- Both include `distinct_id`, `workspace_id`, `zombie_id`, `event_id`; `ZombieCompleted` adds `tokens`, `wall_ms`, `exit_status`.
- Add `zombie_triggered` and `zombie_completed` to `EventKind` enum.
- Re-export from `telemetry.zig`.
- Call sites use `ctx.telemetry.capture(telemetry_mod.ZombieTriggered, .{ ... })` — the existing dispatch pattern.

**A3. Metrics split.** Spec mandates "no new files" but `metrics_counters.zig` is already 327 lines. **RULE FLL (350-line gate)** wins. Create `src/observability/metrics_zombie.zig` for atomics + histogram (mirrors established pattern: `metrics_external.zig`, `metrics_histograms.zig`, `metrics_gate_histogram.zig`). Re-export `incZombies*`, `addZombieTokens`, `observeZombieExecutionSeconds` from `metrics_counters.zig`. Add 5 fields to `Snapshot`; in `snapshot()`, load via `metrics_zombie.snapshotZombieFields()`.

**A4. event_loop.zig 350-gate.** `event_loop.zig` is 347 lines. Move local `logDeliveryResult` (13 lines) to `event_loop_helpers.zig` and extend it to emit metrics + telemetry. Net event_loop.zig line delta: negative.

**A5. Telemetry threading.** Add optional `telemetry: ?*telemetry.Telemetry = null` to `EventLoopConfig` and `ZombieWorkerConfig`. Wire from `src/cmd/worker.zig` where Telemetry is already constructed.

**A6. File-count constraint** (§5.0): updated to ≤7 files: `metrics_zombie.zig` (new), `metrics_counters.zig`, `telemetry_events.zig`, `telemetry.zig`, `webhooks.zig`, `event_loop_types.zig`, `event_loop_helpers.zig`, `event_loop.zig`, `worker_zombie.zig`, `worker.zig`. (Tests additional.)

**A7. `make check-pg-drain` removed.** Target no longer exists; drain-lifecycle rule still enforced by convention — no new `conn.query()` introduced in this spec (all new paths use atomics/events/captures, not DB queries).

---

## 1.0 Prometheus — Extend Snapshot and Counters

**Status:** DONE

Extend `src/observability/metrics_counters.zig`. Add fields to `Snapshot` (compile-time
additive — all existing Grafana queries unaffected). Add one histogram const slice
`ZombieDurationBuckets` for execution wall-time distribution.

**Dimensions:**
- 1.1 PENDING
  - target: `src/observability/metrics_counters.zig:Snapshot`
  - input: N/A — struct extension
  - expected: `Snapshot` gains `zombies_triggered_total`, `zombies_completed_total`,
    `zombies_failed_total`, `zombie_tokens_total` (all `u64`), and
    `zombie_execution_seconds: HistogramSnapshot`
  - test_type: unit (comptime field existence)
- 1.2 PENDING
  - target: `src/observability/metrics_counters.zig:incZombiesTriggered`
  - input: called from webhooks.zig after XADD succeeds
  - expected: `zombies_triggered_total` increments atomically; renders in `/metrics` output
  - test_type: unit (snapshot comparison)
- 1.3 PENDING
  - target: `src/observability/metrics_counters.zig:addZombieTokens` and
    `observeZombieExecutionSeconds`
  - input: token_count=1500, wall_ms=4200
  - expected: `zombie_tokens_total += 1500`; histogram bucket for 4.2s incremented
  - test_type: unit (snapshot comparison)

---

## 2.0 PostHog — ZombieEventProps and Track Functions

**Status:** DONE

Extend `src/observability/posthog_events.zig`. Add one borrowed-slice struct and two
functions, each ≤50 lines. Mirror the `trackRunStarted` / `trackRunCompleted` pattern.

**Dimensions:**
- 2.1 PENDING
  - target: `src/observability/posthog_events.zig:ZombieEventProps`
  - input: N/A — struct definition
  - expected: `zombie_id`, `workspace_id`, `event_id`, `tokens_used: u64`, `wall_ms: u64`
    all `[]const u8` or `u64`; no optionals; no heap allocation
  - test_type: unit (comptime)
- 2.2 PENDING
  - target: `src/observability/posthog_events.zig:trackZombieTriggered`
  - input: valid PostHog client, `ZombieEventProps{ .zombie_id="z1", .event_id="e1", ... }`
  - expected: `ph.capture(.{ .event = "zombie_triggered", .properties = &props })` called;
    no error even if client is null
  - test_type: unit (mock client)
- 2.3 PENDING
  - target: `src/observability/posthog_events.zig:trackZombieCompleted`
  - input: null client (PostHog disabled)
  - expected: function returns immediately, no panic, no log noise
  - test_type: unit
- 2.4 PENDING
  - target: call site `src/zombie/event_loop.zig:processEvent`
  - input: successful delivery, client non-null
  - expected: `trackZombieCompleted` called with correct `wall_ms` derived from
    `std.time.milliTimestamp()` delta around `deliverEvent()`
  - test_type: integration (real PostHog client stub)

---

## 3.0 Wire Call Sites

**Status:** DONE

Two files get new calls — no new files, no new imports beyond what they already have.

**Dimensions:**
- 3.1 PENDING
  - target: `src/http/handlers/webhooks.zig:handleReceiveWebhook`
  - input: successful XADD (202 response path)
  - expected: `incZombiesTriggered()` called; `trackZombieTriggered()` called before return
  - test_type: integration (real Redis)
- 3.2 PENDING
  - target: `src/zombie/event_loop.zig:processEvent`
  - input: delivery success, executor returns wall_ms + token_count
  - expected: `incZombiesCompleted()`, `addZombieTokens()`, `observeZombieExecutionSeconds()`,
    `trackZombieCompleted()` — all called in sequence before XACK
  - test_type: integration

---

## 4.0 Interfaces

**Status:** DONE

### 4.1 New Struct (`src/observability/posthog_events.zig`)

```zig
pub const ZombieEventProps = struct {
    zombie_id:    []const u8,  // borrowed
    workspace_id: []const u8,  // borrowed
    event_id:     []const u8,  // borrowed
    tokens_used:  u64,
    wall_ms:      u64,
};
```

### 4.2 New Functions (each ≤ 50 lines)

```zig
// posthog_events.zig
pub fn trackZombieTriggered(client: ?*posthog.PostHogClient, props: ZombieEventProps) void
pub fn trackZombieCompleted(client: ?*posthog.PostHogClient, props: ZombieEventProps) void

// metrics_counters.zig additions (counter functions mirror existing incRunsCreated pattern)
pub fn incZombiesTriggered() void
pub fn incZombiesCompleted() void
pub fn incZombiesFailed() void
pub fn addZombieTokens(n: u64) void
pub fn observeZombieExecutionSeconds(wall_ms: u64) void
```

### 4.3 New Snapshot Fields (`src/observability/metrics_counters.zig:Snapshot`)

```zig
zombies_triggered_total:   u64,
zombies_completed_total:   u64,
zombies_failed_total:      u64,
zombie_tokens_total:       u64,
zombie_execution_seconds:  HistogramSnapshot,
```

### 4.4 New Const Slice

```zig
// Mirror DurationBuckets (line 6) — wall-time distribution for zombies
pub const ZombieDurationBuckets = [_]u64{ 1, 5, 10, 30, 60, 120, 300, 600 };
```

### 4.5 Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| PostHog client null | `if (client) \|ph\|` guard — early return | nothing |
| PostHog `capture` returns error | `catch {}` — best-effort only | nothing |
| Counter increment fails | atomic add never fails | N/A |

---

## 5.0 Implementation Constraints

| Constraint | Verify |
|-----------|--------|
| No new files — additive edits to existing modules only | `git diff --name-only \| wc -l` = 4 files max |
| `posthog_events.zig` stays ≤ 400 lines post-edit | `wc -l` |
| `metrics_counters.zig` stays ≤ 400 lines post-edit | `wc -l` |
| Each new function ≤ 50 lines | manual count |
| `ZombieEventProps` has zero heap allocations | no `alloc` parameter |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |

---

## 6.0 Test Specification

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `snapshot_has_zombie_fields` | 1.1 | `Snapshot` | N/A | comptime field access |
| `inc_triggered_increments_counter` | 1.2 | `incZombiesTriggered` | call once | snapshot delta = 1 |
| `add_tokens_accumulates` | 1.3 | `addZombieTokens` | 1500 | `zombie_tokens_total` = 1500 |
| `zombie_props_struct_compiles` | 2.1 | `ZombieEventProps` | N/A | comptime |
| `null_client_no_panic` | 2.3 | `trackZombieCompleted` | null client | no panic |

### Integration Tests

| Test name | Dim | Infra | Input | Expected |
|-----------|-----|-------|-------|----------|
| `webhook_increments_triggered` | 3.1 | DB + Redis | valid webhook | metrics snapshot +1 |
| `event_loop_emits_completion` | 3.2 | DB + Redis | delivery success | all 4 counters updated |

### Spec-Claim Tracing

| Spec claim | Test | Type |
|-----------|------|------|
| `zombies_triggered_total` visible in `/metrics` | `webhook_increments_triggered` | integration |
| PostHog receives `zombie_triggered` event | `trackZombieTriggered` mock | unit |
| Token count accumulated per delivery | `add_tokens_accumulates` | unit |

---

## 7.0 Execution Plan

| Step | Action | Verify |
|------|--------|--------|
| 1 | Add `ZombieDurationBuckets`, `zombie_*` fields to `Snapshot` | `zig build` |
| 2 | Add counter functions to `metrics_counters.zig` | `zig build` |
| 3 | Add `ZombieEventProps` + two track functions to `posthog_events.zig` | `zig build` |
| 4 | Wire call sites in `webhooks.zig` and `event_loop.zig` | `zig build` |
| 5 | Write unit tests (snapshot, null-client) | `make test` |
| 6 | Write integration tests | `make test-integration-db` |
| 7 | Cross-compile + lint | `make lint` |

---

## 8.0 Acceptance Criteria

- [ ] `GET /metrics` includes `zombies_triggered_total`, `zombies_completed_total`,
  `zombie_tokens_total` — verify: `curl localhost:PORT/metrics | grep zombie`
- [ ] PostHog receives `zombie_triggered` and `zombie_completed` events — verify: integration test
- [ ] Null PostHog client causes no panic — verify: unit test 2.3
- [ ] `make lint` passes — verify: `make lint`
- [ ] Both new files stay within 350L — verify: `wc -l src/observability/*.zig`
- [ ] Cross-compiles — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`

---

## Applicable Rules

- RULE XCC — cross-compile before commit
- RULE FLL — 350-line gate on touched files
- RULE FLS — drain all results (new DB queries must use PgQuery)
- RULE ORP — orphan sweep if renaming symbols

## Invariants

N/A — no compile-time guardrails for this workstream.

## Eval Commands

```bash
# E1: Build + test
zig build 2>&1 | head -5; echo "build=$?"
zig build test 2>&1 | tail -5; echo "test=$?"

# E2: Lint + cross-compile + gitleaks
make lint 2>&1 | grep -E "✓|FAIL"
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E3: Memory leak check
zig build test 2>&1 | grep -i "leak" | head -5
echo "E3: leak check (empty = pass)"

# E4: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'
```

## Dead Code Sweep

N/A — no files deleted.

## Verification Evidence

**Status:** DONE — fill during VERIFY phase.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration-db` | | |
| Metrics output | `curl localhost:PORT/metrics \| grep zombie` | | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | | |
| Lint | `make lint` | | |
| Line gate | `wc -l src/observability/posthog_events.zig src/observability/metrics_counters.zig` | | |

---

## Out of Scope

- Grafana dashboard JSON — ops concern, not a code change
- `zombie_errored` event (delivery failure path) — add in a follow-on once failure taxonomy is stable
- Per-zombie breakdown in Prometheus labels — high cardinality risk; aggregate counters only for now
- Credit deduction PostHog event (`CREDIT_DEDUCTED`) — covered in M15_001
