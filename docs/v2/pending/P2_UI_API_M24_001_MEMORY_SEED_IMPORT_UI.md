# M24_001: Memory Seed + Import UI — Write and Import Memory Entries from the Dashboard

**Prototype:** v2
**Milestone:** M24
**Workstream:** 001
**Date:** Apr 13, 2026
**Status:** PENDING
**Priority:** P2 — Closes the "export → edit locally → CLI import" loop for non-CLI operators
**Batch:** B7 — after M14_003 (memory dashboard, read-only view)
**Branch:** feat/m24-memory-import-ui
**Depends on:** M14_003 (memory dashboard UI), M14_001 (memory backend, done), M14_005 (CLI export/import)

---

## Overview

**Goal (testable):** An operator setting up a fresh Blog Writer zombie can open the Memory tab, click "Add Entry", create a `pending_topics` entry with a list of topics in the body, save it, and on the next Tuesday run the zombie recalls those topics without any CLI interaction. An operator can also upload a zip of markdown entries (exported via `zombiectl memory export`) directly from the browser to import bulk memory, replacing the current `export → edit locally → CLI import` workflow for non-CLI users.

**Problem:** M14_003 ships the memory tab as read-only. Operators can see what the zombie knows but cannot add or correct knowledge from the dashboard. Seeding memory before a zombie's first run (role rubrics for the Hiring Agent, SLO definitions for the Ops Zombie, topic backlogs for the Blog Writer) currently requires the CLI: `zombiectl memory import`. Non-CLI operators have no self-serve path to give their zombie its initial knowledge. For archetypes like the Ops Zombie, seeding known noise patterns before going live is the difference between paging humans every night and silent suppression.

**Solution summary:** Extend M14_003's memory tab with write operations: (1) "Add Entry" button opening an inline editor with key, category, tags, and a markdown body field; (2) Inline edit of existing entries; (3) Delete with confirmation; (4) Bulk import via file upload (zip of markdown files from `zombiectl memory export`); (5) Quick-add for topic queues (one-line form for content zombies). The API surface extends M14_003's read-only endpoints with POST/PATCH/DELETE.

**DX paths:**

| Action | CLI (M14_005) | UI (this milestone) | API |
|---|---|---|---|
| Add entry | `zombiectl memory import --from ./dir/` | Add Entry inline editor | `POST /v1/zombies/{id}/memory` |
| Edit entry | export → edit file → `zombiectl memory import` | Inline edit in memory tab | `PUT /v1/zombies/{id}/memory/{key}` |
| Delete entry | `zombiectl memory forget --key {key}` | Delete button + confirm | `DELETE /v1/zombies/{id}/memory/{key}` |
| Bulk import | `zombiectl memory import --from ./zip/` | Upload zip in memory tab | `POST /v1/zombies/{id}/memory/import` |

---

## 1.0 Add Entry Inline Editor

**Status:** PENDING

"+ Add Entry" button on the Memory tab opens an inline panel below the entry list. Fields: key (slug, validated), category (core/daily), tags (comma-separated), body (markdown editor with preview toggle). Submit → POST to API → entry appears in list.

**Layout:**

```
Memory (47 entries)      [+ Add Entry]  [Import ▲]  [Export ▼]

+ Add Entry
┌──────────────────────────────────────────────────────┐
│ Key            [pending_topics               ]       │
│ Category       [core ▾]                              │
│ Tags           [queue, blog                  ]       │
│                                                      │
│ Body (markdown)                       [Preview]      │
│ ┌────────────────────────────────────────────────┐   │
│ │ - "How UseZombie handles credential isolation" │   │
│ │ - "The case for write-only vaults"             │   │
│ │ - "Zombie chaining: lead research pipelines"   │   │
│ └────────────────────────────────────────────────┘   │
│                                                      │
│            [Cancel]    [Save Entry →]                │
└──────────────────────────────────────────────────────┘
```

**Dimensions:**
- 1.1 PENDING
  - target: `app/zombies/[id]/memory/components/AddEntryPanel.tsx`
  - input: user fills key="pending_topics", category=core, tags="queue", body with 3 topics, clicks Save
  - expected: `POST /v1/zombies/{id}/memory` with entry payload → entry appears at top of list — success toast
  - test_type: integration (API mock)
- 1.2 PENDING
  - target: `app/zombies/[id]/memory/components/AddEntryPanel.tsx`
  - input: user submits with empty key
  - expected: validation "Key is required" — Save blocked
  - test_type: unit (component test)
