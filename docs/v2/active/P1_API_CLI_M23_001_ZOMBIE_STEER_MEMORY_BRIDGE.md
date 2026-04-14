# M23_001: Zombie Steer + Memory Bridge — Chat-to-Coach Persists Across Runs

**Prototype:** v2
**Milestone:** M23
**Workstream:** 001
**Date:** Apr 13, 2026: 10:55 PM
**Status:** IN_PROGRESS
**Branch:** feat/m23-001-zombie-steer
**Priority:** P1 — Operators currently re-teach the same correction to every zombie run. Without this, memory-based steering (referenced in docs/nostromo/memory.md and archetype guides) has no surface to land through.
**Batch:** B1
**Depends on:**
- M14_001 (persistent zombie memory — done) — `memory_store` / `memory_recall` primitives must exist on `memory.memory_entries`.
- M18_001 (zombie execution telemetry / SSE stream — done) — live-run injection depends on the SSE event channel being reachable for `steering_stored` / `interrupt_ack` events.
- M21_001 (agent interrupt and steer, v1 — done) — provides the run-scoped `POST /v1/runs/{id}:interrupt` primitive + Redis checkpoint polling pattern. This workstream generalizes that to zombie-scoped addressing and bridges it to memory. M21_001's CLI chat bar and worker polling logic are the reference implementation we're adapting.

---

## Overview

**Goal (testable):** An operator can send a steering message via `POST /v1/zombies/{id}:steer` and the handler (a) writes a durable `steering:*` entry into the zombie's memory that influences all future runs, (b) if an active run exists for that zombie, injects the message into the current executor turn so the in-flight run also benefits, and (c) emits an SSE `steering_stored` event (always) plus an `interrupt_ack` event (when a run was also steered) so CLI/UI surfaces can render confirmation. Validated end-to-end by a test where a second run of the same zombie — fired after the steer call, with no further operator input — applies the stored steering automatically.

**Problem:** v1's `POST /v1/runs/{id}:interrupt` (M21_001) injects corrections into a single live run. The correction dies when the run ends. Every subsequent run of the same zombie is amnesiac — the operator has to re-type the same guidance. For v2 zombies, which are event-driven (inbound email, Slack event, Grafana webhook) and often sub-second, the live-interrupt surface is rarely usable at all: by the time a human opens a replay view and types, the run is over. What operators actually need is durable steering — "from now on, this zombie should behave this way" — stored in memory and recalled on every subsequent run. Memory.md already promises this behavior; this workstream ships the surface that writes to it.

**Solution summary:** New HTTP endpoint `POST /v1/zombies/{id}:steer` accepts a message and optional scope hint. The handler always writes a `steering:<derived_key>` entry via the existing `memory_store` primitive, with the key's namespace segment decided by an LLM pass over the message content + optional operator-provided scope hint. If the zombie has an active run (resolved via `core.zombie_sessions.execution_id`), the handler additionally dispatches an in-band injection to the executor using the same IPC channel M21_001 built. Both outcomes (store + optional inject) emit SSE events for CLI/UI observability. The v1 endpoint `POST /v1/runs/{id}:interrupt` stays in place as an alias that resolves `run_id → zombie_id` server-side and then calls the same handler, preserving existing CLI workflows. This gives scheduled zombies (Ops Zombie polling Loki every 5 min, long-running research runs) continued access to live-interrupt while making durable steering the primary surface for the common event-driven case.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/http/handlers/zombie_steer_http.zig` | CREATE | New `POST /v1/zombies/{id}:steer` endpoint — parses request, calls bridge, emits events |
| `src/memory/steering_bridge.zig` | CREATE | Scope inference (LLM pass) + `memory_store` invocation + active-run injection orchestration |
| `src/http/handlers/run_interrupt_http.zig` | MODIFY | Existing `POST /v1/runs/{id}:interrupt` handler resolves `run_id → zombie_id` and delegates to `steering_bridge` to dual-write (store + inject) instead of injection-only |
| `src/worker/interrupt_polling.zig` | MODIFY | Extend Redis GETDEL polling to also log the steering entry key so worker observability reflects which memory entry caused a corrected turn |
| `src/memory/zombie_memory.zig` | MODIFY | Expose `storeSteering(zombie_id, key_suffix, content, scope_hint)` as a thin typed wrapper over `memory_store` to centralize the `steering:` prefix convention |
| `src/http/events/sse_events.zig` | MODIFY | Add `steering_stored` event type; keep `interrupt_ack` |
| `public/openapi.json` | MODIFY | Declare `POST /v1/zombies/{id}:steer`, update `/runs/{id}:interrupt` to reference the shared response shape, add new SSE event types |
| `src/http/routes.zig` | MODIFY | Register new route |
| `tests/integration/zombie_steer_test.zig` | CREATE | End-to-end: steer → memory entry lands → next run recalls it → verify draft reflects steering |
| `tests/integration/run_interrupt_bridge_test.zig` | CREATE | Legacy path: POST to `/runs/{id}:interrupt` still works AND dual-writes to memory |

