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

# M146_001: Runner surface opens on leases, not on an event count

**Prototype:** v2.0.0
**Milestone:** M146
**Workstream:** 001
**Date:** Jul 29, 2026
**Status:** PENDING
**Priority:** P1 — platform operators cannot answer "what is this host doing and why did that run fail" from any existing surface
**Categories:** API, UI
**Batch:** B1 — independent of M145 secret rotation; no shared files
**Branch:** set at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5, Jul 29, 2026) — design board reviewed screen-by-screen with Indy before authoring
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Runner state, §Observability

---

## Overview

**Goal (testable):** `/admin/runners/{runner_id}` opens on that runner's leases — live leases first, each failed lease reading as the shared plain-English sentence — while Activity carries lifecycle records only, so a runner holding 4,021 leases renders 4,021 lease rows and roughly 214 lifecycle records instead of one undifferentiated count of 8,126.

**Problem:** A platform operator opening Runners today gets a table and, behind an icon, a dialog listing raw runner events. The only number on that dialog is the event total, and it roughly doubles the real execution count because a successful execution appends both `lease_acquired` and `lease_released`. From that surface an operator cannot see what a host is working on right now, how many Fleets it is serving, why any individual run failed, or what the host has done over its life. There is no addressable page for a single runner, so a colleague cannot be sent a link to one.

