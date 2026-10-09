<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M219_002: The two model tables read alike, fleet tabs answer at once, and invite actions line up

**Prototype:** v2.0.0
**Milestone:** M219
**Workstream:** 002
**Date:** Oct 09, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — dashboard polish Indy reported while eyeballing the branch; no boundary or money path changes
**Categories:** UI
**Batch:** B1 — the four Sections touch different screens and ship together
**Branch:** docs/event-runtime-positioning
**Folded-into:** `M219_001`
**Baseline revision:** 2bf456efdf7be559b6c67a2e254dc92600d257b2
**Test Baseline:** pending — inherited from `M219_001`: one branch, measured once
**Baseline evidence:** pending — inherited from `M219_001`
**Depends on:** none. Ships in the `docs/event-runtime-positioning` Pull Request (PR) beside `M219_001`, as Indy chose for scope arriving on this branch.
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 09, 2026) from Indy's four screenshots of the dev dashboard on Oct 09, 2026.
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` (the model library), `ui/packages/design-system` (table and tab primitives)

---

## Overview

**Goal (testable):** A model row reads the same in the admin Model library and the workspace Models table, by a human name with its provider id on hover; a clicked fleet tab is marked pending before the server answers; invite actions sit right-aligned like every other table's, and a secret's row icons line up whatever its name's length.
**Problem:** The Model column shows raw provider ids (`claude-fable-5`). The workspace Models table prints context as `1049k tokens` and rates as `$0.15 in · $0.03 cached · $0.50 out` while the library prints `1,048,576` and `0.15 / 0.03 / 0.50`. Fleet tabs are `?view=` queries on one server-rendered page, so a click shows nothing until the fleet and the view's data come back. The Members invite row puts "Email sent" text and a bordered icon inside the actions cell.
**Solution summary:** One display module formats a model's name, context and rates for both tables. Each fleet tab link shows its own pending state, and the view's panel streams behind a skeleton while the header and tabs render. Invite email status moves to the Time column and the actions become a plain icon row.

## PR Intent & comprehension handshake

- **PR title (eventual):** shared with `M219_001`.
- **Intent (one sentence):** The dashboard reads one way for one thing — a model, a tab, a row's actions — wherever it appears.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/admin/models/components/CatalogueList.tsx` — the library's columns; its context and rate formats are the ones both tables keep.
2. `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCells.tsx` — the workspace table's cells and its own `formatContext` and `formatRates`, which §1 replaces.
3. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` — one server render per tab; §2 moves the view behind a boundary.
4. `ui/packages/app/app/(dashboard)/settings/members/components/TeamTable.tsx` and `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.tsx` — the invite actions and the row actions they should match.
5. `.orly/dispatch/write_ts_adhere_bun.md` — design-system primitives and token utilities only.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/models/display.ts`, `ui/packages/app/lib/models/display.test.ts` | CREATE | §1: a model's name, context and rates, formatted once |
| `ui/packages/app/app/(dashboard)/admin/models/components/CatalogueList.tsx` | EDIT | §1: the library reads through the display module |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/{ModelsRegistryCells.tsx,ModelsRegistryTable.tsx}` | EDIT | §1: the workspace table reads through it too |
| `ui/packages/app/tests/{models-registry-table.test.tsx,models-registry-edit-remove.test.tsx,admin-models-ui.test.ts,admin-models-management.test.ts}` | EDIT | §1: rows found by the id's hover; formats follow the shared module |
| `ui/packages/app/tests/timestamp-standard.test.ts` | EDIT | §1: the model library no longer calls a locale formatter, so its exemption goes |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` | EDIT | §2: the view renders behind a boundary keyed by the view |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/{FleetSubnavigation.tsx,FleetTabLink.tsx,FleetTabLink.test.tsx,FleetViewSkeleton.tsx,FleetViewSkeleton.test.tsx}` | EDIT, CREATE | §2: a pending tab and the panel's skeleton |
| `ui/packages/app/tests/{helpers/dashboard-mocks.tsx,fleets-routes/harness.ts,fleets-routes/detail-header.test.ts,fleets-routes/detail-lifecycle.test.ts,fleets-routes/detail-viewer.test.ts,fleets-routes/detail-views.test.ts}` | EDIT | §2: the link mock answers `useLinkStatus`; route tests render the settled stream and prove the panel streams behind its skeleton |
| `ui/packages/app/app/(dashboard)/settings/members/components/{TeamTable.tsx,MembersView.test.tsx}`, `ui/packages/app/tests/{helpers/members-fixtures.tsx,e2e/acceptance/team-members.spec.ts}` | EDIT | §3: email status in the Time column as "Invite emailed"; plain icon actions |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/{secret-row-cells.tsx,SecretsList.test.tsx}` | EDIT | §3: the name sits in a fixed slot that ellipsizes, its icons after it |
| `docs/v2/active/M219_002_P2_UI_MODEL_TABLES_MATCH_TABS_ANSWER_AT_ONCE.md` | CREATE | This spec |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — UFS: column headers and formats are named constants in the display module, and ui/ literals repeated in a file become constants by hand. NDC: the workspace table's own formatters are deleted, not left beside the shared ones. NLR: stale comments about the old formats go in the same commit. TST-NAM: test names describe behaviour.
- `.orly/dispatch/write_ts_adhere_bun.md` — UI GATE (design-system primitives: `DataTable`, `IconAction`, `TabNav`, `Tooltip`) and DESIGN TOKEN GATE (token utilities, no arbitrary values).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UI / DESIGN TOKEN | Yes | Only design-system components and token classes; no raw `<button>` or `*-[...]` |
| File & Function Length (≤350/≤50/≤70) | Yes | `ModelsRegistryCells.tsx` shrinks; `page.tsx` moves the view load into its own component |
| UFS | Yes | ui/ manual pass: repeated literals become constants |
| MILESTONE-ID | Yes | No milestone identifiers in code or test names |
| SPEC TEMPLATE | Yes | `bash .orly/audits/spec-template.sh --staged` on every spec edit |

