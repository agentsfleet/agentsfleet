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
**Status:** PENDING
**Priority:** P1 — customer-facing: operators cannot read the wall/console today (cryptic labels, overlapping columns, buried composer)
**Categories:** UI
**Batch:** B1 — standalone; no sibling workstreams
**Branch:** — set at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none — M131_001 (console) and M132_001 (wall) are in `done/`
**Provenance:** LLM-drafted (Claude Fable 5, Jul 20, 2026) — from Indy's dev-session review of `app-dev` screenshots
**Canonical architecture:** `docs/DESIGN_SYSTEM.md` §Operational Restraint; frozen reference `~/.gstack/projects/agentsfleet-agentsfleet/designs/fleet-dashboard-20260714/{variant-F-ia.html,FREEZE.md}`

---

## Overview

**Goal (testable):** The fleet console renders three non-overlapping columns with the steer thread as the visually primary surface, a `← Fleets` back affordance, and every wall-tile / metrics label readable as plain English — asserted by unit label tests and an e2e pass over the rendered console.
**Problem:** Operators reading the wall and console today hit three walls: (1) cryptic labels — "LAST KNOWN", "10 ev", metrics "WALL" — that Indy himself could not decode; (2) the console's left-column cards inflate past their grid track on long source lines and paint under the middle column, wrecking the page; (3) the Source editor visually dominates while the steer composer — the point of the page per the Jul 14 freeze — is buried, and there is no way back to the wall except the sidebar.
**Solution summary:** Copy-and-layout changes inside the frozen variant F design, no backend. Wall tiles spell out their footer ("$0.00 spent · 10 events · 6 hours ago") and replace the "last known" eyebrow with "not live" plus a tooltip. The console gains the frozen-but-never-built `← Fleets` back link, renames the metrics "Wall" label to "Time", collapses the Source card to its header by default (expand on demand; Edit auto-expands), orders the steer column first when columns stack, and locks column children to their grid track (`min-w-0` chain) so wide source/commands scroll inside their own blocks.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(ui): steer-first console, plain-language wall and console labels
- **Intent (one sentence):** An operator landing on the wall or console understands every label without decoding, immediately sees the steer thread as the main surface, and can navigate back to the wall.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

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
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` | EDIT | Back link, steer-column stacking order, `min-w-0` column-child lock (partially in working tree — port the uncommitted diff) |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/console-copy.ts` | EDIT | `METRICS_WALL_LABEL` → `METRICS_TIME_LABEL = "Time"`; new back-link and disclosure consts |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/RunMetricsStrip.tsx` | EDIT | Consume renamed label const |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/RunMetricsStrip.test.tsx` | EDIT | Assert "Time" label |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/SkillEditor.tsx` | EDIT | Collapsed-by-default source card with expand disclosure; Edit auto-expands; collapse disabled while editing |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/SkillEditor.test.tsx` | EDIT | Collapsed default, expand, Edit-auto-expand, draft-preservation tests |
| `ui/packages/app/tests/dashboard-fleets-wall.test.tsx` | EDIT | Wall-level assertions updated to new labels |
| `ui/packages/app/tests/e2e/acceptance/fleet-console.spec.ts` | EDIT | Back-nav walk + no-horizontal-overflow assertion |

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

- **Dimension 1.1** — Footer events figure renders with the word "events", never bare "ev" → Test `test_tile_footer_spells_out_events`
- **Dimension 1.2** — Footer spend renders with the suffix "spent" from the server figure → Test `test_tile_footer_labels_spend`
- **Dimension 1.3** — Snapshot tile shows eyebrow "not live" with the explanatory tooltip; live tile shows neither → Test `test_snapshot_eyebrow_reads_not_live`
- **Dimension 1.4** — All new tile strings are named consts referenced by component and tests → Test `test_wall_copy_consts_are_single_source`

### §2 — Console back affordance and plain metrics

The console header gains `← Fleets` linking to the wall (frozen in variant F, never built), and the metrics strip's "Wall" label becomes "Time" — "Wall" collides with the product's own Live Wall vocabulary and means nothing to an operator. The const renames to `METRICS_TIME_LABEL` (RULE NLR).

- **Dimension 2.1** — Console header renders a `← Fleets` link whose href is the workspace fleets route → Test `test_console_back_link_targets_wall`
- **Dimension 2.2** — Metrics strip renders "Time" for `wall_ms`; no rendered surface says "Wall" → Test `test_metrics_strip_labels_time`

### §3 — Steer-first console hierarchy

