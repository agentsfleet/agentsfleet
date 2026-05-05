# M61_004: UI sweep — duplicate Card primitive, unwired analytics, unused design-system exports

**Prototype:** v2.0.0
**Milestone:** M61
**Workstream:** 004
**Date:** May 04, 2026
**Status:** IN_PROGRESS
**Priority:** P3 — small bundle and reader-cognitive-load wins. Zero user-visible behavior change. The biggest piece is one local `Card` primitive that duplicates `@usezombie/design-system`'s `Card` (RULE NLG: pre-v2.0 we do not carry parallel implementations of the same shape) and two analytics components that have unit tests but never render in any production page.
**Categories:** UI
**Batch:** B1
**Branch:** feat/m61-ui-dead-sweep
**Depends on:** none — orthogonal to M61_001/002/003.

**Canonical architecture:** `docs/architecture/` §UI / Design System (M26_001b unified the design-system; this spec keeps the unification honest).

---

## Implementing agent — read these first

1. `docs/v2/done/M26_001b_P1_UI_DESIGN_SYSTEM_UNIFICATION.md` — established `@usezombie/design-system` as the single source for primitives. Local re-implementations of primitives are a unification regression.
2. `ui/packages/design-system/src/index.ts` — read the export list; the audit found ~58 exported names with zero importers in `app/` or `website/`. Many are `*Props`/`*Variant` type aliases (low-value to remove). The actionable subset is the named runtime components nobody uses.
3. `ui/packages/app/components/ui/card.tsx` — local Card duplicate. Read it. Then read `ui/packages/design-system/src/design-system/card.tsx` (or wherever the canonical Card lives). The local file's surface (`Card`, `CardHeader`, `CardTitle`, `CardDescription`, `CardContent`, `CardFooter`) overlaps the design-system Card; the only "extra" is a `featured?: boolean` prop and a custom orange-glow Tailwind class.
4. `ui/packages/app/components/analytics/{AnalyticsPageEvent,TrackedAnchor,AnalyticsBootstrap}.tsx` — `AnalyticsBootstrap` IS wired into `app/layout.tsx` and is live. The other two are imported only by `tests/app-components.test.ts` and `tests/app-pages.test.ts`. They render nothing in production.

---

## Applicable Rules

- `AGENTS.md` — RULE NLG, Verification Gate, UI Component Substitution Gate (per-edit), Length Gate.
- `docs/BUN_RULES.md` — TS file shape, anti-patterns, Bun-primitive discipline.
- `docs/greptile-learnings/RULES.md` — RULE ORP, RULE FLL.

---

## Overview