**Solution summary:** Rebuild the Runners surface to the same product grammar Fleets already uses. `/admin/runners` becomes a card wall whose whole card links to `/admin/runners/{runner_id}`. That detail page mirrors `fleets/[id]/page.tsx`: a breadcrumb-plus-actions header with no second title, a left rail, and a default view that is the page's main object. For a runner the main object is the lease, so the page lands on **Leases** — a metrics strip over the standard `DataTable`, live leases first, each failed row rendering `failureSentenceFor()` rather than a machine tag, each row opening a Review lease panel for the fencing token, provider, model and token meters. **Activity** becomes lifecycle records only, with `lease_acquired` and `lease_released` filtered out because the lease table already states each of them once with its outcome. Three operator-plane reads land behind it: a single-runner read (none exists), a keyset-paginated lease read joined to its Fleet event for outcome and failure cause (nothing like it exists), and multi-value `event_type` filtering on the existing runner-events read so Activity can exclude two types in one call.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(app): open the runner surface on its leases
- **Intent (one sentence):** A platform operator can open one runner, see what it is working on, read why any run failed in plain English, and share the link — without decoding a lifecycle-event count that means nothing.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` — the detail-page shape this spec copies: header alignment spacer, `FleetHeader` as breadcrumb-plus-actions on one centred row, screen-reader-only `<h1>`, rail beside a content pane, and a `loadFleetView` switch whose default arm is the main object. There is no Overview view; do not add one for runners.
2. `~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md` — §3 mandates Stripe-style `?starting_after=&limit=` keyset pagination and the exact `{items, total, next_cursor}` envelope for every new list endpoint, and explicitly forbids copying the page-based shape the existing runner endpoints use. §7 lists the six registration points; §8 fixes the handler signature.
3. `src/agentsfleetd/http/handlers/fleets/list.zig` — the keyset reference the guidelines name. Mirror its `keyset_cursor.zig` usage and its local `parseLimitFromQs`. Do **not** mirror its response field name: it emits `cursor`, while §3 requires `next_cursor`, which is what `ui/packages/app/lib/api/events.ts` already consumes.
4. `src/agentsfleetd/http/handlers/fleet/runners_list.zig` — `deriveLiveness` lives here and is the only liveness implementation. The single-runner read imports it; it must not grow a second copy.
5. `ui/packages/app/lib/events/event-summary.ts` — `FAILURE_PRESENTATION` and `failureSentenceFor()` already map every runner failure class to operator English. The runner surface imports them; a second vocabulary is a defect.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/routes.zig` | EDIT | Two new `Route` variants: `fleet_runner_get`, `fleet_runner_leases` |
| `src/agentsfleetd/http/route_matchers_fleet.zig` | EDIT | Segment matchers for the two new paths |
| `src/agentsfleetd/http/route_table.zig` | EDIT | `specFor()` arms for the new variants |
| `src/agentsfleetd/http/route_table_invoke_runner.zig` | EDIT | Invoke shims; the existing `.PATCH`/`.DELETE` switch gains a `.GET` arm |
| `src/agentsfleetd/http/route_scopes.zig` | EDIT | Both new routes require `runner:read`, joining the existing arm |
| `src/agentsfleetd/http/route_template.zig` | EDIT | Path templates for trace and metric labels |
| `src/agentsfleetd/http/handlers/fleet/runner_get.zig` | CREATE | Single-runner operator read with live-lease summary and durable lifetime counters |
| `src/agentsfleetd/http/handlers/fleet/runner_leases.zig` | CREATE | Keyset lease list joined to its Fleet event for outcome and failure cause |
| `src/agentsfleetd/http/handlers/fleet/sql.zig` | EDIT | Statements for the two reads, beside the existing runner-page statement |
| `src/agentsfleetd/http/handlers/fleet/runner_events.zig` | EDIT | Accept a comma-separated `event_type` set |
| `src/agentsfleetd/fleet/runner_events.zig` | EDIT | `Filter.event_type` becomes a set; `listForRunner` binds it |
| `src/agentsfleetd/fleet/sql.zig` | EDIT | Runner-event page statement takes a type set rather than one nullable tag |
| `src/agentsfleetd/http/fleet_runner_events_integration_test.zig` | EDIT | Coverage for the multi-value filter |
| `src/agentsfleetd/http/runner_read_integration_test.zig` | CREATE | Integration coverage for both new reads against real schema |
| `public/openapi/paths/fleet.yaml` | EDIT | The two new operations plus the widened `event_type` parameter |
| `public/openapi/root.yaml` | EDIT | Path entries for the two new operations |
| `public/openapi.json` | EDIT | Regenerated bundle |
| `ui/packages/app/lib/api/runners.ts` | EDIT | `getRunner`, `listRunnerLeases`, lease and outcome types, lifecycle-type set constant |
| `ui/packages/app/lib/api/runners.test.ts` | EDIT | Coverage for the new callers |
| `ui/packages/app/app/(dashboard)/admin/runners/page.tsx` | EDIT | Renders the wall instead of the table |
| `ui/packages/app/app/(dashboard)/admin/runners/actions.ts` | EDIT | Server actions for the new reads; the dialog-events action is retired |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnersView.tsx` | EDIT | Wall container replaces list container |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerWall.tsx` | CREATE | Responsive card grid mirroring `FleetWall` |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerTile.tsx` | CREATE | One card; whole card is the link |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerStatus.tsx` | CREATE | The dot-plus-uppercase status treatment, shared by wall, header and lease rows |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.tsx` | DELETE | The table surface this spec replaces |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.test.tsx` | DELETE | Tests for the deleted table |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerListCells.tsx` | EDIT | Keeps `ACTION_CONFIG` / `DELETE_ACTION_CONFIG` / `actionsFor` / `canDelete`, loses the table cells |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerDialogs.tsx` | EDIT | `RunnerActionConfirm` stays; `RunnerActivityDialog` is removed |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/page.tsx` | CREATE | Detail shell: header, rail, view switch defaulting to Leases |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/loading.tsx` | CREATE | Skeleton matching the shell |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/RunnerSubnavigation.tsx` | CREATE | Two-item rail mirroring `FleetSubnavigation` |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.tsx` | CREATE | Breadcrumb, runner-id `CopyButton`, admin actions, identity line |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/RunnerMetricsStrip.tsx` | CREATE | Six-cell strip mirroring `RunMetricsStrip` |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/LeaseTable.tsx` | CREATE | `DataTable` of leases with outcome and failure sentence |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/ReviewLease.tsx` | CREATE | Per-lease panel: fencing token, kind, provider, model, posture, token meters, expiry |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/ActivityTable.tsx` | CREATE | `DataTable` of lifecycle records |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/runner-copy.ts` | CREATE | Outcome labels, expired-lease sentence, rail labels as named constants |
| `ui/packages/app/lib/analytics/events.ts` | EDIT | `runner_viewed` joins the typed catalog beside `fleet_viewed` |
| `ui/packages/app/lib/runner-routes.ts` | CREATE | `runnerPath()` so the route string is written once |
| `ui/packages/app/tests/runners-page.test.ts` | EDIT | Retargeted at the wall |
| `ui/packages/app/tests/runners-list.test.ts` | DELETE | Covers the deleted table |
| `ui/packages/app/tests/runners-list-actions.test.ts` | DELETE | Covers the deleted table's actions |
| `ui/packages/app/tests/runners-list-activity-open-change.test.ts` | DELETE | Covers the deleted activity dialog |
| `ui/packages/app/tests/e2e/acceptance/runner-detail.spec.ts` | CREATE | End-to-end walk: wall → detail → failed lease → Review lease |
| `docs/architecture/runner_fleet.md` | EDIT | Records the operator-plane read surface and the lifecycle-versus-work event split |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **KYS** (the lease cursor is composite `(created_at, id)`; a bare timestamp drops rows sharing a millisecond), **UFS** (outcome tags, lifecycle-type sets, rail labels and route strings are named constants shared verbatim across Zig and TypeScript), **NDC** and **ORP** (the table, its cells, the activity dialog and their tests are deleted, then swept for orphaned references), **NLR** (the runner list is being replaced, so its page-based idiom is not carried forward into new code), **NSQ** (the new statements are schema-qualified with named constants), **FLS** (every `conn.query()` in the new handlers drains before `deinit()`), **CNX** (neither new handler holds two pool connections at once), **HXX** (both handlers answer through `Hx`, never raw `common.writeJson`), **RAD** (both new endpoints pass the REST checklist at CHORE(close)), **QPC** (the widened `event_type` grammar matches the enum the list endpoint already documents), **TVR** and **TFX** (tests exercise reachable values and share production constants), **TST-NAM** and **TNM** (no milestone identifiers in test names), **DID** (any generated React identifier uses `React.useId()`), **ASE** (async row handlers catch rejections), **OBS** (the new reads log their failure branches), **EMS** (error detail follows the registry's structure).
- `~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md` — §1 URL design, §3 pagination and list envelope, §4 datetime and status codes, §7 the six registration points, §8 handler signature. Load-bearing: §3 forbids the `page`/`page_size` shape the existing runner endpoints use.
- `dispatch/write_zig.md` — the two new handlers and the widened filter are Zig; memory lifecycle, `errdefer` placement, tagged-union results, file and function length caps, cross-compile.
- `dispatch/write_ts_adhere_bun.md` — every new component is TypeScript; design-system primitives over raw markup, design tokens over arbitrary values, `const` and import discipline.
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` — the new handlers' warn branches.
- `docs/DESIGN_SYSTEM.md` — dark Operational Restraint, the 4px scale, mint reserved for genuine live signals, administrative state before liveness, borders over shadows.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — two new handlers plus a widened filter | Drain every `conn.query()` in the same function before `deinit()`; `errdefer` per owned slice in row decoding; cross-compile both linux targets before commit |
| PUB / Struct-Shape | yes — new `pub` surface in `runner_get.zig`, `runner_leases.zig`, `runner-routes.ts` | One `FILE SHAPE DECISION` per new file at PLAN; the lease item struct is a flat record with no optional-field unions |
| File & Function Length (≤350/≤50/≤70) | yes — `runners.ts` and `RunnerListCells.tsx` are already near the cap and the detail page adds components | Split by role as the existing surface already does: data flow in the page, presentation in components, copy in `runner-copy.ts` |
| UFS (repeated/semantic literals) | yes — outcome tags and the lifecycle-type set cross the Zig and TypeScript boundary | Named constants both sides, spelled identically; the lifecycle set is one exported array consumed by the Activity caller |
| UI Substitution / DESIGN TOKEN | yes — every new component | `DataTable`, `Badge`, `Card`, `CopyButton`, `Time`, `Nav`, `EmptyState` from the design system; no arbitrary `*-[…]` utilities |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING yes, LIFECYCLE yes, ERROR REGISTRY yes, SCHEMA no | Scoped loggers with error codes on every warn; arena-owned slices freed on the error path; reuse `UZ-RUN-014` and `UZ-REQ-001` rather than minting codes; no migration — every column this spec reads already exists |

## Prior-Art / Reference Implementations

