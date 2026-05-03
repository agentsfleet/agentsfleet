# M60_001: Add `Time`, `List`, `DescriptionList` primitives to `@usezombie/design-system`

**Prototype:** v2.0.0
**Milestone:** M60
**Workstream:** 001
**Date:** May 03, 2026
**Status:** PENDING
**Priority:** P2 — design-system hygiene; closes the remaining raw-HTML drift surfaced during M59_001 PR review (Captain flagged `<time>` / `<ul>` / `<dl>` patterns the dashboard re-discovers per site).
**Categories:** UI
**Batch:** B1
**Branch:** feat/m60-001-design-system-time-list-dl (to be created)
**Depends on:** M59_001 (DONE) — established the design-system promotion pattern (`cva` variants, file-as-struct shape, co-located test, exports through `src/design-system/index.ts` + `src/index.ts`, dual `asChild` + variant API).

**Canonical architecture:** `docs/ARCHITECTURE.md` §UI Design System — primitive boundary between `ui/packages/design-system/` (cross-app primitives) and `ui/packages/app/components/ui/` (app-local helpers awaiting promotion).

---

## Implementing agent — read these first

1. `ui/packages/design-system/src/design-system/Alert.tsx` — `cva` + discriminated-union prop-shape pattern landed in M59_001. Mirror its file-as-struct layout.
2. `ui/packages/design-system/src/design-system/Card.tsx` — sub-part composition (`Card` + `CardHeader` + `CardTitle` + …). `DescriptionList` follows the same pattern.
3. `ui/packages/design-system/src/design-system/Select.tsx` — Radix-backed compound API with portal/animation tokens. `Time`'s tooltip composition reuses the existing `Tooltip` primitive the same way.
4. `ui/packages/design-system/src/design-system/Tooltip.tsx` — what `Time` will compose with when `tooltip={true}` and `format="relative"`.
5. `ui/packages/app/components/domain/ExhaustionBanner.tsx` — current `new Date(...).toLocaleString()` site; first `Time` consumer post-migration.
6. `ui/packages/app/app/(dashboard)/settings/provider/page.tsx` — current `<dl><div class="flex justify-between"><dt>...</dt><dd>...</dd></div></dl>` block; first `DescriptionList` consumer.
7. `docs/gates/ui-substitution.md` — extend trigger list + end-of-turn audit regex.
8. `docs/BUN_RULES.md` — TS file-shape decision applies to every new `*.tsx` / `*.ts`.

The agent should be able to plan after reading these without further clarification.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal repo discipline (RULE TST-NAM for test names, RULE NLR for the migration sweep, RULE NDC for any helpers deleted along the way).
- `docs/gates/ui-substitution.md` — UI Component Substitution Gate; this spec extends it for `<time>` / `<ul>` / `<ol>` / `<dl>` / `<dt>` / `<dd>`.
- `docs/gates/file-length.md` — ≤ 350 lines per file, ≤ 50 per function.
- `docs/BUN_RULES.md` — file-shape decision at PLAN time for every new `*.tsx`.

No Zig touched; `docs/ZIG_RULES.md` does not apply.

---

## Overview

**Goal (testable):** `import { Time, List, ListItem, DescriptionList, DescriptionTerm, DescriptionDetails } from "@usezombie/design-system"` resolves; every dashboard surface that today hand-rolls a user-visible `<time>` / `<ul>` / `<dl>` for semantic content is migrated to the primitive; UI Substitution Gate prose + end-of-turn audit regex extend to cover those tags; coverage thresholds in `ui/packages/app/vitest.config.ts` (95/90/95/95) continue to pass; design-system test suite gains co-located tests for each new primitive.

**Problem:**