**Not changed:** `schema/029_memory_entries.sql` — no new tables. Steering entries reuse `memory.memory_entries` with `category = 'core'` and `key LIKE 'steering:%'`. Zero migrations in this workstream.

---

## Applicable Rules

- **RULE FLS** — drain every pg query in the handler path before deinit (memory_store path, active-run resolution path).
- **RULE FLL** — 350-line gate on every created/modified `.zig` file. The bridge + handler must be split if they grow past limit.
- **RULE ORP** — `POST /v1/runs/{id}:interrupt` changes behavior (dual-write). Orphan-sweep for any test or doc that asserted "interrupt only injects, does not persist" — update or retire those assertions.
- **RULE OBS** — every observable state transition emits an event (`steering_stored`, `interrupt_ack`, `steering_skipped_no_active_run` for CLI/UI transparency).
- **RULE EP4** (pre-v2.0 carve-out) — new endpoint, no deprecations, no 410s needed. v1 `interrupt` endpoint changes behavior but keeps its path, so no drift.

---

## §1 — `POST /v1/zombies/{id}:steer` Endpoint

**Status:** PENDING

New HTTP endpoint scoped to the zombie, not to a specific run. Accepts a free-form message and optional scope hint. Returns an ack that includes the memory entry key that was written and whether a run was also steered.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `zombie_steer_http.zig:handle` | `POST /v1/zombies/zom_abc:steer` with `{"message": "skip demo CTA for sub-50 companies"}` and valid workspace auth | 200 with body `{"ack": true, "stored_key": "steering:sub_50_employees", "run_steered": false, "scope": "zombie"}` | integration |
| 1.2 | PENDING | `zombie_steer_http.zig:handle` | same as 1.1 but operator includes `{"scope_hint": "entity:acme_corp"}` | stored_key prefixed with `steering:acme_corp:`; scope hint is honored over LLM inference | integration |
| 1.3 | PENDING | `zombie_steer_http.zig:handle` | POST with missing workspace auth header | 401 `UZ-AUTH-001` | integration |
| 1.4 | PENDING | `zombie_steer_http.zig:handle` | POST with zombie_id owned by a different workspace | 404 `UZ-ZOMBIE-NOT-FOUND` (not 403 — do not leak existence across workspaces) | integration |

---

## §2 — Memory Bridge: Scope Inference + Store

**Status:** PENDING

The bridge decides what key namespace to store under. Three-tier resolution: explicit `scope_hint` from caller wins; otherwise an LLM pass inspects the message content and chooses a namespace from the archetype's skill-template-declared set; otherwise default to `steering:general:<content_hash>`.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `steering_bridge.zig:inferScope` | message "skip demo CTA for sub-50 companies", no scope_hint, archetype = lead_collector with declared namespaces `[sub_50_employees, tone, competitor_mentions]` | returns `"sub_50_employees"` | unit |
| 2.2 | PENDING | `steering_bridge.zig:store` | scope = "acme_corp", message = "Jane is allergic to competitor mentions" | `memory.memory_entries` has row `(zombie_id, key="steering:acme_corp:no_competitor_mentions", category="core", tags=["steering"])` | integration |
| 2.3 | PENDING | `steering_bridge.zig:store` | upsert path: steer twice with same scope → latest content wins, `updated_at` advances | single row, `content` reflects second call, `updated_at > created_at` | integration |
| 2.4 | PENDING | `steering_bridge.zig:inferScope` | LLM pass fails (timeout, malformed response) | fallback to `"general:<sha256(message)[0..12]>"`, warning logged, no user-visible error | integration |