- **Reference (page shape):** `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` — breadcrumb-plus-actions header on one centred row, screen-reader-only `<h1>`, header alignment spacer matching the rail width, rail beside pane, view switch whose default arm is the main object. Copied wholesale; the only divergence is two rail items instead of five.
- **Reference (wall):** `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/{FleetWall,FleetTile}.tsx` — responsive grid, absolutely-positioned whole-card `Link` over `pointer-events-none` content, bottom-bordered action affordance. Divergence: a runner is a host, not an agent, so it gets a plain server glyph in the same 56px bordered square rather than a deterministic sigil; the design system scopes sigils to Fleet tiles.
- **Reference (strip):** `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/RunMetricsStrip.tsx` — `Card` wrapping a six-column `DescriptionList`, uppercase mono label over tabular value over a quieter detail line, left-border dividers.
- **Reference (keyset read):** `src/agentsfleetd/http/handlers/fleets/list.zig` — `keyset_cursor.zig` parse and format, local `parseLimitFromQs`. Divergence: emit `next_cursor`, not that file's `cursor`, per the guidelines §3.
- **Reference (operator read):** `src/agentsfleetd/http/handlers/fleet/runners_list.zig` — arena-owned row decoding with per-slice `errdefer`, the count-only sentinel row, and `deriveLiveness`, which the single read imports rather than re-implements.

## Sections (implementation slices)

### §1 — Single-runner operator read

`GET /v1/fleets/runners/{runner_id}` does not exist, so the detail page cannot be addressable without it: a refresh or a shared link has nothing to hydrate from. This slice adds the read, returning the runner record with derived liveness, a live-work summary, and lifetime counters computed from durable lease and event state.

**Implementation default:** lifetime counters are **lifetime, not windowed**, because the only index on the lease side is `(runner_id, status)` — a windowed summary would scan the runner's whole lease history, and `core.fleet_activity_counters` already establishes lifetime counting as the house pattern for Fleets. A window is a follow-up that lands with its index.

**Implementation default:** counters come from `fleet.runner_leases` joined to `core.fleet_events`, never from the `agentsfleet_runner_*` Prometheus families. Those are process-global and in-memory: zeroed on every `agentsfleetd` restart, capped at 4096 runners with the 4097th collapsing into `runner_id="_other"`, and `active_leases` is documented approximate under more than one replica (`docs/architecture/runner_fleet.md` §Multi-replica).

- **Dimension 1.1** — The read returns the runner record and never emits `token_hash` → Test `test_runner_get_omits_token_hash`
- **Dimension 1.2** — Liveness is derived by the same `deriveLiveness` the list read uses, agreeing across a matrix of `last_seen_at` and live-lease inputs → Test `test_runner_get_liveness_agrees_with_list`
- **Dimension 1.3** — An unknown runner id answers 404 `UZ-RUN-014` → Test `test_runner_get_unknown_id_is_not_found`
- **Dimension 1.4** — The route requires `runner:read`; a principal without it is refused → Test `test_runner_get_requires_runner_read_scope`
- **Dimension 1.5** — The response carries `active_lease_count` and `active_fleet_count`, the latter counting distinct fleets across live leases only → Test `test_runner_get_counts_distinct_fleets_across_live_leases`
- **Dimension 1.6** — Lifetime counters split acquired, succeeded, failed and expired from durable state → Test `test_runner_get_lifetime_counters_from_durable_state`
- **Dimension 1.7** — A lease whose deadline has passed but whose row still reads `active` counts as neither live nor expired until reclaim marks it → Test `test_runner_get_stale_active_lease_is_not_live`

### §2 — Operator-plane lease read

Nothing in the system exposes a runner's leases to an operator: `POST /v1/runners/me/leases` is the runner's own grant call on the self-plane. This slice adds the read that the Leases view renders, joining each lease to its Fleet event so outcome and failure cause arrive in one round trip.

**Implementation default:** Stripe-style keyset pagination (`?starting_after=&limit=`, response `next_cursor`), because the guidelines §3 mandate it for every new endpoint and explicitly name the page-based shape the neighbouring runner endpoints use as legacy not to be copied. The cursor is composite `(created_at, id)` per RULE KYS. This means the Leases and Activity views page by different idioms until the events read is migrated, which is named in Out of Scope.

**Implementation default:** outcome is derived server-side into a single closed tag rather than shipping raw statuses for the client to combine, so the two surfaces cannot drift on what `expired` means.

- **Dimension 2.1** — The response envelope is exactly `{items, total, next_cursor}`, with `total` declared nullable in the schema → Test `test_runner_leases_envelope_is_items_total_next_cursor`
- **Dimension 2.2** — `starting_after` plus `limit` pages forward; `limit` defaults to 50 and is refused above 100 → Test `test_runner_leases_keyset_pages_forward`
- **Dimension 2.3** — Rows sharing one `created_at` millisecond are all returned across page boundaries → Test `test_runner_leases_same_millisecond_rows_are_not_skipped`
- **Dimension 2.4** — Outcome maps `processed` to succeeded, `fleet_error` to failed, lease `expired` to expired, and a live unexpired `active` lease to running → Test `test_runner_leases_outcome_mapping`
- **Dimension 2.5** — A failed item carries `failure_label` and `failure_detail` verbatim from its Fleet event → Test `test_runner_leases_failed_item_carries_failure_fields`
- **Dimension 2.6** — `request_json` is absent from the item shape → Test `test_runner_leases_never_emits_request_payload`
- **Dimension 2.7** — Items carry `fleet_id`, `workspace_id` and the fleet name so the client builds the Fleet link without a second read → Test `test_runner_leases_carries_fleet_link_fields`
- **Dimension 2.8** — An expired lease reads expired even when its Fleet event later settled `processed` under the runner that reclaimed it → Test `test_runner_leases_expired_lease_is_not_credited_with_successor_outcome`

### §3 — Lifecycle-only event filtering

Activity must exclude exactly two event types. Today `event_type` accepts one value, so the view would need seven calls. This slice widens the parameter to the comma-separated multi-value equality grammar the guidelines §3 already define, leaving single-value callers untouched.

