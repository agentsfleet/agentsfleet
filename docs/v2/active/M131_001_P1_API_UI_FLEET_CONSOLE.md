<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M131_001: The Fleet Console — one page that says what a fleet is, does, knows, and costs

**Prototype:** v2.0.0
**Milestone:** M131
**Workstream:** 001
**Date:** Jul 14, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the fleet detail page today is a stack of unlabelled panels with no source view, no cost figure, and a config card that lies about which endpoints exist; it is the page an operator lives on and it cannot answer the four questions it exists to answer.
**Categories:** API, UI
**Batch:** B1 — first of the console/wall trio; M132 and M133 build on the surfaces this lands.
**Branch:** feat/m131-fleet-console
**Test Baseline:** unit=2642 integration=334
**Depends on:** M130_001 (shipped the catalog surface and the `CopyButton` / `EventsList` primitives this console reuses; already in `done/`)
**Provenance:** Large Language Model (LLM)-drafted (claude-opus-4-8, Jul 14, 2026) — authored from the frozen variant-F design (`designs/fleet-dashboard-20260714/FREEZE.md`) and its panel→route→handler→schema vetting matrix, cross-checked against a live read of `route_table_invoke.zig`, `fleet_events_store.zig`, `memory/handler.zig`, and the console `page.tsx`.
**Canonical architecture:** `docs/architecture/memory.md` and `docs/architecture/fleet_bundles.md`

## Overview

**Goal (testable):** The route `/w/{ws}/fleets/{id}` renders a three-column console — what the fleet IS (source, triggers, danger zone), what it DOES (steer thread + composer + run-metrics strip), what it KNOWS and COSTS (memory, approvals, runs ledger + 7-day rollup) — where every cost figure is server truth from `credit_deducted_nanos`, the composer disables for the duration of a run, and saving the source takes effect on the next wake without a re-provision.

**Problem:** The detail page shows a fleet's name and status, a trigger list, a config card whose copy claims rename/pause/resume "become available once the backend adds `PATCH`/`:pause`/`:resume` endpoints" — which shipped in M80 and which the kill switch already calls — and a flat activity feed. An operator cannot read the fleet's `SKILL.md`, cannot edit it in place, cannot see what a run cost, cannot forget a memory the fleet learned wrong, and cannot tell at a glance whether the last seven days were cheap or ruinous. The one number the page could show today (`budget_used_nanos`) is dropped by the client `Fleet` type. Delete offers no warning that deleting the fleet destroys everything it ever learned.

