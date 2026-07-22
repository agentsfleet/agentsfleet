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

# M140_001: Sort, paginate, and bound every dashboard table

**Prototype:** v2.0.0
**Milestone:** M140
**Workstream:** 001
**Date:** Jul 22, 2026
**Status:** DONE
**Priority:** P1 — dashboard users need dense datasets to remain navigable without scrolling the whole page
**Categories:** User Interface (UI)
**Batch:** B1 — standalone dashboard table improvement
**Branch:** `feat/data-table-tanstack`
**Test Baseline:** unit=2814 integration=376
**Depends on:** none
**Provenance:** Large Language Model-drafted (LLM-drafted, Codex, Jul 22, 2026)
**Canonical architecture:** `docs/architecture/direction.md` — dashboard behavior stays a presentation concern and does not create another runtime

---

## Overview

**Goal (testable):** A dashboard user can sort supported columns, move through client-, cursor-, or page-paginated rows, and browse long event feeds inside a bounded table viewport.
**Problem:** Tables render as static, visually flat lists; event feeds extend the entire page; sort affordances and pagination behavior vary by screen; and the dashboard shell has visible one-pixel geometry drift.
**Solution summary:** This Pull Request (PR) keeps the public `DataTable` API as the design-system boundary, delegates row modeling to TanStack Table, centralizes rendering and pagination, adopts the component across dashboard tables, and makes the shell own viewport scrolling with exact header geometry.

## PR Intent & comprehension handshake

- **PR title (eventual):** Add sortable, paginated dashboard tables
- **Intent (one sentence):** Make large dashboard datasets fast to scan without exposing TanStack internals to application code.
- **Handshake:** The implementation preserves the existing `DataTable` call shape while adding explicit sortable columns, bounded table scrolling, three pagination modes, standard buttons, and exact shell geometry. `ASSUMPTIONS I'M MAKING: 1. Existing server-backed lists keep their current fetch semantics. 2. Client pagination defaults to 25 rows. 3. TanStack remains private to the design-system implementation.`

## Implementing agent — read these first

