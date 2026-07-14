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

# M132_001: The Live Wall and Getting Started

**Prototype:** v2.0.0
**Milestone:** M132
**Workstream:** 001
**Date:** Jul 14, 2026
**Status:** PENDING
**Priority:** P1 — the "dashboard" stops existing; a new operator either sees a first-run checklist that lands them on their first steered fleet, or a wall of live tiles — and every live tile must survive the Server-Sent Events (SSE) stream cap without going dark.
**Categories:** API, UI
**Batch:** B1 — the wall and Getting Started ship together: the same route (`/w/{ws}/`) decides which surface a user lands on, so they cannot be split across Pull Requests without a broken landing in between.
**Branch:** feat/mNN-name — added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** M131_001 (every tile links into the fleet console it builds — a tile whose `Open →` lands nowhere is a dead tile)
**Provenance:** Large Language Model (LLM)-drafted (claude-opus-4-8, Jul 14, 2026) — authored from the frozen variant-F design (`designs/fleet-dashboard-20260714/FREEZE.md`) and its route→handler→schema vetting matrix; every wire fact below is cited to that matrix rather than re-derived.
**Canonical architecture:** `docs/architecture/product_analytics.md` (onboarding funnel) · `docs/architecture/scaling.md` §SSE knobs (`SSE_MAX_STREAMS` budget — the wall's binding constraint)

---

## Overview

**Goal (testable):** `/w/{ws}/fleets` renders one tile per fleet — every live tile opens its own per-fleet SSE stream with a pulse and a footer of server-truth spend + event counts, every parked/killed tile is drained and dimmed, every tile links to its console, and a tile whose stream is refused by `sse_max_streams` or errors falls back to a last-event snapshot with a `snapshot` eyebrow (never a dead tile); `/w/{ws}/` renders a first-run Getting Started checklist (4 required + 2 optional steps, completion derived from live API state) plus a persistent bottom-left sidebar widget whose dismissal/collapse/manual-tick state is a per-user, per-workspace server preference that survives across devices.

**Problem:** The current `/w/{ws}/` is a static tile-count "dashboard" — it shows how many fleets are in each status and nothing living. A new operator has no guided path from zero to a steered fleet, and a returning operator has no at-a-glance view of what their fleets are doing right now. The information is on the wire — per-fleet spend and event counts already ride the fleet list row, workspace event history is index-backed, onboarding progress is derivable from `total`/`secrets`/`events` — but the client discards it and renders a count instead of a wall.

**Solution summary:** Replace the dashboard with two surfaces. `/w/{ws}/fleets` becomes a Live Wall: the client `Fleet` type widens to carry `budget_used_nanos` and `events_processed` (already served per row per `list.zig:173-175`), each live tile opens its per-fleet SSE stream (reusing the console's frame reducer), and a refused or errored stream degrades to a snapshot tile — visually indistinguishable except an eyebrow — so the uncapped-stream policy never produces a dead tile. `/w/{ws}/` becomes Getting Started: a checklist page whose four required steps are computed from existing endpoints (fleet list, secrets, workspace events) plus two optional steps (one manual tick, one provider check), and a persistent collapsible widget. Widget/checklist persistence is a new small per-user UI-prefs endpoint backed by one migration (gap G6, pulled forward per Indy's server-pref decision). The landing route picks Getting Started while onboarding is incomplete-and-not-dismissed, and the wall otherwise.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m132): the live wall and getting started — every tile streams or snapshots, never dies
- **Intent (one sentence):** A new operator is walked from zero to a steered fleet, and a returning operator sees a wall of tiles that are live where they can be and honest (snapshot) where the stream budget is spent — never blank.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `designs/fleet-dashboard-20260714/FREEZE.md` — §1 information architecture, §2 vetting matrix (every EXISTS/UI-ONLY/PARTIAL/MISSING verdict with evidence), §5 the AGREED plan incl. Indy's two overrides. Canonical; do not re-derive a verdict this file already carries.
2. `src/agentsfleetd/http/handlers/fleets/events_stream.zig` — the per-fleet SSE handler; `tryRegister(…, sse_max_streams)` at line 93 is the cap the wall's degradation path is built around (`ERR_SSE_STREAM_CAP` + Retry-After on refusal).
3. `src/agentsfleetd/http/handlers/tenant_provider.zig` + its route in `routes.zig` (`tenant_provider`) and scope in `route_scopes.zig` — the closest existing tenant-scoped GET/PUT handler for the new UI-prefs endpoint to mirror (route enum → invoke → scope → store).
4. `ui/packages/app/lib/streaming/fleet-stream-frames.ts` — the frame reducer the console uses (M130); the live tile reuses it rather than inventing a second stream consumer.
5. `docs/architecture/scaling.md` §"SSE_MAX_STREAMS" — the per-replica stream budget (~0.25 MiB stack per tail, default 64); the PLAN headroom line reads from here.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/028_core_user_ui_prefs.sql` | CREATE | Per-user, per-workspace UI-prefs key/value store (G6). Single-concern migration; GRANT to `api_runtime`; no static strings (pref keys validated in app). |
| `schema/embed.zig` | EDIT | Register migration 28 in the `migrations` array (RULE MIG — index tracks position). |
| `src/agentsfleetd/state/user_ui_prefs_store.zig` | CREATE | Read/upsert prefs rows keyed on `(user_id, workspace_id, pref_key)`; drain-audited queries. |
| `src/agentsfleetd/http/handlers/user_ui_prefs.zig` | CREATE | `GET …/ui-prefs` + `PUT …/ui-prefs/{key}` handler (via `Hx`); resolves the Clerk subject → `core.users.user_id`, authorizes the workspace; validates key registry + value size. |
| `src/agentsfleetd/http/routes.zig` | EDIT | New `workspace_ui_prefs` route variant. |
| `src/agentsfleetd/http/route_table_invoke.zig` | EDIT | Dispatch the new route to the handler. |
| `src/agentsfleetd/http/route_matchers.zig` | EDIT | Segment matchers for `/v1/workspaces/{ws}/ui-prefs` and `…/ui-prefs/{key}`. |
| `src/agentsfleetd/http/route_scopes.zig` | EDIT | Scope for the route (authenticated-only; ownership via `authorizeWorkspace`). |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | `ERR_UI_PREF_KEY_UNKNOWN` (`UZ-…`) for a pref key outside the named registry. |
| `src/agentsfleetd/http/handlers/user_ui_prefs_integration_test.zig` | CREATE | Real-schema integration coverage (RULE ITF): read-default, upsert, cross-tenant isolation, unknown key. |
| `ui/packages/app/lib/types.ts` | EDIT | Widen `Fleet` with `budget_used_nanos` and `events_processed` (already on the wire, dropped by the client). |
| `ui/packages/app/lib/api/fleets.ts` | EDIT | Surface the two widened fields through the list parse. |
| `ui/packages/app/lib/api/tenant_billing.ts` | EDIT | Comment-only: the `StatusTiles` reference at line 10 follows the tiles' removal, so the S9 sweep grep reaches 0 without leaving R6. |
| `ui/packages/app/lib/api/ui-prefs.ts` | CREATE | Client for `GET …/ui-prefs` / `PUT …/ui-prefs/{key}`; fail-open read (missing/error → empty prefs). |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.tsx` | CREATE | One tile: live (own SSE stream + pulse + footer) vs drained (parked/killed) vs snapshot (stream refused/errored); links to the console. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetWall.tsx` | CREATE | The wall grid over the fleet list; replaces the list layout. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetsList.tsx` | EDIT | Superseded by the wall for the fleets route; retained only where still referenced (else swept). |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/page.tsx` | EDIT | Render the wall. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/page.tsx` | EDIT | Becomes Getting Started (or redirects to the wall when onboarding is complete/dismissed) — the old status-tile rollup is removed. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/components/GettingStarted.tsx` | CREATE | The checklist page: 4 required + 2 optional steps, completion from live state. |
| `ui/packages/app/lib/onboarding.ts` | CREATE | Pure step-derivation from `{fleetTotal, secretCount, hasProcessedEvent, hasSteerEvent, providerSet, cliTicked}` → step states; single source for page and widget. |
| `ui/packages/app/lib/onboarding.test.ts` | CREATE | Step-derivation truth table. |
| `ui/packages/app/components/layout/GettingStartedWidget.tsx` | CREATE | The persistent bottom-left widget: strikethrough on completion, collapsible, dismissible; reads/writes prefs. |
| `ui/packages/app/components/layout/Shell.tsx` | EDIT | Mount the widget (bottom-left); no change to the existing non-persisted nav collapse. |
| `ui/packages/app/lib/analytics/events.ts` | EDIT | Onboarding funnel events (`getting_started_viewed`, `getting_started_dismissed`, `getting_started_cli_ticked`). |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **STS** (no static strings in the migration — pref keys are app-validated, not a SQL `CHECK`), **NSQ** (schema-qualified, named-constant SQL in the store), **SGR** (migration GRANTs `api_runtime`), **MIG** (migration index assertion tracks position 28), **WAUTH** (the new workspace-scoped handler calls `authorizeWorkspace` after authenticate), **HGD/RAD/HXX** (new endpoint follows the handler guide + REST checklist, responds through `Hx`), **ERR** (new `ERR_UI_PREF_KEY_UNKNOWN` in the registry), **UFS** (pref-key names, eyebrow labels, and the stream-cap error code are named constants shared verbatim across runtimes), **TGU** (tile state is a tagged union `live|drained|snapshot`, not optional-field soup), **TVR** (tests assert only reachable pref keys / tile states), **ASE** (the tile's SSE consumer catches rejections, never a bare try/finally), **PTK** (the prefs bag is read with `Object.hasOwn`, never `in`), **DID** (widget/checklist DOM ids via `React.useId()`), **NDC/NLR/ORP** (the old dashboard rollup + `FleetsList` remnants are deleted and swept, not stranded), **ITF** (integration tests hit the real schema), **XCC/ZIG** (Zig store + handler cross-compiled, init/deinit disciplined), **FLS/CNX** (queries drained; no two pool connections held per request).
- **`dispatch/write_sql.md`** — the migration + Schema Table Removal Guard (a CREATE-only migration; embed.zig + array updated in the same diff).
- **`dispatch/write_http.md`** / `docs/REST_API_DESIGN_GUIDELINES.md` — the new endpoint's shape, status codes, and error envelope.
- **`dispatch/write_zig.md`** — the store + handler (tagged-union results, errdefer, pg-drain, cross-compile).
- **`dispatch/write_ts_adhere_bun.md`** — UI substitution + design-token discipline for the tiles, wall, checklist, and widget.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — store + handler + tests | cross-compile both linux targets; tagged-union `PrefResult`; drain audit (`make check-pg-drain`). |
| PUB / Struct-Shape | yes — new pub store/handler surface | FILE SHAPE DECISION at PLAN for each new Zig + TS file. |
| File & Function Length (≤350/≤50/≤70) | yes | tile logic split by state (live/drained/snapshot) so no component nears the cap; wall grid stays presentational. |
| UFS (repeated/semantic literals) | yes | pref keys, eyebrow copy, `snapshot` marker, `SSE_MAX_STREAMS` client-side awareness — named constants, shared verbatim. |
| UI Substitution / DESIGN TOKEN | yes | tiles/widget/checklist use design-system primitives + theme tokens; dimming/pulse via tokens, not arbitrary values. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes | handler logs through `log`; store init/deinit; `ERR_UI_PREF_KEY_UNKNOWN` registered; migration single-concern ≤100 lines + embed.zig array. |

## Prior-Art / Reference Implementations

- **API/schema:** `tenant_provider.zig` + `027_core_tenant_model_entries.sql` — the tenant-scoped GET/PUT + `uid GENERATED … STORED` + UUIDv7 CHECK + GRANT shape the prefs endpoint and migration mirror exactly.
- **Streaming:** the console's per-fleet stream (`events_stream.zig`, `fleet-stream-frames.ts`) — the tile reuses this consumer; it does NOT invent a second SSE path.
- **UI capped-affordance honesty:** the wake-`PULSE_CAP` pattern (M130 `FleetsList.tsx`) for a bounded live affordance that degrades visibly rather than lying.

## Sections (implementation slices)

### §1 — The Live Wall renders every fleet

`/w/{ws}/fleets` becomes a grid of tiles from the fleet list. Each tile shows static facts (name, status, last wake — EXISTS via `fleets/list.zig`) and a footer of lifetime spend + event count. Both footer values are already on the wire per row (`budget_used_nanos`, `events_processed`, `list.zig:173-175`) and dropped by the client `Fleet` type — the fix is to widen the type, never to compute cost client-side. Cost is server truth (`credit_deducted_nanos` aggregate), never token×rate math. Parked (`stopped`/`paused`) and `killed` tiles render drained and dimmed with no stream; `installing` tiles show their transient state. The fleet list is cursor-paginated (`DEFAULT_LIST_PAGE_LIMIT=20`, `MAX_LIST_PAGE_LIMIT=100`, `list.zig:23-24`): the wall requests `limit=100` and keeps the load-more affordance — a fleet beyond the rendered page has no tile yet, and a tile streams only once rendered.

- **Dimension 1.1** — `Fleet` carries `budget_used_nanos` + `events_processed`; the list parse surfaces both → Test `test_fleet_type_carries_wire_aggregates`
- **Dimension 1.2** — a tile renders name/status/last-wake + a footer of server-truth spend + event count; no client cost arithmetic exists → Test `test_tile_footer_is_server_truth`
- **Dimension 1.3** — parked/killed tiles are drained+dimmed and open no stream; every tile (any state) links to `/w/{ws}/fleets/{id}` → Test `test_drained_tiles_no_stream_all_link_console`

### §2 — Uncapped per-fleet streams, with a snapshot floor (Indy override 2)

Every live tile opens its own per-fleet SSE stream and mints a pulse — no client-side cap on how many stream (Indy overrode the K=5 recommendation). The load-bearing consequence: a tile whose stream is refused by the server's `sse_max_streams` cap (`events_stream.zig:93` → `ERR_SSE_STREAM_CAP` + Retry-After) or errors mid-flight MUST fall back to a last-event snapshot + pulse, visually indistinguishable from a live tile except a `snapshot` eyebrow. Never a dead tile. Tile liveness is a tagged union `live | drained | snapshot`. The snapshot's last event comes from the workspace event history (`GET /v1/workspaces/{ws}/events`, index-backed per `schema/015:49`), so a refused tile still shows real recent activity.

**Implementation default:** the tile treats a `503`/`ERR_SSE_STREAM_CAP` and any stream error identically → snapshot, because the operator-visible outcome is the same (no live feed available) and a single fallback path is one thing to test, not two. **Snapshot semantics:** on cap-refusal the tile honors `Retry-After` with one retry, then stays `snapshot` until page reload; the snapshot's last event refreshes from the polled workspace history at the wall's existing refresh cadence.

**PLAN-time headroom line (conscious, mandatory):** `SSE_MAX_STREAMS` defaults to 64 per replica (`runtime_loader.zig:39`, ~0.25 MiB stack/tail per `scaling.md`). The wall's effective demand is `viewers × min(live fleets, rendered tiles)` — tiles beyond the load-more boundary open no stream (§1). The PLAN must state the realistic product against that default and note that raising it is a config change (`SSE_MAX_STREAMS` env), not code — the degradation path is the safety net, not the capacity plan. Browser connection limits bite only on HTTP/1.1; production rides HTTP/2 where SSE multiplexes, so dev-mode (h1) may degrade earlier — acceptable, it exercises the fallback.

- **Dimension 2.1** — a live tile opens its own stream and renders streaming frames + pulse via the reused reducer → Test `test_live_tile_streams_frames`
- **Dimension 2.2** — a tile whose stream is refused (`ERR_SSE_STREAM_CAP`/503) renders the snapshot state with last event + pulse + `snapshot` eyebrow, never blank → Test `test_capped_tile_degrades_to_snapshot` (FAILURE MODE)
- **Dimension 2.3** — a tile whose stream errors mid-flight transitions live→snapshot without a dead frame → Test `test_stream_error_falls_back_to_snapshot` (FAILURE MODE)

### §3 — Getting Started: the checklist

`/w/{ws}/` renders a first-run checklist. Four required steps, each derived from an existing endpoint (no new detection backend): ① Install a fleet — fleet list `total ≥ 1`. ② Connect its credential — `GET …/secrets ≥ 1` (the install gate already blocks creation on missing with `ERR_FLEET_BUNDLE_SECRETS_MISSING` + `missing_secrets[]`). ③ Watch it wake — `≥1` processed event via `GET /v1/workspaces/{ws}/events`. ④ Steer it — `≥1` event with actor prefix `steer:` (the endpoint's `actor_prefix` filter, `events.zig:91`). Two optional steps: install the Command-Line Interface (CLI) — NOT server-detectable, so a manual user tick (`npm install -g @agentsfleet/cli@next`) — per-workspace by construction (the pref row is keyed `(user, workspace, key)`); re-ticking in a second workspace is accepted; and bring your own model key — `GET /v1/tenants/me/provider`. Step derivation is a pure function so the page and the widget agree by construction. **Displaced dashboard content:** `ExhaustionBanner` mounts on Getting Started; the balance figure lives at `/settings/billing` (linked from Getting Started); the free-credit pill folds into step ① copy.

- **Dimension 3.1** — the 4 required steps compute true/false from `{fleetTotal, secretCount, hasProcessedEvent, hasSteerEvent}` → Test `test_required_steps_derive_from_state`
- **Dimension 3.2** — the 2 optional steps: provider from `GET /tenants/me/provider`, CLI from the persisted manual tick → Test `test_optional_steps_provider_and_cli_tick`
- **Dimension 3.3** — "onboarding complete" = all 4 required done; optional steps never block completion → Test `test_completion_requires_only_required`

### §4 — The persistent sidebar widget

A bottom-left widget mirrors the checklist for a returning user: each step strikes through on completion, the widget is collapsible, and it is dismissible once onboarding is complete. Its dismissal/collapse/CLI-tick state is read from and written to the prefs endpoint (§5), so it survives across devices. The existing non-persisted nav collapse (`Shell.tsx`) is unchanged.

- **Dimension 4.1** — a completed step renders struck-through; the widget collapses/expands → Test `test_widget_strikethrough_and_collapse`
- **Dimension 4.2** — dismiss writes the pref and hides the widget; a fresh load with the pref set keeps it hidden → Test `test_widget_dismiss_persists`

### §5 — G6: the per-user UI-prefs endpoint + migration (Indy override 1)

Getting Started persistence is a server preference from day one — Indy rejected the localStorage v1. A new migration adds `core.user_ui_prefs`, a small key/value store scoped `(user_id, workspace_id, pref_key)`: per user, per workspace, one row per named pref key. The Clerk subject on the principal resolves to `core.users.user_id`; the handler authorizes the workspace. Pref keys are a named registry validated in app (dismissed, collapsed, cli_ticked) — NOT a SQL `CHECK` (RULE STS). `GET` returns the bag (empty when unset); `PUT …/ui-prefs/{key}` upserts that one key — the client supplies the identifier on the path (PUT-as-upsert per the REST guide), so an unknown key is refused on the path (`ERR_UI_PREF_KEY_UNKNOWN`). Degrade honestly: a read that fails or 404s yields an empty bag → the widget shows (never hides onboarding on a read failure); a `PUT` that fails leaves the widget in its pre-dismiss state and surfaces a non-blocking retry — the fail-open direction is always "show onboarding", never "silently hide it".

**Implementation default:** value column is JSON text (`pref_value`) not typed columns, because the pref set grows (future toggles) and each key's shape is client-owned — the server stores an opaque small value it never interprets beyond the key allowlist — bounded in app by `MAX_UI_PREF_VALUE_BYTES` (named constant, 1024 bytes): an unbounded opaque JSON column is free tenant storage. ETag/`If-Match` is deliberately opted out (see Failure Modes).

- **Dimension 5.1** — `GET …/ui-prefs` returns an empty bag when nothing is set; `PUT` then `GET` round-trips a key → Test `test_ui_prefs_roundtrip`
- **Dimension 5.2** — a `PUT` with a pref key outside the named registry is rejected `ERR_UI_PREF_KEY_UNKNOWN`; no row written → Test `test_ui_prefs_unknown_key_rejected` (FAILURE MODE)
- **Dimension 5.3** — one user's prefs are invisible to another user / another tenant in the same query path → Test `test_ui_prefs_cross_tenant_isolation` (FAILURE MODE)

### §6 — Landing logic

Sign-in → `/` (existing: resolves the first workspace, redirects to `/w/{ws}/`). `/w/{ws}/` then decides: onboarding incomplete AND not dismissed → render Getting Started; otherwise redirect to the wall (`/w/{ws}/fleets`). The decision reads the same pure step-derivation (§3) plus the dismissed pref (§5).

- **Dimension 6.1** — an incomplete, non-dismissed workspace lands on Getting Started; a complete or dismissed one redirects to the wall → Test `test_landing_picks_surface`

## Interfaces

```
NEW  GET  /v1/workspaces/{ws}/ui-prefs        → 200 { prefs: { <pref_key>: <json-value>, … } }   (authenticated + workspace owner)
NEW  PUT  /v1/workspaces/{ws}/ui-prefs/{key}  body = <json-value> → 200 { prefs: {…} }
                                            unknown {key} → 400 ERR_UI_PREF_KEY_UNKNOWN · value > MAX_UI_PREF_VALUE_BYTES → 400
     pref_key ∈ named registry: "getting_started_dismissed" | "getting_started_collapsed" | "getting_started_cli_ticked"
     ETag/If-Match: opted out — last-write-wins per key (recorded in Failure Modes per the REST guide's concurrency section)

CONSUMED AS-IS (no wire change):
  FleetListItem.budget_used_nanos: i64, .events_processed: i64          (already served, list.zig:173-175)
  GET /v1/workspaces/{ws}/events?actor_prefix=steer:                    (steer detection, events.zig:91)
  GET /v1/workspaces/{ws}/fleets/{id}/events/stream                     (per-tile SSE; ERR_SSE_STREAM_CAP on refusal)
  GET /v1/tenants/me/provider                                          (optional step 6)

Client Fleet type gains: budget_used_nanos: number, events_processed: number.
Tile state (client): { kind: "live" } | { kind: "drained" } | { kind: "snapshot", lastEvent, reason }.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Stream refused at cap | `sse_max_streams` reached (`events_stream.zig:93`) → `ERR_SSE_STREAM_CAP` + Retry-After | Tile → `snapshot`: last event from workspace history + pulse + `snapshot` eyebrow. Never blank. |
| Stream errors mid-flight | Network blip / server drop | Tile transitions live→snapshot; no dead frame; pulse retained. |
| Prefs read fails / 404 | Endpoint down or nothing set | Empty bag; widget SHOWS (fail-open toward onboarding, never hides it silently). Pinned by `test_prefs_read_failure_widget_shows`. |
| Prefs write fails | Transient write error | Widget stays in its pre-action state; non-blocking retry surfaced; no silent dismiss. Pinned by `test_prefs_write_failure_no_silent_dismiss`. |
| Unknown pref key | Client sends a `{key}` outside the registry | `400 ERR_UI_PREF_KEY_UNKNOWN`; no row written. |
| Oversize pref value | `pref_value` exceeds `MAX_UI_PREF_VALUE_BYTES` (1 KiB, app-validated named constant) | `400`; no row written — the opaque JSON column is not free tenant storage. |
| Concurrent `PUT` to one key | Two devices toggle the same pref | Last-write-wins by design — explicit ETag/`If-Match` opt-out per the REST guide's concurrency section: a pref is a single scalar toggle; a lost write costs one click, not authored content. |
| Cross-tenant/user prefs read | Malicious or buggy workspace id | Workspace ownership check + user scoping → another user's/tenant's prefs are never returned. |

## Invariants

1. **No dead tile.** A tile is always exactly one of `live | drained | snapshot` (tagged union); a stream refusal or error maps to `snapshot`, enforced by the reducer's exhaustive switch and pinned by 2.2/2.3 — a "blank" state is unrepresentable.
2. **Cost is server truth.** The tile footer reads `budget_used_nanos`; no token×rate expression exists in the wall path — enforced by a grep rubric row.
3. **Onboarding never silently hidden.** The widget hides only when the `getting_started_dismissed` pref is truthy; a read failure yields an empty bag (shows) — enforced by the fail-open read and pinned by test.
4. **Pref keys are a closed named registry.** A key outside it is rejected in app (`ERR_UI_PREF_KEY_UNKNOWN`) — no SQL `CHECK` string (RULE STS); enforced by 5.2.
5. **Step derivation is single-sourced.** The page, widget, and landing logic all call the one pure function — enforced by construction (no second derivation exists) and the orphan sweep.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `getting_started_viewed` | product | Getting Started page renders for a user | workspace id, completed-step count | no email/token/secret material | `test_gs_viewed_emitted` |
| `getting_started_cli_ticked` | product | user manually ticks the CLI step | workspace id | none beyond ids | `test_gs_cli_ticked_emitted` |
| `getting_started_dismissed` | product | dismiss pref written | workspace id, completed-step count | none beyond ids | `test_gs_dismissed_emitted` |

Onboarding funnel: these three client events plus the server-authoritative `FleetTriggered`/`FleetCompleted` (already emitted, `product_analytics.md`) form the zero→steered funnel. Events single-sourced in `lib/analytics/events.ts` (`EVENTS`, `EVENT_PROP_KEYS`); no bare event-name literals. No server telemetry added — the four required steps are derived from existing endpoints, not new emits.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_fleet_type_carries_wire_aggregates` | a list row `{budget_used_nanos:1200000000, events_processed:7}` parses both fields onto `Fleet`; neither is dropped. |
| 1.2 | unit | `test_tile_footer_is_server_truth` | footer renders spend from `budget_used_nanos` + event count; no `tokens*rate` expression in the wall module (grep). |
| 1.3 | unit | `test_drained_tiles_no_stream_all_link_console` | `stopped`/`killed`/`paused` tiles are `drained`, open no stream; every tile (all kinds) has an href to `…/fleets/{id}`. |
| 2.1 | unit | `test_live_tile_streams_frames` | an `active` tile subscribes and renders a streamed frame + pulse via the shared reducer. |
| 2.2 | unit | `test_capped_tile_degrades_to_snapshot` | stream open returns `503 ERR_SSE_STREAM_CAP` → tile kind `snapshot`, last event shown, `snapshot` eyebrow present, not blank. |
| 2.3 | unit | `test_stream_error_falls_back_to_snapshot` | a mid-flight stream error transitions the tile live→snapshot; pulse retained; no empty render. |
| 3.1 | unit | `test_required_steps_derive_from_state` | `{fleetTotal:0…}` all false; `{fleetTotal:1, secretCount:1, hasProcessedEvent:true, hasSteerEvent:true}` all four true. |
| 3.2 | unit | `test_optional_steps_provider_and_cli_tick` | provider set → step 6 true; `cli_ticked` pref true → step 5 true; both independent of required steps. |
| 3.3 | unit | `test_completion_requires_only_required` | 4 required done + 0 optional → complete; 3 required + 2 optional → not complete. |
| 4.1 | unit | `test_widget_strikethrough_and_collapse` | a done step renders struck-through; toggling collapse hides/shows the step list. |
| 4.2 | unit | `test_widget_dismiss_persists` | dismiss PUTs `getting_started_dismissed`; a re-render with that pref set renders no widget. |
| 5.1 | integration | `test_ui_prefs_roundtrip` | `GET` before any write → empty bag; `PUT …/ui-prefs/getting_started_collapsed` body `true` then `GET` → bag carries it. |
| 5.2 | integration | `test_ui_prefs_unknown_key_rejected` | `PUT …/ui-prefs/bogus` → `400 ERR_UI_PREF_KEY_UNKNOWN`; row count unchanged. |
| FM §5 | integration | `test_ui_prefs_value_too_large_rejected` | `PUT` with a body over `MAX_UI_PREF_VALUE_BYTES` → `400`; row count unchanged. |
| FM §5 | unit | `test_prefs_read_failure_widget_shows` | prefs client mocked to fail (rejects/500) → empty bag; the widget renders; onboarding is never hidden by a read failure. |
| FM §5 | unit | `test_prefs_write_failure_no_silent_dismiss` | dismiss `PUT` mocked to fail → widget stays in its pre-action state; retry affordance rendered; no silent dismiss. |
| 5.3 | integration | `test_ui_prefs_cross_tenant_isolation` | user A's pref in ws-A is not returned to user B nor under ws-B; ownership denial is `403`. |
| 6.1 | integration/e2e | `test_landing_picks_surface` | incomplete+not-dismissed ws → Getting Started rendered; complete or dismissed → redirect to `…/fleets`. |
| e2e | e2e | `test_wall_and_onboarding_walk` | a user with no fleets sees Getting Started; after install+secret+event+steer the wall shows a live tile (or snapshot under a forced low cap). |
| regression | unit | `test_nav_collapse_still_unpersisted` | the existing `Shell` nav collapse remains per-load (not written to prefs) — the widget's persistence must not leak into it. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | No dead tile — refusal/error maps to snapshot (§2) | `cd ui/packages/app && bunx vitest run app/(dashboard)/w/[workspaceId]/fleets/components` | exit 0, snapshot-fallback tests green | P0 | |
| R2 | Cost is server truth — no token×rate in the wall (§1, Inv.2) | `git grep -nE "tokens? *[*] *rate|rate *[*] *tokens?" -- ui/packages/app/app/'(dashboard)'/w` | 0 matches | P0 | |
| R3 | Step derivation single-sourced + correct (§3) | `cd ui/packages/app && bunx vitest run lib/onboarding.test.ts` | exit 0 | P0 | |
| R4 | Prefs endpoint round-trips, rejects unknown keys, isolates tenants (§5) | `make test-integration-db` | exit 0 | P0 | |
| R5 | Migration is single-concern and registered (§5) | `wc -l schema/028_core_user_ui_prefs.sql && grep -c "version = 28" schema/embed.zig` | `≤100` lines and `1` | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes (HTTP + schema + Redis touched) | `make test-integration` | exit 0 | P0 | |
| S4 | e2e walks the wall + onboarding path | `make acceptance-e2e` | exit 0 | P0 | |
| S5 | No leaks (Zig allocator wiring touched) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -vE '\.md$\|\.test\.(ts\|tsx)$\|_test\.zig$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep (old dashboard rollup / list remnants) | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| (conditional) `ui/packages/app/lib/fleet-rollup.ts` if the status-tile rollup has no remaining consumer after the dashboard→Getting Started swap | `git grep -nw "countFleets" -- ui/packages/app \| grep -v test` → 0, then `test ! -f ui/packages/app/lib/fleet-rollup.ts` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| dashboard `StatusTiles` rollup | `git grep -nw "StatusTiles" -- ui/packages/app` | 0 matches (rollup replaced by Getting Started) |
| `countFleets` (if swept) | `git grep -nw "countFleets" -- ui/packages/app` | 0 matches outside its own deleted test |

## Out of Scope

- **The workspace-multiplexed SSE stream (gap G4) — M133_001.** M133 replaces the N-per-fleet connections with one multiplexed `GET /v1/workspaces/{ws}/events/stream` and **DELETES the N-connection mode this spec ships**. The uncapped-stream + snapshot-fallback design here is deliberately the interim: correct under the cap, replaced (not extended) by M133.
- **Server-side fleet-status rollup / 7-day aggregation endpoints (G3-v2)** — client-side or console-owned for now.
- **Cron/schedule create/update/delete** — triggers render read-only; write is **M105_001** (already pending).
- **Fleet console internals (G1/G2/G5)** — M131_001 owns the console; this spec only links into it.
- **Memory forget (G5), stale-copy + delete-confirm copy fixes (G7/G8)** — M131_001.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A new operator lands on Getting Started, installs a fleet, connects its credential, watches it wake, and steers it — then the checklist completes and they land on a wall where that fleet's tile is live and pulsing.
2. **Preserved user behaviour** — Existing routes still resolve (`/` → first workspace); the console, install flow, secrets, and events pages are untouched; the nav collapse stays per-load. The wall replaces the dashboard rollup, not any user action.
3. **Optimal-way check** — Direct: the wall reads facts already on the wire (widen a type), onboarding derives from existing endpoints (no detection backend), and only persistence needs new backend — the smallest addition Indy's cross-device requirement allows.
4. **Rebuild-vs-iterate** — Rebuild the dashboard surface (it was a dead-end count), iterate everything else. The stream model is knowingly interim: M133 rebuilds it multiplexed. No determinism is traded — tile state is a pure function of stream outcome.
5. **What we build** — Wall (widened type + tiles + snapshot fallback), Getting Started (checklist page + persistent widget + pure derivation), the UI-prefs endpoint + migration, and the landing decision.
6. **What we do NOT build** — Multiplexed stream (M133), server rollup endpoints, schedule writes (M105), console internals (M131), memory forget (M131). One-line reasons each in Out of Scope.
7. **Fit with existing features** — Compounds with M131 (tiles link into the console) and M130 (reuses the stream frame reducer and server-truth cost). Must not destabilize the per-fleet SSE handler — the wall is a new consumer of it, not a change to it.
8. **Surface order** — Both API and UI in one workstream: the persistence endpoint and the surfaces that consume it ship together because the landing decision needs the pref on day one (Indy's server-pref override). No CLI surface.
9. **Dashboard restraint** — The wall shows only counters already backed by wire truth (spend, events); no quality claim, no control the backend cannot honor. A tile never claims "live" when its stream is gone — the `snapshot` eyebrow is the honesty marker.
10. **Confused-user next step** — A refused stream self-explains via the `snapshot` eyebrow (with the last real event); a failed prefs write surfaces a retry, never a silent loss; the checklist itself is the self-serve next step from zero.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Six Sections split by surface and by the two overrides — wall render (§1), the uncapped-stream + snapshot floor (§2, the load-bearing risk gets its own slice), the checklist (§3), the widget (§4), the persistence backend (§5), and the landing decision (§6). The stream degradation is isolated so its two failure tests are unmissable.
- **Alternatives considered:** (a) cap live streams at K=5 with a "+N more" affordance — rejected by Indy (override 2); the snapshot floor makes uncapped safe. (b) localStorage persistence for v1 — rejected by Indy (override 1); server pref from day one, hence G6 lands here not in M133. (c) fold the wall into M133's multiplexed stream — rejected: the wall ships now on per-fleet streams; M133 swaps the data source under it.
- **Patch-vs-refactor verdict:** **patch** for the data (widen a type, reuse the reducer, derive from existing endpoints) plus a **small new backend** (one endpoint, one migration) — the minimum Indy's cross-device requirement demands. The genuine refactor (multiplexed stream) is deliberately deferred to M133, named, not mud-patched here.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage:
- **Metrics review** —
- **Skill-chain outcomes** —
- **Deferrals** —
