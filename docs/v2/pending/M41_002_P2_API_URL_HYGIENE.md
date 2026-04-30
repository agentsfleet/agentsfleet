# M41_002: API URL Hygiene — /steer → /messages + /memory resource collection

**Prototype:** v2.0.0
**Milestone:** M41
**Workstream:** 002
**Date:** Apr 30, 2026
**Status:** PENDING
**Priority:** P2 — REST §1 cleanup; not user-blocking
**Categories:** API | CLI | UI
**Batch:** B1 — single workstream
**Depends on:** M41_001 (lands the `/pause`, `/complete`, `/kill` rename pass and the substrate `match(method)` refactor that this spec relies on)

**Canonical architecture:** `docs/architecture/README.md` (capabilities + data_flow). `docs/REST_API_DESIGN_GUIDELINES.md §1` is the canonical rule this spec enforces.

---

## Implementing agent — read these first

1. `scripts/check_openapi_url_shape.py` — `PENDING_RENAME_PATHS` carries the 5 entries this spec is scheduled to retire. The script's `VENDOR_PATH_CARVE_OUTS` set is the model for how vendor-immortal paths get classified separately. Read both.
2. `src/http/handlers/zombies/steer.zig` — current handler shape; renames to `messages.zig`. Storage write to `core.zombie_events` is unchanged.
3. `src/http/handlers/memory/handler.zig` — current four-RPC surface (`store`, `recall`, `list`, `forget`). The reshape into a workspace-scoped `/memories` collection is mechanical once the path shape is locked (it is — see §3).
4. `docs/v2/done/M41_001_P1_API_CONTEXT_LAYERING.md` — the parent spec. The substrate refactor (method-aware `router.match`) and the rename precedent (PATCH /workspaces/{id}, PATCH /auth/sessions/{id}, PATCH /zombies/{id} body status) live in M41_001.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE UFS (no inline literals), RULE NLG (pre-v2.0 no legacy framing — this spec deletes `PENDING_RENAME_PATHS` per the tracking-list ban), RULE NLR (touch-it-fix-it), RULE TST-NAM (milestone-free test names).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — §1 (URL shape — the rule this spec enforces), §7 (5-place route registration — every rename touches all 5), §8 (`Hx` handler contract).
- **`docs/ZIG_RULES.md`** — pg-drain discipline, errdefer ordering, file-as-struct vs conventional decision, length gates. Memory handler refactor will trip the FILE SHAPE gate.

---

## Overview

**Goal (testable):** After this spec lands, `python3 scripts/check_openapi_url_shape.py` reports `0 pre-§1 endpoints with pending-rename carve-outs` and `PENDING_RENAME_PATHS` in that script is deleted entirely (along with the carve-out function that consumes it).

**Problem:** Two REST §1 violations remain in `public/openapi.json` after M41_001:

1. `POST /v1/workspaces/{ws}/zombies/{id}/steer` — verb in URL.
2. `/v1/memory/{forget,list,recall,store}` — four top-level RPCs predating §1.

**Solution summary:**

- Steer becomes its own noun: `POST /v1/workspaces/{ws}/zombies/{id}/messages` body `{content: "..."}`. Mono-shaped handler, mono-shaped OpenAPI body, no discriminator. Storage still writes to `core.zombie_events` with `actor_kind:"steer"` / `event_type:"chat"` — that's a substrate detail the URL doesn't leak. The Redis stream entry id remains the canonical event_id returned to the caller.
- Memory ops collapse into workspace-scoped `/v1/workspaces/{ws}/zombies/{id}/memories` (symmetric with every other zombie sub-resource). Operations: `GET` (list / multi-key via `?keys=`), `GET /{key}` (single recall), `POST` (store, body `{key, value}`), `DELETE /{key}` (forget).

---

## Design Notes

An earlier draft proposed folding steer into `POST /events` with a polymorphic body discriminated by `EventType`. Rejected for three reasons:

1. **Substrate leak.** `events` is the `core.zombie_events` row name. Public URLs shouldn't expose storage substrate. Chat-from-user, system continuation, and vendor webhook share a row only because the substrate is a unified event log — they don't share auth, validation, body shape, or response semantics.
2. **Fat handler.** One endpoint dispatching three flows means branch-per-type validation and per-type 4xx surface. Splitting by noun keeps each handler narrow.
3. **Tooling friction.** Body-discriminated unions in OpenAPI generate weak types in `zombiectl` and the UI client.