1. `ui/packages/design-system/src/design-system/DataTable.tsx` — public wrapper whose API remains stable.
2. `ui/packages/design-system/src/design-system/Button.tsx` — standard interactive primitive for sort and pagination controls.
3. `docs/DESIGN_SYSTEM.md` — typography, semantic color, spacing, and interaction rules.
4. `dispatch/write_ts_adhere_bun.md` — TypeScript, Bun, UI substitution, and design-token rules.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/done/M140_001_P1_UI_SORTABLE_PAGINATED_TABLES.md` | CREATE | Record intent, proof, and review outcomes. |
| `VERSION` | EDIT | Advance the user-facing feature release to `0.20.0`. |
| `build.zig.zon` | EDIT | Keep the daemon package version synchronized. |
| `cli/package.json` | EDIT | Keep the command-line package version synchronized. |
| `~/Projects/docs/changelog.mdx` | EDIT | Describe sortable, paginated tables for users. |
| `bun.lock` | EDIT | Lock the TanStack Table dependency. |
| `ui/packages/design-system/package.json` | EDIT | Declare TanStack Table. |
| `ui/packages/design-system/src/design-system/DataTable.tsx` | EDIT | Keep a thin public wrapper. |
| `ui/packages/design-system/src/design-system/DataTable.types.ts` | CREATE | Own public table types without vendor leakage. |
| `ui/packages/design-system/src/design-system/DataTableModel.ts` | CREATE | Adapt public state to TanStack row models. |
| `ui/packages/design-system/src/design-system/DataTableView.tsx` | CREATE | Centralize table chrome, scrolling, and controls. |
| `ui/packages/design-system/src/design-system/DataTable.test.tsx` | EDIT | Prove sorting, pagination, loading, and empty states. |
| `ui/packages/design-system/src/design-system/Pagination.tsx` | EDIT | Centralize pagination discriminants and standard buttons. |
| `ui/packages/design-system/src/design-system/Pagination.test.tsx` | EDIT | Prove cursor status, loading, exhaustion, and numeric totals. |
| `ui/packages/design-system/src/design-system/DashboardShellHeader.tsx` | CREATE | Render exact-height header chrome without border drift. |
| `ui/packages/design-system/src/design-system/DashboardShellHeader.test.tsx` | CREATE | Pin semantic header geometry. |
| `ui/packages/design-system/src/design-system/index.ts` | EDIT | Export public table and shell primitives. |
| `ui/packages/design-system/src/index.ts` | EDIT | Export the package-level public surface. |
| `ui/packages/app/components/layout/Shell.tsx` | EDIT | Make the shell viewport-fixed and main-content scrollable. |
| `ui/packages/app/components/domain/EventsList.tsx` | EDIT | Add sortable event columns and cursor pagination. |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/PlatformCatalogTable.tsx` | EDIT | Add sortable catalog columns. |
| `ui/packages/app/app/(dashboard)/admin/models/components/CatalogueList.tsx` | EDIT | Add sortable model columns. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.tsx` | EDIT | Add controlled runner sorting and safe refresh defaults. |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/ApiKeyList.tsx` | EDIT | Add controlled key sorting and page navigation. |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingUsageTab.tsx` | EDIT | Add sortable billing columns and cursor navigation. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.tsx` | EDIT | Add sortable secret columns. |
| `ui/packages/app/tests/admin-models-ui.test.ts` | EDIT | Prove model sort wiring. |
| `ui/packages/app/tests/api-keys-components.test.ts` | EDIT | Prove key sorting and pagination. |
| `ui/packages/app/tests/api-keys-create-dialog.test.ts` | EDIT | Keep package mocks aligned with exports. |
| `ui/packages/app/tests/app-shell-navigation.test.ts` | EDIT | Prove viewport and header geometry classes. |
| `ui/packages/app/tests/billing-charge-cell.test.tsx` | EDIT | Keep billing mocks aligned with exports. |
| `ui/packages/app/tests/billing-usage-tab.test.ts` | EDIT | Prove billing sorting and pagination. |
| `ui/packages/app/tests/events-components.test.ts` | EDIT | Prove event sorting and loading navigation. |
| `ui/packages/app/tests/helpers/dashboard-mocks.tsx` | EDIT | Expose pagination constants to tests. |
| `ui/packages/app/tests/platform-catalog-table.test.tsx` | CREATE | Prove catalog sorting. |
| `ui/packages/app/tests/runners-list.test.ts` | EDIT | Prove runner sorting and fallback behavior. |
| `ui/packages/app/tests/secrets-list.test.ts` | EDIT | Prove secret sorting. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — No Dead Code (`RULE NDC`), No Redundant Comments (`RULE NRC`), No Legacy Retained (`RULE NLR`), Uniform Free Strings (`RULE UFS`), Test Naming (`RULE TST-NAM`), and Cross-layer Orphan Sweep (`RULE ORP`).
- **`dispatch/write_ts_adhere_bun.md`** — Bun-first dependency management, TypeScript file shape, UI component substitution, and design-token adherence.
- **`dispatch/write_any.md`** — file/function length and source-wide constant discipline.
- **`docs/DESIGN_SYSTEM.md`** — semantic tokens, standard buttons, density, and typography.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no Zig source changes | Not applicable. |
| PUB / Struct-Shape | no — no Zig public surface | Not applicable. |
| File & Function Length | yes — TypeScript source changes | Split wrapper, types, model, and view; keep new files within limits. |
| UFS | yes — pagination discriminants and labels | Export `PAGINATION_KIND`; reuse existing semantic constants where applicable. |
| UI Substitution / DESIGN TOKEN | yes — dashboard controls and chrome | Use design-system `Button` and semantic Tailwind utilities without arbitrary token equivalents. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — no related surfaces | Not applicable. |

## Prior-Art / Reference Implementations

- **Reference:** existing design-system `Button`, `Table`, and `Pagination` primitives — compose established shadcn/Tailwind utilities and semantic tokens while TanStack owns only data modeling.
- **Reference:** TanStack Table row-model architecture — keep vendor state inside the wrapper and expose project-owned types to consumers.

## Sections (implementation slices)

### §1 — Encapsulated table engine

Keep `DataTable` as the only application-facing table API while separating public types, TanStack state adaptation, and rendering.

- **Dimension 1.1 — DONE** — TanStack types never appear in the exported `DataTable` API → Test `exports_project_owned_data_table_types`
- **Dimension 1.2 — DONE** — sortable local columns cycle ascending, descending, and unsorted → Test `sorts_client_rows_through_header_controls`
- **Dimension 1.3 — DONE** — controlled server sorting delegates without locally reordering rows → Test `delegates_controlled_sorting_without_local_reorder`

### §2 — Bounded, coherent table interaction

Make dense tables readable with a sticky header, bounded vertical scrolling, horizontal overflow, standard controls, reduced-motion handling, and pagination appropriate to the data source.

- **Dimension 2.1 — DONE** — client pagination defaults to 25 rows and honors custom page size → Test `paginates_client_rows_with_default_and_custom_sizes`
- **Dimension 2.2 — DONE** — cursor and numeric page modes preserve navigation across loading and empty results → Test `preserves_external_navigation_states`
- **Dimension 2.3 — DONE** — long datasets scroll within the table instead of extending the page → Test `renders_bounded_scrollable_table_viewport`