- **Dimension 3.1** — `event_type` accepts a comma-separated set and returns the union → Test `test_runner_events_accepts_comma_separated_type_set`
- **Dimension 3.2** — An unrecognised tag anywhere in the set is refused with `UZ-REQ-001` and no partial result → Test `test_runner_events_rejects_unknown_type_in_set`
- **Dimension 3.3** — A single-value request behaves exactly as before → Test `test_runner_events_single_value_filter_unchanged`
- **Dimension 3.4** — An empty `event_type` value is refused rather than silently meaning "all" → Test `test_runner_events_rejects_empty_type_parameter`

### §4 — Runner card wall

`/admin/runners` is a `DataTable` whose rows carry icon-only actions. A table cannot show what a host is doing, and none of its rows are addressable. This slice replaces it with the Fleet wall grammar so a runner is something you click into.

- **Dimension 4.1** — One card renders per runner and the whole card links to `/admin/runners/{runner_id}` → Test `test_runner_wall_card_links_to_detail`
- **Dimension 4.2** — Status renders as a dot with uppercase text, administrative state before liveness → Test `test_runner_status_renders_admin_state_before_liveness`
- **Dimension 4.3** — Only a runner whose derived liveness is genuinely awake gets the wake ring; a cordoned or offline host gets a static dot → Test `test_runner_status_wake_ring_only_when_live`
- **Dimension 4.4** — A card states its current work in one line, or that it is idle → Test `test_runner_tile_states_current_work_or_idle`
- **Dimension 4.5** — Zero runners renders the empty state, not an empty grid → Test `test_runner_wall_empty_state`

### §5 — Detail shell, Leases landing, Review lease

The runner's main object is the lease, so the detail page opens on it. This slice builds the shell and the landing view together because the shell has no other default: there is no Overview.

**Implementation default:** the header carries no page title. The breadcrumb already names the host and the Fleet detail page proves the pattern by keeping its `<h1>` screen-reader-only. Identity — isolation tier, labels, runner id — rides one line below the header; enrolment is not repeated there because Activity's `registered` record already carries it with the real date.

- **Dimension 5.1** — The header is a breadcrumb and an action cluster on one vertically-centred row, with an accessible name present but not rendered as a second title → Test `test_runner_header_has_no_visible_second_title`
- **Dimension 5.2** — The identity line renders status, the isolation tier as a `Badge`, and label badges; the runner id is reachable only through a `CopyButton` → Test `test_runner_header_identity_line`
- **Dimension 5.3** — The rail renders Leases and Activity, and an absent or unknown view parameter resolves to Leases → Test `test_runner_view_resolves_to_leases_by_default`
- **Dimension 5.4** — The strip renders six cells and each outcome counter carries its status colour → Test `test_runner_metrics_strip_cells_and_colours`
- **Dimension 5.5** — Leases render through `DataTable` with live leases ordered first → Test `test_lease_table_orders_live_leases_first`
- **Dimension 5.6** — A failed row renders `failureSentenceFor()` output and never the raw tag → Test `test_lease_table_failed_row_renders_failure_sentence`
- **Dimension 5.7** — An expired row states that the lease was not renewed and the work was re-leased → Test `test_lease_table_expired_row_states_reclaim`
- **Dimension 5.8** — Activating a row opens Review lease carrying lease id, kind, fencing token, provider, model, posture, token meters and expiry → Test `test_review_lease_renders_lease_facts`
- **Dimension 5.9** — Review lease renders no request payload field under any outcome → Test `test_review_lease_never_renders_request_payload`
- **Dimension 5.10** — `Open Grafana` renders only when its base address is configured → Test `test_grafana_action_hidden_without_configured_base`
- **Dimension 5.11** — Opening the page captures `runner_viewed` with the runner id and coarse liveness, never a token or host secret → Test `test_runner_viewed_event_properties`

### §6 — Activity view and retirement of the table surface

Activity is the second rail item and the last piece of the 8K problem. The old table, its cells and the activity dialog are deleted in the same slice so no dead surface survives the replacement.

- **Dimension 6.1** — Activity requests the lifecycle type set, and neither `lease_acquired` nor `lease_released` appears in the rendered feed → Test `test_activity_excludes_lease_work_events`
- **Dimension 6.2** — A state-change record renders its from-state and to-state from event metadata → Test `test_activity_renders_admin_state_transition`
- **Dimension 6.3** — The registration record renders the host identifier and isolation tier from event metadata → Test `test_activity_renders_registration_record`
- **Dimension 6.4** — Activity renders through the same `DataTable` as Leases → Test `test_activity_uses_data_table`
- **Dimension 6.5** — `RunnerList`, `RunnerActivityDialog` and their tests are gone, and no reference to them survives anywhere → Test `test_no_orphaned_runner_table_references`

## Interfaces