## Prior-Art / Reference Implementations

- **Formats:** the library's own cells in `CatalogueList.tsx` — pinned `en-US` grouping and two-decimal rates.
- **Row actions:** `SecretsList.tsx` — ghost `IconAction`s in a right-aligned row, destructive last.
- **Pending navigation:** Next.js `useLinkStatus` (`next/link`), which marks the one link whose navigation is in flight.

## Sections (implementation slices)

### §1 — A model reads the same in both tables — DONE

`display.ts` turns a provider id into the name the provider markets (`claude-fable-5` → `Fable 5`, `accounts/fireworks/models/glm-5p3-flash` → `GLM 5.3 Flash`), formats context with pinned `en-US` grouping, and prints rates as `in / cached / out` at two decimals. Both tables render the name with the exact id on hover and keep sorting and labels on the id. **Implementation default:** derive the name from the id rather than add a column, because the catalogue has no display field and the id stays one hover away.

- **Dimension 1.1** — Provider ids read as marketed names → Test `names a provider id the way its provider markets the model` — DONE (`ui/packages/app/lib/models/display.test.ts`)
- **Dimension 1.2** — Both tables print one row's context and rates identically → Test `formats context and rates the way the model library prints them` — DONE (`ui/packages/app/lib/models/display.test.ts`)
- **Dimension 1.3** — The exact id stays on the row → Test `the model cell shows the name and keeps the id on hover` — DONE (`ui/packages/app/tests/admin-models-ui.test.ts`)

### §2 — A clicked fleet tab answers at once

The tab the user clicked shows its pending state through `useLinkStatus` while its navigation is in flight. `page.tsx` renders the view inside a `Suspense` boundary keyed by the view and its cursor, so the header and tabs paint as soon as the fleet reads back and the panel shows a skeleton until its own data arrives. **Implementation default:** keep the views as `?view=` queries; separate routes would change every link and test for no gain the boundary does not give.

- **Dimension 2.1** — A clicked tab is marked pending before the server answers → Test `a clicked fleet tab reads as loading while its view is on the way` — DONE (`FleetTabLink.test.tsx`)
- **Dimension 2.2** — The panel's skeleton matches each view's frame → Test `every fleet view has a skeleton` — DONE (`FleetViewSkeleton.test.tsx`)
- **Dimension 2.3** — Click to first paint, measured on the running app before and after → Test `fleet_tab_paints_before_its_data` (manual)

### §3 — Row icons line up — DONE

The invite email status moves into the Time column under the dates, reading "Invite emailed", "Email not sent" or "Email not set up" (the last keeps its tooltip). The actions cell becomes resend, copy and revoke as right-aligned ghost icons, destructive last, as Secrets does. A secret's name sits in a `max-w-trim` slot that ellipsizes past it, with the full name on hover; its copy and rename icons follow the slot, so they line up on every row.

- **Dimension 3.1** — An invite row's actions are icons only → Test `an invite row's actions are icons only, right-aligned` — DONE (`MembersView.test.tsx`)
- **Dimension 3.2** — The Time column says whether the invite email went → Test `the time column says whether the invite email went` — DONE (`MembersView.test.tsx`)
- **Dimension 3.3** — A long secret name ellipsizes and its icons stay in line → Test `a long secret name ellipsizes and keeps its icons in line` — DONE (`SecretsList.test.tsx`)

### §4 — Indy eyeballs the dashboard

The app runs on `http://localhost:3000` from `AGENTSFLEET_UI_ENV_FILE`; the browse tool captures each changed screen for the PR, and Indy reviews the running app.

- **Dimension 4.1** — Indy accepts the four screens on the running app → Test `indy_eyeballs_the_changed_screens` (manual)

## Interfaces

```
ui/packages/app/lib/models/display.ts
  modelLabel(modelId: string): string
  formatContextTokens(tokens: number | undefined): string
  formatRatesPerMtok(rates: RateFields | null): string
```