---

## §3 — Active-Run Injection (Live-Interrupt for Scheduled / Long-Running Zombies)

**Status:** PENDING

When a steer arrives, check `core.zombie_sessions` for an active `execution_id`. If present, dispatch IPC injection to the executor using the same path M21_001 built. Memory store happens FIRST (durability before liveness) — if injection fails, the steering still persists for the next run. Target use case: scheduled Ops Zombie poll runs that span minutes, long-running research runs.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `steering_bridge.zig:maybeInject` | zombie has active session with `execution_id`, message = "also check their LinkedIn" | IPC `injectUserMessage(execution_id, message)` called; returns success; response includes `run_steered: true, execution_id: "..."` | integration |
| 3.2 | PENDING | `steering_bridge.zig:maybeInject` | zombie has no active session (between runs) | no IPC call; response includes `run_steered: false`; memory store still happens | integration |
| 3.3 | PENDING | `steering_bridge.zig:maybeInject` | zombie has stale `execution_id` (TOCTOU: worker died between DB read and IPC send) | IPC returns error; memory store already completed; response includes `run_steered: false, inject_error: "executor_unreachable"` — never a 5xx | integration |
| 3.4 | PENDING | `worker/interrupt_polling.zig` | worker gate loop polls `run:{id}:interrupt` Redis key (existing M21_001 path), finds message | message injected AND logged to `core.activity_events` with `steering_entry_key` field referencing the memory row — observability ties the run correction back to the durable entry | integration |

---

## §4 — SSE Events + v1 Endpoint Bridging

**Status:** PENDING

Two new / changed SSE events: `steering_stored` (always, after memory write) and `interrupt_ack` (only when active-run injection succeeded, preserving M21_001 CLI behavior). Existing `POST /v1/runs/{id}:interrupt` handler modified to resolve `run_id → zombie_id → call shared bridge` so legacy CLI users (v1 `zombied run --watch` chat bar) automatically gain memory persistence without code changes on their side.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | `sse_events.zig:emitSteeringStored` | successful memory write | SSE `event: steering_stored\ndata: {"key": "...", "scope": "...", "stored_at": ...}` | contract |
| 4.2 | PENDING | `sse_events.zig:emitInterruptAck` | successful active-run injection | SSE `event: interrupt_ack\ndata: {"mode": "instant", "received_at": ..., "steering_entry_key": "..."}` (`steering_entry_key` is new vs M21_001) | contract |
| 4.3 | PENDING | `run_interrupt_http.zig:handle` | POST `/v1/runs/{id}:interrupt` with `{"message": "...", "mode": "queued"}` | handler resolves run→zombie, calls bridge, returns same ack shape as `/zombies/{id}:steer` PLUS backwards-compatible `mode` field. Response also includes `stored_key` — new field — so old clients ignore it and new clients see it | integration |
| 4.4 | PENDING | `zombie_steer_http.zig:handle` | POST with no active run AND memory_store throws (DB down) | 503 `UZ-MEM-001`, no partial state, no SSE emission — the whole operation is atomic from caller's perspective | integration |

---

## Interfaces

**Status:** PENDING

### Public Endpoints

```
POST /v1/zombies/{zombie_id}:steer
Headers: Authorization: Bearer {workspace_token}
Body: {
  "message": string (1..8192),
  "scope_hint": string | null (optional; one of "zombie" | "entity:<key>" | "workspace")
}
Response 200: {
  "ack": true,
  "stored_key": string,       // e.g. "steering:sub_50_employees"
  "scope": string,            // resolved scope, even if hint was null
  "run_steered": boolean,
  "execution_id": string | null,  // present iff run_steered=true
  "inject_error": string | null   // present iff attempt was made and failed
}
```

```
POST /v1/runs/{run_id}:interrupt      (v1 alias, now dual-writes)
Headers: Authorization: Bearer {workspace_token}
Body: {
  "message": string,
  "mode": "instant" | "queued"
}
Response 200: {
  "ack": true,
  "mode": "instant" | "queued",
  "stored_key": string,       // NEW vs M21_001 — memory entry created
  "run_steered": boolean
}
```

### SSE Events