1. **Dates.** Several call sites construct user-visible timestamps inline via `new Date(x).toLocaleString()` / `.toLocaleDateString()` / hand-rolled relative-time strings (e.g. `ExhaustionBanner.tsx`, plus every Approvals/Events surface where ISO timestamps appear). No primitive owns "render a backend ISO-8601 timestamp consistently with a hover tooltip + a stable machine-readable `<time datetime>` element". Locale handling, hydration safety (server vs. client divergence), and "X minutes ago" all get re-discovered ad-hoc.
2. **Lists.** `<ul class="space-y-2">` and `<ol class="list-decimal pl-5">` recur across `ApprovalsList`, `EventsList`, settings panels. No `List` primitive locks in spacing tokens, separator behaviour, or bullet style — each call site re-decides padding and dividers.
3. **Description lists.** The `<dl><div class="flex justify-between"><dt>...</dt><dd>...</dd></div></dl>` key/value pattern appears in `settings/provider/page.tsx`, `BillingBalanceCard`, `ZombieConfig`, and others. No primitive captures the "label + value, right-aligned monospace value, optional copy-button" shape.

**Solution summary:** Promote three new primitives into `ui/packages/design-system/src/design-system/`:

- **`Time.tsx`** — wraps `<time datetime>`. Props: `value: string | Date`, `format?: "absolute" | "relative" | "datetime"` (default `absolute`), `tooltip?: boolean` (default `true` when `format === "relative"`, surfaces the absolute form on hover via the existing `Tooltip` primitive), `locale?: string` (default `en-US`). Hydration-safe: ISO `datetime` attr is identical server-and-client; visible relative string flagged with `suppressHydrationWarning`.
- **`List.tsx`** — `List` + `ListItem` compound. Variants via `cva`: `unordered` (default `disc` bullets) / `ordered` (decimal) / `plain` (no marker, just spacing). `divided?: boolean` adds `border-b border-border` on each item except the last. Renders `<ul>` / `<ol>` semantically; `asChild` for the rare case the caller needs a different host.
- **`DescriptionList.tsx`** — `DescriptionList` + `DescriptionTerm` + `DescriptionDetails`. Layout variant: `inline` (term + detail same row, `flex flex-wrap justify-between` — current dashboard convention) / `stacked` (term above detail, for forms). `mono?: boolean` on `DescriptionDetails` adds `font-mono` for IDs / hashes / credential refs.

Migrate the discovered call sites; extend the UI Substitution Gate prose and audit regex; preserve coverage.

**Pre-edit discovery command (run BEFORE editing):**

```bash
grep -rln '<time\b\|<ul\b\|<ol\b\|<dl\b\|<dt\b\|<dd\b\|toLocaleString\|toLocaleDateString' ui/packages/app/
```

Every match either becomes a primitive consumer or gets a one-line "raw HTML kept" justification per the gate's existing format (e.g. nav `<ul>` for layout, not semantic listing).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/design-system/src/design-system/Time.tsx` | CREATE | Public primitive |
| `ui/packages/design-system/src/design-system/Time.test.tsx` | CREATE | Co-located coverage |
| `ui/packages/design-system/src/design-system/List.tsx` | CREATE | Public primitive |
| `ui/packages/design-system/src/design-system/List.test.tsx` | CREATE | Co-located coverage |
| `ui/packages/design-system/src/design-system/DescriptionList.tsx` | CREATE | Public primitive |
| `ui/packages/design-system/src/design-system/DescriptionList.test.tsx` | CREATE | Co-located coverage |
| `ui/packages/design-system/src/design-system/index.ts` | EDIT | Export new primitives + types |
| `ui/packages/design-system/src/index.ts` | EDIT | Re-export from package root |
| `ui/packages/app/components/domain/ExhaustionBanner.tsx` | EDIT | Replace `toLocaleString()` with `<Time>` |
| `ui/packages/app/app/(dashboard)/settings/provider/page.tsx` | EDIT | Replace inline `<dl>` blocks with `DescriptionList` |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx` | EDIT | `DescriptionList` for balance / `exhausted_at` metadata |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/ZombieConfig.tsx` | EDIT | `DescriptionList` for config key/value rows |
| `ui/packages/app/app/(dashboard)/approvals/components/ApprovalsList.tsx` | EDIT | `List` wrapper for approvals + `Time` for `created_at` |
| `ui/packages/app/components/domain/EventsList.tsx` | EDIT | `Time` for event timestamps; `List` if applicable |
| `docs/gates/ui-substitution.md` | EDIT | Extend trigger list + end-of-turn audit regex |

