# M23_001: Zombie Steer — Live Chat Steering for Active Runs

**Prototype:** v2
**Milestone:** M23
**Workstream:** 001
**Date:** Apr 13, 2026: 10:55 PM
**Updated:** Apr 14, 2026
**Status:** DONE
**Branch:** feat/m23-001-zombie-steer
**Priority:** P1 — Operators cannot redirect a running zombie without killing it. Live chat steering closes the feedback loop for watched runs.
**Batch:** B1
**Depends on:**
- M18_001 (zombie execution telemetry / SSE stream — done) — `execution_id` is tracked in `core.zombie_sessions`.
- Queue constants `run:interrupt:` prefix (in place from M21_001 v1).

---

## Overview

**Goal (testable):** An operator sends `POST /v1/zombies/{id}:steer` with a message. If the zombie has an active run, the message is written to the Redis interrupt key `run:{execution_id}:interrupt` (TTL 300s) so the worker gate loop picks it up on the next checkpoint. The response always reports whether a run was steered. No memory persistence — corrections are run-scoped only.

**Scope (confirmed Apr 14):**
- ✅ Live steering only: inject message into the current active run via Redis
- ✅ Response indicates whether a run was actually steered
- ❌ No memory persistence across runs (M14_001 parked)
- ❌ No LLM scope inference
- ❌ No SSE event emission (CLI reads response ack directly)

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/http/handlers/zombie_steer_http.zig` | CREATE | New `POST /v1/zombies/{id}:steer` handler |
| `src/http/route_matchers.zig` | MODIFY | Add `matchZombieAction` helper |
| `src/http/router.zig` | MODIFY | Add `zombie_steer` route variant |
| `src/http/route_table.zig` | MODIFY | Register route with bearer middleware |
| `src/http/route_table_invoke.zig` | MODIFY | Add invoke shim |
| `public/openapi.json` | MODIFY | Declare new endpoint |

**Not changed:** schema (no new tables), memory module, worker/event_loop, SSE events.

---

## Applicable Rules

- **RULE FLS** — drain every pg query before deinit.
- **RULE FLL** — 350-line gate on every created/modified `.zig` file.
- **RULE NSQ** — schema-qualified SQL (`core.zombie_sessions`, `core.zombies`).

---

## §1 — `POST /v1/zombies/{id}:steer` Endpoint

**Status:** PENDING

Scoped to the zombie, not to a specific run. Looks up active `execution_id` from `core.zombie_sessions`. If found, writes message to Redis `run:{execution_id}:interrupt`. Returns ack.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `zombie_steer_http.zig` | `POST /v1/zombies/zom_abc:steer` with `{"message": "skip demo CTA"}` and valid bearer token | 200 `{"ack": true, "run_steered": false}` (no active run) | integration |
| 1.2 | PENDING | `zombie_steer_http.zig` | same but zombie has active `execution_id` in `zombie_sessions` | 200 `{"ack": true, "run_steered": true, "execution_id": "..."}` | integration |
| 1.3 | PENDING | `zombie_steer_http.zig` | POST with missing auth header | 401 | integration |
| 1.4 | PENDING | `zombie_steer_http.zig` | POST with zombie_id from different workspace | 404 (no existence leak) | integration |
| 1.5 | PENDING | `zombie_steer_http.zig` | POST with empty message | 400 | integration |
| 1.6 | PENDING | `zombie_steer_http.zig` | POST with message > 8192 bytes | 400 | integration |

---

## Interfaces

### Public Endpoint

```
POST /v1/zombies/{zombie_id}:steer
Headers: Authorization: Bearer {workspace_token}
Body: {
  "message": string (1..8192)
}
Response 200: {
  "ack": true,
  "run_steered": boolean,
  "execution_id": string | null
}
```

### Input Contracts

| Field | Type | Constraints |
|-------|------|-------------|
| `message` | string | 1–8192 chars, non-empty |
| `zombie_id` | path param | UUID format |

### Error Contracts

| Condition | Behavior |
|-----------|----------|
| Missing auth | 401 `UZ-AUTH-002` |
| Wrong workspace | 404 `UZ-ZOMBIE-NOT-FOUND` (no existence leak) |
| Empty message | 400 `UZ-REQ-001` |
| Message > 8192 | 400 `UZ-REQ-001` |
| No active run | 200 with `run_steered: false` (not an error) |
| Redis write fails | 200 with `run_steered: false`, logs warn |

---

## Execution Plan (Ordered)

| Step | Action | Verify |
|------|--------|--------|
| 1 | Add `matchZombieAction` to `route_matchers.zig` + `zombie_steer` route to `router.zig` | `zig build` compiles |
| 2 | Register in `route_table.zig` + `route_table_invoke.zig` | `zig build` compiles |
| 3 | Implement `zombie_steer_http.zig` handler | `zig build test` passes |
| 4 | Update `public/openapi.json` | `make check-openapi-errors` passes |
| 5 | Lint + cross-compile + 350L gate | all pass |

---

## Acceptance Criteria

- [ ] `POST /v1/zombies/{id}:steer` returns 200 with `run_steered: false` when zombie has no active run
- [ ] Returns 200 with `run_steered: true` and `execution_id` when zombie has active `zombie_sessions` row
- [ ] Redis key `run:{execution_id}:interrupt` is written with 300s TTL when active run found
- [ ] Returns 404 (not 403) for cross-workspace zombie
- [ ] Returns 401 for missing auth
- [ ] Returns 400 for empty or oversized message
- [ ] Handler file ≤ 350 lines

---

## Eval Commands

```bash
# E1: Build
zig build 2>&1 | tail -5; echo "build=$?"

# E2: Unit tests
zig build test 2>&1 | tail -5; echo "unit=$?"

# E3: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"

# E4: Lint + drain
make lint 2>&1 | grep -E "✓|FAIL"
make check-pg-drain 2>&1 | tail -3

# E5: OpenAPI
make check-openapi-errors 2>&1 | tail -5

# E6: 350L gate
git diff --name-only origin/main | grep '\.zig$' | grep -v '_test\.' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'
```

---

## Out of Scope

- Memory persistence across runs (M14_001 parked)
- LLM scope inference
- SSE event emission from steer endpoint
- v1 `/runs/{id}:interrupt` alias (not needed without memory bridge)
- UI chat panel (tracked separately as M24)

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Build | `zig build` | clean | ✅ |
| Unit tests | `zig build test` | exit=0 | ✅ |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | exit=0 | ✅ |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | exit=0 | ✅ |
| Lint | `make lint` | all ✓ | ✅ |
| OpenAPI drift | `make check-openapi-errors` | valid | ✅ |
| 350L gate | handler=189L router=315L invoke=326L | all <350 | ✅ |
