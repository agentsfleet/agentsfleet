# M59_001: Promote `DataTable` + add `Alert` to `@usezombie/design-system`

**Prototype:** v2.0.0
**Milestone:** M59
**Workstream:** 001
**Date:** May 03, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — design-system hygiene; reduces duplication across 6+ existing call sites and unblocks future tabular surfaces.
**Categories:** UI
**Batch:** B1
**Branch:** feat/m59-001-design-system-datatable-alert
**Depends on:** M48_001 (DONE) — surfaces the gap that motivates this spec; the Settings → Billing Usage tab is the first DataTable consumer that should switch from the app-local primitive to the design-system version once it lands.

**Canonical architecture:** `docs/ARCHITECTURE.md` §UI Design System — defines the primitive boundary between `ui/packages/design-system/` (cross-app primitives) and `ui/packages/app/components/ui/` (app-local helpers awaiting promotion).

---

## Implementing agent — read these first

1. `ui/packages/design-system/src/design-system/Card.tsx` — canonical primitive shape (file-as-struct, `cn()` for className merge, theme tokens, `data-slot` for tests). Mirror this layout.
2. `ui/packages/app/components/ui/data-table.tsx` — the DataTable that needs to move. Read its prop surface (`columns`, `rowKey`, `numeric`, `hideOnMobile`, `caption`, `onRowClick`, `empty`, `isLoading`) — the promoted version preserves it byte-for-byte so no consumer breaks.
3. `ui/packages/app/components/domain/ExhaustionBanner.tsx` — current inline-banner pattern (`role="alert"` + theme-token classes). The Alert primitive replaces every site that re-implements this pattern.
4. `ui/packages/design-system/src/design-system/Badge.tsx` — `cva` variant API the Alert primitive should mirror (`info` / `success` / `warning` / `destructive` matches the existing token semantics in `theme.css`).
5. `ui/packages/design-system/src/theme.css` — `--info`, `--success`, `--warning`, `--destructive` tokens are already defined; Alert reuses them, no new tokens.
6. `docs/gates/ui-substitution.md` — the existing UI Component Substitution Gate; when `Alert` lands, the gate's prose adds `<div role="alert">` to the substitution list (single-line edit).

The agent should be able to plan after reading these without further clarification.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline (RULE TST-NAM, RULE NLR for the migration sweep, RULE NDC for the now-dead app-local file).
- **`docs/gates/ui-substitution.md`** — UI Component Substitution Gate; this spec extends it.
- **`docs/gates/file-length.md`** — File & Function Length Gate; promoted primitives stay under 350 lines.
- **`docs/BUN_RULES.md`** — TS file-shape decision applies to every new `*.tsx` / `*.ts`.

No Zig touched; `docs/ZIG_RULES.md` does not apply.

---

## Overview

**Goal (testable):** `import { DataTable, Alert } from "@usezombie/design-system"` resolves; every existing `role="alert"` banner that re-implements `border-{token}/40 bg-{token}/10 text-{token}` is replaced with `<Alert variant="...">`; the app-local `ui/packages/app/components/ui/data-table.tsx` is deleted; coverage thresholds in `ui/packages/app/vitest.config.ts` still pass; design-system test suite has ≥ 6 new tests covering both primitives.

**Problem:** The design-system package is the source of truth for visual primitives, but two recurring patterns are missing:

1. **Tabular data.** `DataTable` lives at `ui/packages/app/components/ui/data-table.tsx` — app-local, not exported via `@usezombie/design-system/index.ts`. New table surfaces (the M48 BillingUsageTab was the trigger) end up either re-discovering the primitive or — worse — falling back to raw `<table>` markup that the UI Substitution Gate has no jurisdiction to flag because `<table>` has no design-system equivalent in scope.
2. **Inline status banners.** `border-{token}/40 bg-{token}/10 text-{token}` is duplicated across at least `ExhaustionBanner.tsx`, `BillingBalanceCard` (exhausted), `BillingUsageTab` (load-more error), `ProviderSelector` (auth/API error), `provider/page.tsx` (resolver error), `InstallZombieForm.tsx` (validation error), `ApprovalsList` (resolve error), and `ResolveButtons.tsx`. No primitive captures the pattern, so each call site re-decides padding, border opacity, role, and animation.