```
GET /v1/fleets/runners/{runner_id}
  scope: runner:read
  200 → the resource, no envelope:
  {
    "id": "01J2WQ8F3K7VZ9XB4N6MTYD5AR",
    "host_id": "runner-prod-ams-01.internal",
    "sandbox_tier": "landlock_full",
    "admin_state": "active",
    "liveness": "busy",
    "labels": ["gpu", "prod", "ams"],
    "last_seen_at": 1785312000000,
    "created_at": 1780000000000,
    "active_lease_count": 2,
    "active_fleet_count": 2,
    "leases_acquired": 4021,
    "leases_succeeded": 3945,
    "leases_failed": 42,
    "leases_expired": 34
  }
  404 UZ-RUN-014 — no runner with this id
  403 — principal lacks runner:read

GET /v1/fleets/runners/{runner_id}/leases?starting_after=<lease_id>&limit=<1..100>
  scope: runner:read
  200 → { "items": [ … ], "total": 4021 | null, "next_cursor": "<lease_id>" | null }
  item:
  {
    "id": "01J2X7NCS8T63ZP0000000000",
    "fleet_id": "01J2WQ0000000000000000000",
    "fleet_name": "Search Services",
    "workspace_id": "01J2WQ1111111111111111111",
    "event_id": "evt_01J2X7NBQ4M91KD",
    "event_type": "index_build",
    "actor": "system",
    "outcome": "running" | "succeeded" | "failed" | "expired" | "unknown",
    "failure_label": "oom_kill" | null,
    "failure_detail": "Container exceeded its 2 GiB memory limit …" | null,
    "kind": "fresh" | "reclaim",
    "fencing_token": 1884,
    "provider": "azure_openai",
    "model": "gpt-4o-mini",
    "posture": "metered",
    "metered_input_tokens": 18204,
    "metered_cached_tokens": 4096,
    "metered_output_tokens": 2881,
    "wall_ms": 242000 | null,
    "lease_expires_at": 1785311000000,
    "created_at": 1785303420000
  }
  400 UZ-REQ-001 — unparseable starting_after, or limit outside 1..100
  404 UZ-RUN-014 — no runner with this id

GET /v1/fleets/runners/{runner_id}/events?event_type=<tag>[,<tag>…]&page=&page_size=
  unchanged except event_type, which now accepts a comma-separated set.
  400 UZ-REQ-001 — any unrecognised tag in the set, or an empty value.

Client:
  getRunner(token, runnerId): Promise<RunnerDetail>
  listRunnerLeases(token, runnerId, { starting_after?, limit? }): Promise<RunnerLeaseResponse>
  runnerPath(runnerId, view?): string        // "/admin/runners/{id}" | "…?view=activity"
  RUNNER_LIFECYCLE_EVENT_TYPES: readonly RunnerEventType[]   // the seven non-lease tags
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unknown runner | Id does not resolve, or was deleted between wall render and click | 404 `UZ-RUN-014`; the page renders Next.js `notFound()`, not an empty shell |
| Malformed cursor | `starting_after` is not a lease id this runner owns | 400 `UZ-REQ-001`; the client falls back to the first page rather than showing an empty table |
| Limit out of range | `limit` is zero, negative, non-numeric, or above 100 | 400 `UZ-REQ-001`; message names the accepted range |
| Unknown event type in set | A caller sends a tag outside the enum | 400 `UZ-REQ-001` with no partial result — never a silent drop that looks like "no such events" |
| Database unavailable | Pool acquire fails on either new read | 503 through `common.internalDbUnavailable`; the view renders its error state, and the wall stays usable |
| Fleet deleted under a lease | Cascade removed the fleet after the lease settled | The item is still returned with `fleet_name` null; the row renders the fleet id and suppresses the Fleet link rather than linking to a 404 |
| Fleet event missing for a settled lease | Report landed, event row absent | `outcome` is `unknown`; the row says the outcome is not recorded — never a fabricated success |
| Stale active lease | Deadline passed, reclaim has not run | Counted as neither live nor expired; the strip's live count excludes it and the row reads running until reclaim marks it |
| Runner with no leases | Freshly enrolled host | 200 `{items: [], total: 0, next_cursor: null}`; the table renders its empty state, never a spinner |
| Grafana unconfigured | The public base address env var is unset | The action does not render at all; no dead link, no placeholder |
| Clipboard unavailable | Insecure context or denied permission | `CopyButton` reports the failure in its own accessible name, per its documented behaviour |
| Concurrent revoke | Runner revoked while its detail page is open | The next action answers 409 `UZ-RUN-016`; the header refreshes to the new administrative state |

## Invariants

1. **One liveness implementation.** `deriveLiveness` in `runners_list.zig` is the only derivation; the single read imports it. Enforced by a unit test that drives both handler paths over the same input matrix and asserts equality — a second implementation cannot agree by accident across every boundary case.
2. **`token_hash` cannot leave the server.** The item struct in both new handlers has no such field, so emitting it is a compile error rather than a review catch.
3. **`request_json` cannot leave the server.** Same mechanism — the lease item struct omits it entirely.
4. **A reclaimed lease's successor outcome is never credited to the expired holder.** Outcome is computed from the lease's own `status` first: an `expired` lease reads expired regardless of what its Fleet event later settled. Enforced by Dimension 2.8's test.
5. **Failure copy has exactly one source.** `FAILURE_PRESENTATION` in `event-summary.ts`; the runner surface imports `failureSentenceFor`. Enforced by a grep test asserting no runner component spells a failure tag literal.
6. **Lifecycle and work events never mix in Activity.** The lifecycle type set is one exported constant; the Activity caller passes it verbatim. Enforced by a test asserting the two lease tags are absent from the constant and from the rendered feed.
7. **The route string is written once.** `runnerPath()` is the only producer of `/admin/runners/{id}`. Enforced by a grep test asserting no component inlines the path.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `runner_viewed` | product | A platform operator opens `/admin/runners/{runner_id}`, once per mount, mirroring `fleet_viewed` | `runner_id`, `liveness`, `admin_state` | No `host_id`, no token, no label values, no lease identifiers | `test_runner_viewed_event_properties` |
| `agentsfleet_http_requests_total` (existing) | ops | Both new routes serve a request | Route template label only, via `route_template.zig` | Route templates carry `{runner_id}`, never a resolved id | `test_runner_read_route_templates_registered` |

The four `agentsfleet_runner_*` Prometheus families are unchanged: this spec reads durable state and adds no counter. No funnel changes, so no analytics playbook update is required — recorded in Discovery at CHORE(close).

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_runner_get_omits_token_hash` | A runner with a stored token hash → the response body contains no `token_hash` key |
| 1.2 | unit | `test_runner_get_liveness_agrees_with_list` | Matrix of never-seen / fresh / stale `last_seen_at` × live-lease true/false → both read paths return the same tag for every cell |
| 1.3 | integration | `test_runner_get_unknown_id_is_not_found` | A well-formed id with no row → 404, body error code `UZ-RUN-014` |
| 1.4 | integration | `test_runner_get_requires_runner_read_scope` | A principal without `runner:read` → 403, and the handler never reaches the pool |
| 1.5 | integration | `test_runner_get_counts_distinct_fleets_across_live_leases` | Three live leases across two fleets → `active_lease_count` 3, `active_fleet_count` 2 |
| 1.6 | integration | `test_runner_get_lifetime_counters_from_durable_state` | Seeded leases: 4 processed, 1 `fleet_error`, 2 expired → acquired 7, succeeded 4, failed 1, expired 2 |
| 1.7 | integration | `test_runner_get_stale_active_lease_is_not_live` | One lease `status='active'` with `lease_expires_at` in the past → `active_lease_count` 0 and `leases_expired` unchanged |
| 2.1 | integration | `test_runner_leases_envelope_is_items_total_next_cursor` | Any successful read → exactly the three keys, no `results`/`data`/`page` |
| 2.2 | integration | `test_runner_leases_keyset_pages_forward` | 7 leases, `limit=3` → three pages of 3/3/1, `next_cursor` null only on the last |
| 2.3 | integration | `test_runner_leases_same_millisecond_rows_are_not_skipped` | 5 leases sharing one `created_at`, `limit=2` → all 5 returned across pages, no duplicates |
| 2.4 | unit | `test_runner_leases_outcome_mapping` | Each `(lease status, event status, expiry)` triple → the single expected outcome tag |
| 2.5 | integration | `test_runner_leases_failed_item_carries_failure_fields` | A lease whose event settled `fleet_error` with `oom_kill` → item carries that label and its detail |
| 2.6 | integration | `test_runner_leases_never_emits_request_payload` | A lease with a populated `request_json` → the key is absent from every item |
| 2.7 | integration | `test_runner_leases_carries_fleet_link_fields` | Any item → `fleet_id`, `workspace_id` and `fleet_name` are present and match the seeded fleet |
| 2.8 | integration | `test_runner_leases_expired_lease_is_not_credited_with_successor_outcome` | Lease expired, its event later `processed` by another runner → this runner's item reads expired |
| 3.1 | integration | `test_runner_events_accepts_comma_separated_type_set` | `event_type=runner_online,runner_offline` → only those two types, both present |
| 3.2 | integration | `test_runner_events_rejects_unknown_type_in_set` | `event_type=runner_online,not_a_type` → 400 `UZ-REQ-001`, empty body items |
| 3.3 | integration | `test_runner_events_single_value_filter_unchanged` | `event_type=lease_acquired` → same rows and total as before the widening |
| 3.4 | integration | `test_runner_events_rejects_empty_type_parameter` | `event_type=` → 400, never a full unfiltered page |
| 4.1 | unit | `test_runner_wall_card_links_to_detail` | A runner row → the card contains one link whose target is that runner's detail path |
| 4.2 | unit | `test_runner_status_renders_admin_state_before_liveness` | `admin_state=active`, `liveness=busy` → accessible text reads active then busy, uppercase |
| 4.3 | unit | `test_runner_status_wake_ring_only_when_live` | `liveness` offline and cordoned → no wake ring; busy → ring present |
| 4.4 | unit | `test_runner_tile_states_current_work_or_idle` | Two live leases → the fleets are named; zero → the idle sentence renders |
| 4.5 | unit | `test_runner_wall_empty_state` | Zero runners → the empty state renders, the grid does not |
| 5.1 | unit | `test_runner_header_has_no_visible_second_title` | Detail page → the host name appears once in visible text, inside the breadcrumb |
| 5.2 | unit | `test_runner_header_identity_line` | A runner with tier and labels → tier and each label render as badges; the raw runner id is not visible text |
| 5.3 | unit | `test_runner_view_resolves_to_leases_by_default` | View param absent, empty, and unknown → all three resolve to Leases |
| 5.4 | unit | `test_runner_metrics_strip_cells_and_colours` | Seeded counters → six cells; succeeded, failed and expired carry distinct status colour classes |
| 5.5 | unit | `test_lease_table_orders_live_leases_first` | Mixed running and settled items → every running row precedes every settled row |
| 5.6 | unit | `test_lease_table_failed_row_renders_failure_sentence` | `failure_label=oom_kill` → the row reads the shared sentence, and `oom_kill` is not visible text |
| 5.7 | unit | `test_lease_table_expired_row_states_reclaim` | An expired item → the row states the lease was not renewed and the work was re-leased |
| 5.8 | unit | `test_review_lease_renders_lease_facts` | An item with every field → lease id, kind, fencing token, provider, model, posture, three meters and expiry all render |
| 5.9 | unit | `test_review_lease_never_renders_request_payload` | Every outcome tag → no request-payload field renders in any state |
| 5.10 | unit | `test_grafana_action_hidden_without_configured_base` | Base address unset → the action is absent; set → it renders with the runner filter |
| 5.11 | unit | `test_runner_viewed_event_properties` | Page mount → one capture with runner id, liveness and admin state, and no host identifier |
| 6.1 | unit | `test_activity_excludes_lease_work_events` | The lifecycle set → neither lease tag is a member; a feed seeded with both renders neither |
| 6.2 | unit | `test_activity_renders_admin_state_transition` | Metadata carrying from and to states → both render in the detail column |
| 6.3 | unit | `test_activity_renders_registration_record` | Registration metadata → host identifier and isolation tier render |
| 6.4 | unit | `test_activity_uses_data_table` | Activity view → the rendered table carries the shared table structure, not bespoke markup |
| 6.5 | unit | `test_no_orphaned_runner_table_references` | Repository grep for the deleted symbols → zero matches outside this spec |
| failure | integration | `test_runner_leases_rejects_malformed_cursor` | `starting_after` that is not a lease id this runner owns → 400 `UZ-REQ-001`, no partial page |
| failure | integration | `test_runner_leases_rejects_limit_out_of_range` | `limit` of 0, -1, `abc` and 101 → 400 each, message names the 1..100 range |
| failure | integration | `test_runner_read_db_unavailable_is_service_error` | Pool acquire injected to fail on both new reads → 503 with the shared unavailable body, never a 200 with empty items |
| failure | integration | `test_runner_leases_deleted_fleet_suppresses_link` | Lease whose fleet was cascade-deleted → item returned with `fleet_name` null and the link fields absent |
| failure | integration | `test_runner_leases_missing_event_reads_unknown` | Lease `status='reported'` with no matching Fleet event row → `outcome` is `unknown`, never `succeeded` |
| failure | integration | `test_runner_leases_empty_returns_empty_envelope` | A runner that has never held a lease → 200 `{items: [], total: 0, next_cursor: null}`, never 204 |
| failure | unit | `test_runner_header_copy_failure_is_reported` | Clipboard write rejects → the copy control announces the failure and does not show a success state |
| failure | unit | `test_runner_header_revoke_conflict_surfaces_state` | Revoke answers 409 `UZ-RUN-016` → the header shows the returned administrative state and the error, not a stale badge |
| regression | integration | `test_runner_list_read_unchanged` | The existing list endpoint → same envelope, same fields, same sort allowlist as before this milestone |
| regression | unit | `test_runner_admin_actions_unchanged` | Cordon, drain, revoke and delete → same confirm copy, same eligibility rules, same error handling as the retired table applied |
| replay | integration | `test_runner_leases_repeated_cursor_is_stable` | The same `starting_after` requested twice with no writes between → byte-identical pages |
| e2e | e2e | `runner-detail.spec.ts` | Wall renders → clicking a card lands on that runner's Leases → a failed lease reads its sentence → activating the row opens Review lease |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A runner detail page is addressable and hydrates from a cold load (§1, §5) | `curl -s -o /dev/null -w '%{http_code}' "$API/v1/fleets/runners/$RID" -H "Authorization: Bearer $TOKEN"` | `200` | P0 | |
| R2 | The lease read pages by keyset, never by page number (§2) | `grep -c 'starting_after' src/agentsfleetd/http/handlers/fleet/runner_leases.zig` | at least 1, and `grep -c 'parsePageParams' …/runner_leases.zig` is 0 | P0 | |
| R3 | Activity carries no lease work events (§3, §6) | `grep -n 'lease_acquired\|lease_released' ui/packages/app/app/\(dashboard\)/admin/runners/\[runnerId\]/components/ActivityTable.tsx` | no output | P0 | |
| R4 | No runner component spells a failure tag literal (§5) | `grep -rn 'oom_kill\|timeout_kill\|transport_loss\|renewal_terminate' 'ui/packages/app/app/(dashboard)/admin/runners'` | no output | P0 | |
| R5 | The retired table surface is gone from disk and from every reference (§6) | `test ! -f 'ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.tsx' && ! grep -rn 'RunnerActivityDialog' ui/packages/app --include='*.ts*'` | exit 0, no output | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes (HTTP and Redis touched) | `make test-integration` | exit 0 | P0 | |
| S4 | End-to-end walks the operator path | `make test-e2e-acceptance` | exit 0 | P0 | |
| S5 | No leaks (new handlers allocate per request) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |
| S10 | OpenAPI bundle in sync and lint-clean | `make check-openapi` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.tsx` | `test ! -f 'ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.tsx'` |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.test.tsx` | `test ! -f 'ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.test.tsx'` |
| `ui/packages/app/tests/runners-list.test.ts` | `test ! -f ui/packages/app/tests/runners-list.test.ts` |
| `ui/packages/app/tests/runners-list-actions.test.ts` | `test ! -f ui/packages/app/tests/runners-list-actions.test.ts` |
| `ui/packages/app/tests/runners-list-activity-open-change.test.ts` | `test ! -f ui/packages/app/tests/runners-list-activity-open-change.test.ts` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `RunnerActivityDialog` | `grep -rn -w "RunnerActivityDialog" ui/packages/app --include="*.ts*"` | 0 matches |
| `RunnerList` | `grep -rn -w "RunnerList" ui/packages/app --include="*.ts*"` | 0 matches |
| `RunnerListHandle` | `grep -rn -w "RunnerListHandle" ui/packages/app --include="*.ts*"` | 0 matches |
| `HostCell` | `grep -rn -w "HostCell" ui/packages/app --include="*.ts*"` | 0 matches |
| `StatusCell` | `grep -rn -w "StatusCell" ui/packages/app --include="*.ts*"` | 0 matches |
| `LabelsCell` | `grep -rn -w "LabelsCell" ui/packages/app --include="*.ts*"` | 0 matches |
| `ActionsCell` | `grep -rn -w "ActionsCell" ui/packages/app --include="*.ts*"` | 0 matches |

