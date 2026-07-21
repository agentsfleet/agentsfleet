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

# M137_001: Steer-first console with plain-language wall and console copy

**Prototype:** v2.0.0
**Milestone:** M137
**Workstream:** 001
**Date:** Jul 20, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — customer-facing: operators cannot read the wall/console today (cryptic labels, overlapping columns, buried composer)
**Categories:** UI
**Batch:** B1 — standalone; no sibling workstreams
**Branch:** feat/m137-steer-first-console
**Test Baseline:** unit=2806 integration=371
**Depends on:** none — M131_001 (console) and M132_001 (wall) are in `done/`
**Provenance:** LLM-drafted (Claude Fable 5, Jul 20, 2026) — from Indy's dev-session review of `app-dev` screenshots
**Canonical architecture:** `docs/DESIGN_SYSTEM.md` §Operational Restraint; frozen reference `~/.gstack/projects/agentsfleet-agentsfleet/designs/fleet-dashboard-20260714/{variant-F-ia.html,FREEZE.md}`

---

## Overview

**Goal (testable):** The fleet console renders three non-overlapping columns with the steer thread as the visually primary surface, a `← Fleets` back affordance, and every wall-tile / metrics label readable as plain English — asserted by unit label tests and an e2e pass over the rendered console.
**Problem:** Operators reading the wall and console today hit three walls: (1) cryptic labels — "LAST KNOWN", "10 ev", metrics "WALL" — that Indy himself could not decode; (2) the console's left-column cards inflate past their grid track on long source lines and paint under the middle column, wrecking the page; (3) the Source editor visually dominates while the steer composer — the point of the page per the Jul 14 freeze — is buried, and there is no way back to the wall except the sidebar.
**Solution summary:** Copy-and-layout changes inside the frozen variant F design, no backend. Wall tiles spell out their footer ("$0.00 spent · 10 events · 6 hours ago") and replace the "last known" eyebrow with "not live" plus a tooltip; the wall header drops the legacy search box (M94 carry-over, not in variant F — Indy's removal call, Jul 21, 2026) leaving title · live count · Install fleet. The console gains the frozen-but-never-built `← Fleets` back link, renames the metrics "Wall" label to "Time", collapses the Source card to its header by default (expand on demand; Edit auto-expands), orders the steer column first when columns stack, and locks column children to their grid track (`min-w-0` chain) so wide source/commands scroll inside their own blocks.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(ui): steer-first console, plain-language wall and console labels
- **Intent (one sentence):** An operator landing on the wall or console understands every label without decoding, immediately sees the steer thread as the main surface, and can navigate back to the wall.
- **Handshake** (filled at PLAN) — Restatement: make the wall and console legible and honest for a first-time operator — plain words on every figure, the steer thread visually first, a way back to the wall, no layout breakage, and the legacy search box gone. ASSUMPTIONS I'M MAKING: (1) "not live" is the eyebrow wording, with the one-sentence tooltip carrying the explanation; (2) the collapse state is per-visit React state, not persisted; (3) the back affordance is a single eyebrow-styled link above the page header, not a breadcrumb component; (4) stacking order is CSS `order-first lg:order-none` on the steer column, DOM order unchanged for the three-column layout.

## Implementing agent — read these first

1. `~/.gstack/projects/agentsfleet-agentsfleet/designs/fleet-dashboard-20260714/variant-F-ia.html` — the frozen design; its console header carries `← Fleets · active · wake on event` (the back affordance this spec implements) and the tile/console vocabulary to stay within.
2. `~/.gstack/projects/agentsfleet-agentsfleet/designs/fleet-dashboard-20260714/FREEZE.md` — §1 fixes the console column roles ("the steer thread + composer (the point of the page)") and §5.4 fixes snapshot degradation semantics the eyebrow rename must preserve.
3. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.tsx` + `ui/packages/app/lib/wall/tile-liveness.ts` — current tile markup, liveness derivation, and the footer formatters to relabel.
4. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` + `components/console-copy.ts` — console shell (grid, column order) and the single home for console strings.
5. `ui/packages/design-system/src/index.ts` — the primitive set; any new disclosure/back-link affordance composes existing primitives (UI GATE), no new raw HTML controls.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.tsx` | EDIT | Plain-language footer labels, "not live" eyebrow + tooltip, strings extracted to named consts |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.test.tsx` | EDIT | Assert new labels, eyebrow, tooltip |
| `ui/packages/app/lib/wall/tile-liveness.ts` | EDIT | Home for the wall copy consts if the agent places them beside the formatters |
| `ui/packages/app/lib/wall/tile-liveness.test.ts` | EDIT | Cover any moved/added consts and formatter suffix changes |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetWall.tsx` | EDIT | Remove the search box, client-side filter state, and its empty-state branch; header becomes live count + Install fleet per variant F |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetWall.test.tsx` | EDIT | Replace the filter tests with the no-search header assertion |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` | EDIT | Back link, steer-column stacking order, `min-w-0` column-child lock (partially in working tree — port the uncommitted diff) |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/console-copy.ts` | EDIT | `METRICS_WALL_LABEL` → `METRICS_TIME_LABEL = "Time"`; new back-link and disclosure consts |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/RunMetricsStrip.tsx` | EDIT | Consume renamed label const |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/RunMetricsStrip.test.tsx` | EDIT | Assert "Time" label |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/SkillEditor.tsx` | EDIT | Collapsed-by-default source card with expand disclosure; Edit auto-expands; collapse disabled while editing |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/SkillEditor.test.tsx` | EDIT | Collapsed default, expand, Edit-auto-expand, draft-preservation tests |
| `ui/packages/app/tests/dashboard-fleets-wall.test.tsx` | EDIT | Wall-level assertions updated to new labels |
| `ui/packages/app/tests/e2e/acceptance/fleet-console.spec.ts` | EDIT | Back-nav walk + no-horizontal-overflow assertion |
| `ui/packages/app/components/domain/EventsList.tsx` | EDIT | Card list → standard `DataTable`; dead preview/fleet arms removed |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/events/page.tsx` | EDIT | Pass the simplified `EventsList` props |
| `ui/packages/app/tests/events-components.test.ts` | EDIT | Assert table rendering; drop preview/fleet-arm tests |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/events/actions.ts` | EDIT | Delete `listFleetEventsAction` — the table rewrite removed its last production caller, and an exported server action is a live network endpoint |
| `ui/packages/app/tests/events-actions.test.ts` | EDIT | Drop the deleted action's forwarder tests |
| `ui/packages/app/tests/e2e/acceptance/logs-detail.spec.ts` | EDIT | Header comment updated — described the deleted card list |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (every relabel becomes a named const referenced by component and test; ui/ manual pass per the façade carve-out), NLR (touching `METRICS_WALL_LABEL` renames the const, not just its value), NDC (no dead disclosure states or unused copy consts), NRC (no "renamed from Wall" comments), TST-NAM (test names describe behaviour, no milestone IDs).
- `dispatch/write_ts_adhere_bun.md` — the entire diff is `*.tsx`/`*.ts` under `ui/packages/app`; §2 const discipline and the UI/DESIGN TOKEN gate sections govern every edit.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no Zig touched | — |
| PUB / Struct-Shape | no — no Zig pub surface | — |
| File & Function Length (≤350/≤50/≤70) | yes — SkillEditor.tsx is ~300 lines pre-edit | If the disclosure state pushes it near 350, extract `DocumentPane`/`ChangePreview` into a sibling component file added to this table |
| UFS (repeated/semantic literals) | yes — new user-visible strings | All labels/tooltips as named consts in `console-copy.ts` / wall copy home; tests reference the consts |
| UI Substitution / DESIGN TOKEN | yes — every edit is dashboard `*.tsx` | Compose existing primitives (Card, Button, Tooltip, Link via `asChild`); token utilities only, no new arbitraries |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — no logging, lifecycle, error-code, or schema surface touched | — |

## Prior-Art / Reference Implementations

- **Reference:** `variant-F-ia.html` (frozen Jul 14, 2026) — console header back affordance and vocabulary; this spec implements what the freeze already drew, diverging only where FREEZE.md amended it (footer = spend · events · last update, not tokens · cost).
- **Reference:** `ui/packages/design-system/src/index.ts` primitives + `theme.css` tokens — disclosure and back link compose existing `Button`/`Link`/eyebrow patterns as used by `GettingStarted` and `KillSwitch`; no new visual primitive is invented.

## Sections (implementation slices)

### §1 — Plain-language wall tiles

The wall tile's footer and degradation eyebrow become readable without decoding. Footer renders `{spend} spent · {n} events · {relative time}`; the `ev` abbreviation dies. The snapshot eyebrow "last known" becomes **"not live"** with a tooltip explaining "Live feed unavailable — showing the last activity received." "catching up" stays (already plain). **Implementation default:** keep the existing `title` tooltips on spend/events and the mono/tabular footer styling — Operational Restraint means words, not layout changes.

- **Dimension 1.1** — Footer events figure renders with the word "events", never bare "ev" → Test `test_tile_footer_spells_out_events` — **DONE**
- **Dimension 1.2** — Footer spend renders with the suffix "spent" from the server figure → Test `test_tile_footer_labels_spend` — **DONE**
- **Dimension 1.3** — Snapshot tile shows eyebrow "not live" with the explanatory tooltip; live tile shows neither → Test `test_snapshot_eyebrow_reads_not_live` — **DONE**
- **Dimension 1.4** — All new tile strings are named consts referenced by component and tests → Test `test_wall_copy_consts_are_single_source` — **DONE**
- **Dimension 1.5** — The wall header renders live count + Install fleet and no search input; the client-side filter state and its "No fleets match" branch are deleted (a zero-fleet workspace routes to Getting Started before the wall renders, so the branch is unreachable) → Test `test_wall_header_has_no_search` — **DONE**

### §2 — Console back affordance and plain metrics

The console header gains `← Fleets` linking to the wall (frozen in variant F, never built), and the metrics strip's "Wall" label becomes "Time" — "Wall" collides with the product's own Live Wall vocabulary and means nothing to an operator. The const renames to `METRICS_TIME_LABEL` (RULE NLR).

- **Dimension 2.1** — Console header renders a `← Fleets` link whose href is the workspace fleets route → Test `test_console_back_link_targets_wall` — **DONE**
- **Dimension 2.2** — Metrics strip renders "Time" for `wall_ms`; no rendered surface says "Wall" → Test `test_metrics_strip_labels_time` — **DONE**

### §3 — Steer-first console hierarchy

The steer thread + composer is the point of the page (FREEZE §1); the layout must say so. The Source card collapses to its header row (title + Edit + expand disclosure) by default; expanding reveals the tabs and viewer; pressing Edit from collapsed auto-expands into the editor; collapse is disabled while editing so a draft can never be hidden or lost. Below the `lg` breakpoint the steer column stacks first. Column children are locked to their grid track (`min-w-0` on each column's direct children) so long source lines, webhook URLs, and registration commands scroll inside their own `overflow-x-auto` blocks instead of painting under the neighbouring column — this half exists as an uncommitted working-tree diff on `main`; port it into the branch.

- **Dimension 3.1** — Source card renders collapsed by default: header visible, document panes absent until expanded → Test `test_source_card_collapsed_by_default` — **DONE**
- **Dimension 3.2** — Expand disclosure reveals the SKILL.md/TRIGGER.md panes; collapsing hides them again → Test `test_source_card_expand_toggle` — **DONE**
- **Dimension 3.3** — Edit pressed while collapsed expands into the editor; collapse control is disabled while editing → Test `test_edit_auto_expands_and_pins_open` — **DONE**
- **Dimension 3.4** — Steer column is first in stacked (below-`lg`) order, middle in the three-column order → Test `test_steer_column_stacks_first` — **DONE**
- **Dimension 3.5** — Each console column applies the `min-w-0` child lock; the rendered console page has no horizontal document overflow → Test `test_console_columns_never_overlap` (e2e) — **DONE**

### §4 — Events page in the standard table

The workspace Events page renders card-shaped rows today while every sibling data surface (API keys, secrets, runners, billing usage) uses the design-system `DataTable`. Indy's call (Jul 21, 2026): Events joins the standard table. Columns: Time · Status · Fleet · Actor · Type · Summary (truncated preview, or the warning-toned failure reason) · Tokens · Duration; secondary columns hide on mobile via the primitive's `hideOnMobile`. Cursor pagination, the error alert, and the empty state stay. The rewrite also deletes `EventsList`'s two production-dead arms — the `viewAllHref` preview mode and the `fleet` scope (only tests exercise them; the sole live consumer is the workspace Events page) — per RULE NLR.

- **Dimension 4.1** — Events page renders the standard `DataTable` with the column set above; failure rows carry their plain-language reason → Test `test_events_page_uses_standard_table` — **DONE**
- **Dimension 4.2** — Dead preview/fleet arms removed; `EventsList` takes `workspaceId` + `initial` only → Test `test_events_table_paginates_by_cursor` (regression: load-more appends) — **DONE**

## Interfaces

```
No HTTP, CLI, or wire interface changes. Locked component surfaces:
- FleetTile props unchanged: { fleet: Fleet; workspaceId: string }
- SkillEditor props unchanged; new internal disclosure state only
- console-copy.ts remains the single export home for console strings;
  METRICS_WALL_LABEL is renamed (not aliased) to METRICS_TIME_LABEL
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Long unbreakable line | SKILL.md line, webhook URL, or registration command wider than the column | Scrolls inside its own block; column tracks and neighbours unaffected — e2e asserts no horizontal document overflow |
| Stream refused/errored | Server stream cap or reconnect loop | Tile degrades to "not live" eyebrow + static dot + last event (FREEZE §5.4 semantics unchanged) — never a dead tile |
| Zero-event fleet | Fresh install, nothing processed | Footer reads "0 events"; metrics strip renders its existing empty copy unchanged |
| Empty TRIGGER.md | `trigger_markdown` null | Expanded trigger pane shows the existing empty hint (regression) |
| Draft at risk | Operator tries to collapse mid-edit | Collapse control disabled while editing; draft state untouched |

## Invariants

1. Every user-visible string added or changed by this diff is a named const referenced by both component and test — enforced by the UFS manual pass and by tests importing the consts rather than re-typing literals.
2. Spend/cost figures remain server truth rendered through the existing formatters (`formatTileSpend`, `formatDollars`); the diff introduces no client-side cost arithmetic — enforced by no new numeric operations on `*_nanos` fields in the diff.
3. Collapsing the source card can never discard an editing draft — enforced at runtime by disabling collapse while `editing` is true, proven by `test_edit_auto_expands_and_pins_open`.
4. The page issues no new network reads; data loading in `page.tsx` is unchanged — enforced by the untouched `Promise.all` read set.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes; copy and layout only, existing `fleet_source_saved` analytics untouched | — | — | — | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | FleetTile "live kind + server-truth footer" | Fleet with `events_processed: 7` → footer text "7 events", never the bare token "ev"; missing aggregates render "— events" |
| 1.2 | unit | FleetTile "live kind + server-truth footer" | Fleet with known `budget_used_nanos` → footer "$1.20 spent"; missing field renders "— spent" |
| 1.3 | unit | FleetTile "reconnecting stream degrades to a snapshot tile" | Stream state reconnecting/hello-without-live → eyebrow "not live" + tooltip attribute present; live state → neither rendered |
| 1.4 | unit | `test_wall_copy_consts_are_single_source` | Rendered labels equal the exported consts (imported, not re-typed) |
| 1.5 | unit | `test_wall_header_has_no_search` | Wall with fleets → no search input in the header; live count and Install fleet render; all tiles render unfiltered |
| 2.1 | e2e | `test_console_back_link_targets_wall` (in the console e2e walk) | Clicking "← Fleets" on the console navigates to the workspace fleets wall |
| 2.2 | unit | `test_metrics_strip_labels_time` | Event with `wall_ms` → strip shows "Time"; rendering contains no "Wall" label |
| 3.1 | unit | `test_source_card_collapsed_by_default` | Fresh render → header present, SKILL.md pane absent |
| 3.2 | unit | `test_source_card_expand_toggle` | Expand → panes visible; collapse → hidden; empty TRIGGER.md shows existing hint (regression) |
| 3.3 | unit | `test_edit_auto_expands_and_pins_open` | Edit from collapsed → textarea visible; collapse control disabled while editing; draft text survives |
| 3.4 | e2e | `test_steer_column_stacks_first` (in the console e2e walk) | At a 390px viewport the "What it does" region's top sits above "What it is" |
| 3.5 | e2e | `test_console_columns_never_overlap` (in the console e2e walk) | Seeded fleet with long source lines → document scrollWidth ≤ clientWidth + 1 |
| reg | unit | existing FleetTile/SkillEditor/RunMetricsStrip suites | All pre-existing behaviour (liveness derivation, save flow, 412 reload, cost fields) passes unmodified except label assertions |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Wall tile carries no cryptic labels (§1) | `grep -cE '\bev\b\|last known' "ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.tsx"` | 0 | P0 | ✅ `0` |
| R2 | Back affordance exists in console copy (§2) | `grep -c "← Fleets" "ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/console-copy.ts"` | 1 | P0 | ✅ `1` |
| R6 | Wall search box is gone (§1) | `grep -rn "Search loaded fleets\|Search fleets" ui/packages/app --include='*.tsx' \| grep -v '.test.tsx' \| wc -l` | 0 (test-file negative assertions excluded) | P0 | ✅ `0` — sole remaining hit is the test asserting absence |
| R7 | Events page is the standard table (§4) | `grep -c "DataTable" ui/packages/app/components/domain/EventsList.tsx` | ≥ 1 (and `git grep -w viewAllHref` = 0 in source) | P0 | ✅ `4`; viewAllHref 0 source matches |
| R3 | Metrics label renamed, old const gone (§2) | `grep -rc "METRICS_WALL_LABEL" ui/packages/app --include='*.ts*' \| grep -v ':0' \| wc -l` | 0 | P0 | ✅ `0` |
| R4 | Steer-first + overlap lock in the console shell (§3) | `grep -cE 'order-first\|\*:min-w-0' "ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx"` | ≥ 2 (class usages; comments may also match) | P0 | ✅ `4` |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | ✅ 20/20 paths listed |
| S1 | Unit tests pass | `make test-unit-app` | exit 0 | P0 | ✅ `1644 passed (175 files)` |
| S2 | Lint clean (Oxlint + tsc + design tokens) | `make lint-app` | exit 0 | P0 | ✅ `Lint passed` |
| S4 | e2e walks the console (back-nav + no overflow) | `cd ui/packages/app && bun run test:e2e:acceptance:local tests/e2e/acceptance/fleet-console.spec.ts` | exit 0 | P0 | ⏳ VERIFY GATE: skipped per environment constraint (needs the live acceptance stack + fixture credentials; runs in CI on the PR) |
| S7 | No secrets | `gitleaks protect --staged` | exit 0 | P0 | ✅ `no leaks found` (pre-commit hook) |
| S8 | No oversize source file (tests exempt per the canonical length audit) | `git diff --name-only origin/main \| grep -v -E '\.md$\|^docs/\|\.test\.\|\.spec\.\|/tests?/' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ no output |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `METRICS_WALL_LABEL` | `git grep -rn -w "METRICS_WALL_LABEL"` | 0 matches |
| Wall search filter (placeholder, aria-label, empty-state copy) | `git grep -rn "Search loaded fleets\|No fleets match"` | 0 matches |
| `viewAllHref` preview mode (production-dead; only the deleted tests used it) | `git grep -rn -w "viewAllHref"` | 0 matches |
| `listFleetEventsAction` (caller-less exported server action = dead network surface) | `git grep -rn -w "listFleetEventsAction"` | 0 matches |
| `METRICS_COST_UNKNOWN` (renamed — the dash placeholders every figure, not just cost) | `git grep -rn -w "METRICS_COST_UNKNOWN"` | 0 matches |

## Out of Scope

- Workspace-multiplexed live stream and reconnect robustness — M133_001, already IN_PROGRESS on `feat/m133-workspace-stream`; folding a backend stream into this UI-polish PR is forbidden by the spec-authoring discipline, and this spec's tests do not depend on a healthy stream (the "not live" state is fully testable — it is dev's current state).
- Server-side fleet search — if walls ever grow past a screenful, search returns as a server-backed feature spec, not a client-side filter.
- Runners "Runner activity" popup → standard table — Indy asked for a suggestion (Jul 21, 2026); recommendation is a runner detail page with a `DataTable` of activity, mirroring the fleets list→console pattern. Lands as its own workstream once Indy picks a shape.
- Why `github-app` wakes fail instantly with zero tokens on dev — backend/runner investigation, separate spec.
- Runs-ledger copy overhaul ("LATEST 200 EVENTS IN 7 DAYS") — revisit only if Indy flags it after this lands.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy opens `github-pr-reviewer` on dev and the first thing his eye lands on is the steer composer; nothing overlaps; he reads "not live · 10 events · $0.00 spent" on the wall without asking what it means; one click on `← Fleets` returns him to the wall.
2. **Preserved user behaviour** — Editing SKILL.md/TRIGGER.md (PATCH + If-Match + 412 reload), steer composer semantics, approvals, memory, ledger, tile links, and snapshot degradation all behave exactly as today; only labels, emphasis, and navigation change.
3. **Optimal-way check** — Yes for copy and hierarchy; the unconstrained-optimal console might redesign the ledger column too, but that has no evidence of confusion yet — the gap is deliberate restraint.
4. **Rebuild-vs-iterate** — Iterate. The frozen variant F layout is right; the implementation under-delivers its emphasis and vocabulary. No determinism trade.
5. **What we build** — Relabelled tile footer + eyebrow with tooltip; search-box removal from the wall header; `← Fleets` link; "Time" metrics label; collapsed-by-default source card; steer-first stacking; column-track lock.
6. **What we do NOT build** — Stream fixes (M133, in flight); server-side search; server changes of any kind; new analytics events; a breadcrumb system (one back link suffices).
7. **Fit with existing features** — Compounds M131 console + M132 wall; must not destabilize the SkillEditor save flow (draft/etag state machine) — the collapse state must sit beside it, not inside it.
8. **Surface order** — UI-only by nature; the CLI already has `fleet update`/`steer` verbs covering these actions.
9. **Dashboard restraint** — No new controls, counters, or claims; the collapse hides a heavy editor until asked for, which is restraint applied.
10. **Confused-user next step** — The eyebrow's tooltip explains "not live" in one sentence; `← Fleets` is the escape hatch from the console; no ticket-shaped dead ends added.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Three Sections mirroring the three user complaints (tile language / console navigation+language / hierarchy+overlap) — each independently shippable and testable, one Workstream.
- **Alternatives considered:** A full console redesign spec (rejected — variant F is frozen and correct; the defects are implementation fidelity, not design); splitting the overlap fix into its own trivial PR (rejected — it shares files and tests with §3 and would orphan the working-tree diff).
- **Patch-vs-refactor verdict:** this is a **patch** because every change realigns the implementation with an already-frozen design; no structure is rearchitected.

## Discovery (consult log)

- **Consults** —
  > Indy (2026-07-21): "i think the `wall's searchbox must be removed`" — context: wall search box (M94 carry-over, absent from frozen variant F) surfaced Jul 20 as awaiting his call; removal is now in scope (§1, Dimension 1.5).
  > Indy (2026-07-21): "Additionally i want this to be in the standard table" — context: the workspace Events page (card rows) joins the design-system `DataTable` like API keys/secrets; folded in as §4 per the same-tree default.
  > Indy (2026-07-21): "This must be a standard table and shouldnt be a popup, what can you suggest here?" — context: the Runners page "Runner activity" dialog; suggestion delivered (runner detail page + standard table), awaiting his pick — tracked in Out of Scope, not folded into this diff.
- **Metrics review** —
- **Skill-chain outcomes** —
- **Deferrals** —