> Layout-only `<ul>` / `<nav>` (e.g. dashboard top nav, dropdown menus) stay raw with a one-line "Raw HTML kept" justification — the primitive applies to semantic listing, not navigation chrome.

---

## Sections (implementation slices)

### §1 — `Time` primitive (greenfield)

Wraps `<time datetime>`. Props:

- `value: string | Date` — ISO-8601 string or `Date`; coerced once.
- `format?: "absolute" | "relative" | "datetime"` — default `absolute`. `relative` produces "X minutes ago" via a small inline helper (no `date-fns` / `dayjs` dep — the calculation is bounded and deterministic). `datetime` returns the full ISO string.
- `tooltip?: boolean` — default `true` when `format === "relative"`, otherwise `false`. When set, wraps the rendered `<time>` in the existing `Tooltip` primitive showing the absolute form.
- `locale?: string` — default `"en-US"`; passed to `Intl.DateTimeFormat` for `absolute` format.

**Hydration safety:** `datetime` attribute is the canonical ISO-8601 string and matches byte-for-byte across SSR + CSR. The visible label uses `suppressHydrationWarning` for `relative` (since "5 minutes ago" advances on render) and is deterministic for `absolute` / `datetime` given the same `locale`.

### §2 — `List` + `ListItem` primitive (greenfield)

`cva` variants: `unordered` (default), `ordered`, `plain`. `plain` resets `list-style: none` to defeat Tailwind preflight. `divided?: boolean` adds `border-b border-border` on `:not(:last-child)`. Renders `<ul>` for `unordered`/`plain`, `<ol>` for `ordered`. `ListItem` is a thin `<li>` wrapper with token spacing; `asChild` on `List` for the rare alternate host (e.g. `<menu>`).

### §3 — `DescriptionList` primitive (greenfield)

`DescriptionList` + `DescriptionTerm` + `DescriptionDetails`. Layout variants:

- `inline` (default) — `flex flex-wrap items-baseline justify-between gap-2` per row group; matches the existing provider-page convention; wraps gracefully on narrow viewports.
- `stacked` — term above detail, for vertical forms.

`DescriptionDetails` accepts `mono?: boolean` to add `font-mono text-xs` for IDs, credential refs, hashes (matches existing dashboard convention).

### §4 — Migration sweep

Mechanical, grep-driven (command in Overview). Per-site judgement:

- Hand-rolled `toLocaleString()` / `toLocaleDateString()` → `<Time value={iso}>` (caller picks `format`).
- `<dl><div class="flex justify-between"><dt>...</dt><dd>...</dd></div></dl>` → `<DescriptionList layout="inline">…<DescriptionTerm>…</DescriptionTerm><DescriptionDetails mono>…</DescriptionDetails>…</DescriptionList>`.
- `<ul class="space-y-2">…</ul>` containing semantic items → `<List variant="unordered" divided?>`. Pure-layout `<ul>` keeps a Raw-HTML-Kept justification.

Each migrated file's diff shrinks; no API change visible to users.

### §5 — Substitution Gate extension

Single edit to `docs/gates/ui-substitution.md`:

- Add to the substitute table: `<time>` → `<Time>`; `<ul>` / `<ol>` (semantic) → `<List variant>`; `<dl>` / `<dt>` / `<dd>` → `<DescriptionList>` + sub-parts.
- Extend the end-of-turn audit regex to include `time|ul|ol|dl|dt|dd` (anchored to the same multi-line audit landed in M59_001).