## Out of Scope

- **Migrating `GET /v1/fleets/runners/{id}/events` and `GET /v1/fleets/runners` to keyset pagination.** The guidelines forbid the page-based shape for *new* endpoints; converting the two existing ones is a separate read-surface change with its own client and test blast radius. Consequence accepted here: Leases pages by cursor while Activity pages by number until that lands. Follow-up spec, not a deferral of this one.
- **Windowed performance counters and their index.** Lifetime counting ships now; a "last 24 hours" selector needs an index on `fleet.runner_leases (runner_id, created_at)` and a bounded plan, which belongs with the window that motivates it.
- **Filtering the lease list to a single outcome.** The shared table offers sort, not filter, and a server-side outcome filter needs a sort or filter key the endpoint does not yet expose. Sorting is available; a dedicated failures filter is follow-up work.
- **Provisioning Grafana dashboards or a runner deep link target.** This spec renders the action against a configured base address and hides it when unset; standing up dashboards is operational work under the observability playbooks.
- **Deterministic identity sigils for runners.** A host is not an agent and the design system scopes sigils to Fleet tiles; runners get the shared server glyph.
- **Runner capacity, utilisation or overload display.** The heartbeat body is empty, so the control plane holds no worker-capacity fact. Nothing honest can be rendered until the protocol carries it.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator gets paged, opens `/admin/runners`, sees one card pulsing with two leases, clicks it, and reads: *Search Services · index_build · 2h 17m ago · failed · Ran out of memory.* They know which host, which fleet, which work and why, without opening Grafana or asking anyone.
2. **Preserved user behaviour** — Cordon, drain, revoke and delete keep their exact confirm copy, eligibility rules and error handling; delete still appears only once a runner is revoked. Enrolling a runner is untouched. The list endpoint keeps its envelope so any other caller is unaffected.
3. **Optimal-way check** — The most direct shape is the one Fleets already ships, and this copies it rather than inventing. The gap to unconstrained-optimal is the lease list's inability to filter to failures server-side; sorting covers the common case, and closing it properly needs an endpoint capability that is named in Out of Scope rather than half-built here.
4. **Rebuild-vs-iterate** — Rebuild. The table cannot express "what is this host doing", and no addressable page exists to iterate toward; the surface is being replaced, not patched. Determinism is unaffected: every number rendered comes from durable rows, and the one non-durable source available (the in-memory Prometheus families) is explicitly rejected in §1.
5. **What we build** — Three operator-plane reads, a card wall, a two-view detail page landing on Leases, and a Review lease panel.
6. **What we do NOT build** — No Overview view (Fleets has none and the questions it would answer are answered by the header and the strip); no Details view (four static facts do not earn a destination); no outcome filter chips (the shared table has no filter slot); no window selector; no capacity meter; no success-rate percentage.
7. **Fit with existing features** — Compounds with the Fleet console: every lease row links into the Fleet that produced the work, so runner triage and fleet triage are one path. The feature it must not destabilize is runner enrolment and the administrative state machine — those endpoints are untouched and covered by regression rows.
8. **Surface order** — User-Interface-first, and deliberately so: the repository default is Command-Line-Interface-first, but this is a platform-admin triage surface with no `agentsfleet` command today, and the operator reaching for it is already in the console. The two new reads are public, so a command-line consumer can follow without redesign.
9. **Dashboard restraint** — No control ships ahead of its evidence: `Open Grafana` renders only when configured; `total` may be null rather than fabricated; a lease with no Fleet event reads unknown rather than succeeded; a stale active lease is counted as neither live nor expired. No percentage, ratio or capacity figure appears anywhere.
10. **Confused-user next step** — A failed row already carries the cause in plain English plus the daemon's detail line, and links into the Fleet whose event failed. Transient database, queue and acknowledgement errors leave no terminal row, and for those the header's `Open Grafana` is the honest escape hatch.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Six Sections split API-before-UI so the three reads can land and be integration-tested before any component consumes them, then wall, then detail, then Activity-and-retirement together so no dead surface survives a partial landing. One workstream rather than several: the reads have no consumer without the pages, and the pages cannot hydrate without the reads.
- **Alternatives considered:** (a) *Hydrate the detail page from the existing list response.* Rejected — a refresh or a shared link would have nothing to read, which defeats the addressable-page goal. (b) *Keep the table and add an Overview page.* Rejected by Indy after review of the dense mockup; and Fleets proves the pattern needs no Overview. (c) *Three separate treatments of an Overview page* (one-line, two-card, lease-as-object) — all superseded once the Fleets pattern was adopted, since it removes the page rather than simplifying it. (d) *Match the neighbouring runner endpoints' page-based pagination for consistency.* Rejected — the guidelines name that shape as legacy and forbid it for new endpoints; the rule is the constant, the spec is the instance.
- **Patch-vs-refactor verdict:** this is a **refactor** because the existing surface has no shape to extend — a table with an icon dialog cannot express live work, and there is no runner page at all. Solution size matches problem size: the replacement copies a shipped pattern rather than inventing one, and the two existing endpoints it does not need are left alone rather than opportunistically migrated.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.

