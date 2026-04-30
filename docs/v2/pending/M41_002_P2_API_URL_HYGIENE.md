# M41_002: API URL Hygiene — /steer rename + /memory resource collection

**Prototype:** v2.0.0
**Milestone:** M41
**Workstream:** 002
**Date:** Apr 30, 2026
**Status:** PENDING
**Priority:** P2 — REST §1 cleanup behind a body-shape design call; not user-blocking
**Categories:** API | CLI | UI
**Batch:** B1 — single workstream
**Depends on:** M41_001 (lands the `/pause`, `/complete`, `/kill` rename pass and the substrate match(method) refactor that this spec relies on)

**Canonical architecture:** `docs/architecture/README.md` (capabilities + data_flow). `docs/REST_API_DESIGN_GUIDELINES.md §1` is the canonical rule this spec enforces.

---

## Implementing agent — read these first

1. `scripts/check_openapi_url_shape.py` — `PENDING_RENAME_PATHS` carries the 5 entries this spec is scheduled to retire. The script's `VENDOR_PATH_CARVE_OUTS` set is the model for how vendor-immortal paths get classified separately. Read both.
2. `src/http/handlers/zombies/steer.zig` + `src/http/handlers/zombies/events.zig` — steer's current handler shape and the event resource it has to fold into.
3. `src/zombie/event_envelope.zig` — `EventType` is the discriminator that the polymorphic `/events` body has to dispatch on (chat | continuation | webhook | ...). The rename can't ship without a body-type design that survives that dispatch.
4. `src/http/handlers/memory/handler.zig` — current four-RPC surface (`store`, `recall`, `list`, `forget`). The reshape into `/v1/memories` needs a key-shape design call (compound key `{zombie_id, key}` — does the URL carry both? headers? mixed?).
5. `docs/v2/done/M41_001_P1_API_CONTEXT_LAYERING.md` — the parent spec. The substrate refactor (method-aware `router.match`) and the rename precedent (PATCH /workspaces/{id}, PATCH /auth/sessions/{id}, PATCH /zombies/{id} body status) live in M41_001.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE UFS (no inline literals), RULE NLG (pre-v2.0 no legacy framing), RULE NLR (touch-it-fix-it), RULE TST-NAM (milestone-free test names).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — §1 (URL shape — the rule this spec enforces), §7 (5-place route registration — every rename touches all 5), §8 (`Hx` handler contract).
- **`docs/ZIG_RULES.md`** — pg-drain discipline, errdefer ordering, file-as-struct vs conventional decision, length gates. Memory handler refactor will trip the FILE SHAPE gate.

---

## Overview

**Goal (testable):** After this spec lands, `python3 scripts/check_openapi_url_shape.py` reports `0 pre-§1 endpoints with pending-rename carve-outs` and `PENDING_RENAME_PATHS` in that script is the empty set (or deleted entirely).

**Problem:** Two REST §1 violations remain in `public/openapi.json` after M41_001:

1. `POST /v1/workspaces/{ws}/zombies/{id}/steer` — verb in URL. Storage already treats steer as one actor among many writing to `core.zombie_events`; the URL is the last layer that pretends steer is its own thing.
2. `/v1/memory/{forget,list,recall,store}` — four top-level RPCs predating §1. The right shape is a single `/v1/memories` collection with the four operations as `GET /memories`, `GET /memories/{key}`, `POST /memories`, `DELETE /memories/{key}` (or similar — the key-shape design call drives this).

**Solution summary:**

- Steer folds into `POST /v1/workspaces/{ws}/zombies/{id}/events` with a polymorphic body discriminated by `event_envelope.zig::EventType`. The handler dispatches on `body.type` (chat | webhook | continuation) and the steer flow corresponds to `type:"chat"` with `actor_kind:"steer"`. The Redis stream entry id remains the canonical event_id returned to the caller.
- Memory ops collapse into `/v1/memories` with the resource collection shape. The compound `{zombie_id, key}` identity needs a design call — `zombie_id` is naturally a header (the API key already binds it), `key` rides as the path param. The deferred design choice is: header-vs-path for `zombie_id`, and whether `recall` is `GET /memories/{key}` (single-key) or `GET /memories?keys=a,b` (multi-key — current `list` shape).

---

## Files Changed (blast radius — approximate)

> Counts approximated from M41_001's m41-rename-set.txt. The actual diff lands when this spec activates; counts here are for sizing.

### Steer rename (~14 files)