### §6 — Coverage parity

Co-located DS tests + ensure app vitest thresholds (95/90/95/95) continue passing. Time component branch coverage matters: format default selection + locale path + tooltip-on-relative are three branches.

---

## Interfaces

```tsx
// @usezombie/design-system
export interface TimeProps
  extends Omit<ComponentProps<"time">, "datetime" | "children"> {
  value: string | Date;
  format?: "absolute" | "relative" | "datetime";
  tooltip?: boolean;
  locale?: string;
}
export function Time(props: TimeProps): JSX.Element;

export const listVariants: ReturnType<typeof cva>;
export type ListVariant = "unordered" | "ordered" | "plain";
export interface ListProps extends Omit<ComponentProps<"ul">, "children"> {
  variant?: ListVariant;
  divided?: boolean;
  children: ReactNode;
  asChild?: boolean;
}
export function List(props: ListProps): JSX.Element;
export function ListItem(props: ComponentProps<"li">): JSX.Element;

export interface DescriptionListProps extends ComponentProps<"dl"> {
  layout?: "inline" | "stacked";
}
export function DescriptionList(props: DescriptionListProps): JSX.Element;
export function DescriptionTerm(props: ComponentProps<"dt">): JSX.Element;
export interface DescriptionDetailsProps extends ComponentProps<"dd"> {
  mono?: boolean;
}
export function DescriptionDetails(props: DescriptionDetailsProps): JSX.Element;
```

The new exports MUST land in `src/design-system/index.ts` AND `src/index.ts`. Renames after merge are forbidden by Invariant 1 — amend the spec first.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Time hydration mismatch | `format="relative"` SSR string differs from CSR string | `datetime` attr is the canonical machine-readable form (identical byte-for-byte); visible relative label opts into `suppressHydrationWarning`. Absolute / datetime formats are deterministic given a fixed `locale`. |
| List variant collision with Tailwind preflight | `list-disc` vs `list-none` reset semantics differ across browsers | `cva` resets `list-style` explicitly on every variant; `plain` sets `list-none` + `pl-0`. |
| `DescriptionList layout="inline"` overflow on narrow viewport | Long detail values push past the term column | `flex flex-wrap` on the row group lets the detail wrap below the term gracefully. |
| Migration misses a hand-rolled `<dl>` whose siblings are not in a flex wrapper | Pre-edit grep returns the literal but the structural shape isn't a 1:1 match | Per-site judgement; spec §4 says "caller picks layout". The substitution-gate audit catches stragglers next edit. |
| Existing test depends on raw `<dl>` / `<time>` markup | Test asserts `tag === "DT"` or similar | Update assertions to query by role / text via `@testing-library`. |
| Coverage drops below threshold | Migration removes inline JSX, branches collapse | Co-located primitive tests in design-system package compensate. App thresholds must remain 95/90/95/95. |

---

## Invariants

