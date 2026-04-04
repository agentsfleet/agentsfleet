# M25_001: Stripe-Style Keyset Pagination

**Prototype:** v1.0.0
**Milestone:** M25
**Workstream:** 001
**Date:** Apr 04, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — API consumers cannot page through large result sets; list endpoints silently truncate at LIMIT
**Batch:** B1
**Branch:** feat/m25-keyset-pagination
**Depends on:** None (scores handler is the reference implementation)

---

## Background

The `GET /v1/agents/{agent_id}/scores` handler already implements production-grade Stripe-style keyset pagination (`starting_after`, `has_more`, `next_cursor`). The remaining list endpoints either have no pagination or declare it in the OpenAPI spec without implementing it.

### Why keyset (not offset/page)?

| Pattern | Complexity | Stability | Performance |
|---------|-----------|-----------|-------------|
| **Offset/page** (`?page=3&per_page=50`) | Simple | Unstable — inserting/deleting rows shifts pages, causing duplicate or missed items | O(n) — DB must scan and discard `offset` rows |
| **Keyset cursor** (`?starting_after=uuid`) | Moderate | Stable — cursor is a row identity, immune to concurrent writes | O(1) — index seek directly to cursor position |

All primary keys are UUIDv7 (lexicographically ordered by creation time), making keyset pagination natural: `WHERE id < $cursor ORDER BY id DESC LIMIT $n`.

### Reference implementation

`src/http/handlers/agents/scores.zig` — proven pattern:
- Accepts `starting_after` (score_id of last item on previous page) and `limit` (default 50, max 100)
- Fetches `limit + 1` rows to detect `has_more` without a separate COUNT query
- Returns `{ data: [...], has_more: bool, next_cursor: string|null, request_id: uuid }`
- SQL: `WHERE score_id < $cursor ORDER BY score_id DESC LIMIT $n`

---

## 1.0 Standardize List Response Envelope

**Status:** PENDING

Define a consistent pagination response shape across all list endpoints. Every list response will use the same envelope:

```json
{
  "data": [ ... ],
  "has_more": true,
  "next_cursor": "01963a7f-...",
  "request_id": "..."
}
```

This replaces the current inconsistent patterns (`specs` array + `total` count, `runs` array + `total`, `data` array with no metadata).

**Dimensions:**
- 1.1 PENDING Define `PaginatedResponse` helper in `src/http/common.zig` that writes the envelope (data array, has_more bool, next_cursor nullable string, request_id)
- 1.2 PENDING Standardize query param parsing: `starting_after` (optional string, UUIDv7), `limit` (optional int, default 50, max 100, min 1)
- 1.3 PENDING Extract fetch-ahead pattern (query limit+1, slice to limit, derive has_more) into a reusable function

---

## 2.0 Migrate `GET /v1/specs` to Keyset Pagination

**Status:** PENDING

Currently: `WHERE workspace_id = $1 ORDER BY created_at DESC LIMIT $2` — no cursor, no status filter, uses `created_at` ordering (not PK).

Target: `WHERE workspace_id = $1 AND spec_id < $cursor ORDER BY spec_id DESC LIMIT $n` with `starting_after` and `status` filter support.

**Dimensions:**
- 2.1 PENDING Add `starting_after` and `status` query param parsing to `handleListSpecs`
- 2.2 PENDING Rewrite SQL to keyset cursor on `spec_id` (UUIDv7) with optional `AND status = $filter`
- 2.3 PENDING Return standardized envelope (`data`, `has_more`, `next_cursor`, `request_id`) — drop legacy `specs` key and `total` count
- 2.4 PENDING Integration test: page through 120 specs in chunks of 50, verify no duplicates or gaps

---

## 3.0 Migrate `GET /v1/runs` to Keyset Pagination

**Status:** PENDING

Currently: `WHERE workspace_id = $1 ORDER BY created_at DESC LIMIT $2` — same pattern as specs.

Target: keyset on `run_id` (UUIDv7) with `starting_after` support.

**Dimensions:**
- 3.1 PENDING Add `starting_after` query param parsing to runs list handler
- 3.2 PENDING Rewrite SQL to keyset cursor on `run_id`
- 3.3 PENDING Return standardized envelope — drop legacy `runs` key and `total` count
- 3.4 PENDING Integration test: page through runs, verify stable ordering under concurrent inserts

---

## 4.0 Align Scores Handler with Standard Envelope

**Status:** PENDING

The scores handler already implements correct keyset pagination but may need minor alignment with the shared helper from Section 1.0.

**Dimensions:**
- 4.1 PENDING Refactor scores handler to use the shared `PaginatedResponse` helper
- 4.2 PENDING Verify no behavioral change via existing tests

---

## 5.0 Update OpenAPI Spec

**Status:** PENDING

Update `public/openapi.json` to match the implemented pagination contract for all list endpoints.

**Dimensions:**
- 5.1 PENDING Add `starting_after` and `limit` query params to `list_specs`, `list_runs`
- 5.2 PENDING Add `has_more` (boolean) and `next_cursor` (string|null) to all list response schemas
- 5.3 PENDING Rename response array keys from `specs`/`runs` to `data` for consistency
- 5.4 PENDING Document the pagination pattern in the spec description (link to Stripe-style convention)

---

## 6.0 Acceptance Criteria

**Status:** PENDING

- [ ] 6.1 All list endpoints (`/v1/specs`, `/v1/runs`, `/v1/agents/{id}/scores`) use identical keyset cursor pattern
- [ ] 6.2 Response envelope is `{ data, has_more, next_cursor, request_id }` on all list endpoints
- [ ] 6.3 `starting_after` with an invalid/non-existent UUIDv7 returns 400
- [ ] 6.4 OpenAPI spec matches implementation — no phantom params or missing response fields
- [ ] 6.5 `make test` passes with pagination integration tests

---

## 7.0 Out of Scope

- `GET /v1/agents/{id}/proposals` — low cardinality, unbounded fetch is acceptable for now
- `GET /v1/admin/platform-keys` — admin-only, low cardinality
- Full-text search or filtering beyond `status` — separate milestone
- Backward-compatible `total` count field — keyset pagination intentionally omits total count (it requires a separate COUNT query which defeats the performance benefit)
