# M14_003: Memory Dashboard View — Operators See What Their Zombie Knows

**Prototype:** v2
**Milestone:** M14
**Workstream:** 003
**Date:** Apr 12, 2026: 03:50 PM
**Status:** PENDING
**Priority:** P2 — Without this, users don't trust memory; with it, memory feels tangible
**Batch:** B2
**Depends on:** M14_001 (export/import tool), M12_001 (app dashboard base)
**Extended by:** M24_001 (B7) — adds write operations to this read-only tab: inline add/edit/delete entries, bulk zip import with preview. M14_003 ships the read surface; M24_001 closes the write loop without needing CLI export-then-edit.

---

## Overview

**Goal (testable):** An operator viewing a zombie's page in the dashboard sees a "Memory" tab that lists all `core` and `daily` entries for that zombie rendered from the markdown export, can click into any entry to view the full body, can trigger "Export to laptop" (downloads a zip of the markdown), and sees a last-updated timestamp that matches the zombie's most recent `memory_store` tool call.

**Problem:** M14_001 ships storage + CLI export/import. But most operators don't live in a terminal. Memory that's only reachable via `zombiectl memory export` is a feature users don't experience. Without a dashboard surface, the product feels like a black box — "the zombie says it remembers but I can't see what." Trust is the blocker.

**Solution summary:** Add a "Memory" tab on the zombie dashboard page. Server-side: a read-only endpoint that lists and fetches memory entries scoped to the operator's workspace. Client-side: a list + detail pane rendering markdown-formatted entries. Download-as-zip triggers the same export path as the CLI. Editing is out-of-scope for this workstream (operators still use CLI for edit-then-replay to preserve the "review locally, then import" discipline).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/http/handlers/memory_http.zig` | MODIFY | Add read-only `GET /v1/zombies/:id/memory` and `GET /v1/zombies/:id/memory/:key` |
| `src/http/handlers/memory_http.zig` | MODIFY | Add `GET /v1/zombies/:id/memory/export` returning zip |
| `public/openapi.json` | MODIFY | Declare the new read endpoints |
| `app/src/routes/zombies/[id]/memory/+page.svelte` | CREATE | List + detail UI for memory entries |
| `app/src/lib/api/memory.ts` | CREATE | Typed client for memory endpoints |
| `zombiectl/src/commands/memory.js` | MODIFY | Ensure dashboard export path uses the same markdown format as CLI |

---

## Applicable Rules

- **RULE FLL** — 350-line gate on every touched file
- **RULE EP4** — no endpoints removed; new endpoints follow the envelope convention
- **RULE FLS** — drain pg queries in the list and fetch paths

---

## §1 — Read-Only Memory API

**Status:** PENDING

Endpoints for list + fetch + export. Read-only — editing still flows through CLI.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `GET /v1/zombies/:id/memory?category=core&tag=warm` | operator token, zombie in their workspace | paginated list of entries with key, category, tags, updated_at, content_preview (first 200 chars) | integration |
| 1.2 | PENDING | `GET /v1/zombies/:id/memory/:key` | operator token | full entry with body | integration |
| 1.3 | PENDING | `GET /v1/zombies/:id/memory/export` | operator token | zip stream with markdown files (same format as CLI export) | integration |
| 1.4 | PENDING | scope enforcement | operator A requesting zombie belonging to workspace B | 403 `UZ-MEM-SCOPE` | integration |

---

## §2 — Dashboard UI

**Status:** PENDING

List pane + detail pane + export button. Markdown rendered with syntax
highlighting on frontmatter.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | Memory tab on zombie page | zombie with 47 memory entries | list pane renders 47 items, shows key, tags, updated_at, content preview | e2e |
| 2.2 | PENDING | entry detail pane | click an entry | body renders as markdown with frontmatter as structured chips | e2e |
| 2.3 | PENDING | category + tag filter | select "core" + tag "warm" | list filters to matching entries; URL reflects filter | e2e |
| 2.4 | PENDING | export button | click "Export to laptop" | browser downloads zip named `zombie-{id}-memory-{date}.zip` | e2e |

---

## Interfaces

**Status:** PENDING

### Public HTTP Endpoints

```
GET  /v1/zombies/:id/memory?category=&tag=&limit=&cursor=
GET  /v1/zombies/:id/memory/:key
GET  /v1/zombies/:id/memory/export
```

### Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `:id` | string | valid zombie_id in caller's workspace | `zom_01JQ...` |
| `category` | string | optional; one of core/daily | `core` |
| `tag` | string | optional; repeatable | `warm` |
| `limit` | int | 1-100, default 50 | `50` |
| `cursor` | string | opaque pagination cursor | — |

### Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Zombie not in caller workspace | 403 | `UZ-MEM-SCOPE` |
| Key not found | 404 | `UZ-MEM-NOT-FOUND` |
| Memory backend unreachable | 503 | `UZ-MEM-UNAVAILABLE` |

---

## Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Memory backend down | Memory Postgres unreachable | 503 with retry-after | Dashboard shows "memory unavailable" banner |
| Very large memory (10k+ entries) | Zombie accumulated a lot | Pagination engages; default 50/page | Smooth scroll, no hang |
| Export of very large memory | Same | Zip streamed server-side | Progress indicator in UI |

---

## Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| No new Zig file > 350 lines | `wc -l` |
| No new Svelte component > 300 lines | `wc -l` |
| Export endpoint streams (no full buffer) | Integration test asserts response Transfer-Encoding: chunked |

---

## Invariants

**Status:** PENDING

N/A — UI concerns only.

---

## Test Specification

**Status:** PENDING

### Integration Tests

| Test name | Dim | Infra | Input | Expected |
|-----------|-----|-------|-------|----------|
| `memory_list_scoped` | 1.1 | memory DB, http | list for zombie in workspace | 200 with entries |
| `memory_list_cross_workspace_403` | 1.4 | memory DB, http | list for zombie in other workspace | 403 |
| `memory_export_zip_stream` | 1.3 | memory DB, http | export for zombie with 100 entries | zip streamed, chunked encoding |

### E2E Tests

| Test name | Dim | Infra | Input | Expected |
|-----------|-----|-------|-------|----------|
| `memory_tab_renders` | 2.1 | browser + fixture zombie | navigate to zombie page, click Memory tab | 47 entries listed |
| `entry_detail_markdown` | 2.2 | browser | click an entry | markdown rendered |
| `export_download` | 2.4 | browser | click Export to laptop | zip downloaded with expected name |

### Spec-Claim Tracing

| Spec claim | Test that proves it | Test type |
|-----------|-------------------|-----------|
| Operators can see memory in dashboard | `memory_tab_renders` | e2e |
| Scope enforced across workspace boundary | `memory_list_cross_workspace_403` | integration |
| Export format matches CLI | roundtrip with `zombiectl memory import` | integration |

---

## Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Read-only list + fetch endpoints | `memory_list_scoped` passes |
| 2 | Scope enforcement on workspace boundary | `memory_list_cross_workspace_403` passes |
| 3 | Export zip endpoint streams | `memory_export_zip_stream` passes |
| 4 | OpenAPI updates | `make check-openapi-errors` passes |
| 5 | Memory tab UI (list + detail) | `memory_tab_renders` passes |
| 6 | Export button wiring | `export_download` passes |
| 7 | Full gate | Eval block PASS |

---

## Acceptance Criteria

**Status:** PENDING

- [ ] Memory tab renders on zombie page — verify: e2e `memory_tab_renders`
- [ ] Scope enforced at API — verify: integration `memory_list_cross_workspace_403`
- [ ] Export from dashboard matches CLI export format (roundtrip) — verify: integration test
- [ ] No dashboard load > 1s for zombies with 500 entries — verify: performance test
- [ ] OpenAPI reflects new endpoints — verify: `make check-openapi-errors`

---

## Eval Commands

**Status:** PENDING

```bash
make test 2>&1 | tail -5
make test-integration 2>&1 | grep memory | tail -10
make check-openapi-errors
cd app && npm test 2>&1 | tail -5
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

**Status:** PENDING

N/A — additive only.

---

## Verification Evidence

**Status:** PENDING

Filled in during VERIFY.

---

## Out of Scope

- In-dashboard editing of memory entries (preserve the "review locally, then import" discipline)
- Memory search / full-text query in UI
- Sharing a zombie's memory view with another operator (auth model not ready)
- Memory audit trail (who edited what when) — separate compliance workstream