**Solution summary:** Land the single-fleet read the console needs (`GET …/fleets/{id}`, route + serializer, no migration — the columns exist; `trigger_markdown` and `bundle_content_hash` are nullable), carry per-event cost on the events row via a LEFT JOIN into `core.fleet_execution_telemetry.credit_deducted_nanos` (no migration; the `event_id` index exists), and add tenant-plane memory forget (`DELETE …/memories/{key}`) so the memory panel can correct a bad lesson. On the client, rebuild `page.tsx` into three columns: the left rail reads and edits the source over the *existing* PATCH with next-wake save semantics, `If-Match` optimistic concurrency, and a "what changes when you save" diff; the middle reuses `FleetThread`/`SteerComposer` unchanged and gains a run-metrics strip; the right rail carries the memory panel, the existing approvals panel, and a runs ledger whose 7-day rollup is computed client-side over the events pages plus lifetime `budget_used_nanos`. Cron triggers stay read-only — schedule editing is M105_001. Fix the config card's stale copy; make the delete confirm state the memory trap.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m131): the fleet console — source, steer, memory, and cost on one honest page
- **Intent (one sentence):** An operator opens a fleet and can read and edit what it is, watch and steer what it does, and see what it knows and what it has cost — with every cost figure coming from the server, never from client-side token math.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/http/route_table_invoke.zig` (`invokePatchWorkspaceFleet`, `invokeWorkspaceFleetMemories`) — the fleet-scoped invoke handlers. Today GET on the `{id}` route falls through to 405; the single-fleet read adds the GET arm here. Mirror the acquire/release/`authorizeWorkspaceAndSetTenantContext` shape the events handler already uses — do not invent a second authorization idiom.
2. `src/agentsfleetd/state/fleet_events_store.zig` (`EventRow`, `listForFleet`) — the events row and its query. `tokens` and `wall_ms` already ride the row; the cost LEFT JOIN adds one nullable `BIGINT` field alongside them. `EventRow.deinit` frees every owned slice — a new owned field frees there too.
3. `src/agentsfleetd/http/handlers/memory/handler.zig` (`innerListMemories`) + `src/agentsfleetd/memory/fleet_memory.zig` — the tenant-plane memory surface. The entry field is **`content`**, not `text`; list is `fleet:read`, limit-only, max 100. Forget is a new tenant-plane DELETE mirroring the runner-plane forget already in `fleet_memory.zig`.
4. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` — the page being rebuilt into three columns, and `ui/packages/app/components/domain/FleetThread.tsx` / `SteerComposer.tsx` — reused **unchanged**; `isRunning` (`event_received`→`event_complete`) already gates the composer, and interrupting a running fleet is a backend capability that does not exist and must not be added here.
5. `docs/architecture/memory.md` — the memory lifecycle the forget endpoint and the delete-confirm copy must not contradict (fleet memory is keyed by `fleet_id`; a new `fleet_id` is a fresh, empty memory).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/fleets/get.zig` | CREATE | The single-fleet read (G1): serializes `source_markdown`, `trigger_markdown`, `bundle_content_hash`, `status`, `triggers`, `budget_used_nanos`, `events_processed`; sets the `ETag` response header (content hash of the editable markdown, §4). Split from `api.zig` so the read path has its own home under the 350-line cap (RULE FLL). |
| `src/agentsfleetd/http/handlers/fleets/api.zig` | EDIT | Re-exports the new read from its module; the route table calls through it. |
| `src/agentsfleetd/http/handlers/fleets/patch.zig` | EDIT | The existing PATCH gains the `If-Match` check — stale ETag → 412 Precondition Failed with the current etag in the body; the response carries the fresh `ETag` (§4). Body parse/validate and the transaction split to the two siblings below (RULE FLL — the If-Match check pushed it past the cap). |
| `src/agentsfleetd/http/handlers/fleets/patch_body.zig` | CREATE | The PATCH's body parsing + field validation, split from `patch.zig`. |
| `src/agentsfleetd/http/handlers/fleets/patch_txn.zig` | CREATE | The PATCH's transaction: `SELECT … FOR UPDATE` → `If-Match` verdict → reparse → FSM-gated `UPDATE`; owns `TxnOutcome`. Split from `patch.zig`. |
| `src/agentsfleetd/http/handlers/fleets/sql.zig` | CREATE | SQL statement text for the fleets handler domain (RULE SQLMOD): the single-fleet detail read (§1) and the single-pass page query (§8). |
| `src/agentsfleetd/http/etag.zig` | CREATE | **§9** — the shared ETag capability: `compute` over a declared field list, `staleTag` (the `If-Match` verdict), `ifMatch`, `attach`. Sits at the http layer, not under `fleets/`, because the catalog row adopts it too. |
| `src/agentsfleetd/http/handlers/fleets/list.zig` | EDIT | **§8** — the page query reads the denormalized `events_processed` / `budget_used_nanos` columns instead of aggregating the child tables; SQL moves to `sql.zig`. |
| `schema/005_core_fleets.sql` | EDIT | **§8** — `core.fleets` gains the two counter columns (`NOT NULL DEFAULT 0`); declared on the table here (pre-2.0 teardown-rebuild re-runs 005) rather than via `ALTER` (SCHEMA GUARD blocks it). Same structural-default class as `required_tags`. |
| `schema/028_fleet_activity_counters.sql` | CREATE | **§8** — the two `AFTER` triggers that maintain the counters (event-count on insert; budget on insert + renewal-delta on update) plus a one-time backfill. Invoker-rights, no new grant. |
| `schema/embed.zig` | EDIT | **§8** — registers migration 28 (contiguity test enforces no gap; renumber to 29 if M132's 28 lands first). |
| `src/agentsfleetd/http/handlers/library/catalog_patch.zig` | EDIT | **§9** — the second ETag adopter: `If-Match` on the catalog row PATCH; a stale re-send of `source_repo` can no longer discard the bundle and unpublish. One `RowState` read now serves both the publish precheck and the concurrency verdict. |
| `src/agentsfleetd/http/handlers/library/catalog.zig` | EDIT | **§9** — `RowState` carries the row's editable surface; the catalog entry (list + single) carries its `etag`. |
| `src/agentsfleetd/fleet_library/sql.zig` | EDIT | **§9** — `SELECT_CATALOG_ROW` projects the fields the row's ETag hashes. |
| `src/agentsfleetd/http/handlers/common.zig` | EDIT | The RFC 7807 writer gains the `etag` extension the 412 mandates (REST guide §4), beside the existing `current_state` for 409. Omitted from the wire unless the status sets it — the base envelope is unchanged for every other endpoint. |
| `src/agentsfleetd/errors/error_entries.zig` + `error_entries_runtime.zig` + `error_registry.zig` | EDIT | `UZ-AGT-014` (fleet source stale), `UZ-CATALOG-005` (catalog row stale), `UZ-MEM-004` (memory entry not found). The mechanism is shared; the operator-facing copy is per-resource. |
| `src/agentsfleetd/errors/internal_op_error_sweep_test.zig` | EDIT | Baseline bump for the ETag-attach failure paths (plain-English details, per the sweep's own bump rule). |
| `src/agentsfleetd/http/route_table_invoke.zig` | EDIT | `invokePatchWorkspaceFleet` gains the GET arm (G1); a new memory-item invoke wires the DELETE (G5). |
| `src/agentsfleetd/http/router.zig` | EDIT | New route variant for the memory item (`…/memories/{key}`), deepest-shape-first before the collection match. |
| `src/agentsfleetd/http/route_matchers.zig` | EDIT | `matchWorkspaceFleetMemoryItem` — the `{key}` leaf matcher, mirroring `matchWorkspaceFleetKeyDelete`. |
| `src/agentsfleetd/http/route_scopes.zig` | EDIT | GET on `patch_workspace_fleet` → `fleet:read`; the memory-item DELETE → `fleet:write` (the scope decision, §5). |
| `src/agentsfleetd/state/fleet_events_store.zig` | EDIT | `EventRow` gains a nullable cost field; `listForFleet` LEFT JOINs `fleet_execution_telemetry.credit_deducted_nanos` on `event_id`; `deinit` unchanged (the field is a scalar). |
| `src/agentsfleetd/http/handlers/fleets/events.zig` | EDIT | Serializes the cost field onto each event row (G2). |
| `src/agentsfleetd/memory/fleet_memory.zig` | EDIT | `deleteEntry(conn, fleet_id, key)` — one `DELETE … RETURNING key` graded on row count, mirroring the store's existing statements. |
| `src/agentsfleetd/http/handlers/memory/handler.zig` | EDIT | `innerDeleteMemory` — the tenant-plane forget handler (G5). |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` | EDIT | Rebuilt into the three-column console; server-fetches the single fleet, events (with cost), approvals, and billing in parallel. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/SkillEditor.tsx` | CREATE | Left rail: `SKILL.md`/`TRIGGER.md` viewer + editor over the existing PATCH, with the next-wake save dialog and the "what changes when you save" diff panel. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/MemoryPanel.tsx` | CREATE | Right rail: the memory list (`content`, `category`, `updated_at`); the forget button always renders — this spec ships the DELETE (G5), and scope refusal is the server's job (§5). |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/RunsLedger.tsx` | CREATE | Right rail: events newest-first with a cost column and the client-side 7-day rollup (G3-v1). |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/RunMetricsStrip.tsx` | CREATE | Middle: tokens · wall · cost for the current/last run, all from server fields. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetConfig.tsx` | EDIT | The danger zone (stop/resume/kill/delete); the stale "endpoints don't exist" copy is removed (G7); the delete confirm states the memory trap (G8). |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/console-copy.ts` | CREATE | The console's copy constants — save-dialog text, rollup labels, delete-confirm text — as named constants (RULE UFS). |
| `ui/packages/app/lib/api/fleets.ts` | EDIT | `getFleet` calls the real `GET …/fleets/{id}` and drops the list-scan fallback (RULE NDC); returns the widened detail shape. |
| `ui/packages/app/lib/api/fleets.test.ts` + `events.test.ts` | EDIT | The scan-cap test (`fleets.test.ts:54-67`) dies with the fallback it tests; the event type gains `cost_nanos`. |
| `ui/packages/app/lib/api/memory.ts` + `memory.test.ts` | CREATE | Neither file exists yet: `listMemories` (first dashboard caller) + `forgetMemory` (DELETE), with their unit tests. |
| `ui/packages/app/lib/api/events.ts` | EDIT | The event type gains the nullable cost field the ledger renders. |
| `ui/packages/app/lib/types.ts` | EDIT | `FleetDetail` (the single-read shape); `MemoryEntry` (`content`, not `text`); the event cost field. |
| `ui/packages/app/lib/analytics/events.ts` | EDIT | The source-saved and memory-forgotten operator events. |
| `public/openapi/paths/fleets.yaml` + `paths/memory.yaml` + `root.yaml` + `public/openapi.json` | EDIT | The `{fleet_id}` path gains GET and the events item gains `cost_nanos` (the description names the `cost_nanos` ↔ `credit_deducted_nanos` mapping); `memory.yaml` gains the `{key}` DELETE; `root.yaml` registers the new path; the `.json` bundle is regenerated (`make check-openapi`), never hand-edited. |
| `src/agentsfleetd/http/handlers/fleets/get_integration_test.zig` | CREATE | G1: read serialization, not-found, cross-workspace refusal, scope enforcement. |
| `src/agentsfleetd/http/etag_test.zig` | CREATE | **§9**: the shared module — field-boundary unambiguity, null-vs-empty, verdict on match/stale/absent. |
| `src/agentsfleetd/http/handlers/fleets/list_aggregate_integration_test.zig` | CREATE | **§8**: the single-pass page query returns per-row-subselect-identical numbers; zero-event fleets read 0, not null. |
| `src/agentsfleetd/http/handlers/library/catalog_etag_integration_test.zig` | CREATE | **§9**: catalog stale `If-Match` → 412 + current etag, nothing written; the destructive stale-resend cannot unpublish; absent `If-Match` still succeeds. |
| `src/agentsfleetd/http/handlers/fleets/events_cost_integration_test.zig` | CREATE | G2: cost joined onto the row; null when no telemetry; no double-count across the two telemetry rows per event. |
| `src/agentsfleetd/http/handlers/memory/memory_forget_integration_test.zig` | CREATE | G5: forget removes the entry; missing key 404; cross-fleet refusal; scope enforcement. |
| `src/agentsfleetd/tests.zig` | EDIT | Registers the three new integration files. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/SkillEditor.test.tsx` | CREATE | Save semantics, the diff panel, disabled-mid-run behaviour, the 412 reload-and-rediff. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/MemoryPanel.test.tsx` | CREATE | List render, the forget action, `content` field. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/RunsLedger.test.tsx` | CREATE | Cost column from server truth; the 7-day rollup arithmetic; the empty-window case. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetConfig.test.tsx` | CREATE | The delete-confirm memory-trap copy; the removed stale copy; the four danger-zone actions. |
| `ui/packages/app/tests/fleets.test.ts` | EDIT | Console page assertions follow the rebuild. |
| `ui/packages/app/tests/e2e/acceptance/fleet-console.spec.ts` | CREATE | The operator reads the source, steers, sees a cost, and edits + saves the source end to end. |
| `ui/packages/app/tests/e2e/acceptance/logs-detail.spec.ts` + `fleet-thread.spec.ts` | EDIT | Their selectors follow the three-column rebuild. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (every copy string, rollup label, SQL identifier, and route literal is a named constant), **NDC** (the list-scan `getFleet` fallback and its `UZ-AGT-SCAN-CAP` branch go — they exist only because G1 was missing), **NLR** (touch-it-fix-it on `FleetConfig.tsx` and `page.tsx`), **ORP** (widening `getFleet`'s return is a cross-layer change — sweep every caller), **ITF** (the three integration tests run against the real schema, not TEMP-table mocks), **ERR** (every new refusal cites a registered `UZ-` code — reuse the fleet-not-found and forbidden codes; mint none that duplicate them), **FLL** (`page.tsx` splits into column components rather than growing past the cap), **TSC**/**TSJ** (TypeScript conventions on every `.ts`/`.tsx`), **PUB** (the new Zig read handler's pub surface), **LOG** (the forget and read handlers log a scoped event with `error_code` on failure).
- **`dispatch/write_zig.md`** — the Zig surface: tagged-union results, `errdefer` placement, `conn.query()` drains before `deinit` (`make check-pg-drain`), file ≤350 / fn ≤50, cross-compile both linux targets.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the fleet read and the memory DELETE are public API surfaces; a not-found is a 404 with a registered code, a cross-workspace read is a 404 (not a 403 that leaks existence).
- **`dispatch/write_ts_adhere_bun.md`** — the console: design-system primitives over raw HTML, token utilities over arbitrary values, on every new component.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `get.zig`, `fleet_events_store.zig`, `fleet_memory.zig`, handlers | Cross-compile `x86_64-linux` + `aarch64-linux`; `make check-pg-drain` clean; `errdefer` on every partial allocation in the read serializer. |
| PUB / Struct-Shape | yes — the read handler's response shape; `EventRow`'s new field | Shape verdict per new pub surface; mirror `list.zig`'s serializer and the existing `EventRow` field order. |
| File & Function Length (≤350/≤50/≤70) | yes — `page.tsx` is rebuilt; the read serializer is new | Split the console into per-column components (already in Files Changed); keep the read serializer under the fn cap by lifting the trigger serialization it shares with `list.zig`. |
| UFS (repeated/semantic literals) | yes | Copy strings, rollup labels, the `7d` window, SQL identifiers, and route literals are named constants; the memory field name `content` is single-sourced. |
| UI Substitution / DESIGN TOKEN | yes — every new component | Design-system primitives only (no raw `<textarea>`/`<a>`); token utilities only (no `text-[…]`). |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING + ERROR REGISTRY yes; SCHEMA **no** | No Data Definition Language (DDL): every column the read serializes exists (`schema/005` — `source_markdown` NOT NULL at :30; `trigger_markdown` :31 and `bundle_content_hash` :41 are NULLABLE), `credit_deducted_nanos` exists (`schema/011`), and `UNIQUE (event_id, charge_type)` (schema/011:26) backs the cost join — this spec changes no migration and does not touch `schema/embed.zig`. |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/http/handlers/fleets/list.zig` — the fleet serializer that already emits `budget_used_nanos`, `events_processed`, and the trigger list from `config_json`. The single-fleet read serializes the same fields plus `source_markdown`/`trigger_markdown`/`bundle_content_hash` for one row; it mirrors this serializer, it does not invent a second one.
- **Reference:** `src/agentsfleetd/http/handlers/fleets/events.zig` + `fleet_events_store.zig` — the cursor-paginated events query the cost column rides on. The LEFT JOIN mirrors how `tokens`/`wall_ms` already ride the row.
- **Reference:** `ui/packages/app/components/domain/FleetThread.tsx` / `SteerComposer.tsx` — reused unchanged; the middle column composes them, and the `isRunning` gate they already carry is the composer's disable source.
- **Reference:** M130's `CopyButton` and `EventsList` (`ui/packages/app/components/domain/EventsList.tsx`) — the copy affordance for the source view and the row primitive the runs ledger extends.

## Sections (implementation slices)

### §1 — The single-fleet read (G1)

The console needs one row's full detail — including `source_markdown` and `trigger_markdown`, which are write-only today (only PATCH/DELETE are routed at the `{id}` path; GET falls to 405). This slice adds `GET /v1/workspaces/{ws}/fleets/{id}` under `fleet:read`, serializing `source_markdown`, `trigger_markdown`, `bundle_content_hash`, `status`, the trigger list, `budget_used_nanos`, and `events_processed`. No migration: every column exists (`schema/005`) — `source_markdown` is NOT NULL (:30), but `trigger_markdown` (:31) and `bundle_content_hash` (:41) are NULLABLE and serialize as JSON null. The response carries an `ETag` header — a content hash of the editable markdown — which §4's editor sends back as `If-Match`. The client's `getFleet` stops scanning the fleet list (which capped at 100 and 404'd larger workspaces) and calls the real endpoint; the scan fallback and its bespoke error code are deleted (RULE NDC).

**Implementation default:** a cross-workspace read returns 404, not 403 — an operator must not learn a fleet exists in a workspace they cannot see.

- **Dimension 1.1** — `GET …/fleets/{id}` returns the fleet with `source_markdown`, `trigger_markdown`, `bundle_content_hash`, `status`, `triggers`, `budget_used_nanos`, `events_processed`, including a seeded fleet whose `bundle_content_hash` is NULL serialized as null → Test `test_get_fleet_serializes_full_detail`
- **Dimension 1.2** — a missing id returns 404 with the registered fleet-not-found code; a fleet in another workspace returns 404, never 403 → Test `test_get_fleet_missing_and_cross_workspace`
- **Dimension 1.3** — GET requires `fleet:read`; a token without it is refused → Test `test_get_fleet_requires_fleet_read`

### §2 — Cost rides the event row (G2)

The runs ledger and the metrics strip both need per-event cost, and cost is server truth: it lives in `core.fleet_execution_telemetry.credit_deducted_nanos` under time-based billing (`RUN_NANOS_PER_SEC`). This slice LEFT JOINs the telemetry cost onto the fleet events list on `event_id` (index exists; no migration) so cost arrives on the same row as `tokens`/`wall_ms`. An event with no telemetry row carries a null cost, rendered as `—`, never zero. Two telemetry rows exist per event (`receive`, `stage`); the query sums them — `SUM` + `GROUP BY` (or a correlated subselect), never a bare LEFT JOIN that doubles the event row per telemetry leg — with `UNIQUE (event_id, charge_type)` (schema/011:26) as the join's backing index. An in-flight event's summed cost is partial until its `stage` row settles.

**Implementation default:** the client never computes cost from tokens — `cost_nanos` is only ever the server field; a missing value renders as unknown, and there is no fallback estimate.

- **Dimension 2.1** — an event with telemetry carries `cost_nanos` equal to the sum of its telemetry rows' `credit_deducted_nanos` → Test `test_events_carry_summed_cost`
- **Dimension 2.2** — an event with no telemetry carries a null cost; the list still returns it → Test `test_event_without_telemetry_has_null_cost`
- **Dimension 2.3** — the ledger renders `cost_nanos` verbatim from the server and renders `—` when it is null; no client token×rate arithmetic exists in the component → Test `test_ledger_cost_is_server_truth`

### §3 — The console, three columns

`page.tsx` becomes the three-column console: left is what the fleet IS, middle is what it DOES, right is what it KNOWS and COSTS. The middle reuses `FleetThread` and `SteerComposer` unchanged; the composer stays disabled for the duration of a run (`isRunning`, `event_received`→`event_complete`) — interrupting a running fleet is not a capability that exists and is not added. A run-metrics strip sits above the thread showing tokens · wall · cost for the latest run, every figure a server field. The page splits into per-column components so no single file crosses the length cap (RULE FLL); below the content breakpoint the columns stack and the body never scrolls horizontally.

- **Dimension 3.1** — the console renders three labelled regions (is / does / knows-and-costs), each with its panels → Test `test_console_renders_three_columns`
- **Dimension 3.2** — the composer is disabled while a run is in flight and re-enabled on `event_complete`; no interrupt control is rendered → Test `test_composer_disabled_while_running`
- **Dimension 3.3** — the metrics strip shows tokens, wall, and cost from server fields, and shows cost as `—` when the run has no telemetry → Test `test_metrics_strip_is_server_truth`

### §4 — The source editor with next-wake save

The left rail views and edits `SKILL.md`/`TRIGGER.md` over the *existing* `PATCH …/fleets/{id}` (`fleet:write`) — no new backend. Saving does not re-provision and emits no reload event (M80 removed it; config is re-read per lease). The save dialog states exactly: *"Takes effect on the next wake. In-flight runs finish on the current source. Memory is kept — same fleet_id."* A "what changes when you save" diff panel shows the pending source change before the operator commits. Saves are optimistically concurrent: the editor sends `If-Match` with the `ETag` from §1's GET; a stale ETag gets **412 Precondition Failed** with the current etag, and the dialog reloads-and-rediffs — never a silent overwrite. Cron triggers render read-only — schedule create/update/delete is M105_001. The rail is a viewer until the operator explicitly enters edit mode, so an accidental keystroke never stages a change.

- **Dimension 4.1** — the source editor saves via the existing PATCH and shows the exact next-wake copy; no reload/re-provision call is made → Test `test_source_save_next_wake_semantics`
- **Dimension 4.2** — the diff panel shows the pending source change before save and nothing after a no-op → Test `test_source_diff_panel_shows_pending_change`
- **Dimension 4.3** — cron triggers render read-only; the editor exposes no schedule create/update/delete control → Test `test_triggers_render_read_only`
- **Dimension 4.4** — a PATCH carrying a stale `If-Match` returns 412 with the current etag; a matching `If-Match` succeeds → Test `test_patch_if_match_stale_412`

### §5 — The memory panel and tenant forget (G5)

The right rail lists what the fleet knows — `key`, `content` (the field is **`content`**, not `text`), `category`, `updated_at` — from the existing `GET …/fleets/{id}/memories` (`fleet:read`, limit-only, max 100), which has no dashboard caller today — the Command-Line Interface (CLI) does call memories. This slice adds tenant-plane forget: `DELETE …/fleets/{id}/memories/{key}` under `fleet:write` (forget mutates fleet state, so it takes the write scope, not read; it is not a lifecycle transition, so not `fleet:admin`). A successful forget answers **204 with no body**, matching the fleet DELETE precedent (`delete.zig:83`). The forget button always renders — this spec ships the endpoint (G5); refusing an unscoped caller is the server's job, already tested.

**Implementation default:** forgetting a missing key is a 404, not a silent success — an operator who mistypes a key learns the key was not there.

- **Dimension 5.1** — the panel lists entries with `content`/`category`/`updated_at` from the tenant read → Test `test_memory_panel_lists_entries`
- **Dimension 5.2** — `DELETE …/memories/{key}` removes the entry under `fleet:write` and answers 204 no-body; a token without it is refused; forgetting across fleets is refused → Test `test_memory_forget_scope_and_isolation`
- **Dimension 5.3** — forgetting a missing key returns 404; the panel surfaces it and leaves the list unchanged → Test `test_memory_forget_missing_key_404`

### §6 — The runs ledger and the 7-day rollup (G3-v1)

The right rail's runs ledger is the events list newest-first with the §2 cost column. Above it, a 7-day rollup — wakes · tokens · spend · failed — computed client-side over the events pages (`since=7d`, which already carry tokens, status, and now cost) plus lifetime `budget_used_nanos` from §1. No server rollup endpoint (that is M133, optional). Spend in the rollup sums the events' `cost_nanos`; the lifetime figure is `budget_used_nanos` verbatim — both server truth, neither estimated.

**Implementation default:** an event with a null cost contributes zero to the rollup's spend sum but is still counted as a wake — a missing telemetry row does not vanish a run from the count.

- **Dimension 6.1** — the rollup sums wakes, tokens, spend, and failures over the 7-day events window → Test `test_rollup_aggregates_seven_day_window`
- **Dimension 6.2** — the rollup's spend sums `cost_nanos` over the window and shows lifetime `budget_used_nanos` separately; a null-cost event counts as a wake with zero spend → Test `test_rollup_spend_is_server_truth`
- **Dimension 6.3** — an empty 7-day window renders the rollup with zeros, not a broken or absent panel → Test `test_rollup_empty_window`

### §7 — Delete names the memory trap (G7 + G8)

The danger zone's stop/resume/kill/delete already work over the existing status PATCH and DELETE (DELETE is `fleet:admin`). Two copy faults remain. The config card claims rename/pause/resume "become available once the backend adds `PATCH`/`:pause`/`:resume` endpoints" — those shipped in M80 and the kill switch calls them; the stale copy is removed (G7). And the delete confirm gives no warning that deleting the fleet destroys its memory: editing the source keeps `fleet_id` (memory survives), but delete + reinstall mints a new `fleet_id` (memory is lost). The confirm gains: *"Its memory is deleted with it. Editing the source instead keeps everything it learned."* (G8).

- **Dimension 7.1** — the config card carries no copy asserting PATCH/pause/resume are unbuilt; that string survives nowhere → Test `test_config_card_no_stale_endpoint_copy`
- **Dimension 7.2** — the delete confirm states the memory is deleted with the fleet and that editing keeps it → Test `test_delete_confirm_states_memory_trap`

### §8 — The fleet list stops re-aggregating the child tables per read (performance)

`fleets/list.zig` computed both of its aggregates by scanning the child tables on **every read**: `COUNT(*)` over `core.fleet_events` and `SUM(credit_deducted_nanos)` over `core.fleet_execution_telemetry`, per fleet. Measured against a mature workspace (100 fleets × 3000 events = 300k events, 600k telemetry rows) that page took **1.77 seconds** — and every query-shape rewrite (single-pass `GROUP BY`, per-page `LATERAL`) still scans the whole child tables and lands at 500ms–1.8s, because the cost is inherent to aggregating hundreds of thousands of child rows on a read. M132 turns this route into the Live Wall — the workspace's landing surface, re-fetched on every wall render — so a half-second-plus read is not viable.

`events_processed` (lifetime event count) and `budget_used_nanos` (lifetime spend) are **monotonic counters**, so this slice denormalizes them onto `core.fleets` and maintains them at write time. The list and the detail read then read plain columns — **0.999 ms** at the same 300k-event scale, a pure index scan with zero child-table access, constant regardless of history (~1800× the old cost). Two `AFTER` triggers (migration 028) keep the columns exact: an `AFTER INSERT` on `core.fleet_events` increments the count (events are insert-only + dedup-guarded, so a replay never double-counts), and an `AFTER INSERT OR UPDATE OF credit_deducted_nanos` on the telemetry table adds the value on insert and the delta on the renewal upsert's accumulation. Both child tables are written only by `api_runtime`, which already holds `UPDATE` on `core.fleets`, so the invoker-rights triggers need no new grant. The telemetry `fleet_id` is `TEXT` with no foreign key, so the budget trigger guards the `::uuid` cast — a non-UUID id updates nothing rather than erroring.

**Implementation default:** the counters are denormalized state, so they are **maintained by DB triggers, not scattered app increments** — a single, drift-proof source that stays correct no matter which code path (or test fixture) inserts a child row. A migration backfills any pre-existing fleet (a no-op on a fresh teardown-rebuild).

**Implementation default:** a fleet with no events / no telemetry reads `0`, not `NULL` — the columns are `NOT NULL DEFAULT 0`, so a brand-new fleet's counters are born correct on the wall.

- **Dimension 8.1** — after seeding events + telemetry directly, the list's `events_processed` / `budget_used_nanos` equal the child-table aggregates (the triggers kept them in step) → Test `test_list_counters_match_children`
- **Dimension 8.2** — a fleet with zero events and zero telemetry reports `0`/`0`, not null → Test `test_list_aggregates_zero_not_null`
- **Dimension 8.3** — the page query reads the counters as columns: it contains no aggregate over `core.fleet_events` / `core.fleet_execution_telemetry` → Test `test_list_query_reads_counter_columns`
- **Dimension 8.4** — the budget counter tracks the renewal upsert's accumulation (INSERT then `credit_deducted_nanos` UPDATE), not just the first insert → Test `test_budget_counter_tracks_renewal_delta`

### §9 — ETag is a capability, not a fleet feature (generalization + second adopter)

§1/§4 landed optimistic concurrency as fleet-local code. This slice promotes the mechanism to a shared HTTP capability (`src/agentsfleetd/http/etag.zig`) any handler opts into in three lines — `compute` over an ordered list of the fields whose change must invalidate a caller's edit, `staleTag` for the `If-Match` verdict, `ifMatch`/`attach` for the wire — and proves the abstraction on a **second, independent adopter**: the platform catalog row.

The catalog row earns it on merit, not symmetry. Its PATCH is a partial update over a form the operator edits from a list read, and **a stale re-send is destructive, not merely lossy**: re-sending the `source_repo` the form was loaded with repoints the row, and repointing discards the bundle — `content_hash` goes null and the row falls back to draft (`catalog_patch.zig`, `UPDATE_CATALOG_IDENTITY`). Two platform operators curating the same row can therefore silently unpublish a fleet that tenants are installing. `If-Match` makes that unrepresentable.

The mechanism generalizes; the **copy does not**. Each adopter keeps its own registered 412 code so the operator reads a sentence about their resource (`UZ-AGT-014` for the fleet source, `UZ-CATALOG-005` for the catalog row) — the shared module owns the hash and the verdict, the registry owns the words.

**Implementation default:** each resource declares its own **editable surface** rather than hashing the whole row. The fleet hashes `source_markdown` + `trigger_markdown` (so a stop/resume never 412s an open editor that has no source conflict); the catalog row hashes the operator-owned fields — `name`, `description`, `source_repo`, `source_ref`, `required_credentials_reasons`, `visibility` — and excludes `content_hash`, so a bundle refetch does not invalidate an unrelated description edit.

**Implementation default:** `If-Match` stays **optional on every adopter**. A caller that omits it behaves exactly as it does today (last-write-wins). This keeps the CLI and every existing client working unchanged, and makes the header a capability a client opts into rather than a breaking change.

- **Dimension 9.1** — one shared module computes the tag over a declared field list; the fleet and the catalog both consume it, and no second ETag implementation exists → Test `test_etag_module_is_single_sourced`
- **Dimension 9.2** — a catalog PATCH carrying a stale `If-Match` returns 412 with the current etag and writes nothing; a matching `If-Match` succeeds → Test `test_catalog_patch_if_match_stale_412`
- **Dimension 9.3** — the destructive case: an operator's stale form re-sends `source_repo` after another operator repointed the row; the write is refused (412), so `content_hash` and `visibility` survive → Test `test_catalog_stale_resend_cannot_unpublish`
- **Dimension 9.4** — the catalog list and single-row responses carry an `etag` field, and a PATCH without `If-Match` still succeeds (opt-in, not breaking) → Test `test_catalog_etag_on_wire_and_optional`

## Interfaces

```
GET /v1/workspaces/{ws}/fleets/{id}                  scope: fleet:read
  200 → { id, name, status, source_markdown, trigger_markdown (nullable),
          bundle_content_hash (nullable), triggers[] (read-only), budget_used_nanos,
          events_processed, created_at, updated_at }   + ETag header (content hash of the editable markdown)
  404 <fleet-not-found>   no fleet with that id in this workspace (also cross-workspace)