```
event: steering_stored
data: {"key": "<memory_entry_key>", "scope": "<resolved>", "stored_at": <unix_ms>}

event: interrupt_ack
data: {"mode": "instant"|"queued", "received_at": <unix_ms>, "steering_entry_key": "<key>"}
```

### Public Functions (Zig)

```zig
// src/memory/steering_bridge.zig
pub fn storeAndMaybeInject(
    allocator: std.mem.Allocator,
    db: *pg.Conn,
    zombie_id: []const u8,
    message: []const u8,
    scope_hint: ?[]const u8,
) !SteerResult;

pub const SteerResult = struct {
    stored_key: []const u8,
    scope: []const u8,
    run_steered: bool,
    execution_id: ?[]const u8,
    inject_error: ?[]const u8,
};

// src/memory/zombie_memory.zig  (new thin wrapper)
pub fn storeSteering(
    allocator: std.mem.Allocator,
    db: *pg.Conn,
    zombie_id: []const u8,
    key_suffix: []const u8,   // e.g. "sub_50_employees" — prefix added here
    content: []const u8,
) !StoredEntry;
```

### Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `message` | string | 1–8192 chars, UTF-8, no nulls | "skip demo CTA for sub-50 companies" |
| `scope_hint` | string \| null | optional; pattern `^(zombie\|workspace\|entity:[a-z0-9_\-:]{1,64})$` | "entity:acme_corp" |
| `zombie_id` | path param | ULID format (`zom_01J...`) | "zom_01JQX..." |
| `mode` (v1 only) | enum | "instant" or "queued" | "queued" |

### Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| `stored_key` | string | always (memory write is mandatory) | "steering:sub_50_employees" |
| `scope` | string | always — resolved even if hint was null | "zombie" |
| `run_steered` | bool | always | true |
| `execution_id` | string \| null | non-null iff `run_steered=true` | "exec_01JQY..." |
| `inject_error` | string \| null | non-null iff attempted AND failed | "executor_unreachable" |

### Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Missing / invalid workspace auth | Reject before DB touch | 401 `UZ-AUTH-001` |
| `zombie_id` belongs to different workspace | Treat as not found (no existence leak) | 404 `UZ-ZOMBIE-NOT-FOUND` |
| `message` empty or > 8192 | Reject at parse | 400 `UZ-VALIDATION-001` with `field: "message"` |
| `scope_hint` malformed | Reject at parse | 400 `UZ-VALIDATION-001` with `field: "scope_hint"` |
| Memory DB unreachable | No store, no inject, no SSE emission | 503 `UZ-MEM-001` |
| Active-run injection fails (TOCTOU, executor down) | Memory store STILL succeeds; response includes `inject_error`; status remains 200 | 200 with `run_steered: false, inject_error: "..."` |
| LLM scope inference fails | Fallback to `general:<hash>`; warn-log; response normal | 200 (no visible failure) |
| Rate limit (future; not this workstream) | N/A — out of scope | N/A |

---

## Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Steer arrives during run shutdown (TOCTOU) | worker exits between DB active-session read and IPC send | memory store completes; IPC returns error; response status 200 with `run_steered: false, inject_error: "executor_unreachable"` | partial success — steering persists, live run missed it. Next run will pick it up from memory. |
| Two steers arrive simultaneously for same scope | concurrent POSTs | memory_store is upsert by key — last write wins; both return 200; SSE emits two `steering_stored` events in arrival order | both confirmations shown in replay; final stored content is the later one |
| LLM scope inference returns hallucinated namespace | LLM picks `steering:completely_made_up_namespace` not declared in template | accept the key (don't block on template matching); warn-log with template-declared set for later audit | entry lands under hallucinated key; harmless — future recalls in the skill template just won't hit it. Fix is template-level (tighten inference prompt). Tracked as tech debt. |
| Memory write succeeds, SSE emit fails (subscriber dropped) | transient Redis pubsub outage | memory entry is durable; response returns 200; no SSE — subscribers reconnect and backfill via `core.activity_events` timeline | CLI chat bar may show no confirmation; operator refreshes replay view and sees the entry in memory tab |
| v1 `/runs/{id}:interrupt` caller is pre-M24 CLI (old client) | old client ignores new `stored_key` response field | forward-compatible — old client sees ack, new field is silently dropped. | old behavior preserved; operator gets memory bridge transparently |

**Platform constraints:**
- Redis `run:{id}:interrupt` GETDEL requires Redis ≥ 6.2; check deployment Redis version before merging.
- IPC `injectUserMessage` is fire-and-forget; we get delivery confirmation only by observing the subsequent SSE `interrupt_ack` from the worker side. Do NOT depend on IPC return value for user-visible success.
- LLM scope inference adds latency (typical 200–600ms). Steer endpoint p95 budget is <1s; if inference exceeds 800ms, short-circuit to fallback namespace.

---

## Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Endpoint p95 latency < 1s (store + optional inject + SSE emit) | benchmark in integration test with `API_BENCH_MAX_P95_MS=1000` |
| Memory store is transactional — row either fully written or not at all | integration test: abort mid-store via injected panic; verify no partial row |
| Store happens BEFORE inject attempt (durability precedence) | unit test asserts call order via mock bridge |
| File under 350 lines (each) | `wc -l < 350` on every created/modified `.zig` file (RULE FLL) |
| No new tables, no new migrations | `diff schema/ main` shows no new `.sql` files |
| Backwards compatible v1 interrupt endpoint | existing `tests/integration/run_interrupt_test.zig` continues to pass unchanged |
| Cross-compiles | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |

---

## Invariants (Hard Guardrails)

**Status:** PENDING

| # | Invariant | Enforcement mechanism |
|---|-----------|----------------------|
| 1 | Every `steering:*` memory entry has non-empty `content` field | comptime assertion in `storeSteering` body: `if (content.len == 0) @compileError(...)` at callsite would be ideal; runtime reality: `assert(content.len > 0)` at entry to `storeSteering` |
| 2 | Every key written via this workstream has prefix `steering:` | `storeSteering` prepends the prefix — callers pass `key_suffix` only; enforced by function signature |
| 3 | No direct `memory_store` calls bypass `storeSteering` for steering content | grep-based lint in `make lint`: `grep -rn 'memory_store.*"steering:' src/ --include="*.zig" | grep -v 'steering_bridge\|zombie_memory'` must be empty |

---

## Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `infer_scope_from_content` | 2.1 | steering_bridge.zig | message + template namespaces | picks matching namespace |
| `infer_scope_honors_hint` | 2.1 | steering_bridge.zig | message + scope_hint | hint wins over inference |
| `infer_scope_fallback_on_llm_fail` | 2.4 | steering_bridge.zig | forced LLM failure | fallback `general:<hash>` |
| `store_steering_prepends_prefix` | 2.2 | zombie_memory.zig | key_suffix="foo" | stored key = "steering:foo" |

### Integration Tests

| Test name | Dim | Infra needed | Input | Expected |
|-----------|-----|-------------|-------|----------|
| `steer_persists_to_memory` | 2.2 | DB + Redis | POST steer, then read memory_entries | row exists with correct key + content |
| `steer_upsert_latest_wins` | 2.3 | DB | two POSTs same scope | single row, latest content |
| `steer_no_active_run` | 3.2 | DB | zombie with no execution_id | `run_steered: false`, memory still written |
| `steer_active_run_injects` | 3.1 | DB + Redis + fake executor | zombie with active session | `run_steered: true`, IPC called |
| `steer_toctou_executor_died` | 3.3 | DB + Redis | stale execution_id in DB | memory written, IPC error surfaced, status 200 |
| `steer_then_next_run_recalls` | all | DB + full worker | steer → trigger new run → verify recall | next run's draft reflects steering |
| `v1_interrupt_dual_writes` | 4.3 | DB + Redis | POST /runs/{id}:interrupt | memory entry created + run steered |
| `worker_logs_steering_entry_key` | 3.4 | DB + Redis + worker | interrupt picked up from Redis | activity_events row has `steering_entry_key` |

### Negative Tests (error paths that MUST fail)

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|---------------|
| `reject_empty_message` | 1.1 | `{"message": ""}` | 400 `UZ-VALIDATION-001` |
| `reject_oversized_message` | 1.1 | 8193-char message | 400 `UZ-VALIDATION-001` |
| `reject_cross_workspace_zombie` | 1.4 | zombie_id from another workspace | 404 `UZ-ZOMBIE-NOT-FOUND` (not 403) |
| `reject_missing_auth` | 1.3 | no Authorization header | 401 `UZ-AUTH-001` |
| `reject_malformed_scope_hint` | 1.2 | `scope_hint: "entity:has spaces"` | 400 `UZ-VALIDATION-001` |
| `memory_down_returns_503` | 4.4 | DB offline | 503 `UZ-MEM-001`, no SSE emitted |

### Edge Case Tests (boundary values)

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| `steer_with_8192_char_message` | 1.1 | exactly-at-limit message | 200, stored |
| `steer_with_unicode_message` | 1.1 | emoji + non-latin content | 200, stored byte-exact |
| `steer_concurrent_same_zombie` | failure-modes | 10 concurrent POSTs | all 200; final state is last-write |
| `steer_with_null_scope_hint` | 1.1 | `scope_hint: null` | LLM inference runs; scope resolved |

### Regression Tests (pre-existing behavior that MUST NOT change)

| Test name | What it guards | File |
|-----------|---------------|------|
| v1 interrupt SSE event shape | `event: interrupt_ack` still emits when active-run injected | `tests/integration/run_interrupt_test.zig` (existing) |
| v1 Redis GETDEL polling cadence | worker still polls Redis at each gate iteration | `tests/integration/worker_interrupt_polling_test.zig` (existing) |
| Cross-workspace isolation | zombie A's memory never visible to zombie B's steer | `tests/integration/memory_isolation_test.zig` (existing) |

### Leak Detection Tests

| Test name | Dim | What it proves |
|-----------|-----|---------------|
| `steer_allocates_zero_leaks` | all | std.testing.allocator reports 0 after full handler execution |
| `steer_under_load_no_leaks` | all | 1000 sequential POSTs, allocator report clean |

### Spec-Claim Tracing

| Spec claim | Test that proves it | Test type |
|-----------|-------------------|-----------|
| "next run applies steering without operator input" | `steer_then_next_run_recalls` | integration |
| "scheduled zombies retain live-interrupt" | `steer_active_run_injects` | integration |
| "v1 interrupt endpoint continues to work" | `v1_interrupt_dual_writes` + regression suite | integration |
| "memory store precedes inject (durability first)" | unit test with call-order mock | unit |

---

## Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify (must pass before next step) |
|------|--------|--------------------------------------|
| 1 | Define `SteerResult`, `storeSteering`, `storeAndMaybeInject` signatures in `steering_bridge.zig` + `zombie_memory.zig` | `zig build` compiles |
| 2 | Implement `storeSteering` wrapper + `inferScope` LLM path with fallback | `zig build && zig build test` (unit tests pass) |
| 3 | Implement `storeAndMaybeInject` orchestration (store → maybe inject → emit SSE) | `zig build test` |
| 4 | Create `zombie_steer_http.zig` handler + wire into `routes.zig` | integration test `steer_persists_to_memory` passes |
| 5 | Modify `run_interrupt_http.zig` to delegate to bridge | `v1_interrupt_dual_writes` passes; existing v1 regression suite still passes |
| 6 | Update worker polling to log `steering_entry_key` | `worker_logs_steering_entry_key` passes |
| 7 | Update `public/openapi.json` — new endpoint + response fields + events | `make check-openapi-errors` passes |
| 8 | Write `/write-unit-test` for all dimensions | `zig build test` (all pass) |
| 9 | Lint + cross-compile + gitleaks + 350L gate | all E-commands pass |

---

## Acceptance Criteria

**Status:** PENDING

- [ ] `POST /v1/zombies/{id}:steer` with valid input writes a `steering:*` memory entry — verify: `tests/integration/zombie_steer_test.zig::steer_persists_to_memory`
- [ ] Endpoint returns `stored_key` + `run_steered` in response body — verify: contract test on response shape
- [ ] Second run of the same zombie, triggered after steer with no further operator input, applies the stored steering in its draft output — verify: `steer_then_next_run_recalls`
- [ ] `POST /v1/runs/{id}:interrupt` continues to work AND additionally writes a memory entry — verify: `v1_interrupt_dual_writes` + v1 regression suite unchanged
- [ ] Active-run injection fires when zombie has in-flight execution — verify: `steer_active_run_injects`
- [ ] No active run = memory still written, response `run_steered: false` — verify: `steer_no_active_run`
- [ ] Memory DB down → 503 `UZ-MEM-001`, no partial state — verify: `memory_down_returns_503`
- [ ] Cross-workspace steer returns 404 not 403 (no existence leak) — verify: `reject_cross_workspace_zombie`
- [ ] Endpoint p95 < 1s including LLM scope inference — verify: `make bench` with `API_BENCH_MAX_P95_MS=1000`
- [ ] OpenAPI spec declares new endpoint + response fields + SSE events — verify: `make check-openapi-errors`

---

## Eval Commands (Post-Implementation Verification)

**Status:** PENDING

```bash
# E1: Unit tests
zig build test 2>&1 | tail -5 && echo "unit=$?"

# E2: Integration — memory bridge
make test-integration 2>&1 | grep -E "zombie_steer|run_interrupt_bridge" | tail -20

# E3: v1 regression — existing interrupt behavior preserved
make test-integration 2>&1 | grep run_interrupt_test | tail -10

# E4: Cross-workspace isolation regression
make test-integration 2>&1 | grep memory_isolation | tail -5

# E5: Leak detection
zig build test 2>&1 | grep -i "leak" | head -5
echo "E5: leak check (empty = pass)"

# E6: OpenAPI drift
make check-openapi-errors 2>&1 | tail -5

# E7: Build
zig build 2>&1 | tail -5; echo "build=$?"

# E8: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"

# E9: Lint + drain + 350L
make lint 2>&1 | grep -E "✓|FAIL"
make check-pg-drain 2>&1 | tail -3
git diff --name-only origin/main | grep -v '\.md$' | grep -v '^vendor/' | grep -v '_test\.' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines" }'

# E10: Gitleaks
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E11: Invariant-3 grep lint — no bypass of storeSteering for steering keys
grep -rn 'memory_store.*"steering:' src/ --include="*.zig" | grep -v 'steering_bridge\|zombie_memory'
echo "E11: bypass check (empty = pass)"

# E12: Benchmark — p95 under 1s
API_BENCH_MAX_P95_MS=1000 API_BENCH_URL=http://localhost:8080/v1/zombies/zom_bench:steer make bench
```

---

## Dead Code Sweep

**Status:** PENDING

N/A — this workstream is additive (new endpoint, new bridge module) plus an in-place modification of `run_interrupt_http.zig`. No files deleted. The v1 `interrupt` endpoint keeps its path; only its behavior extends. If the orphan sweep on any removed/renamed symbols comes up empty at VERIFY, note it explicitly and move on.

---

## Verification Evidence

**Status:** PENDING

Filled in during VERIFY phase.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration — new path | `make test-integration | grep zombie_steer` | | |
| Integration — v1 regression | `make test-integration | grep run_interrupt` | | |
| p95 benchmark | `make bench` | | |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | | |
| Lint | `make lint` | | |
| OpenAPI drift | `make check-openapi-errors` | | |
| Gitleaks | `gitleaks detect` | | |
| 350L gate | `wc -l` per modified file | | |
| Invariant-3 bypass grep | `grep -rn memory_store...steering...` | | |

---

## Out of Scope

- **UI for steering** — dashboard replay chat panel. Tracked separately as M24_002 (blocked on this workstream's Interfaces locking).
- **CLI `zombiectl zombie watch <id>`** — terminal streaming + chat bar. Tracked separately as M24_003.
- **Slack/Discord DM reply as steering channel** — routing inbound DMs to the steer endpoint. Deferred (weak UX win, adds webhook-parsing complexity). Possible M24_004 or later.
- **Rate limiting per workspace** — anti-abuse. Deferred until observed need; memory table growth is the natural signal.
- **Voice input** — v1 M21_001 §4.0 deferred; belongs with M24_002 UI spec, not here.
- **Per-scope retention policies** — e.g., `steering:session:*` with 24h TTL. Current workstream treats all steering as `category: "core"` (permanent until forgotten). Granular retention is future work.
- **Retroactive steering application to in-flight runs that are NOT the zombie's "active" execution** — e.g., if Ops Zombie has 3 parallel executions, we only inject into the one recorded in `zombie_sessions.execution_id`. Multi-execution fan-out is out of scope.
- **Conflict resolution when two operators steer with contradictory content** — last-write-wins is accepted; no merge UX in v1 of this workstream.