- 1.3 PENDING
  - target: `app/zombies/[id]/memory/components/AddEntryPanel.tsx`
  - input: user submits with key that already exists
  - expected: API returns 409; toast "Entry 'pending_topics' already exists — use edit to update it"
  - test_type: unit (component test)
- 1.4 PENDING
  - target: `app/zombies/[id]/memory/components/AddEntryPanel.tsx`
  - input: user clicks Preview toggle
  - expected: markdown body renders as formatted HTML with frontmatter shown as chips
  - test_type: unit (component test)

---

## 2.0 Inline Edit and Delete

**Status:** PENDING

Clicking an existing entry in the list expands it (from M14_003 §1.3). The expanded view now includes an Edit button that converts the body to an editable textarea and the metadata to editable fields. A Delete button opens a confirmation.

**Dimensions:**
- 2.1 PENDING
  - target: `app/zombies/[id]/memory/components/EntryDetail.tsx`
  - input: user clicks Edit on "lead_acme_corp" entry, changes body, clicks Save
  - expected: `PUT /v1/zombies/{id}/memory/lead_acme_corp` with updated body — entry refreshes in list
  - test_type: integration (API mock)
- 2.2 PENDING
  - target: `app/zombies/[id]/memory/components/EntryDetail.tsx`
  - input: user clicks Delete on "stale_entry"
  - expected: confirmation "Delete 'stale_entry'? This cannot be undone." → `DELETE /v1/zombies/{id}/memory/stale_entry` → entry removed from list
  - test_type: integration (API mock)
- 2.3 PENDING
  - target: `app/zombies/[id]/memory/components/EntryDetail.tsx`
  - input: user edits an entry and clicks Cancel
  - expected: original content restored, no API call
  - test_type: unit (component test)

---

## 3.0 Bulk Import (Zip Upload)

**Status:** PENDING

Operators export memory via `zombiectl memory export` or the M14_003 "Export to laptop" button, edit the markdown files locally, then re-import. This panel closes the loop by accepting a zip file directly from the browser.

**Layout:**

```
Import Memory Entries

Upload a zip of markdown files (same format as Export).
Entries will be upserted — existing keys are overwritten.

[ Drag & drop or click to select .zip ]

Preview (after file selected):
47 entries to import
  core/pending_topics.md     — UPSERT (exists)
  core/lead_acme_corp.md     — NEW
  daily/followup_acme.md     — NEW
  3 entries with mismatched zombie_id — SKIPPED
  1 entry with invalid frontmatter — SKIPPED

[Cancel]    [Import 44 entries →]
```

**Dimensions:**
- 3.1 PENDING
  - target: `app/zombies/[id]/memory/components/ImportPanel.tsx`
  - input: user uploads a zip with 47 markdown entries (3 with mismatched zombie_id)
  - expected: preview shows 44 valid entries + 3 skipped (with reason); Import button enabled
  - test_type: unit (component test)
- 3.2 PENDING
  - target: `app/zombies/[id]/memory/components/ImportPanel.tsx`
  - input: user confirms import of 44 entries
  - expected: `POST /v1/zombies/{id}/memory/import` (multipart/form-data, zip file) → `{ upserted: 44, rejected: 3 }` — success toast
  - test_type: integration (API mock)
- 3.3 PENDING
  - target: `POST /v1/zombies/{id}/memory/import` (backend)
  - input: zip with 100 entries — all valid
  - expected: all upserted in a single transaction; if any fail → ROLLBACK all
  - test_type: integration (DB)
- 3.4 PENDING
  - target: `POST /v1/zombies/{id}/memory/import` (backend)
  - input: zip entry with `zombie_id` in frontmatter that doesn't match URL param
  - expected: entry skipped (not rejected globally); `{ rejected: 1, reason: "zombie_id mismatch" }`
  - test_type: integration (DB)

---

## 4.0 Quick-Add for Topic Queues

**Status:** PENDING

Content zombies (Blog Writer, Lead Opportunist) maintain a topic queue as a core memory entry. A quick-add strip at the top of the memory tab (when the filter shows `tags=queue`) lets operators add items without opening the full editor.

**Layout:**

```
Memory — filtered: tag=queue (3 entries)

[+ Add topic: _______________________________ ] [Add]

pending_topics (core) · queue
  • "How UseZombie handles credential isolation"
  • "The case for write-only vaults"
  [Edit]
```