`POST /messages` is the standard chat-shaped noun (Anthropic, OpenAI, Slack all use it). If a unified read-feed becomes useful later, `GET /zombies/{id}/events` can aggregate over the substrate read-only — read aggregation does not require write aggregation.

Continuations and webhooks stay out of this spec (see Out of Scope).

---

## Files Changed (blast radius — approximate)

> Counts approximated from M41_001's m41-rename-set.txt. The actual diff lands when this spec activates; counts here are for sizing.

### Steer → messages rename (~13 files)

| File | Action | Why |
|------|--------|-----|
| `src/http/router.zig` | EDIT | Drop `workspace_zombie_steer` matcher; add `workspace_zombie_messages` (POST) |
| `src/http/route_table.zig` | EDIT | Replace steer arm with messages arm |
| `src/http/route_table_invoke.zig` | EDIT | Replace `invokeZombieSteer` with `invokeZombieMessagesPost` |
| `src/http/route_manifest.zig` | EDIT | `/steer` row → `POST /messages` row |
| `src/http/handlers/zombies/steer.zig` | RENAME → `messages.zig` | Same logic, renamed handler; storage write unchanged |
| `src/http/handlers/zombies/api.zig` | EDIT | Re-export rename (`innerZombieSteer` → `innerZombieMessagesPost`) |
| `src/main.zig` | EDIT | Import path rename |
| `public/openapi/paths/zombies.yaml` | EDIT | Replace `/steer` path entry with `/messages` (POST only); body `{content: string}` |
| `public/openapi/root.yaml` | EDIT | `/steer` $ref → `/messages` $ref |
| `zombiectl/src/lib/api-paths.js` | EDIT | `wsZombieSteerPath` → `wsZombieMessagesPath` |
| `zombiectl/src/commands/zombie_steer.js` | EDIT | Switch to `POST /messages` body `{content}`; CLI subcommand name stays `zombie steer` (UX surface) |
| `zombiectl/test/zombie.unit.test.js` (or steer-specific test file) | EDIT | URL/method assertions |
| `scripts/check_openapi_url_shape.py` | EDIT | Drop steer from `PENDING_RENAME_PATHS` (and delete the constant + carve-out fn at §5) |

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

### §1 — Steer → messages rename

Pure rename on the existing steer handler. No body-shape design call needed: body is `{content: string}` (whatever the current steer body carries — agent confirms shape during PLAN by reading `src/http/handlers/zombies/steer.zig`). The storage layer (`core.zombie_events` write with `actor_kind:"steer"`) is unchanged. Handler renames `steer.zig` → `messages.zig`, function `innerZombieSteer` → `innerZombieMessagesPost`. 5-place route registration applied per M41_001 pattern.

### §2 — CLI migration

`zombiectl/src/lib/api-paths.js` rename + `zombiectl/src/commands/zombie_steer.js` URL/method swap. The user-facing CLI command `zombie steer` stays — it's a verb in CLI UX surface (where verbs are fine), and renaming it is a separate UX call out of scope here.

### §3 — Memory key shape

Workspace-scoped path locked. No design call deferred — symmetric with every other zombie sub-resource (`/v1/workspaces/{ws}/zombies/{id}/...`). Compound `{zombie_id, key}` identity rides on the path.

### §4 — Memory rename

Apply §3's shape across the 5-place registration. Recall, list, store, forget collapse into resource verbs:

- `GET /v1/workspaces/{ws}/zombies/{id}/memories` — list
- `GET /v1/workspaces/{ws}/zombies/{id}/memories?keys=a,b` — multi-key recall
- `GET /v1/workspaces/{ws}/zombies/{id}/memories/{key}` — single recall
- `POST /v1/workspaces/{ws}/zombies/{id}/memories` body `{key, value}` — store
- `DELETE /v1/workspaces/{ws}/zombies/{id}/memories/{key}` — forget

### §5 — Carve-out cleanup

Delete `PENDING_RENAME_PATHS` and the carve-out function consuming it (RULE NLG tracking-list ban). The constant exists only to legitimize deferral; with this spec there is nothing left to defer.

---

## Interfaces

### Steer → /messages POST

```
POST /v1/workspaces/{workspace_id}/zombies/{zombie_id}/messages
Content-Type: application/json
Authorization: Bearer <user-jwt>

Body: { "content": "..." }

Response: 202 Accepted, body { event_id: <redis-stream-entry-id> }
```