No API, route or wire change.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unrecognised id shape | A provider id the name rules do not cover | `modelLabel` returns the id's last path segment unchanged; the hover still shows the full id |
| Missing context or rates | A row with no catalogue match | The cell prints the existing empty marker, as today |
| Slow view data | The view's API read is slow or fails | The header and tabs stay; the skeleton holds until the panel resolves or its error boundary renders |

## Invariants

1. One formatter per value — both tables import `display.ts`; R1 counts no second `formatRates` definition under `ui/packages/app/app`.
2. Sorting and accessible names use the provider id — `sortValue` and `label` read `model_id`, never the display name.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| No product or operator signal changes | not applicable | never | none | Existing tab and table events keep their names | `every fleet view has a skeleton` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `names a provider id the way its provider markets the model` | `claude-fable-5` → `Fable 5`; `claude-opus-5-5` → `Opus 5.5`; `accounts/fireworks/models/glm-5p3-flash` → `GLM 5.3 Flash`; `gpt-6.1-sol` → `GPT-6.1 Sol`; an unknown shape → its last segment |
| 1.2 | unit | `formats context and rates the way the model library prints them` | `1048576` → `1,048,576`; rates in nanos → `0.15 / 0.03 / 0.50` |
| 1.3 | unit | `the model cell shows the name and keeps the id on hover` | Row `claude-fable-5` → text `Fable 5`, hover title `claude-fable-5` |
| 2.1 | unit | `a clicked fleet tab reads as loading while its view is on the way` | Link status pending → the tab carries the pending marker |
| 2.2 | unit | `every fleet view has a skeleton` | Each of the five views → a skeleton with its frame |
| 2.3 | manual | `fleet_tab_paints_before_its_data` | Click to first paint on `localhost:3000`, before and after, recorded in the PR Session Notes |
| 3.1 | unit | `an invite row's actions are icons only, right-aligned` | Invite row → no status text in the actions cell; resend, copy, revoke as icons |
| 3.2 | unit | `the time column says whether the invite email went` | Sent → `Invite emailed`; failed → `Email not sent` |
| 3.3 | unit | `a long secret name ellipsizes and keeps its icons in line` | A 60-character name → the slot truncates with the full name as its title; copy and rename sit in the same row |
| 4.1 | manual | `indy_eyeballs_the_changed_screens` | Indy reviews the running app; his reply is recorded in Discovery |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | One rate formatter in the app (§1) | `git grep -n 'function formatRates' -- ui/packages/app` | one line, in `lib/models/display.ts` | P1 | |
| R2 | The fleet view renders behind a boundary (§2) | `git grep -c 'Suspense' -- 'ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx'` | `1` or more | P1 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Lint passes | `make lint-all` | exit 0 | P0 | |
| S3 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S4 | Integration passes | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |

## Dead Code Sweep

`formatContext` and `formatRates` in `ModelsRegistryCells.tsx` are deleted; R1 proves no copy remains.

## Out of Scope

- A display-name column in `core.model_library`; the name derives from the id.
- Separate routes per fleet tab.
- Any change to what the tables list or to the model library's data.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An admin reads `Fable 5` in either table and sees `claude-fable-5` on hover; a fleet tab responds the moment it is clicked.
2. **Preserved user behaviour** — Sorting, editing, defaults, resend, copy and revoke work as today.
3. **Optimal-way check** — One formatter beats two tables keeping two formats in step.
4. **Rebuild-vs-iterate** — Iterate; the tables and tabs exist and only their presentation changes.
5. **What we build** — A display module, a pending tab, a streamed panel, a rearranged invite row.
6. **What we do NOT build** — Catalogue display names, tab routes, new data.
7. **Fit with existing features** — Uses `DataTable`, `IconAction`, `TabNav` and the library's existing formats.
8. **Surface order** — Admin Model library, workspace Models, fleet tabs, Members.
9. **Dashboard restraint** — No new columns, badges or controls.
10. **Confused-user next step** — Hover a model or secret name for its full id; a pending tab shows its view is loading.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one shared module and three local presentation edits.
- **Alternatives considered:** a catalogue display-name column (rejected: a schema change for a presentation need); client-side tab switching with all views prefetched (rejected: fetches every view's data on every page open).
- **Patch-vs-refactor verdict:** this is a **patch** because each screen keeps its structure and data.

## Discovery (consult log)

- **Consults** — Indy, Oct 09, 2026, from four screenshots: the Model column should read `Fable 5` with `claude-fable-5` on hover; Events and Memory tabs take a while to load; the Models columns must match the Model library; Members icons should align like Secrets, and "Email sent" reads awkwardly. He asked to eyeball the result on `http://localhost:3000` run with `AGENTSFLEET_UI_ENV_FILE`. Then: "is it possible to align the copy clipboard icon, edit pencil" on Secrets, and how a long name should truncate.
- **Metrics review** — No product event changes.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