| File | Action | Why |
|------|--------|-----|
| `src/http/router.zig` | EDIT | Drop `workspace_zombie_steer` matcher; route `POST /events` body-dispatch |
| `src/http/route_table.zig` | EDIT | Drop `workspace_zombie_steer` arm |
| `src/http/route_table_invoke.zig` | EDIT | Drop `invokeZombieSteer`; existing `invokeWorkspaceZombieEvents` grows POST handling |
| `src/http/route_manifest.zig` | EDIT | Drop `/steer` row; `POST /events` already exists |
| `src/http/handlers/zombies/steer.zig` | DELETE | Logic folds into `events.zig` POST branch |
| `src/http/handlers/zombies/events.zig` | EDIT | Add POST handler with EventType body dispatch |
| `src/http/handlers/zombies/api.zig` | EDIT | Drop `innerZombieSteer` re-export |
| `src/main.zig` | EDIT | Drop steer.zig import |
| `public/openapi/paths/zombies.yaml` | EDIT | Remove `/steer` path entry; extend `POST /events` body schema with EventType discriminator |
| `public/openapi/root.yaml` | EDIT | Drop `/steer` $ref |
| `zombiectl/src/lib/api-paths.js` | EDIT | Drop `wsZombieSteerPath`; steer command uses `wsZombieEventsPath` with POST |
| `zombiectl/src/commands/zombie_steer.js` | EDIT | Switch to POST `/events` with body `{type:"chat", actor_kind:"steer", message:...}` |
| `zombiectl/test/zombie.unit.test.js` (or steer-specific test file) | EDIT | URL/method assertions |
| `scripts/check_openapi_url_shape.py` | EDIT | Drop steer from `PENDING_RENAME_PATHS` |

### Memory reshape (~8 files)

| File | Action | Why |
|------|--------|-----|
| `src/http/router.zig` | EDIT | Drop 4 memory_* variants; add `memories_collection`, `memory_by_key` |
| `src/http/route_table.zig` | EDIT | Replace 4 arms with 2 |
| `src/http/route_table_invoke.zig` | EDIT | Replace 4 invoke fns with 2 (or method-dispatched per resource) |
| `src/http/route_manifest.zig` | EDIT | Replace 4 rows with the new collection shape |
| `src/http/handlers/memory/handler.zig` | EDIT | Restructure four entry points into resource-shaped handlers |
| `public/openapi/paths/memory.yaml` | EDIT | Replace 4 path entries with the resource-collection shape |
| `public/openapi/root.yaml` | EDIT | Replace 4 $refs with 2 |
| `scripts/check_openapi_url_shape.py` | EDIT | Drop 4 memory entries from `PENDING_RENAME_PATHS` |

---

## Sections (implementation slices)

### §1 — Steer body-polymorphism design

The new `POST /v1/.../zombies/{id}/events` body must discriminate on `event_envelope.zig::EventType`. Implementation default: `body.type` is the discriminator string ("chat" | "continuation" | "webhook"); each variant carries its own field set; the parser uses the existing `EventEnvelope.fromJson` (or whatever the canonical parser is) and returns 400 for unknown types. The steer flow corresponds to `type:"chat"` with `actor_kind:"steer"` (or whatever shape `actor_kind` has in the existing storage layer — agent reads `core.zombie_events` schema).

### §2 — Steer rename + CLI migration

Once §1 lands, follow the M41_001 pattern: edit all 5 route registration places, regenerate OpenAPI, update `zombiectl/src/lib/api-paths.js`, update `zombiectl/src/commands/zombie_steer.js`. The router test updates are mechanical.

### §3 — Memory key-shape design call

The compound `{zombie_id, key}` identity needs a placement decision before any rename. Options:

- **Header + path:** `GET /v1/memories/{key}` with `X-Zombie-Id` header. Mirrors the existing API-key auth pattern that already binds zombie_id implicitly.
- **Path only:** `GET /v1/zombies/{zombie_id}/memories/{key}`. Symmetric with workspaces/zombies hierarchy.
- **Mixed:** path for zombie_id, query for key (multi-key recall).

Implementation default: workspace-scoped path per the existing convention — `GET /v1/workspaces/{ws}/zombies/{id}/memories` and `/memories/{key}`. Mirrors how every other zombie sub-resource lives. The agent confirms the call before §4.

### §4 — Memory rename

Apply §3's chosen shape across the 5-place registration. Recall, list, store, forget collapse into resource verbs (GET, GET/{key}, POST, DELETE/{key}).

### §5 — Carve-out cleanup

Empty `PENDING_RENAME_PATHS` in `scripts/check_openapi_url_shape.py`. Either delete the constant entirely (preferred — RULE NLG tracking-list ban applies) OR leave the empty set with a one-line comment that says "kept as a hook for the next rename pass; never populate without explicit user override per RULE NLG".

---

## Interfaces

### Steer → events POST