### Authoring record — design decisions and Indy's verbatim words

Design exploration ran as `/design-shotgun` on `main` before any code. Board artifacts:
`~/.gstack/projects/agentsfleet-usezombie/designs/runner-detail-20260729/` (`design-board.html` — the three rejected Overview treatments; `fleets-pattern.html` — the accepted direction).

**Accepted direction — the Fleets pattern.** Indy redirected away from an Overview page entirely:

> Indy (2026-07-29): "The main object is leases, and activity and why somethign failed, can we follow the pattern like Fleets?"

Verified against `fleets/[id]/page.tsx`: the Fleet detail page opens on `chat`, its main object, and has no Overview view. The runner equivalent opens on Leases.

**Rejected — a dense Overview page.** The prior mockup carried six sections, two tables of nine and six columns, nine figures, three footnotes, a window selector and a derived success rate:

> Indy: "Overview page is very complicated. i want you to think simple."

**Rejected — three simpler Overview treatments.** Authored and rendered before the Fleets redirect, all superseded: **A** one prose status line over flat lease rows with four trailing counters; **B** two cards splitting Now from Record; **C** one card per live lease with the counters collapsed to a single line. All three were still an Overview, which the Fleets pattern shows the product does not need.

**Accepted refinements, in Indy's words:**

> Indy (2026-07-29): "The Leases filter selection pill on All, Running, Failed, Expired must be removed."