### §3 — Dashboard adoption and shell geometry

Enable appropriate sortable columns on every current table and make the application shell own viewport scrolling without consuming header height through borders.

- **Dimension 3.1 — DONE** — catalog, model, runner, key, billing, secret, and event tables expose meaningful sort controls → Test `dashboard_tables_wire_sortable_columns`
- **Dimension 3.2 — DONE** — runner refresh and invalid sort input return to deterministic default sorting → Test `runner_refresh_uses_default_sort`
- **Dimension 3.3 — DONE** — header chrome and shell occupy the viewport without one-pixel or asymmetric edge drift → Test `dashboard_shell_uses_exact_header_geometry`

## Interfaces

```text
DataTable<T>(DataTableProps<T>) -> React element
DataTablePagination = false | client pagination | cursor pagination | page pagination
PAGINATION_KIND = client | cursor | page
Vendor-specific TanStack types are internal and are not exported.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Empty external page | Server returns no rows after navigation | Empty state remains visible with Previous or Load more navigation when recovery is possible. |
| Pagination request in flight | Cursor or page callback has not completed | Sorting and navigation controls are disabled and loading state remains visible. |
| Unsupported local sort | Column is marked sortable without a comparable value | No sort arrow or inert control is rendered. |
| Invalid runner sort | URL or server state contains an unsupported sort key | Runner list retries with the named default sort. |
| Single server page | Total rows fit within one page | Pagination chrome stays hidden. |

## Invariants

1. Application consumers cannot import TanStack table types through the design-system barrel — enforced by project-owned exported types and TypeScript checks.
2. Pagination kind strings have one declaration site — enforced by `PAGINATION_KIND` and the Uniform Free Strings gate.
3. Table buttons use the standard design-system primitive — enforced by the UI substitution gate.
4. A page border cannot alter header height — enforced by pseudo-element chrome and the shell regression test.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | Existing table interactions only | none | No new collection. | `dashboard_tables_wire_sortable_columns` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `exports_project_owned_data_table_types` | Package and application type checks succeed without exported TanStack types. |
| 1.2 | unit | `sorts_client_rows_through_header_controls` | Repeated header activation produces ascending, descending, then original order. |
| 1.3 | unit | `delegates_controlled_sorting_without_local_reorder` | Controlled callback receives direction while input row order remains unchanged. |
| 2.1 | unit | `paginates_client_rows_with_default_and_custom_sizes` | A dataset larger than one page renders the expected slice and row count. |
| 2.2 | unit | `preserves_external_navigation_states` | Loading disables controls; empty later pages retain recovery navigation; one-page results hide it. |
| 2.3 | unit | `renders_bounded_scrollable_table_viewport` | Table frame carries bounded overflow, sticky-header, and reduced-motion-safe classes. |
| 3.1 | integration | `dashboard_tables_wire_sortable_columns` | Rendered dashboard tables expose sort buttons only on meaningful columns. |
| 3.2 | integration | `runner_refresh_uses_default_sort` | Refresh and invalid-sort retries fetch with the default host ordering. |
| 3.3 | integration | `dashboard_shell_uses_exact_header_geometry` | Header uses fixed height with non-layout divider and main content owns scrolling. |
| failures | unit | `data_table_failure_modes` | Empty, loading, unsupported-sort, invalid-runner-sort, and single-page cases match the Failure Modes table. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Table engine and interaction tests pass (§1–§2) | `cd ui/packages/design-system && bun run test` | exit 0; 52 files and 488 tests pass | P0 | Pass — 52 files and 488 tests passed; branch coverage 99.56%. |
| R2 | Dashboard adoption tests pass (§3) | `cd ui/packages/app && bunx vitest run tests/admin-models-ui.test.ts tests/api-keys-components.test.ts tests/api-keys-create-dialog.test.ts tests/app-shell-navigation.test.ts tests/billing-charge-cell.test.tsx tests/billing-usage-tab.test.ts tests/events-components.test.ts tests/platform-catalog-table.test.tsx tests/runners-list.test.ts tests/secrets-list.test.ts` | exit 0; 10 files and 153 tests pass | P0 | Pass — 10 files and 153 tests passed. |
| R3 | Full unit lanes pass | `make test-unit-all` | exit 0; all unit lanes pass | P0 | Pass — application, website, command-line interface, and design-system unit lanes passed. |
| R4 | TypeScript and design-system lint stays clean | `make lint-apps-ds-ctl` | exit 0 | P0 | Pass — application, design-system, and command-line interface lint passed. |
| R5 | Repository conformance stays green | `make harness-verify` | exit 0; ALL GATES GREEN | P0 | Pass — all staged gates green. |
| R6 | Release versions stay synchronized | `make check-version` | exit 0; all versions match `0.20.0` | P0 | Pass — all versions match `0.20.0`. |
| R7 | No secrets enter the repository | `gitleaks detect` | exit 0; no leaks found | P0 | Pass — no leaks found. |
| R8 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | every path is listed above or is this spec after lifecycle movement | P0 | Pass — every path is listed in Files Changed. |
| R9 | New source files stay within the repository line limit | `git diff --diff-filter=A --name-only origin/main | grep -Ev '\.(md|lock)$' | xargs wc -l 2>/dev/null | awk '$1>350 && $2!="total"'` | no new source file exceeds the limit | P0 | Pass — command produced no output. |

## Dead Code Sweep

N/A — no files or public symbols were deleted or renamed.

## Out of Scope

- Column filtering, row selection, column resizing, and virtualization; current datasets need sorting, pagination, and bounded scrolling only.
- Backend pagination or endpoint changes; existing page and cursor behavior remains authoritative.
- New analytics events; this work changes presentation without changing user intent.

## Product Clarity (authoring record)

1. **Successful user moment** — A user opens Events, sorts a column, browses the bounded list, and reaches older results without the whole dashboard becoming a long document.
2. **Preserved user behaviour** — Existing row actions, navigation links, server fetches, empty states, and displayed values continue unchanged.
3. **Optimal-way check** — A shared design-system wrapper is the direct solution because all tables gain one interaction model without vendor details leaking into screens.
4. **Rebuild-vs-iterate** — A larger refactor is correct: splitting the old monolith produces a stable wrapper, independently testable model, and centralized rendering.
5. **What we build** — TanStack-backed modeling, sort controls, three pagination modes, bounded scrolling, dashboard adoption, and exact shell geometry.
6. **What we do NOT build** — Filtering, selection, resizing, virtualization, backend query changes, or a second table abstraction.
7. **Fit with existing features** — The work compounds with existing design-system buttons and semantic tokens while preserving every table's domain-specific cells and actions.
8. **Surface order** — UI-first because the reported problem is dashboard navigation and visual density; no command-line surface changes.
9. **Dashboard restraint** — Only columns with a real comparator or server sort expose arrows; one-page and exhausted pagination controls stay hidden or disabled.
10. **Confused-user next step** — Sort arrows, Page status, Previous/Next, and Load more controls make the next action self-evident inside the table.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Split public types, model state, rendering, and shell chrome so each concern has one owner while consumers keep a compact API.
- **Alternatives considered:** Adding sort handlers and pagination markup separately to each screen was rejected because it would duplicate behavior and expose inconsistent loading and empty-state handling.
- **Patch-vs-refactor verdict:** this is a **refactor** because the original monolith could not cleanly hide TanStack state while also supporting client and server pagination across every table.

## Discovery (consult log)

- **Consults** — Indy selected TanStack Table behind the existing `DataTable` API, requested Bun, standard buttons, bounded viewport scrolling, minimal motion, exact shell geometry, and approved the `0.20.0` minor release bump.
- **Metrics review** — No analytics or funnel changes are required because the same datasets and actions remain; only presentation and navigation improve.
- **Test audit** — `/write-unit-test` found two uncovered public branches after the first coverage run: controlled server sorting must not reset page state, and non-paginated local sorting must reset the scroll viewport without rendering pagination. Both cases now have focused tests; design-system branch coverage rose from 98.91% to 99.56%. The ship coverage audit then found that Billing hid a recoverable empty cursor page before `DataTable` could render its navigation; a regression test now proves that Load more can populate that state.
- **Review outcomes** — Native Codex review and gstack review found no remaining correctness, performance, security, or design defects. Seven informational design-polish findings were applied before the clean rerun: concise cursor status, live status semantics, inset focus treatment, a cursor-loading spinner, truthful empty-cursor copy, stable empty-cursor loading, and removal of the redundant zero-loaded status.
- **Test delta** — Zig unit and integration counts remain unit=2814 and integration=376 because this work changes no Zig source. Design-system tests increased from 486 to 488; the focused dashboard set finishes at 153 tests.
- **Architecture documentation** — No architecture page changed because this is a presentation-only refactor: it adds no runtime, route, data source, queue, schema, or cross-service flow.
- **Visual verification** — Authenticated browser inspection could not pass the sign-in boundary in this environment. Static geometry tests, interaction tests, type checks, lint, and the production application build provide the shipped evidence.
- **Skill-chain outcomes** — `/write-unit-test`, native Codex review, and gstack review completed cleanly after the fixes above.
- **Deferrals** — None.