**Goal (testable):** After this workstream, `rg --files ui/packages/app/components/ui/` returns no `card.tsx`, `rg -nw 'AnalyticsPageEvent|TrackedAnchor' ui/packages/app/` returns zero hits, the listed unused design-system exports are removed from `ui/packages/design-system/src/index.ts` (or deleted at the source-file level if the export was the file's only consumer), `bun run lint` and `bun run typecheck` are clean across `app/`, `website/`, and `design-system/`, `bun test` passes, and the playwright smoke (`/_design-system` route) still renders.

**Problem:** May 04, 2026 forensic UI audit walked every `*.tsx`/`*.ts` file under `ui/packages/{app,website,design-system}` and matched against `from "..."` importers. Three findings:

1. **`ui/packages/app/components/ui/card.tsx`** (54 LOC) — duplicate of `@usezombie/design-system`'s `Card`. Zero importers anywhere in `app/`. The "extra" `featured?: boolean` prop and orange-glow class haven't shipped to any caller. M26_001b's unification said local primitive duplicates go; this one slipped through.

2. **`ui/packages/app/components/analytics/AnalyticsPageEvent.tsx`** (~17 LOC) and **`TrackedAnchor.tsx`** (~21 LOC) — both have unit tests in `tests/app-components.test.ts` and `vi.mock` registrations in `tests/app-pages.test.ts`, but **zero production page imports**. They were built for analytics instrumentation that never landed in any rendered page. The live analytics surface is `AnalyticsBootstrap.tsx` (used in `app/layout.tsx`) plus the underlying `lib/analytics/posthog.ts` `trackAppEvent` — both of which stay.

3. **Unused design-system exports** — `ui/packages/design-system/src/index.ts` re-exports ~134 named bindings; the audit found ~58 with zero `app/` or `website/` callers. The high-value subset (actual runtime components, not `*Props`/`*Variant` type-only exports) is:
   - `DialogPortal`, `DialogClose`, `DialogOverlay` — Radix primitives re-exported but the `Dialog` consumers (`DesignSystemGallery`, occasional app dialogs) only use `Dialog`/`DialogContent`/`DialogHeader`/`DialogFooter`/`DialogTitle`/`DialogDescription`/`DialogTrigger`.
   - `SelectGroup`, `SelectLabel`, `SelectSeparator` — `Select` consumers (`ByokFields`, `WorkspaceSwitcher`) use `Select`/`SelectContent`/`SelectItem`/`SelectTrigger`/`SelectValue` only.
   - `DropdownMenuGroup`, `DropdownMenuPortal`, `DropdownMenuRadioGroup`, `DropdownMenuSub` — `DropdownMenu` consumers (`WorkspaceSwitcher`, gallery) don't use these sub-primitives.
   The `*Props`/`*Variant` type-alias exports (~50 of them) are a separate, lower-value category — they cost nothing at runtime and removing them means readers have to recompute the public type surface. **Out of scope** for this spec; if they bother someone later, file separately.

**Solution summary:** Delete `components/ui/card.tsx`, delete the two unwired analytics components and their dedicated test cases (keep `AnalyticsBootstrap` and the `posthog.ts` lib), prune the named runtime exports from the design-system `index.ts` (deleting the re-export only — the underlying Radix imports inside the design-system primitive files stay because the *internal* implementation might use them; verify per-export). Run lint, typecheck, vitest, and playwright smoke.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/components/ui/card.tsx` | DELETE | 54 LOC; zero importers; duplicate of design-system `Card`. |
| `ui/packages/app/components/ui/` (directory) | DELETE if empty after card.tsx leaves | Audit during EXECUTE; `ls` first. |
| `ui/packages/app/components/analytics/AnalyticsPageEvent.tsx` | DELETE | ~17 LOC; only test importers; never wired into a page. |
| `ui/packages/app/components/analytics/TrackedAnchor.tsx` | DELETE | ~21 LOC; only test importers. |
| `ui/packages/app/tests/app-components.test.ts` | EDIT | Drop the `AnalyticsPageEvent` and `TrackedAnchor` test cases (~lines 95-105 per audit). |
| `ui/packages/app/tests/app-pages.test.ts` | EDIT | Drop the `vi.mock("@/components/analytics/AnalyticsPageEvent", ...)` and `vi.mock("@/components/analytics/TrackedAnchor", ...)` registrations (~lines 38-44). |
| `ui/packages/design-system/src/index.ts` | EDIT | Remove the named runtime re-exports listed in §3: `DialogPortal`, `DialogClose`, `DialogOverlay`, `SelectGroup`, `SelectLabel`, `SelectSeparator`, `DropdownMenuGroup`, `DropdownMenuPortal`, `DropdownMenuRadioGroup`, `DropdownMenuSub`. Verify each is not consumed in `ui/packages/app/`, `ui/packages/website/`, OR inside the design-system itself (some Radix primitives need `Portal`/`Overlay` at the implementation layer; if a primitive file imports it locally, the export stays as an internal — but it's the *re-export from `index.ts`* that goes). |

No backend changes. No CLI changes.

---

## Sections (implementation slices)

### §1 — Confirm no last-minute consumers

Re-run the audit grep for each candidate before editing:

```bash
rg -nw 'card.tsx' ui/packages/app/ -g '!node_modules' -g '!.next'
rg -nw 'AnalyticsPageEvent|TrackedAnchor' ui/packages/app/ -g '!node_modules' -g '!.next' -g '!*.test.ts'
for n in DialogPortal DialogClose DialogOverlay SelectGroup SelectLabel SelectSeparator \
         DropdownMenuGroup DropdownMenuPortal DropdownMenuRadioGroup DropdownMenuSub; do
  echo "=== $n ==="
  rg -nw "$n" ui/packages/app ui/packages/website -g '!node_modules' -g '!.next' -g '!dist'
done
```

If any candidate has a hit outside the deletion site, surface in `Discovery` and revise §Files Changed.

### §2 — Delete `components/ui/card.tsx`

`git rm ui/packages/app/components/ui/card.tsx`. If `components/ui/` becomes empty, remove the directory. UI Component Substitution Gate (per AGENTS.md) is a per-edit gate — the gate says "use the design-system primitive when one exists"; this deletion is the cleanup, not a regression.

### §3 — Delete the two unwired analytics components

`git rm` `AnalyticsPageEvent.tsx` and `TrackedAnchor.tsx`. Edit `tests/app-components.test.ts` to drop the two test cases. Edit `tests/app-pages.test.ts` to drop the `vi.mock` registrations. Confirm `AnalyticsBootstrap.tsx` is untouched (it stays — it's wired in `app/layout.tsx`).

### §4 — Prune unused DS index.ts exports

For each of the 10 names in §Files Changed, edit `ui/packages/design-system/src/index.ts` to remove the export from the named-export list. **Do not** delete the underlying Radix-primitive import inside the design-system source unless a separate per-file audit confirms zero internal consumers — that's a deeper cleanup with its own decision points. Implementation default: keep the source file's local imports; remove only the `index.ts` re-export.

### §5 — Verify

`bun run lint` (must pass), `bun run typecheck` (must pass), `bun test` across all three packages, `bun run --cwd ui/packages/website test:e2e` for the `/_design-system` smoke (gallery page renders without missing-export errors). If any test or lint fails, fix or revert; CHORE(close) blocks on failure.

---

## Interfaces

`@usezombie/design-system` index surface shrinks by 10 named exports (and any aligned `*Props` types if explicitly listed there — verify; the audit suggested `*Props` exports are mostly already untouched by this set). External consumer impact: none — the marketplace site (`website`) and dashboard (`app`) are the only consumers of `@usezombie/design-system` (workspace dependency); `bun run lint` proves zero broken imports. No package version bump needed because no published package consumes this.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| typecheck fails after pruning DS export | An internal design-system file re-imports the pruned name from `./index` (rare, but possible if there's a circular re-export) | Switch the internal file to the source-of-truth import; commit in same change. |
| Playwright `/_design-system` smoke fails | The gallery page renders one of the pruned exports as a demo | Read `ui/packages/website/src/pages/DesignSystemGallery.tsx`; the audit says it uses `Dialog`/`DialogContent`/`DialogHeader`/`DialogFooter`/`DialogTitle`/`DialogDescription`/`DialogTrigger` plus dropdown sub-pieces — re-verify before deletion. If the gallery uses `DialogPortal` etc. for showcase purposes, keep that export. |
| `card.tsx` removal breaks an undiscovered import | Lazy import string the audit's `from "..."` regex missed | `bun run typecheck` fails; restore via `git restore`. |
| Test loss without behavior loss | The deleted analytics-component tests were the only assertion that those files compiled | Acceptable — we deleted the file the test was guarding. |

---

## Invariants

1. **Zero importers of `components/ui/card.tsx`, `AnalyticsPageEvent.tsx`, `TrackedAnchor.tsx`** — enforced by `bun run typecheck` after deletion (any importer turns into a type error).
2. **`bun run typecheck` clean across all three packages** — CHORE(close) blocks otherwise.
3. **Playwright `/_design-system` smoke passes** — gallery page renders.
4. **No net runtime behavior change in the live dashboard** — manual smoke before COMMIT: `bun run --cwd ui/packages/app dev` and click through Sign-in → Dashboard → Zombies → Approvals → Settings → Billing.

---

## Test Specification

| Test | Asserts | Where |
|------|---------|-------|
| Existing `app-components.test.ts` | Live components (`AnalyticsBootstrap`, etc.) still pass | After dropping the two deleted-component cases. |
| Existing `design-system-smoke.spec.ts` (playwright) | Gallery renders without import errors | `ui/packages/website/tests/e2e/design-system-smoke.spec.ts`. |
| Existing dashboard playwright tests (if any) | Live pages render | `ui/packages/app/playwright-results` configs. |
| `bun run typecheck` | Zero TS errors anywhere | CI. |

No new tests.

---

## Eval Commands

```bash
# E1 — orphan-component sweep (must be empty)
rg -nw 'AnalyticsPageEvent|TrackedAnchor' ui/packages/app -g '!node_modules' -g '!.next' -g '!*.test.ts' -g '!playwright-results'
rg -l 'components/ui/card' ui/packages/app -g '!node_modules' -g '!.next'

# E2 — DS export-pruning verification (must be empty)
for n in DialogPortal DialogClose DialogOverlay SelectGroup SelectLabel SelectSeparator \
         DropdownMenuGroup DropdownMenuPortal DropdownMenuRadioGroup DropdownMenuSub; do
  rg -nw "$n" ui/packages/app ui/packages/website -g '!node_modules' -g '!.next' -g '!dist'
done

# E3 — typecheck + lint + test + playwright
bun run lint && bun run typecheck && bun test
bun run --cwd ui/packages/website test:e2e -- --grep design-system-smoke
```

---

## Discovery (filled during EXECUTE)

Empty `components/ui/` after card.tsx removal: ____
Internal DS file references to pruned re-exports: ____
Gallery page renders correctly post-prune: ____

---

## Out of Scope

- The ~50 unused `*Props`/`*Variant` type-alias exports in `ui/packages/design-system/src/index.ts`. Low value, separate spec if needed.
- Rewriting the `featured?` prop from the deleted `card.tsx` into the design-system `Card`. If a future page needs orange-glow card emphasis, lift it then with a real consumer.
- Touching the live `AnalyticsBootstrap.tsx` or `lib/analytics/posthog.ts` — those are the live analytics surface.
- The `DesignSystemGallery.tsx` — internal smoke route at `/_design-system`; it stays.
- Backend, schema, or zombiectl changes — owned by M61_001/002/003.
- Removing kept files' unused imports (RULE NLR is touch-it-fix-it for the *files we edit*; the design-system index.ts qualifies, but other untouched files don't).