PATCH …/fleets/{id}   (existing, extended) — accepts If-Match: <etag>; stale → 412 Precondition Failed with the current etag in the body; response carries the fresh ETag
GET  …/fleets/{id}/events   scope: fleet:read   (extended) — each item gains
     cost_nanos: number | null   // summed telemetry credit_deducted_nanos (the OpenAPI description names that mapping); null if none

DELETE …/fleets/{id}/memories/{key}   scope: fleet:write
  204 no body   entry forgotten      404 <not-found>   no entry with that key

PATCH /v1/admin/fleet-libraries/{id}   (existing, extended — §9)
  accepts If-Match: <etag>; stale → 412 with the current etag in the body; absent → unchanged (last-write-wins)
GET   /v1/admin/fleet-libraries        (extended) — each entry gains  etag: string
  (the catalog row's editable surface: name, description, source_repo, source_ref,
   required_credentials_reasons, visibility — NOT content_hash, so a bundle refetch
   does not invalidate an unrelated description edit)

Shared capability (src/agentsfleetd/http/etag.zig):
  compute(alloc, fields: []const ?[]const u8) -> "\"<sha256-hex>\""   // ordered, NUL-separated
  staleTag(alloc, if_match, fields) -> ?current_tag                   // null = match or no If-Match
  ifMatch(req) -> ?[]const u8   ·   attach(res, tag)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Fleet not found | `id` names no fleet in the workspace | 404 with the registered fleet-not-found code; the console renders Next.js `notFound()`. |