**Solution summary:** Promote `DataTable` into the design-system package unchanged (preserve the prop API). Add an `Alert` primitive with `cva`-driven variants (`info`, `success`, `warning`, `destructive`) that locks in role + token + spacing + entrance animation. Migrate every duplicated banner to `<Alert variant>`. Delete the app-local `data-table.tsx`. Extend the UI Substitution Gate prose so future banners and tables get auto-flagged.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/design-system/src/design-system/DataTable.tsx` | CREATE | Promoted from app-local — public primitive |
| `ui/packages/design-system/src/design-system/DataTable.test.tsx` | CREATE | Test coverage in design-system package |
| `ui/packages/design-system/src/design-system/Alert.tsx` | CREATE | New primitive (variants: info / success / warning / destructive) |
| `ui/packages/design-system/src/design-system/Alert.test.tsx` | CREATE | Test coverage incl. role / variant / dismissable / a11y |
| `ui/packages/design-system/src/design-system/index.ts` | EDIT | Export `DataTable`, `Alert`, type aliases |
| `ui/packages/design-system/src/index.ts` | EDIT | Re-export from package root |
| `ui/packages/app/components/ui/data-table.tsx` | DELETE | App-local copy is now redundant |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingUsageTab.tsx` | EDIT | Switch import to `@usezombie/design-system` |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx` | EDIT | Replace inline destructive banner with `<Alert variant="destructive">` |
| `ui/packages/app/app/(dashboard)/settings/provider/page.tsx` | EDIT | Replace inline resolver-error banner |
| `ui/packages/app/app/(dashboard)/settings/provider/components/ProviderSelector.tsx` | EDIT | Replace inline error banner |
| `ui/packages/app/app/(dashboard)/settings/provider/components/ByokFields.tsx` | EDIT | Replace warning empty-state inline banner |
| `ui/packages/app/components/domain/ExhaustionBanner.tsx` | EDIT | Replace inline destructive banner |
| `ui/packages/app/app/(dashboard)/zombies/new/InstallZombieForm.tsx` | EDIT | Replace inline error banner |
| `ui/packages/app/app/(dashboard)/approvals/components/ApprovalsList.tsx` | EDIT | Replace inline error banner |
| `ui/packages/app/app/(dashboard)/approvals/[gateId]/ResolveButtons.tsx` | EDIT | Replace inline error banner |
| `ui/packages/app/components/domain/EventsList.tsx` | EDIT | Replace inline error banner |
| `docs/gates/ui-substitution.md` | EDIT | Extend trigger list to include `<div role="alert">` (Alert primitive) and `<table>` (DataTable primitive) |

> The migration sweep is grep-driven. Discovery command (run BEFORE editing): `grep -rln 'role="alert"\|<table\b' ui/packages/app/`. Every match either becomes an `Alert` / `DataTable` consumer or gets a one-line "raw HTML kept" justification per the gate's existing format.

---

## Sections (implementation slices)

### §1 — `Alert` primitive (greenfield)

Define `<Alert variant>` with the `cva` pattern Badge already uses. Variants map to existing theme tokens: `info → --info`, `success → --success`, `warning → --warning`, `destructive → --destructive`. The component sets `role="alert"` (or `role="status"` for non-blocking variants — see Implementation default below) and applies the canonical `border-{token}/40 bg-{token}/10 text-{token}` shape. Slot for an icon + title + description; supports `asChild` for cases where a banner needs to render inside a Tooltip / DialogTrigger.

**Implementation default:** `role="alert"` for `destructive`/`warning` (assistive tech announces immediately); `role="status"` for `info`/`success` (announces non-disruptively). The agent picks unless a call site explicitly opts out via `role` prop.

**Implementation default:** dismissable=false. Adding a dismiss handler is opt-in via `onDismiss` prop. Toast/timer behavior is explicitly out of scope (lives in the future Toast primitive).

### §2 — `DataTable` promotion

Move `ui/packages/app/components/ui/data-table.tsx` to `ui/packages/design-system/src/design-system/DataTable.tsx`. Preserve the prop surface byte-for-byte. The `cn` import already comes from `@usezombie/design-system/utils`, so no path rewrites needed inside the file. The local copy is deleted (RULE NDC + RULE NLR — touch-it-fix-it).

### §3 — Migration sweep

Replace every grep-discovered banner site with `<Alert variant="...">`. The discovery command is in Files Changed. For each site, the variant choice follows existing semantic intent:
- "exhausted" / "failed" / "rejected" → `destructive`
- "no credentials yet" / "coming soon" / amber CTA → `warning`
- "switched to BYOK" / "saved successfully" → `success`
- everything else neutral → `info`

Each migrated file's diff shrinks by 4-8 lines (the inline className collapses to a single prop). The migration is mechanical but per-file judgment: surface intent stays the same, role stays identical or becomes more precise.

### §4 — Substitution Gate extension

Update `docs/gates/ui-substitution.md` so the pre-edit-check list adds two lines: `<table>` → use `DataTable`, `<div role="alert">` (or `<p role="alert">`) → use `<Alert>`. The end-of-turn audit regex extends to flag both. One-line edits each.

### §5 — Storybook / dev playground entry

Add a route under `ui/packages/app/app/(dev)/ds-button-rsc/` (or its equivalent) that renders both new primitives in their full variant matrix. Same `next build` "no client-side hoist" assertion the existing ds-button-rsc fixture enforces. The agent reads the existing fixture and mirrors the structure.

### §6 — Coverage parity

`ui/packages/app/vitest.config.ts` thresholds (95/90/95/95) must continue to pass after the migration. The `Alert` component's branch coverage matters because the variant default selection is one branch per variant. Test names enumerate all four variants + the role-default-by-severity decision.

---

## Interfaces

```tsx
// @usezombie/design-system