1. **`@usezombie/design-system` exports `Time`, `List`, `ListItem`, `DescriptionList`, `DescriptionTerm`, `DescriptionDetails`.** Enforced by `ui/packages/design-system/src/index.test.ts` (`it.each` over the export list).
2. **No file under `ui/packages/app/` calls `toLocaleString()` / `toLocaleDateString()` directly on a user-visible timestamp.** Enforced by extended UI Substitution Gate audit + `grep -rn 'toLocaleString\|toLocaleDateString' ui/packages/app/` returning 0 non-test matches.
3. **No `<dl>` / `<ul>` / `<ol>` raw markup remains in dashboard files where the intent is semantic listing or key/value display.** Layout-only navigation `<ul>` stays raw with a "Raw HTML kept" justification per the gate's existing format.
4. **`Time` renders an identical `datetime` attribute server-side and client-side for the same `value` input.** Enforced by an SSR-vs-CSR snapshot test asserting the rendered `<time datetime="...">` byte-for-byte across `renderToString` + `render`.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_time_renders_iso_datetime_attr_for_string_value` | `<Time value="2026-05-03T10:00:00Z">` renders `<time datetime="2026-05-03T10:00:00Z">`. |
| `test_time_renders_iso_datetime_attr_for_date_value` | `Date` instance is normalized to canonical ISO string in the `datetime` attr. |
| `test_time_absolute_format_uses_locale_default_en_us` | `format="absolute"` produces a `Intl.DateTimeFormat("en-US")`-equivalent string. |
| `test_time_relative_format_renders_x_ago_string_with_tooltip_absolute` | `format="relative"` produces "X minutes ago" / "X hours ago" / "X days ago"; tooltip exposes the absolute form. |
| `test_time_datetime_format_returns_full_iso` | `format="datetime"` produces the full ISO string both as `datetime` attr AND as visible text. |
| `test_time_ssr_csr_datetime_attr_identical` | Render via `renderToString` + via `render`; the `datetime` attribute on `<time>` is identical byte-for-byte. |
| `test_list_unordered_default_renders_ul_with_disc` | `<List>…</List>` renders `<ul>` with `list-disc`. |
| `test_list_ordered_renders_ol_with_decimal` | `<List variant="ordered">` renders `<ol>` with `list-decimal`. |
| `test_list_plain_renders_ul_with_no_marker` | `<List variant="plain">` renders `<ul>` with `list-none` + `pl-0`. |
| `test_list_divided_adds_border_b_on_non_last_items` | `<List divided>` applies `border-b` to every `ListItem` except the last. |
| `test_list_aschild_passes_through` | `<List asChild>…<menu>…</menu></List>` renders `<menu>` with the variant classes. |
| `test_dl_inline_layout_uses_flex_justify_between` | Default `<DescriptionList>` row applies `flex justify-between` (or equivalent inline layout class set). |
| `test_dl_stacked_layout_renders_term_above_detail` | `<DescriptionList layout="stacked">` stacks `<dt>` above `<dd>` (no flex-row). |
| `test_dl_details_mono_modifier_adds_font_mono` | `<DescriptionDetails mono>` includes `font-mono` in `className`. |
| `test_substitution_gate_audit_flags_raw_time_tag` | Gate audit regex over a fixture diff containing `<time>` returns non-empty. |
| `test_substitution_gate_audit_flags_raw_dl_tag` | Same for `<dl>` (and `<ul>`, `<ol>`). |
| `test_no_orphaned_to_locale_string_calls` | `grep -rn 'toLocaleString\|toLocaleDateString' ui/packages/app/` (excluding `Time.tsx` + tests) returns 0. |

---

## Acceptance Criteria

- [ ] `bun run typecheck` clean across `ui/packages/{app,website,design-system}` — verify: `(cd ui/packages/app && bunx tsc --noEmit) && (cd ui/packages/design-system && bunx tsc --noEmit) && (cd ui/packages/website && bunx tsc --noEmit)`
- [ ] `bun run lint` clean across `ui/packages/{app,website,design-system}`
- [ ] `bun run test` passes in all three UI packages
- [ ] `bun run test:coverage` (app) passes 95/90/95/95
- [ ] All three primitives + sub-parts exported — verify: `grep -E "^\s*(Time|List|ListItem|DescriptionList|DescriptionTerm|DescriptionDetails)" ui/packages/design-system/src/design-system/index.ts`
- [ ] No raw `toLocaleString` / `toLocaleDateString` for user-visible dates — verify: `[ -z "$(grep -rln 'toLocaleString\|toLocaleDateString' ui/packages/app/ | grep -v Time.tsx | grep -v '\.test\.')" ]`
- [ ] UI Substitution Gate prose + audit regex updated — verify: `grep -E "<time>|<dl>|<ul>" docs/gates/ui-substitution.md`
- [ ] No file over 350 lines added
- [ ] `gitleaks detect` clean

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: package exports the three primitives + sub-parts
grep -E "^\s*(Time|List|ListItem|DescriptionList|DescriptionTerm|DescriptionDetails)" ui/packages/design-system/src/design-system/index.ts && echo PASS

# E2: no raw toLocaleString in app for user-visible dates
[ -z "$(grep -rln 'toLocaleString\|toLocaleDateString' ui/packages/app/ | grep -v Time.tsx | grep -v '\.test\.')" ] && echo PASS || echo FAIL

# E3: no raw <time> / <dl> in dashboard surfaces (layout justifications allowed)
grep -rln '<time\b\|<dl\b' ui/packages/app/app | grep -v node_modules | head

# E4: typecheck + lint + test:coverage across all three UI packages
(cd ui/packages/app && bun run typecheck && bun run lint && bun run test:coverage)
(cd ui/packages/design-system && bun run lint && bun run test)
(cd ui/packages/website && bun run typecheck && bun run lint && bun run test)

# E5: gitleaks
gitleaks detect 2>&1 | tail -3

# E6: 350L gate
git diff --name-only origin/main | grep -E '\.(tsx|ts)$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER:", $0 }'

# E7: Substitution Gate audit on the migrated diff is clean
git diff -U0 origin/main -- 'ui/packages/app/**/*.tsx' \
  | grep -E '^\+.*<(time|ul|ol|dl|dt|dd)\b' \
  | grep -v -E 'Time\.tsx|List\.tsx|DescriptionList\.tsx' \
  | head
echo "E7: leftover-tag sweep (empty output = pass)"
```