```
POST /v1/workspaces/{workspace_id}/zombies/{zombie_id}/events
Content-Type: application/json
Authorization: Bearer <user-jwt>

Body (discriminated by `type`):
  {"type": "chat", "actor_kind": "steer", "message": "..."}
  {"type": "continuation", ...}    — same shape as today's continuation enqueue
  {"type": "webhook", ...}         — currently behind /v1/webhooks/{id}; folds in if this spec extends scope

Response: 202 Accepted, body { event_id: <redis-stream-entry-id> }
```

### Memories collection (assuming §3's default workspace-scoped shape)

```
GET    /v1/workspaces/{ws}/zombies/{id}/memories?keys=a,b   — recall (multi-key, query-param)
GET    /v1/workspaces/{ws}/zombies/{id}/memories            — list
GET    /v1/workspaces/{ws}/zombies/{id}/memories/{key}      — recall single
POST   /v1/workspaces/{ws}/zombies/{id}/memories            — store (body: {key, value})
DELETE /v1/workspaces/{ws}/zombies/{id}/memories/{key}      — forget
```

The shape is locked once §3 is dispositioned; this section gets edited then.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Unknown `type` in events POST | Client sends `body.type:"foo"` | 400 ERR_INVALID_REQUEST with `unknown event type "foo"` |
| Missing `actor_kind` for chat events | Client sends `type:"chat"` without `actor_kind` | 400 — `actor_kind` is required for chat events |
| Memory key collision on POST | Duplicate `{zombie_id, key}` in store | Today's behavior: PUT-like (overwrite). Preserve unless §3 changes it. |
| Cross-workspace memory access | API key bound to ws_A reads memory of ws_B | 403 — same `authorizeWorkspace` gate as every other workspace resource |

---

## Invariants

1. The OpenAPI URL shape gate stays green: `python3 scripts/check_openapi_url_shape.py` exits 0 with `PENDING_RENAME_PATHS` empty (or deleted). Enforced by CI via `make openapi`.
2. Every steer call site in CLI/UI uses the `/events` POST shape — verified by grep of `wsZombieSteerPath` returning zero hits in `zombiectl/`, `ui/`, and tests.
3. Every memory call site uses the resource shape — `/v1/memory/store|recall|list|forget` returns zero hits across `src/`, `zombiectl/`, `ui/`, tests.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `events_post_chat_steer_resolves` | POST /events with `{type:"chat", actor_kind:"steer", message:"hi"}` returns 202 with an event_id; SSE consumer sees the frame in order. |
| `events_post_unknown_type_400` | POST /events with `{type:"foo"}` returns 400 ERR_INVALID_REQUEST. |
| `events_post_chat_missing_actor_kind_400` | POST /events with `{type:"chat", message:"x"}` (no actor_kind) returns 400. |
| `memories_collection_resolves` | After §4, the four operations work end-to-end against the new shape; old `/v1/memory/*` paths return 404. |
| `cross_ws_memory_isolation` | API key bound to ws_A cannot read memories of a zombie in ws_B (403). |
| `pending_rename_paths_empty_invariant` | Compile-time / script-runtime: `PENDING_RENAME_PATHS` set is empty (or the symbol is deleted entirely). |
| `regression_steer_legacy_404` | `POST /v1/.../zombies/{id}/steer` returns 404 (verb path retired). |
| `regression_memory_legacy_404` | `POST /v1/memory/store` (and the other three) returns 404. |

---

## Acceptance Criteria

- [ ] `python3 scripts/check_openapi_url_shape.py` reports `0 pre-§1 endpoints with pending-rename carve-outs` — verify: `python3 scripts/check_openapi_url_shape.py 2>&1 | grep -E '0 pre-§1'`
- [ ] `make openapi` clean (router ↔ openapi parity holds across the rename) — verify: `make openapi`
- [ ] `make lint && make test && make test-integration` clean — verify: those three commands
- [ ] `make memleak` clean (steer + memory handlers do allocator work) — verify: `make memleak`
- [ ] Cross-compile clean — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] Orphan sweep: zero references to `wsZombieSteerPath`, `memory_store|recall|list|forget`, `innerZombieSteer`, `innerMemoryStore|Recall|List|Forget` — verify: each `grep -rn` returns 0 in non-historical files

---

## Out of Scope

- Renaming `/v1/webhooks/{zombie_id}` and friends — webhooks are vendor-driven URL shapes (the secret rides as a path segment), classified separately. If a future hygiene pass reshapes them, that's a different spec.
- Vendor OAuth callbacks — `/v1/slack/{install,callback}`, `/v1/github/callback` are in `VENDOR_PATH_CARVE_OUTS` permanently.
- Steer continuations / webhook-as-event — body-type polymorphism opens the door to consolidating webhook ingest into `/events` too, but that's a M41_003 conversation, not this spec.