export const alertVariants: cva(...);  // info / success / warning / destructive
export type AlertVariant = "info" | "success" | "warning" | "destructive";
export interface AlertProps extends ComponentProps<"div"> {
  variant?: AlertVariant;
  /** Optional dismiss handler. Renders an <X> button. */
  onDismiss?: () => void;
  /** Override the default role choice (alert for destructive/warning, status otherwise). */
  role?: "alert" | "status";
  /** asChild for rare cases where the banner is inside a constrained slot. */
  asChild?: boolean;
}
export function Alert(props: AlertProps): JSX.Element;

// DataTable: identical to the current ui/packages/app/components/ui/data-table.tsx
// surface — see that file for the canonical signature. Re-exported as-is.
export {
  DataTable,
  type DataTableColumn,
  type DataTableProps,
};
```

The promoted `DataTable` MUST preserve every public name exported by the current app-local file. Renames break consumers and are forbidden by this spec — amend the spec first if a rename is needed.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Migration misses a banner | Grep regex doesn't match a hand-rolled `<div className="rounded-md border ...">` without `role="alert"` | The Substitution Gate's end-of-turn audit (extended in §4) catches the leftover at the next edit. Implementer additionally runs a manual pass: `grep -rn 'border-destructive/40\|bg-warning/10' ui/packages/app/`. |
| Promoted `DataTable` import path is wrong in a consumer | Some consumer imports from `@/components/ui/data-table` (relative) instead of `@usezombie/design-system` (package) | Delete the app-local file as the LAST step of the migration so a stale import surfaces as a build error during `bun run typecheck`, not at runtime. |
| Existing test mocks `@/components/ui/data-table` | A test stubs the local module path; after deletion the mock target doesn't exist, but the test no longer needs to mock it (the design-system primitive is straightforward) | Update the mock to `@usezombie/design-system` OR remove the mock and let the real component render (preferred — fewer indirection layers in tests). |
| `Alert` variant choice is wrong at a call site | Implementer picks `info` where `destructive` was intended (or vice versa), losing the visual urgency of the original | Per-file judgment review; spec §3 lists the semantic mapping, but every call site gets a 1-second look. The /review skill catches inversions. |
| Coverage drops below threshold | Migration removes existing inline JSX, re-routes through `Alert`; some branches collapse and v8 reports lower coverage | Add `Alert` variant tests in the design-system package. The thresholds in app/vitest.config.ts stay; the Alert tests live in design-system/vitest.config.ts (already at higher thresholds). |

---

## Invariants

1. **`@usezombie/design-system` exports `DataTable` and `Alert`.** Enforced by `ui/packages/design-system/src/index.test.ts` (`it.each` over the export list).
2. **No file under `ui/packages/app/` imports `DataTable` from `@/components/ui/data-table`.** Enforced by deletion of the app-local file (build-time error if anyone tries) + a grep assertion in the migration commit.
3. **No `<div role="alert"|status">` with hand-rolled `border-{token}/40 bg-{token}/10` exists under `ui/packages/app/**/*.tsx` after this milestone.** Enforced by the extended UI Substitution Gate's end-of-turn audit regex.
4. **`DataTable` prop surface is unchanged byte-for-byte from the app-local version.** Enforced by `tsc --noEmit` across every existing consumer; any breaking change forces a spec amendment.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_alert_renders_each_variant_with_correct_token` | Render `<Alert variant>` for each of the four variants; assert the rendered className contains the matching theme-token segment (`text-destructive`, `bg-warning/10`, etc.). |
| `test_alert_role_defaults_by_severity` | `destructive` and `warning` default to `role="alert"`; `info` and `success` default to `role="status"`. |
| `test_alert_role_override_respected` | Passing `role="alert"` to an `info` Alert overrides the default. |
| `test_alert_dismissable_fires_callback` | Render with `onDismiss`; click the dismiss button; assert the callback fired exactly once. |
| `test_alert_no_dismiss_button_when_callback_missing` | Render without `onDismiss`; assert no `<button>` is rendered inside the Alert. |
| `test_alert_aschild_passes_props_to_child` | `asChild` with a `<Link>` child surfaces the role + className on the link element. |
| `test_datatable_promotion_export_signature_unchanged` | Compile-time test importing `DataTableProps` from the package and asserting structural compatibility with the previous app-local export. |
| `test_datatable_renders_columns_and_rows_at_package_layer` | Mirror the existing app-local DataTable test inside the design-system package so coverage is owned by the package now. |
| `test_substitution_gate_audit_flags_raw_table` | Run the gate's end-of-turn audit script over a fixture diff that adds `<table>`; assert exit code is non-zero. |
| `test_substitution_gate_audit_flags_raw_alert_div` | Same but for `<div role="alert">` with hand-rolled token classes. |
| `test_app_local_datatable_deleted` | `test ! -f ui/packages/app/components/ui/data-table.tsx`. |
| `test_no_orphaned_imports_to_local_data_table` | `grep -rn '@/components/ui/data-table' ui/` returns 0 matches. |

---

## Acceptance Criteria

- [ ] `bun run typecheck` clean across `ui/packages/{app,website,design-system}`
- [ ] `bun run lint` clean across `ui/packages/{app,website,design-system}`
- [ ] `bun run test` passes in all three UI packages — verify: `(cd ui/packages/app && bun run test) && (cd ui/packages/design-system && bun run test) && (cd ui/packages/website && bun run test)`
- [ ] `bun run test:coverage` passes app's 95/90/95/95 threshold — verify: `(cd ui/packages/app && bun run test:coverage)`
- [ ] DataTable + Alert exported from package — verify: `grep -E "^\s*(DataTable|Alert)" ui/packages/design-system/src/design-system/index.ts`
- [ ] App-local DataTable deleted — verify: `test ! -f ui/packages/app/components/ui/data-table.tsx`
- [ ] No stale imports — verify: `grep -rn '@/components/ui/data-table' ui/`
- [ ] No remaining hand-rolled banner — verify: `grep -rln 'role="alert"' ui/packages/app/ | xargs grep -l 'border-destructive/40\|border-warning/40\|border-info/40\|border-success/40'` returns nothing
- [ ] UI Substitution Gate prose updated — verify: `grep -E "<table>|role=\"alert\"" docs/gates/ui-substitution.md`
- [ ] No file over 350 lines added
- [ ] `gitleaks detect` clean

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Package exports both primitives
grep -E "^\s*(DataTable|Alert)" ui/packages/design-system/src/design-system/index.ts && echo "PASS" || echo "FAIL"

# E2: App-local DataTable deleted
test ! -f ui/packages/app/components/ui/data-table.tsx && echo "PASS" || echo "FAIL"

# E3: No stale imports to the deleted file
[ -z "$(grep -rln '@/components/ui/data-table' ui/)" ] && echo "PASS" || echo "FAIL"

# E4: No leftover hand-rolled banners (spot-check by token)
[ -z "$(grep -rln 'border-destructive/40\|border-warning/40' ui/packages/app/ | grep -v Alert.tsx)" ] && echo "PASS" || echo "FAIL"

# E5: Build the UI workspace
(cd ui/packages/app && bun run typecheck && bun run lint && bun run test:coverage) || echo "FAIL"
(cd ui/packages/design-system && bun run lint && bun run test) || echo "FAIL"
(cd ui/packages/website && bun run typecheck && bun run lint && bun run test) || echo "FAIL"

# E6: Gitleaks
gitleaks detect 2>&1 | tail -3

# E7: 350-line gate
git diff --name-only origin/main | grep -E '\.(tsx|ts)$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines" }'

# E8: Substitution Gate audit on the migrated diff is clean
git diff -U0 origin/main -- 'ui/packages/app/**/*.tsx' \
  | grep -E '^\+.*<(table|div role="alert"|p role="alert")\b' \
  | grep -v Alert.tsx \
  | head
echo "E8: leftover-banner sweep (empty output = pass)"
```

