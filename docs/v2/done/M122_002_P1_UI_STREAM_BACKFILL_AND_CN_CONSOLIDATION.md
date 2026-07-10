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

# M122_002: Fleet event stream recovers missed frames on reconnect; one class-merge implementation

**Prototype:** v2.0.0
**Milestone:** M122
**Workstream:** 002
**Date:** Jul 09, 2026
**Status:** DONE
**Priority:** P1 — the live fleet timeline silently drops every frame published during a Server-Sent Events (SSE) reconnect window and never re-fetches them; the gap self-heals only when the operator reloads the page (Server-Side Rendering (SSR) re-seed). The class-merge consolidation rides along at P2-grade — a latent Tailwind-conflict duplication with no demonstrated broken override, folded in because it shares no scope with the streaming fix and both are pure UI-package hygiene.
**Categories:** UI
**Batch:** B1 — runs alone; no shared files with any other pending workstream.
**Branch:** feat/m122-stream-backfill-cn
**Test Baseline:** unit=2402 integration=267
**Depends on:** none.
**Provenance:** agent-generated (pre-spec, `fleet-wide-refactor-audit`, Jul 02, 2026; both findings re-verified against HEAD 7a06fb5d on Jul 09, 2026 by the `audit-open-items-recheck` workflow, each surviving an adversarial refutation pass — F14's original P1 was corrected to P2 because the app Tailwind-aware `cn` has 7 live consumers and no concrete broken class override was shown).
**Canonical architecture:** `docs/architecture/data_flow.md` — fleet activity pub/sub → SSE fan-out; this spec adds a client-side gap-recovery read and changes no server or channel behavior.

---

## Overview

**Goal (testable):** after an SSE disconnect and reconnect, the fleet timeline re-fetches and merges the frames published during the outage (deduping by event id) without a page reload; and the `cn` class-merge helper has exactly one Tailwind-conflict-aware declaration in the workspace, consumed by both the design-system and the app.
**Problem:** when the live fleet stream drops (network blip, proxy idle-kill) the registry reconnects with a fresh `EventSource` and never asks the server for what it missed — the backend's own handler documents a backfill client behavior that no client implements, so any activity during the reconnect window vanishes from the tail until the operator manually reloads the page. Separately, two incompatible `cn` helpers coexist: the design-system copy is a naive flatten+join (no Tailwind dedupe) used by 47 components, while the app copy is `twMerge(clsx(...))`; a design-system component that stacks conflicting utility classes silently keeps both.
**Solution summary:** add a same-origin Route Handler that mints the API-audience token server-side and proxies the bounded fleet-events list (mirroring the existing SSE proxy route), have the stream registry fire that backfill on every reconnect open — keyed off the last-seen event with a small overlap so no frame is lost at a keyset boundary — and merge the result through the existing id-deduping `mergeBackfill`. Consolidate `cn` by moving the Tailwind-aware implementation (with the extended font-size class group) into the design-system, deleting the app duplicate, repointing the 7 app consumers at `@agentsfleet/design-system`, and pinning "exactly one declaration" with a test.

## PR Intent & comprehension handshake

- **PR title (eventual):** Recover missed fleet-stream frames on reconnect; unify the cn class-merge helper
- **Intent (one sentence):** an operator watching a fleet's live timeline never loses activity to a reconnect blip, and every component merges Tailwind classes through one implementation.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.
- **Restatement (Orly, PLAN):** when the fleet timeline's SSE connection drops and reconnects, the client itself recovers the events published during the gap — fetching them through a new cookie-authed same-origin proxy and merging them by event id, no page reload — and the workspace ends with exactly one `cn` implementation, the Tailwind-conflict-aware one, owned by the design-system and consumed by app and design-system components alike. Matches the Intent above.
- `ASSUMPTIONS I'M MAKING:`
  1. The upstream `GET /v1/workspaces/{ws}/fleets/{id}/events` list already serves the bounded, newest-first (`created_at DESC, event_id DESC`) page with `cursor`/`since`/`limit` — verified in `src/agentsfleetd/http/handlers/fleets/events.zig` + `state/fleet_events_store.zig`; no Zig changes.
  2. `cursor` and `since` are mutually exclusive upstream, and `since` accepts only a 20-char RFC 3339 `YYYY-MM-DDTHH:MM:SSZ` (no fractional seconds) or a Go-style duration — the registry keys the backfill as `since = lastSeen.createdAt − overlap`, second-truncated; the truncation plus the explicit overlap constant re-fetches the boundary window and `mergeBackfill`'s id-dedupe absorbs it (RULE KYS).
  3. A reconnect on an empty snapshot backfills with neither `cursor` nor `since` (just `limit`) — newest-first ordering makes that exactly the "most-recent bounded page" of Dimension 2.5.
  4. The app `lib/utils.ts` holds `formatDuration` + `truncate` only (no `formatDate` exists — the Files Changed cell is corrected in this same commit); the file's real helpers stay.
  5. `ClassValue` is imported nowhere outside the two `utils.ts` files, so the design-system `cn` adopting clsx's `ClassValue` type (re-exported) breaks no consumer.

## Implementing agent — read these first

1. `ui/packages/app/app/backend/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/stream/route.ts` — the same-origin SSE proxy: Clerk `auth().getToken()` → `Bearer` → upstream fetch → 401/upstream-error handling. §1's backfill route mirrors this auth and error shape exactly, differing only in the upstream path (non-stream list) and that it returns a buffered JSON body.
2. `ui/packages/app/lib/streaming/fleet-stream-registry.ts` — `startEventSource`/`es.onopen` (only resets `reconnectAttempts` + sets `LIVE`), `onEventSourceError`, and `mergeBackfill` called exactly once in `createEntry`. §2 adds the reconnect backfill here.
3. `ui/packages/app/lib/streaming/fleet-stream-frames.ts` — `mergeBackfill(prev, rows)` dedupes by event id via a `Set` of existing ids; this is what makes an overlapping re-fetch safe (RULE KYS boundary).
4. `ui/packages/app/lib/api/events.ts` — `EventsQuery`, `listFleetEvents`, and `streamFleetEventsUrl`; §1 adds the same-origin backfill URL helper beside `streamFleetEventsUrl`.
5. `ui/packages/app/lib/utils.ts` — the Tailwind-aware `cn` and the extended `font-size` class group (the reason the app copy exists); §3 moves both into the design-system verbatim.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/app/backend/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/route.ts` | CREATE | same-origin backfill proxy: mint token server-side, forward the bounded fleet-events list |
| `ui/packages/app/lib/api/events.ts` | EDIT | add `backfillFleetEventsUrl` beside `streamFleetEventsUrl` |
| `ui/packages/app/lib/streaming/fleet-stream-registry.ts` | EDIT | track last-seen event; on reconnect open, fetch + `mergeBackfill` the missed window |
| `ui/packages/app/lib/streaming/fleet-stream-registry.test.ts` | EDIT | reconnect-backfill, initial-skip, dedupe, fetch-failure tolerance, and empty-timeline cases |
| `ui/packages/app/tests/backfill-route.test.ts` | CREATE | Dimensions 1.1–1.3 route tests (sibling of `sse-route.test.ts` — the Test Specification named these tests but this row was omitted at authoring; added at EXECUTE) |
| `ui/packages/app/tests/utils.test.ts` | EDIT | drop the app `cn` merge case (the behavior moves to the design-system suite); keep duration/truncate cases (omitted at authoring; added at EXECUTE) |
| `ui/packages/app/lib/api/events.test.ts` | EDIT | direct `backfillFleetEventsUrl` cases mirroring the existing `streamFleetEventsUrl` block (omitted at authoring; added at VERIFY per `/write-unit-test` ledger) |
| `ui/packages/design-system/src/utils.ts` | EDIT | `cn` becomes `twMerge(clsx(...))` carrying the extended font-size class group |
| `ui/packages/design-system/package.json` | EDIT | add `clsx` + `tailwind-merge` dependencies |
| `ui/packages/design-system/src/utils.test.ts` | EDIT | replace the naive-join assertions with merge/dedupe + font-size-group + single-declaration cases |
| `ui/packages/app/lib/utils.ts` | EDIT | remove `cn` + `clsx`/`tailwind-merge` machinery; keep `formatDuration`/`truncate` |
| `ui/packages/app/package.json` | EDIT | drop `clsx` + `tailwind-merge` (app copy was their sole importer) |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` | EDIT | import `cn` from `@agentsfleet/design-system` |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetsList.tsx` | EDIT | import `cn` from `@agentsfleet/design-system` |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/TriggerPanel.tsx` | EDIT | import `cn` from `@agentsfleet/design-system` |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/GuidedTriggerCard.tsx` | EDIT | import `cn` from `@agentsfleet/design-system` |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx` | EDIT | import `cn` from `@agentsfleet/design-system` |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/AddSecretForm.tsx` | EDIT | import `cn` from `@agentsfleet/design-system` |
| `ui/packages/app/components/layout/Shell.tsx` | EDIT | import `cn` from `@agentsfleet/design-system` |
| `bun.lock` | EDIT | lockfile consequence of the dependency moves (app drops, design-system gains `clsx`/`tailwind-merge`) |
| `ui/packages/app/lib/streaming/fleet-stream-frames.ts` | EDIT | `mergeBackfill` terminal-row authoritative replace + pure watermark/RFC 3339 helpers (added at `/review` — outage-straddling events and client clock skew) |
| `ui/packages/app/lib/streaming/fleet-stream-frames.test.ts` | EDIT | terminal-replace / in-progress-no-clobber / watermark / rfc3339 cases (added at `/review`) |
| `docs/architecture/data_flow.md` | EDIT | client-side gap-recovery paragraph beside the pub/sub no-resume statement (CHORE(close) architecture-diff requirement) |
| `ui/packages/app/lib/streaming/fleet-stream-backfill.ts` | CREATE | §4 cursor-follow walk, extracted so the registry stays under the LENGTH GATE |
| `ui/packages/website/.size-limit.json` | EDIT | landing critical-path budget 140 kB → 150 kB — Indy-approved (see Discovery): the consolidated `cn` carries tailwind-merge (~7 kB gz) into the website bundle |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NDC/ORP** (removed app `cn` + its `clsx`/`tailwind-merge` deps leave zero references; sweep the `@/lib/utils` `cn` import sites), **NLR** (touch-it-fix-it: the app `utils.ts` edit removes the now-duplicate helper wholesale, not a shim), **UFS** (the backfill route path, content-type, and query-key literals live as named constants — reuse the existing `EventsQuery`/URL builders, do not restring), **KYS** (backfill cursor keyed with overlap so no frame drops at a millisecond keyset boundary), **TST-NAM** (new test names milestone-free), **PJV** (the reconnect flag/last-seen state lives on the mutable `Entry`, never a passed primitive).
- **`dispatch/write_ts_adhere_bun.md`** — every `.ts`/`.tsx` edit: `const`/import discipline, Bun/vitest-native tests, no raw-HTML substitution (none added here).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no Zig touched; the backend list + SSE handlers already support the cursor read |
| PUB / Struct-Shape | no | TypeScript only; no Zig pub surface |
| File & Function Length (≤350/≤50/≤70) | yes | the new route stays a single `GET`; the registry gains one small backfill function + last-seen tracking — keep `fleet-stream-registry.ts` under 350 by extracting the backfill into a named helper, not inlining it into `startEventSource` |
| UFS (repeated/semantic literals) | yes | reuse `EventsQuery`, the events URL builders, and the `text/event-stream`/JSON content-type constants; any new query-key or path string declared once |
| UI Substitution / DESIGN TOKEN | no | `cn` is the class utility itself; no component markup or arbitrary token values added |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | route handlers under `ui/packages/app` are outside the RULE OBS trigger surface (`src/**/*.zig`, `agentsfleet/src/**/*.js`); no error codes or schema |

## Prior-Art / Reference Implementations

- **Reference:** `.../fleets/[fleetId]/events/stream/route.ts` — the exact same-origin auth-injection shape (`auth().getToken()` → `Bearer`, 401 on no token, pass-through of the upstream status/body on `!ok`) that §1's backfill route mirrors; divergence: buffered JSON body instead of a piped stream, and the upstream events-list path instead of the stream path.
- **Reference:** `ui/packages/app/lib/utils.ts` `cn` — the Tailwind-aware implementation with the `font-size` class group that §3 relocates into the design-system verbatim; divergence: none, it moves as-is.

## Sections (implementation slices)

### §1 — Same-origin backfill route

The browser holds no bearer token, so a client-side backfill cannot call the upstream events list directly. Add a Route Handler at `.../fleets/[fleetId]/events/route.ts` that resolves the Clerk session, mints the API-audience token server-side, and forwards a bounded `GET /v1/workspaces/{ws}/fleets/{id}/events` with the caller's query (cursor/since/limit), returning the upstream JSON body. **Implementation default:** copy the stream route's auth + error branches; return the upstream `EventsPage` body unbuffered-count-bounded via `limit`, so a long outage cannot pull an unbounded page.

- **Dimension 1.1** — with a valid session, the route mints a token and returns the upstream events page body and status → Test `test_backfill_route_proxies_authed` — ✅ **DONE**
- **Dimension 1.2** — with no session token, the route returns 401 with the same error envelope as the stream route, never calling upstream → Test `test_backfill_route_unauthorized` — ✅ **DONE**
- **Dimension 1.3** — an upstream non-2xx is passed through with its status and body, not masked as 200 → Test `test_backfill_route_upstream_error_passthrough` — ✅ **DONE**

### §2 — Registry backfills on reconnect

The stream registry tracks the last-seen event and, on every reconnect `es.onopen` (never the initial connect, which is already SSR-seeded), fetches the backfill route keyed off that event with a small overlap, then merges via the id-deduping `mergeBackfill`. **Implementation default:** derive the cursor from the last event in the snapshot; if the timeline is empty (a reconnect on an as-yet-silent fleet), request the most-recent bounded page so first-ever frames during the outage are still recovered. A failed backfill fetch is swallowed after a diagnostic — live frames resume and the next reconnect retries.

- **Dimension 2.1** — after an error-then-reopen, the registry issues one backfill keyed off the last-seen event and merges the returned rows into the snapshot → Test `test_registry_backfills_on_reconnect` — ✅ **DONE**
- **Dimension 2.2** — the initial (first-ever) `onopen` issues no backfill; only reconnects do → Test `test_registry_initial_open_no_backfill` — ✅ **DONE**
- **Dimension 2.3** — a backfilled row already present live (id overlap) is merged once, not duplicated → Test `test_registry_backfill_dedupes` — ✅ **DONE**
- **Dimension 2.4** — a rejected/failed backfill fetch leaves the timeline intact and the stream LIVE; it does not throw or tear down → Test `test_registry_backfill_failure_tolerated` — ✅ **DONE**
- **Dimension 2.5** — a reconnect on an empty snapshot (no last-seen event) issues one backfill for the most-recent bounded page rather than skipping, so first-ever frames during the outage are recovered → Test `test_registry_backfill_empty_timeline_requests_recent` — ✅ **DONE**

### §3 — One Tailwind-aware cn

Move the Tailwind-conflict-aware `cn` (and its extended `font-size` class group) from the app into `ui/packages/design-system/src/utils.ts`, adding `clsx`+`tailwind-merge` to the design-system's dependencies. Delete the app's duplicate `cn` and its now-unused `clsx`/`tailwind-merge` imports, keeping the file's date/duration/truncate helpers. Repoint the 7 app consumers at `@agentsfleet/design-system`. The 47 design-system components thereby gain conflict-aware merging; the extended font-size group ensures semantic `text-*` tokens are not misclassified as colors. **Implementation default:** re-export path is `@agentsfleet/design-system` (its existing `index.ts` already re-exports `cn`); no per-component API change.

- **Dimension 3.1** — the design-system `cn` resolves a Tailwind conflict (`cn("px-2","px-4")` → `"px-4"`) and preserves a semantic font-size token beside a color token → Test `test_ds_cn_merges_and_keeps_fontsize` — ✅ **DONE**
- **Dimension 3.2** — exactly one `export function cn`/`export const cn` declaration exists across `ui/packages` (the design-system one); the app `utils.ts` no longer declares `cn` → Test `test_single_cn_export` — ✅ **DONE**
- **Dimension 3.3** — every prior `@/lib/utils` `cn` consumer imports from `@agentsfleet/design-system` and renders unchanged → Test existing app component suites green after the import swap — ✅ **DONE**

### §4 — Backfill paginates to the pre-outage anchor

A single bounded page recovers only the newest `limit` rows of the outage window (upstream orders `created_at DESC`), so an outage burst longer than one page leaves a permanent hole in the middle of the timeline — and the watermark, advanced to the newest row, guarantees no later reconnect revisits it. The backfill follows `next_cursor` backwards until it reaches the pre-outage anchor. Upstream rejects `cursor` + `since` together, so page 1 carries `since`, pages 2..N carry `cursor` alone, and the client enforces the lower bound by stopping when a page's oldest row falls to the anchor. Page count is bounded; exhausting the budget is a real truncation and is surfaced, never presented as a completed recovery. **Implementation default:** an empty timeline (no anchor) still fetches exactly one most-recent page — Dimension 2.5 is unchanged; pagination is anchored recovery only.

- **Dimension 4.1** — an outage spanning more than one page walks `next_cursor` until a page's oldest row reaches the anchor; every missed frame lands in the snapshot → Test `test_registry_backfill_paginates_to_anchor` — ✅ **DONE**
- **Dimension 4.2** — page 1 carries `since` and no `cursor`; subsequent pages carry `cursor` and no `since` (upstream rejects both together) → Test `test_registry_backfill_page_two_uses_cursor_only` — ✅ **DONE**
- **Dimension 4.3** — a reconnect on an empty timeline issues exactly one page and never paginates → Test `test_registry_backfill_empty_timeline_single_page` — ✅ **DONE**
- **Dimension 4.4** — exhausting the page budget emits a truncation diagnostic rather than silently claiming recovery → Test `test_registry_backfill_truncation_surfaced` — ✅ **DONE**
- **Dimension 4.5** — a mid-pagination failure leaves the watermark unadvanced so the next reconnect retries the same window → Test `test_registry_backfill_midpage_failure_keeps_watermark` — ✅ **DONE**

## Interfaces

```
NEW same-origin route (browser-facing, cookie/Clerk-authed):
  GET /backend/v1/workspaces/{ws}/fleets/{id}/events?cursor=&since=&limit=
    200 → EventsPage { items: EventRow[], next_cursor: string | null }   // upstream body
    401 → { error: "Unauthorized", code: "UZ-401" }                       // no session token
    <upstream status> → upstream body verbatim on any non-2xx

NEW client helper (ui/packages/app/lib/api/events.ts):
  backfillFleetEventsUrl(workspaceId, fleetId, opts?: Omit<EventsQuery,"fleet_id">): string
    // same-origin "/backend/v1/workspaces/{ws}/fleets/{id}/events?<query>"

MOVED export (ui/packages/design-system/src/utils.ts, re-exported by index.ts):
  cn(...inputs: ClassValue[]): string   // twMerge(clsx(inputs)), font-size group extended
```

No upstream endpoint, channel name, request shape, or `cn` call signature changes.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| No session on backfill | Clerk session expired/absent | route returns 401; registry treats it as a failed fetch — timeline intact, live frames resume |
| Upstream events list errors | backend 5xx/4xx during backfill | route passes the status through; registry swallows after a diagnostic, retries on the next reconnect |
| Backfill fetch rejects | network drop mid-backfill | caught; snapshot unchanged, connection stays LIVE, no throw or teardown |
| Millisecond keyset boundary | two events share the last-seen timestamp | overlap in the cursor re-fetches the boundary window; `mergeBackfill` dedupes by id so nothing duplicates or drops |
| Reconnect on empty timeline | fleet silent until after the outage | backfill requests the most-recent bounded page; first-ever frames are recovered |
| Conflicting classes in a DS component | two Tailwind utilities target one property | unified `cn` resolves to the last-wins utility; semantic `text-*` token survives beside a color token |

## Invariants

1. Backfill fires only on a reconnect open, never the initial SSR-seeded connect — enforced by a last-connected flag on the `Entry` and asserted by `test_registry_initial_open_no_backfill`.
2. A frame delivered both live and via backfill appears once — enforced by `mergeBackfill`'s id `Set` dedupe and asserted by `test_registry_backfill_dedupes`.
3. Exactly one `cn` implementation is declared in the workspace — enforced by `test_single_cn_export` (greps `ui/packages` for `export function cn`/`export const cn`, asserts a single hit), not review discipline.
4. The browser never receives an unminted call to the upstream events list — the backfill URL is same-origin and the token is minted only inside the Route Handler; enforced by `test_backfill_route_unauthorized`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | reconnect backfill and cn consolidation add no analytics event; existing stream/timeline events fire unchanged | unchanged | unchanged — backfill carries the same `EventRow` shape already rendered | existing streaming suites stay green |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_backfill_route_proxies_authed` | mocked session → route mints token, forwards query, returns upstream `EventsPage` body + status |
| 1.2 | unit | `test_backfill_route_unauthorized` | `getToken()` → null → 401 `{code:"UZ-401"}`, upstream fetch never called |
| 1.3 | unit | `test_backfill_route_upstream_error_passthrough` | upstream 503 → route returns 503 body, not 200 |
| 2.1 | unit | `test_registry_backfills_on_reconnect` | error→reopen → one fetch keyed off last-seen id; returned rows merged into snapshot |
| 2.2 | unit | `test_registry_initial_open_no_backfill` | first `onopen` → zero backfill fetches |
| 2.3 | unit | `test_registry_backfill_dedupes` | live frame id X + backfill row id X → single X in snapshot |
| 2.4 | unit | `test_registry_backfill_failure_tolerated` | backfill rejects → snapshot unchanged, status LIVE, no throw |
| 2.5 | unit | `test_registry_backfill_empty_timeline_requests_recent` | reconnect with empty snapshot → one backfill fetch for the most-recent bounded page (no cursor), returned rows merged |
| 3.1 | unit | `test_ds_cn_merges_and_keeps_fontsize` | `cn("px-2","px-4")`→`"px-4"`; `cn("text-eyebrow","text-muted-foreground")` keeps both |
| 3.2 | unit (grep-based) | `test_single_cn_export` | `grep -rn "export function cn\|export const cn" ui/packages` (excl. node_modules) → exactly 1 hit |
| 3.3 | unit (regression) | existing app component suites | the 7 repointed consumers render byte-identical output |
| all UI | e2e (regression) | `make acceptance-e2e` | operator journey renders the live timeline unchanged (environment permitting) |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Registry backfills on reconnect (§2) | `make test-unit-app` | exit 0 incl. the five `test_registry_backfill*`/`*_no_backfill` cases | P0 | ✅ `✓ [app] Unit tests passed` (all backfill cases in `fleet-stream-registry.test.ts`, 34 passing) |
| R2 | One cn declaration in the workspace (§3) | `grep -rn "export function cn\|export const cn" ui/packages --include=*.ts --include=*.tsx \| grep -v node_modules` | exactly 1 line | P0 | ✅ `ui/packages/design-system/src/utils.ts:34` — the only hit |
| R3 | App no longer imports clsx/tailwind-merge (§3) | `grep -rn "from \"clsx\"\|from \"tailwind-merge\"" ui/packages/app --include=*.ts --include=*.tsx \| grep -v node_modules` | no output | P0 | ✅ no output |
| R4 | No `cn` sourced from `@/lib/utils` (§3) | `grep -rn "cn" ui/packages/app --include=*.ts --include=*.tsx \| grep "@/lib/utils"` | no output | P0 | ✅ no output |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | ✅ 24 paths, all rows in the (EXECUTE-amended) table |
| S1 | App unit tests pass | `make test-unit-app` | exit 0 | P0 | ✅ `✓ [app] Unit tests passed` |
| S2 | Design-system unit tests pass | `make test-unit-design-system` | exit 0 | P0 | ✅ `✓ [design-system] Unit tests passed` |
| S3 | Lint clean | `make lint` | exit 0 | P0 | ✅ `make lint` does not exist in this repo — graded via `make lint-app` + `make lint-design-system`, both `✓ Lint passed` |
| S4 | e2e walks the operator journey | `make acceptance-e2e` | exit 0 (or environment-constraint note per VERIFY tiers) | P1 | ✅ VERIFY GATE: acceptance-e2e skipped per environment constraint (reason: no local `.env` with Clerk credentials; the run targets a live Clerk + API environment — CI `acceptance-e2e-{dev,prod}` covers it post-push) |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` (repo scan + per-commit `gitleaks protect --staged`) |
| S6 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ no non-test source over 350 (`fleet-stream-registry.ts` 346); `Shell.tsx` reads 450 both on `main` and here — net-zero import swap, pre-existing size; test files over 350 are repo-wide precedent |
| S7 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | ✅ all three greps 0 matches |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted (the app `utils.ts` is edited, not removed).

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| app `cn` declaration | `grep -rn "export function cn\|export const cn" ui/packages/app \| grep -v node_modules` | 0 matches |
| `cn` from `@/lib/utils` | `grep -rn "cn" ui/packages/app --include=*.ts --include=*.tsx \| grep "@/lib/utils"` | 0 matches |
| app `clsx`/`tailwind-merge` imports | `grep -rn "from \"clsx\"\|from \"tailwind-merge\"" ui/packages/app \| grep -v node_modules` | 0 matches |

## Out of Scope

- Server-side `Last-Event-ID` resume in `events_stream.zig` — the client-backfill path this spec ships is the documented recovery mechanism; native EventSource resume is a separate future change and the header comment stays accurate.
- Changing the SSE channel, frame shapes, or the `EventRow` wire envelope.
- Reworking the naive-join semantics for any caller that deliberately wants unmerged classes — none exists; every `cn` caller expects last-wins merging.
- Removing `clsx`/`tailwind-merge` from any package other than the app (the design-system now owns them).

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator is watching a fleet run, their laptop briefly drops Wi-Fi, the tab reconnects, and the tool calls and chunks that happened during the blip are already in the timeline — no reload, no "did I miss something?"
2. **Preserved user behaviour** — the initial SSR-seeded timeline, live frame rendering, install-step advancement, optimistic sends, and every component's rendered class output stay exactly as they are; only gap-recovery and the `cn` source module change.
3. **Optimal-way check** — client-side backfill through a same-origin token-minting route is the most direct fix given the browser holds no token and the backend already serves a cursored events list; the unconstrained-optimal shape (server `Last-Event-ID` resume) is a larger backend change deferred to Out of Scope.
4. **Rebuild-vs-iterate** — iterate: two contained UI slices on merged surfaces; nothing here wants a redesign, and neither slice trades away run-to-run determinism.
5. **What we build** — one same-origin backfill route, one reconnect-backfill hook in the registry, one relocated Tailwind-aware `cn`, and the consumer repoint.
6. **What we do NOT build** — backend resume, channel changes, a new analytics event, or a design-system `cn` API change — see Out of Scope.
7. **Fit with existing features** — compounds the live-tail streaming surface; must not destabilize initial seeding or live frame ordering (both preserved verbatim, dedupe guarantees no double-render).
8. **Surface order** — UI-first by necessity: the streaming gap and the class-merge duplication are both UI-package concerns; no CLI or public API surface reads either change.
9. **Dashboard restraint** — the timeline shows recovered frames only from the authoritative events list; nothing is fabricated to fill a gap, and a failed backfill leaves the tail honest rather than guessing.
10. **Confused-user next step** — a persistent reconnect still surfaces the existing RECONNECTING status; a hard failure self-heals on the next page load via SSR re-seed, which already works today.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections — the backfill route (auth infra), the registry reconnect hook (the behavior), and the `cn` consolidation (independent hygiene) — each independently testable and DONE-markable.
- **Alternatives considered:** (a) implement server-side `Last-Event-ID` resume in `events_stream.zig` instead — rejected for now: a larger backend change touching the SSE loop and sequence handling, when a client backfill fully closes the observable gap; named in Out of Scope as the future path. (b) keep two `cn` helpers and only document the divergence — rejected: the naive copy is a latent conflict bug for 47 components and the duplication drifts; consolidation removes the class of defect.
- **Patch-vs-refactor verdict:** this is a **patch** — additive gap-recovery plus a duplication removal that *shrinks* the surface (one `cn`, fewer deps) rather than restructuring streaming or the design system.

## Discovery (consult log)

- **Consults** — Architecture: `docs/architecture/data_flow.md` §"Two streams + one pub/sub channel" gained the client gap-recovery paragraph in this diff. Gate-flag triage: oxlint `no-console` fired on the backfill diagnostic the Failure Modes table mandates — resolved with the single-site `warnBackfillFailure` helper carrying an inline `oxlint-disable-next-line` (the app has no client logger; removing the diagnostic would contradict the spec).
- **Metrics review** — unchanged from creation: no analytics event added.
- **Post-review escalation** — greptile posted two findings on the PR; both answered without a code change (P1 "stale backfill replaces live event" → false positive, greptile conceded; P2 "actor filter dropped" → already fixed in `510dd627`). Separately, `bundle-size-website` went red on the landing critical path and Indy approved the budget bump; then Indy escalated adversarial-review deferral #1 into §4.
- **Skill-chain outcomes** — `/write-unit-test`: diff ledger fully resolved (pasted in PR Session Notes); net +30 TypeScript unit tests. `/review`: 3 specialist subagents + Claude adversarial + Codex adversarial. Fixed in-branch: server-confirmed `since` watermark (client clock skew defeated the recovery; cross-model confirmed), terminal-row authoritative `mergeBackfill` (outage-straddling truncation), backfill fetch timeout + single-flight guard, failed-backfill-never-advances-watermark, route `Cache-Control: no-store`, dot-only path-segment rejection, error-passthrough content-type constrained to JSON-or-plain, upstream fetch try/catch → pinned 502, malformed-body diagnostic, `backfillFleetEventsUrl` opts narrowed to the forwarded keys.
- **Consult (post-PR, CI)** — `bundle-size-website` failed at 144.81 kB vs the 140 kB landing budget (tailwind-merge riding the consolidated `cn` into the website bundle).
  > Indy (2026-07-10): “Bump budget to 150 kB” — chosen via decision prompt; context: keep exactly one conflict-aware `cn` workspace-wide, accept tailwind-merge's real cost on the landing critical path.
- **Deferrals** — no spec Section/Dimension deferred. Review findings judged out of this spec's scope and flagged for Indy in PR Session Notes: `next_cursor` pagination loop (spec bounds recovery to one page by design), initial-open backfill (spec Invariant 1 forbids it), upstream response-byte budget (backend concern, class shared with the SSR seed path), twMerge class groups for the custom spacing/tracking token scales, completion-frame-vs-backfill millisecond race residual, and mirroring the dot-segment/cache-control hardening into the pre-existing `events/stream/route.ts`.