| Cross-workspace read | An operator reads a fleet id from another workspace | 404, never 403 — existence is not leaked. **Negative test required.** |
| Event has no telemetry | A run that recorded no telemetry row | `cost_nanos` is null; the ledger renders `—` and the rollup counts the wake with zero spend. Not an error. |
| Forget a missing key | The key was already gone or mistyped | 404; the panel surfaces "no such memory" and leaves the list unchanged. **Named divergence:** the REST guide's DELETE row prescribes idempotent 204 on already-deleted; this endpoint knowingly diverges to 404 — the `delete.zig` fleet-not-found precedent — so a mistyped key is surfaced, not swallowed. |
| Forget without scope | A `fleet:write`-less token calls the DELETE | 403; the button renders regardless — refusal is server-side, already tested. |
| Concurrent editor / stale `If-Match` | Two operators edit the same source; the later save carries an outdated ETag | 412 Precondition Failed with the current etag; the dialog re-loads and re-diffs — no silent overwrite. **Negative test required.** |
| Composer used mid-run | The operator tries to steer during a run | The composer is disabled (`isRunning`); no message is sent and no interrupt is attempted. **Negative test required.** |
| Events page fetch fails | The 7-day window fetch errors | The rollup degrades to lifetime `budget_used_nanos` and a "recent window unavailable" note; the ledger shows its error state, not a blank. |