> Indy (2026-07-29): "The Details has no value, Runner id, 01J2WQ8F3K7VZ9XB4N6MTYD5AR enrollement, label, isolation, can be merged with the Leases (enrolled becomes like ago - hover provides date)"

> Indy (2026-07-29): "Across all the pages we need to use our standard table and standard structure."

> Indy (2026-07-29): "And the status must be communicated like the standard approach across using ACTIVE BUSY etc.. like FLEETS?"

> Indy (2026-07-29): "The second like runner-prod... must be removed. The buttons must be aligned like Fleets Details … Failed count must be in a red color or so, Expired says 34 and reclaimed what does it mean? Succeeded 3945 must be green"

> Indy (2026-07-29): "remove enrollment, and activitys registered row is ffine"

The enrolment date therefore appears only on Activity's `registered` record, with the real date. The word "reclaimed" was replaced everywhere: an expired lease now states that the runner stopped renewing and the work was re-leased to another runner.

**Facts verified on `main` during authoring, not taken from prose:**

- Busy is `status='active' AND lease_expires_at > now` — `handlers/fleet/sql.zig` `SELECT_RUNNER_PAGE_FMT`. A `status='active'` row past its deadline is neither.
- A successful execution appends `lease_acquired` and `lease_released`, so roughly 4,000 executions produce roughly 8,000 lifecycle rows — `service_lease_row.zig` and `fleet/runner_events.zig`.
- The heartbeat body is empty (`handlers/runner/heartbeat.zig` — `_ = req; // S0 request body is empty.`), so the control plane holds no worker-capacity fact; no capacity or utilisation figure can be rendered honestly.
- `runner_online` is emitted only on a real offline-to-online transition — `handlers/runner/sql.zig` `HEARTBEAT_WITH_TRANSITION_EVENT`.
- Reclaim marks the prior lease `expired` atomically — `fleet/reclaim.zig`.
- `core.fleet_events` has no `runner_id`; attribution runs through `fleet.runner_leases (runner_id, fleet_id, event_id)`.
- `/v1/fleets/runners/{id}` serves PATCH and DELETE only — `route_table_invoke_runner.zig`. There is no single-runner GET and no operator-plane lease read.
- Per-runner Prometheus families exist but are process-global, in-memory, zeroed on restart and capped at 4096 runners — `docs/architecture/runner_fleet.md` §The four per-runner families. Rejected as a counter source in §1.
- The app has no Grafana reference of any kind; the base address lives only in the vault and `deploy/grafana/` does not exist.