The steer thread + composer is the point of the page (FREEZE §1); the layout must say so. The Source card collapses to its header row (title + Edit + expand disclosure) by default; expanding reveals the tabs and viewer; pressing Edit from collapsed auto-expands into the editor; collapse is disabled while editing so a draft can never be hidden or lost. Below the `lg` breakpoint the steer column stacks first. Column children are locked to their grid track (`min-w-0` on each column's direct children) so long source lines, webhook URLs, and registration commands scroll inside their own `overflow-x-auto` blocks instead of painting under the neighbouring column — this half exists as an uncommitted working-tree diff on `main`; port it into the branch.

- **Dimension 3.1** — Source card renders collapsed by default: header visible, document panes absent until expanded → Test `test_source_card_collapsed_by_default`
- **Dimension 3.2** — Expand disclosure reveals the SKILL.md/TRIGGER.md panes; collapsing hides them again → Test `test_source_card_expand_toggle`
- **Dimension 3.3** — Edit pressed while collapsed expands into the editor; collapse control is disabled while editing → Test `test_edit_auto_expands_and_pins_open`
- **Dimension 3.4** — Steer column is first in stacked (below-`lg`) order, middle in the three-column order → Test `test_steer_column_stacks_first`
- **Dimension 3.5** — Each console column applies the `min-w-0` child lock; the rendered console page has no horizontal document overflow → Test `test_console_columns_never_overlap` (e2e)

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
| 1.1 | unit | `test_tile_footer_spells_out_events` | Fleet with `events_processed: 10` → footer text contains "10 events", zero matches for the bare token "ev" |
| 1.2 | unit | `test_tile_footer_labels_spend` | Fleet with known `budget_used_nanos` → footer contains "$X.XX spent" |
| 1.3 | unit | `test_snapshot_eyebrow_reads_not_live` | Stream state reconnecting/hello-without-live → eyebrow "not live" + tooltip text present; live state → neither rendered |
| 1.4 | unit | `test_wall_copy_consts_are_single_source` | Rendered labels equal the exported consts (imported, not re-typed) |
| 2.1 | unit | `test_console_back_link_targets_wall` | Console header link "← Fleets" has the workspace fleets href |
| 2.2 | unit | `test_metrics_strip_labels_time` | Event with `wall_ms` → strip shows "Time"; rendering contains no "Wall" label |
| 3.1 | unit | `test_source_card_collapsed_by_default` | Fresh render → header present, SKILL.md pane absent |
| 3.2 | unit | `test_source_card_expand_toggle` | Expand → panes visible; collapse → hidden; empty TRIGGER.md shows existing hint (regression) |
| 3.3 | unit | `test_edit_auto_expands_and_pins_open` | Edit from collapsed → textarea visible; collapse control disabled while editing; draft text survives |
| 3.4 | unit | `test_steer_column_stacks_first` | The "What it does" section carries the stack-first ordering; column labels keep their three-column order |
| 3.5 | e2e | `test_console_columns_never_overlap` | Seeded fleet with long source lines → console page `document.scrollWidth <= viewport width`; back link navigates to the wall |
| reg | unit | existing FleetTile/SkillEditor/RunMetricsStrip suites | All pre-existing behaviour (liveness derivation, save flow, 412 reload, cost fields) passes unmodified except label assertions |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Wall tile carries no cryptic labels (§1) | `grep -cE '\bev\b\|last known' "ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.tsx"` | 0 | P0 | |
| R2 | Back affordance exists in console copy (§2) | `grep -c "← Fleets" "ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/console-copy.ts"` | 1 | P0 | |
| R3 | Metrics label renamed, old const gone (§2) | `grep -rc "METRICS_WALL_LABEL" ui/packages/app --include='*.ts*' \| grep -v ':0' \| wc -l` | 0 | P0 | |
| R4 | Steer-first + overlap lock in the console shell (§3) | `grep -cE 'order-first\|\*:min-w-0' "ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx"` | 2 | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-app` | exit 0 | P0 | |
| S2 | Lint clean (Oxlint + tsc + design tokens) | `make lint-app` | exit 0 | P0 | |
| S4 | e2e walks the console (back-nav + no overflow) | `cd ui/packages/app && bun run test:e2e:acceptance:local tests/e2e/acceptance/fleet-console.spec.ts` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `METRICS_WALL_LABEL` | `git grep -rn -w "METRICS_WALL_LABEL"` | 0 matches |

## Out of Scope

- Removing the wall's search box — legacy carry-over from the M94 list, not in variant F; awaiting Indy's explicit call (surfaced Jul 20, 2026).
- Workspace-multiplexed live stream and reconnect robustness — M133_001 (active).
- Why `github-app` wakes fail instantly with zero tokens on dev — backend/runner investigation, separate spec.
- Runs-ledger copy overhaul ("LATEST 200 EVENTS IN 7 DAYS") — revisit only if Indy flags it after this lands.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy opens `github-pr-reviewer` on dev and the first thing his eye lands on is the steer composer; nothing overlaps; he reads "not live · 10 events · $0.00 spent" on the wall without asking what it means; one click on `← Fleets` returns him to the wall.
2. **Preserved user behaviour** — Editing SKILL.md/TRIGGER.md (PATCH + If-Match + 412 reload), steer composer semantics, approvals, memory, ledger, tile links, and snapshot degradation all behave exactly as today; only labels, emphasis, and navigation change.
3. **Optimal-way check** — Yes for copy and hierarchy; the unconstrained-optimal console might redesign the ledger column too, but that has no evidence of confusion yet — the gap is deliberate restraint.
4. **Rebuild-vs-iterate** — Iterate. The frozen variant F layout is right; the implementation under-delivers its emphasis and vocabulary. No determinism trade.
5. **What we build** — Relabelled tile footer + eyebrow with tooltip; `← Fleets` link; "Time" metrics label; collapsed-by-default source card; steer-first stacking; column-track lock.
6. **What we do NOT build** — Search removal (Indy's call pending); stream fixes (M133); server changes of any kind; new analytics events; a breadcrumb system (one back link suffices).
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
- **Metrics review** —
- **Skill-chain outcomes** —
- **Deferrals** —