**Dimensions:**
- 4.1 PENDING
  - target: `app/zombies/[id]/memory/components/QuickAddTopic.tsx`
  - input: operator types topic text and clicks Add
  - expected: topic appended to the existing `pending_topics` entry body; `PUT /v1/zombies/{id}/memory/pending_topics` called — entry refreshes
  - test_type: integration (API mock)
- 4.2 PENDING
  - target: quick-add visibility
  - input: memory tab with no `queue`-tagged entries
  - expected: quick-add strip not shown (archetype doesn't use topic queues)
  - test_type: unit (component test)

---

## 5.0 Interfaces

**Status:** PENDING

### 5.1 New API Endpoints (extending M14_003 read-only)

```
POST   /v1/zombies/{id}/memory              — create entry (error if key exists)
PUT    /v1/zombies/{id}/memory/{key}        — upsert (create or update)
DELETE /v1/zombies/{id}/memory/{key}        — delete entry
POST   /v1/zombies/{id}/memory/import       — bulk import (multipart/form-data, zip)
```

### 5.2 Existing Endpoints Unchanged (M14_003)

```
GET  /v1/zombies/{id}/memory                — list (read-only, no change)
GET  /v1/zombies/{id}/memory/{key}          — get single (no change)
GET  /v1/zombies/{id}/memory/export         — export zip (no change)
```

### 5.3 Error Contracts

| Error condition | Code | HTTP |
|---|---|---|
| Key already exists on POST | `UZ-MEM-003` | 409 |
| Key not found on DELETE | `UZ-MEM-NOT-FOUND` | 404 |
| zombie_id mismatch in import | skipped per-entry (not 4xx) | — |
| Import transaction failure | `UZ-MEM-004` | 500 + ROLLBACK |

---

## 6.0 Implementation Constraints

| Constraint | How to verify |
|---|---|
| Bulk import uses single transaction; partial failure rolls back | Dim 3.3 |
| No memory value stored in browser localStorage/sessionStorage | grep |
| zombie_id mismatch entries skipped without failing whole import | Dim 3.4 |
| Each component < 400 lines | `wc -l` |

---

## 7.0 Execution Plan

| Step | Action | Verify |
|---|---|---|
| 1 | Backend: `POST /v1/zombies/{id}/memory` + `PUT` + `DELETE` handlers | dims 1.1, 2.1, 2.2 |
| 2 | Backend: `POST /v1/zombies/{id}/memory/import` (zip parse + transaction) | dims 3.3, 3.4 |
| 3 | Add Entry inline editor | dims 1.1–1.4 |
| 4 | Inline edit + delete on existing entries | dims 2.1–2.3 |
| 5 | Bulk import panel | dims 3.1–3.2 |
| 6 | Quick-add topic strip | dims 4.1–4.2 |
| 7 | Cross-compile (Zig) + full test gate | all dims |

---

## 8.0 Acceptance Criteria

- [ ] Add entry from dashboard → zombie recalls on next run — verify: integration test (API mock + memory recall)
- [ ] Edit existing entry updates content — verify: dim 2.1
- [ ] Bulk import: 44/47 valid → 44 upserted — verify: dim 3.2
- [ ] Import transaction rolls back on failure — verify: dim 3.3
- [ ] Quick-add topic appends to queue entry — verify: dim 4.1

---

## Applicable Rules

RULE FLL, RULE FLS, RULE XCC, RULE TXN (import transaction), RULE CTX (SET ROLE memory_runtime for all memory ops).

---

## Eval Commands

```bash
zig build 2>&1 | head -5
make test 2>&1 | tail -5
make test-integration 2>&1 | grep -i memory | tail -10
npm run build 2>&1 | head -5
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3
make check-pg-drain 2>&1 | tail -3
```

---

## Out of Scope

- Memory editing on mobile (desktop-primary for V1)
- Merge conflict resolution (last-write-wins on upsert for V1)
- Memory versioning / history (entries are overwritten; git-based history via export is the workaround)
- Drag-and-drop reordering of entries (no ordered list concept in memory model)

---

**Note (M24 numbering):** M24_001 is already allocated to "Zombie Steer + Memory Bridge" (P1 API backend, created Apr 13 2026 earlier same day). This spec is M24_002, the UI layer that builds on the same M14 memory primitives. The two workstreams are complementary: M24_001 enables durable coaching/steering, M24_002 enables operator-driven seeding and editing from the browser.