## Invariants

1. **Every cost the console shows is server truth.** Enforced by the components consuming only `cost_nanos` / `budget_used_nanos` fields — a lint-checkable absence of any token×rate expression in the console directory (RULE UFS: the rate constant `RUN_NANOS_PER_SEC` is server-only and never imported client-side).
2. **The composer is disabled for the duration of a run.** Enforced by `SteerComposer` reading `isRunning` (existing), asserted by `test_composer_disabled_while_running` — not by review.
3. **The fleet-list read never re-aggregates the child tables.** Enforced by the denormalized `events_processed` / `budget_used_nanos` columns on `core.fleets` (migration 028) that the list and detail read directly — the page query holds no `COUNT`/`SUM` over `core.fleet_events` or `core.fleet_execution_telemetry`. The counters are kept exact by the migration-028 triggers, asserted by `test_list_counters_match_children` and `test_budget_counter_tracks_renewal_delta`. (This supersedes the spec's original no-migration stance — Indy's Jul 15 performance-fold-in, recorded in Discovery — because query-shape alone left the Live Wall at ~0.5–1.8s; denormalization is 0.999ms.)
4. **A cross-workspace read cannot confirm a fleet's existence.** Enforced by the handler returning 404 (not 403) on a workspace-authorization miss, asserted by `test_get_fleet_missing_and_cross_workspace`.
5. **Forget is scoped and fleet-isolated.** Enforced by the route requiring `fleet:write` and the DELETE statement keying on `(fleet_id, key)` so no fleet can forget another's memory, asserted by `test_memory_forget_scope_and_isolation`.
6. **The list page cost is independent of page size × fleet count.** Enforced by the single-pass query (§8): the two child-table aggregates are each evaluated once per page, not once per row — asserted by `test_list_query_has_no_per_row_subselect` and the R9 grep, not by review.
7. **Optimistic concurrency is one mechanism, not per-resource copies.** Enforced by both adopters computing their tag through `src/agentsfleetd/http/etag.zig` over a declared editable-field list — asserted by `test_etag_module_is_single_sourced` and the R10 grep. The 412 *copy* is per-resource (each adopter's own registered code); the *mechanism* is single-sourced.
8. **A stale write cannot silently destroy state.** Enforced on the catalog row by the `If-Match` verdict running before the identity UPDATE, so a stale re-send of `source_repo` is refused rather than nulling `content_hash` — asserted by `test_catalog_stale_resend_cannot_unpublish`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `fleet_source_saved` | product | An operator saves an edited `SKILL.md`/`TRIGGER.md` from the console | `fleet_id`, `field` (skill/trigger), `outcome` (success/failure) | No source contents, no credential material — the fleet id and coarse outcome only | `test_source_save_emits_event` |
| `fleet_memory_forgotten` | product | An operator forgets a memory entry from the panel | `fleet_id`, `outcome` (success/failure) | No memory `content`, no key text — the fleet id and outcome only | `test_memory_forget_emits_event` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_get_fleet_serializes_full_detail` | A seeded fleet → all seven detail fields with the seeded values; a second fleet seeded with `bundle_content_hash = NULL` → `bundle_content_hash: null`, not an error. |
| 1.2 | integration | `test_get_fleet_missing_and_cross_workspace` | A bad id → 404; a real id in another workspace → 404 (never 403). |
| 1.3 | integration | `test_get_fleet_requires_fleet_read` | A token lacking `fleet:read` → 403; a `fleet:admin` token succeeds (scope closure). |
| 2.1 | integration | `test_events_carry_summed_cost` | An event with `receive`+`stage` telemetry rows → `cost_nanos` equals their sum, not one leg. |
| 2.2 | integration | `test_event_without_telemetry_has_null_cost` | An event with no telemetry row → the item is returned with `cost_nanos: null`. |
| 2.3 | unit | `test_ledger_cost_is_server_truth` | `cost_nanos: 42` → renders that value; `null` → `—`; the component contains no arithmetic over `tokens`. |
| 3.1 | unit | `test_console_renders_three_columns` | The page renders the three labelled regions with their panels present. |
| 3.2 | unit | `test_composer_disabled_while_running` | `isRunning=true` → composer disabled, no send, no interrupt control; `false` → enabled. |
| 3.3 | unit | `test_metrics_strip_is_server_truth` | Server tokens/wall/cost render verbatim; null cost → `—`. |
| 4.1 | unit | `test_source_save_next_wake_semantics` | Save calls PATCH, shows the exact next-wake string, makes no reload/re-provision call. |
| 4.2 | unit | `test_source_diff_panel_shows_pending_change` | An edited source → the diff shows the pending change; an unchanged source → no diff. |
| 4.3 | unit | `test_triggers_render_read_only` | Cron triggers render with no create/update/delete affordance. |
| 4.4 | integration | `test_patch_if_match_stale_412` | PATCH with a stale `If-Match` → 412 + the current etag in the body; a matching `If-Match` → 200. |
| 5.1 | unit | `test_memory_panel_lists_entries` | Entries render `content`/`category`/`updated_at`; the field is `content`, not `text`. |
| 5.2 | integration | `test_memory_forget_scope_and_isolation` | `fleet:write` DELETE removes the entry with 204 no-body; no-scope → 403; forgetting fleet B's key from fleet A → refused. |
| 5.3 | integration | `test_memory_forget_missing_key_404` | Forgetting an absent key → 404; the store is unchanged. |
| 6.1 | unit | `test_rollup_aggregates_seven_day_window` | A window of events → wakes/tokens/spend/failed match the summed inputs. |
| 6.2 | unit | `test_rollup_spend_is_server_truth` | Spend sums `cost_nanos`; lifetime shows `budget_used_nanos`; a null-cost event = one wake, zero spend. |
| 6.3 | unit | `test_rollup_empty_window` | Zero events → the rollup renders zeros, not an absent or broken panel. |
| — | unit | `test_rollup_degrades_on_window_fetch_failure` | The 7-day window fetch rejects → the rollup (RunsLedger) shows lifetime `budget_used_nanos` + the "recent window unavailable" note; the ledger renders its error state, not a blank. |
| 7.1 | unit | `test_config_card_no_stale_endpoint_copy` | The rendered config card contains no "endpoints" / "become available" stale string. |
| 7.2 | unit | `test_delete_confirm_states_memory_trap` | The delete confirm states the memory is deleted with the fleet and that editing keeps it. |
| — | unit | `test_source_save_emits_event` | A successful save emits `fleet_source_saved` (`fleet_id`+`field`+`outcome`); a forget emits `fleet_memory_forgotten` (`fleet_id`+`outcome`); neither carries content. |
| — | e2e | `test_e2e_operator_lives_on_the_console` | The operator opens a fleet, reads its source, sees a run's cost, steers it, edits and saves the source, and reads the next-wake confirmation — the whole page this spec exists to build. |
| 8.1 | integration | `test_list_aggregates_match_per_row_semantics` | A workspace of fleets with mixed event/telemetry counts → the single-pass page query returns the same `events_processed`/`budget_used_nanos` per fleet as the per-row-subselect query it replaces. |
| 8.2 | integration | `test_list_aggregates_zero_not_null` | A fleet with no events and no telemetry → `events_processed: 0`, `budget_used_nanos: 0` (the `LEFT JOIN` miss is COALESCEd), never null. |
| 8.3 | unit | `test_list_query_has_no_per_row_subselect` | The page SQL contains no correlated subselect over `core.fleet_events` / `core.fleet_execution_telemetry` — the aggregates are evaluated once per page. |
| 9.1 | unit | `test_etag_module_is_single_sourced` | The shared module hashes an ordered field list unambiguously (`("ab",null)` ≠ `("a","b")`, null ≠ empty); both adopters call it and no second implementation exists. |
| 9.2 | integration | `test_catalog_patch_if_match_stale_412` | Catalog PATCH with a stale `If-Match` → 412 + the current `etag` in the body; the row is unchanged; a matching `If-Match` → 200. |
| 9.3 | integration | `test_catalog_stale_resend_cannot_unpublish` | **The destructive case**: operator B repoints + republishes; operator A's stale form re-sends the OLD `source_repo` → 412, and `content_hash` + `visibility` survive (without `If-Match` this write would null the hash and draft the row). |
| 9.4 | integration | `test_catalog_etag_on_wire_and_optional` | The list and single-row responses carry `etag`; a PATCH with no `If-Match` still succeeds — the header is opt-in, not a breaking change. |
| — | integration | `test_existing_patch_and_delete_unchanged` | **Regression**: the status PATCH and the fleet DELETE behave exactly as before; this spec adds a GET arm and does not touch them. A PATCH with no `If-Match` behaves exactly as it did pre-M131 on both adopters. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The single-fleet read serializes full detail and refuses cross-workspace reads with 404 (§1) | `make test-integration-db` | exit 0 | P0 | |
| R2 | Per-event cost rides the events row from telemetry, null when absent (§2) | `make test-integration-db` | exit 0 | P0 | |
| R3 | Tenant memory forget is scoped, fleet-isolated, and 404s a missing key (§5) | `make test-integration-db` | exit 0 | P0 | |
| R4 | The console renders three columns and the composer disables mid-run (§3) | `make test-unit-all` | exit 0 | P0 | |
| R5 | No client-side cost arithmetic exists in the console — cost is only ever a server field (§2, §6) | `git grep -nE '(tokens\|wall_ms)[^;]*[*/][^;]*(rate\|nanos\|price)' -- 'ui/packages/app/app/(dashboard)/w/[[]workspaceId]/fleets/[[]id]' \| wc -l` | `0` | P0 | |
| R6 | The stale "endpoints don't exist" copy is gone (§7) | `git grep -n "become available once the backend adds" -- ui/ \| wc -l` | `0` | P1 | |
| R7 | The list-scan getFleet fallback and its bespoke code are deleted (§1, RULE NDC) | `git grep -n "UZ-AGT-SCAN-CAP" -- ui/ \| wc -l` | `0` | P0 | |
| R8 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| R9 | The fleet list runs no per-row aggregate subselect (§8) | `git grep -nE "SELECT (COUNT|COALESCE\\(SUM)" -- src/agentsfleetd/http/handlers/fleets/list.zig src/agentsfleetd/http/handlers/fleets/sql.zig \| grep -c "core.fleets.id"` | `0` | P0 | |
| R10 | One ETag implementation, two adopters (§9) | `git grep -ln "std.crypto.hash.sha2.Sha256" -- 'src/agentsfleetd/http/**' \| wc -l` | `1` | P0 | |
| R11 | A stale catalog re-send cannot unpublish (§9) | `make test-integration-db` | exit 0 | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes (HTTP + schema + Redis touched) | `make test-integration` | exit 0 | P0 | |
| S4 | e2e walks the operator's console path | `make acceptance-e2e` | exit 0 | P0 | |
| S5 | No leaks (Zig allocator paths touched) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No source file newly over the length cap | `git diff --name-only origin/main \| grep -vE '\.md$\|_test\.zig$\|\.test\.(ts\|tsx)$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files:** N/A — none deleted (`FleetConfig.tsx` and `page.tsx` are edited in place). **2. Orphaned references — zero remaining imports/uses:**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `UZ-AGT-SCAN-CAP` + the list-scan fallback comment | `git grep -rnE "UZ-AGT-SCAN-CAP\|until a dedicated GET" -- ui/` | 0 matches |

## Out of Scope

- **Cron schedule create/update/delete.** Triggers render read-only here; schedule editing is **M105_001** (already pending, Upstash QStash).
- **Interrupting a running fleet.** No backend capability exists; the composer disables mid-run rather than smuggling in an interrupt. A coherent follow-on, not this spec.
- **A server-side 7-day rollup endpoint.** v1 is client-side over the events pages; the server aggregation is **M133** (optional, only if paging cost hurts).
- **The Live Wall and Getting Started.** The fleets list page and the first-run checklist are **M132**; this spec builds only the console at `/w/{ws}/fleets/{id}`. Upgrading a fleet to a newer platform bundle is a separate, unbuilt path — the editor changes this fleet's source in place only.
- **CLI verbs.** `memory forget` / `fleet get` CLI verbs are deliberately backlogged per the design freeze.

## Product Clarity (authoring record)

1. **Successful user moment** — An operator opens the reviewer fleet, reads its `SKILL.md` in the left rail, sees in the middle that the last run cost $0.004 over 12 seconds, notices the fleet learned a wrong convention, forgets that memory in the right rail, edits one line of the source, and hits Save — reading "Takes effect on the next wake. Memory is kept — same fleet_id." One page answered what it is, what it does, what it knows, and what it costs.
2. **Preserved user behaviour** — Steering (`FleetThread`/`SteerComposer`), the status PATCH lifecycle (stop/resume/kill), delete, and the approvals panel all keep working exactly as they do today. The composer's mid-run disable is preserved, not weakened.
3. **Optimal-way check** — The most direct shape would stream cost live per token; we take per-event cost on the events row because billing is time-based, settled per event, and the telemetry row is the only truth. There is no per-token cost to stream, so the row is the right grain.
4. **Rebuild-vs-iterate** — Rebuild the *page*, iterate the *backend*. The detail page is a flat panel stack that cannot answer its questions; it is rebuilt into three columns. The backend gains three small, additive surfaces (a read, a join, a forget) and no migration — nothing about the data model is rebuilt, and determinism is untouched.
5. **What we build / what we do NOT build** — A single-fleet read with ETag; a cost join onto the events row; a tenant memory forget; a three-column console; a source editor with next-wake save and If-Match; a memory panel; a runs ledger with a client-side 7-day rollup; two copy fixes. NOT: cron editing (M105); a running-fleet interrupt (no backend); a server rollup endpoint (M133); the Live Wall / Getting Started (M132); bundle upgrade (separate); client-side cost estimation (forbidden — cost is server truth).
6. **Fit with existing features** — Compounds with billing: the console's cost figures are the same `credit_deducted_nanos` the tenant billing surface bills on, so an operator's per-fleet view and the invoice cannot disagree. The one feature it must not destabilize is **steering** — `FleetThread`/`SteerComposer` are reused byte-for-byte.
7. **Surface order** — UI-first, justified: this is an operator dashboard page; the three backend gaps (G1/G2/G5) exist only to serve it, and the CLI already has `steer`/`logs`/`memory list` equivalents.
8. **Dashboard restraint** — The metrics strip and rollup show only counters that exist (`cost_nanos`, `budget_used_nanos`, `tokens`, `wall_ms`); nothing is shown that would require an estimate. The forget button ships working alongside its endpoint in this same spec — never rendered disabled with a promise.
9. **Confused-user next step** — An operator whose token lacks `fleet:write` sees the server's refusal surfaced by the panel; one who saves during a run reads exactly when it takes effect; one whose save hits a stale ETag reads the reloaded diff, not a silent overwrite; one who lands on a fleet that 404s sees Next.js not-found, not a blank page.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Seven Sections split by the *question each column answers* and the *backend gap each closes* — the read (§1), the cost join (§2), and the forget (§5) are the three additive backend surfaces; the columns (§3), the editor (§4), the ledger/rollup (§6), and the copy fixes (§7) are the client. Sequencing the backend gaps first keeps the client sections buildable against real endpoints, and each backend gap is independently testable with its own integration file.
- **Alternatives considered + verdict:** (a) **Stream cost live** — rejected: billing is time-based and settled per event; the events row is the correct grain. (b) **A server-side 7-day rollup now** — rejected for v1: the events pages already carry every field the rollup needs; a server endpoint is only worth it if paging cost hurts (M133). (c) **Keep the list-scan getFleet** — rejected: it 404s workspaces above 100 fleets and exists only because G1 was missing. (d) **Last-write-wins saves** — rejected in-session by Indy: `If-Match` optimistic concurrency ships in M131. Verdict: a **patch** on the backend (additive surfaces, zero migration, no new abstraction) and a **rebuild** of one page (`page.tsx` → three columns), confined to the console route, reusing the steering components unchanged.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
  > Indy (2026-07-14): "ETag/If-Match in M131" — context: the GET→edit→PATCH lost-update surface; chose optimistic concurrency over a last-write-wins opt-out.
  > Indy (2026-07-15): "you must add the list-aggregate query fix in to this PR. This is a performance fix. And the eTag generalization as well into this PR" — context: scope-expansion. Orly had recommended filing the list-aggregate N+1 (§8) as a follow-up spec and deferring the ETag generalization to a trigger-at-N=2. Indy overrode both: §8 (single-pass list query) and §9 (shared `http/etag.zig` + catalog-row adopter) fold into this PR. Rationale accepted: M132 promotes `fleets/list.zig` to the Live Wall's hot path, so the N+1 is urgent now; and the catalog row's stale-resend is destructive (nulls `content_hash`, unpublishes), so the second adopter is real, not speculative — clearing RULE NDC's "don't abstract at N=1".
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an Indy-acked verbatim quote here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