---

## Dead Code Sweep

**1. Orphaned files — must be deleted from disk and git.**

| File to delete | Verify deleted |
|----------------|----------------|
| `ui/packages/app/components/ui/data-table.tsx` | `test ! -f ui/packages/app/components/ui/data-table.tsx` |

**2. Orphaned references — zero remaining imports or uses.**

| Deleted symbol or import | Grep command | Expected |
|--------------------------|--------------|----------|
| `@/components/ui/data-table` | `grep -rn '@/components/ui/data-table' ui/` | 0 matches |
| Inline destructive banner | `grep -rln 'border-destructive/40' ui/packages/app/` | 0 matches outside `Alert.tsx` |
| Inline warning banner | `grep -rln 'border-warning/40' ui/packages/app/` | 0 matches outside `Alert.tsx` |

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits coverage of Alert variants + DataTable promotion smoke test + migrated call sites. | Skill returns clean. |
| After tests pass | `/review` | Adversarial diff review against this spec, the design-system contract, and the UI Substitution Gate. Catches missed banner sites. | Skill returns clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comment the open PR; verify the migration sweep didn't miss anything in files outside the original blast radius. | Comments addressed inline. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests (app) | `(cd ui/packages/app && bun run test)` | {paste output snippet} | |
| Unit tests (design-system) | `(cd ui/packages/design-system && bun run test)` | {paste output snippet} | |
| Coverage (app) | `(cd ui/packages/app && bun run test:coverage)` | {paste output snippet} | |
| Lint | `make lint` | {paste output snippet} | |
| Gitleaks | `gitleaks detect` | {paste output snippet} | |
| 350L gate | `wc -l` on changed files | {paste output snippet} | |
| Dead-code sweep — local DataTable deleted | `test ! -f ui/packages/app/components/ui/data-table.tsx; echo $?` | {0 = pass} | |
| Dead-code sweep — banner tokens absent | `grep -rln 'border-destructive/40' ui/packages/app/ \| grep -v Alert.tsx` | {empty = pass} | |

---

## Out of Scope

- **Toast primitive.** Auto-dismissing notifications with timer + queue. The `Alert` primitive intentionally stops at static banners with optional manual dismiss; toast lifecycle deserves its own spec.
- **Form-field error message primitive.** Inline field errors (under an input) are a different visual treatment from page-level Alerts; lives in the existing `FormMessage` from react-hook-form integration.
- **Tabular sorting / filtering on `DataTable`.** The promotion preserves the current contract exactly; sort + filter UI lands in a follow-up spec when at least one consumer needs it.
- **Migration of design-system test files to a Storybook setup.** The current `*.test.tsx` pattern stays; Storybook adoption is a separate prototype-level decision.