---

## Dead Code Sweep

**1. Orphaned files** — none expected at creation; primitives are greenfield. If migration discovers an app-local `time-format.ts` / list helper, list here at CHORE(close).

**2. Orphaned references**

| Deleted symbol or import | Grep command | Expected |
|--------------------------|--------------|----------|
| Any inline `toLocaleString` on a user-visible date | `grep -rn 'toLocaleString\|toLocaleDateString' ui/packages/app/` | 0 non-test matches |
| Hand-rolled flex `<dt>`/`<dd>` rows | `grep -rn 'flex justify-between' ui/packages/app/ \| grep -B1 '<dt'` | 0 |

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| VERIFY | `/write-unit-test` | Audits coverage of Time/List/DescriptionList variants + migrated call sites. | Skill returns clean. |
| Before CHORE(close) | `/review` | Adversarial diff review against this spec, the design-system contract, and the UI Substitution Gate. Catches missed migration sites. | Skill returns clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Comments on the open PR; verifies the migration sweep did not miss anything outside the original blast radius. | Comments addressed inline. |
| After every push | `kishore-babysit-prs` | Polls greptile + applies follow-ups per cadence. | Two consecutive empty polls. |

---

## Discovery (consult log)

_(Empty at creation. Populate as Legacy-Design / Architecture consults fire during EXECUTE.)_

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
| Dead-code sweep — no `toLocaleString` in app | `grep -rln 'toLocaleString\|toLocaleDateString' ui/packages/app/ \| grep -v Time.tsx \| grep -v '\.test\.'` | {empty = pass} | |
| Dead-code sweep — no raw `<time>` / `<dl>` in dashboard | `grep -rln '<time\b\|<dl\b' ui/packages/app/app` | {only justified raw-kept = pass} | |

---

## Out of Scope

- **Calendar / date-picker primitives.** `Time` is read-only; picker is its own milestone (depends on `@radix-ui/react-popover` + a date-library decision).
- **Internationalisation framework.** `Time` accepts a `locale` prop but no global i18n provider is introduced.
- **Sortable / draggable List variants.** Static rendering only; drag-and-drop lives in a future dedicated workstream.
- **Generic data-grid primitive.** Already covered by `DataTable` from M59_001.
- **Time-zone aware formatting beyond `Intl.DateTimeFormat`.** Backend stores UTC; rendering is browser-locale. A `tz` prop is deferred.