### Memories collection

```
GET    /v1/workspaces/{ws}/zombies/{id}/memories                — list
GET    /v1/workspaces/{ws}/zombies/{id}/memories?keys=a,b       — multi-key recall
GET    /v1/workspaces/{ws}/zombies/{id}/memories/{key}          — single recall
POST   /v1/workspaces/{ws}/zombies/{id}/memories                — store (body: {key, value})
DELETE /v1/workspaces/{ws}/zombies/{id}/memories/{key}          — forget
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Missing/empty `content` on POST /messages | Client POSTs `{}` or `{"content":""}` | 400 ERR_INVALID_REQUEST — `content is required` |
| Cross-workspace message | API key bound to ws_A targets zombie in ws_B | 403 — existing `authorizeWorkspace` gate |
| Memory key collision on POST | Duplicate `{zombie_id, key}` in store | Today's behavior: PUT-like (overwrite). Preserved. |
| Cross-workspace memory access | API key bound to ws_A reads memory of ws_B | 403 — same `authorizeWorkspace` gate as every other workspace resource |

---

## Invariants

1. The OpenAPI URL shape gate stays green: `python3 scripts/check_openapi_url_shape.py` exits 0; `PENDING_RENAME_PATHS` symbol does not exist in the script. Enforced by CI via `make openapi`.
2. Every steer call site in CLI/UI uses the `/messages` POST shape — verified by grep of `wsZombieSteerPath` returning zero hits in `zombiectl/`, `ui/`, and tests.
3. Every memory call site uses the resource shape — `/v1/memory/store|recall|list|forget` returns zero hits across `src/`, `zombiectl/`, `ui/`, tests.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `messages_post_resolves` | POST /messages with `{content:"hi"}` returns 202 with an event_id; SSE consumer sees the frame in order. |
| `messages_post_missing_content_400` | POST /messages with `{}` returns 400 ERR_INVALID_REQUEST. |
| `messages_post_empty_content_400` | POST /messages with `{"content":""}` returns 400. |
| `messages_post_cross_workspace_403` | API key bound to ws_A targeting a zombie in ws_B returns 403. |
| `memories_collection_resolves` | After §4, the four operations work end-to-end against the new shape; old `/v1/memory/*` paths return 404. |
| `cross_ws_memory_isolation` | API key bound to ws_A cannot read memories of a zombie in ws_B (403). |
| `pending_rename_paths_deleted_invariant` | `PENDING_RENAME_PATHS` symbol does not exist in `scripts/check_openapi_url_shape.py`. |
| `regression_steer_legacy_404` | `POST /v1/.../zombies/{id}/steer` returns 404 (verb path retired). |
| `regression_memory_legacy_404` | `POST /v1/memory/store` (and the other three) returns 404. |

---

## Acceptance Criteria

- [ ] `python3 scripts/check_openapi_url_shape.py` reports `0 pre-§1 endpoints with pending-rename carve-outs` — verify: `python3 scripts/check_openapi_url_shape.py 2>&1 | grep -E '0 pre-§1'`
- [ ] `make openapi` clean (router ↔ openapi parity holds across the rename) — verify: `make openapi`
- [ ] `make lint && make test && make test-integration` clean — verify: those three commands
- [ ] `make memleak` clean (messages + memory handlers do allocator work) — verify: `make memleak`
- [ ] Cross-compile clean — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] Orphan sweep: zero references to `wsZombieSteerPath`, `memory_store|recall|list|forget`, `innerZombieSteer`, `innerMemoryStore|Recall|List|Forget` — verify: each `grep -rn` returns 0 in non-historical files

---

## Out of Scope

- Renaming `/v1/webhooks/{zombie_id}` and friends — webhooks are vendor-driven URL shapes (the secret rides as a path segment), classified separately.
- Vendor OAuth callbacks — `/v1/slack/{install,callback}`, `/v1/github/callback` are in `VENDOR_PATH_CARVE_OUTS` permanently.
- `POST /events` polymorphic body — rejected, see Design Notes.
- Continuation endpoint design — likely a state transition on the zombie (`PATCH /zombies/{id}`) rather than its own resource. Out of scope until a consumer needs it.
- `GET /messages` listing — read-side of the messages collection. Defer until a consumer needs it; today's read path is the SSE event stream.
- CLI subcommand rename — `zombie steer` stays as the CLI verb-surface noun; renaming the user-facing command is a UX call outside REST hygiene.
