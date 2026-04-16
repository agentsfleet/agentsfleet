# M26_001: Design System Unification — one package, both apps

**Prototype:** v1.0.0
**Milestone:** M26
**Workstream:** 001
**Date:** Apr 16, 2026
**Status:** PENDING
**Priority:** P1 — Unblocks M27 (dashboard pages) and eliminates Button drift between `ui/packages/app` and `ui/packages/website`
**Batch:** B5 — lands between M12 (narrowed foundation) and M27 (dashboard pages)
**Branch:** feat/m26-design-system-unification (not yet created)
**Depends on:** M12_001 (token pyramid, shared primitives, `components.json` + `tw-animate-css` installed in app)
**Blocks:** M27_001 (dashboard pages need the unified Button; see acceptance step 5)

---

## Overview

**Goal (testable):** `@usezombie/design-system` becomes a **self-contained, framework-agnostic React component package** consumable by both `ui/packages/app` (Next.js 16 App Router with Server Components) and `ui/packages/website` (Vite SPA with `react-router-dom`). One Button, one Card, one Dialog — no drift, no duplication, no framework-specific imports in the package.

**Problem (as surfaced during M12_001 EXECUTE):**

1. **Design-system is not self-contained.** `@usezombie/design-system/Button.tsx` emits `.z-btn` class names, but the actual CSS rules live in `ui/packages/website/src/styles.css` — the *consumer*. Dropping this Button into the Next.js app renders unstyled.
2. **Design-system is not RSC-safe.** The package imports `react-router-dom`'s `<Link>`, which breaks in Server Components. Next.js build falls back to client-rendering every page that transitively imports it.
3. **App has its own parallel Button.** `ui/packages/app/components/ui/button.tsx` is a shadcn-style clone written during M12_001 because the package's Button was unusable (per points 1 + 2). Two Button implementations will drift.
4. **Two styling paradigms coexist.** Package uses BEM (`.z-btn--ghost`); app uses utility-first (`bg-primary hover:bg-primary-bright`). Both resolve to the same Layer 0 tokens, but components cannot be swapped between them.

**Architectural decision — paradigm:**

M26 commits the whole monorepo to the **shadcn paradigm**: Tailwind v4 utilities + CSS variables (via `@theme inline`) + Radix primitives underneath + `cva` variants + `cn` helper + `asChild` composition via `@radix-ui/react-slot`. Today `ui/packages/app` already uses this (M12 added `components.json` to unlock the shadcn CLI). `ui/packages/website` and `@usezombie/design-system` migrate to it in M26. Rationale: matches the shadcn/ui v4 reference app, assistant-ui, openhive, and Gemini's 2026 guidance; minimizes runtime dependencies; keeps Server-Component safety; maximizes composability via `asChild`. The legacy BEM `.z-btn--ghost` class names survive as a secondary output of a `buttonClassName(variant)` helper so non-Tailwind callers (if any ever appear) can still style against them — but every first-party consumer uses utility classes.

**Solution summary:**

One package, five ordered workstreams, one visual-regression gate across both consumers:

- **M26.1** — Move component CSS out of `website/styles.css` into `@usezombie/design-system/src/components.css`. Exported via the package. Makes the package self-contained.
- **M26.2** — Rewrite `design-system/Button.tsx` as RSC-safe, framework-agnostic: `@radix-ui/react-slot` + `asChild`, zero framework-specific imports, three-layer semantic tokens via Tailwind v4 utilities (or an exposed `cva` class function for callers that aren't Tailwind-based).
- **M26.3** — Rewrite Card / Terminal / Grid / Section / InstallBlock / AnimatedIcon / ZombieHandIcon to the same framework-agnostic pattern.
- **M26.4** — Migrate `ui/packages/website` to consume the rewritten package (pass `<Link from="react-router-dom" />` as `asChild` child). Playwright visual-regression captured before/after must match.
- **M26.5** — Migrate `ui/packages/app`: delete `components/ui/button.tsx`, re-export from `@usezombie/design-system`. Callers pass Next.js `<Link>` as `asChild` child. Playwright visual-regression captured before/after must match.

Post-M26 both apps import Button from the same file, and you can delete one Button without touching the other (because there *is* only one).

---

## 1.0 CSS Migration — package becomes self-contained

**Status:** PENDING
**Workstream:** M26.1

**Current state audit:**

- `.z-btn`, `.z-btn--ghost`, `.z-btn--double`, `.z-card`, `.z-card--featured` (and any other `z-*` component classes) are defined in `ui/packages/website/src/styles.css`.
- `ui/packages/design-system/src/tokens.css` contains palette / typography / spacing / radii / shadows / transitions / layout / z-index primitives + keyframes. No component classes.

**Target state:**

- New file `ui/packages/design-system/src/components.css` owns every `.z-*` component class.
- Package exports: add `./components.css` to `package.json#exports`.
- `ui/packages/website/src/styles.css` shrinks to website-specific chrome + deletes every `.z-btn*` / `.z-card*` rule it moved.
- Consumers import **both** `tokens.css` and `components.css` (or a single `index.css` that re-imports both).

**Dimensions:**

- 1.1 PENDING — target: `ui/packages/design-system/src/components.css` — input: copy every `.z-*` component rule from `website/styles.css` — expected: file exists, classes verbatim, references `var(--z-*)` primitives only (never raw hex) — test_type: unit (grep assertion)
- 1.2 PENDING — target: `ui/packages/design-system/package.json` — input: add `"./components.css": "./src/components.css"` under `exports` — expected: `bun ui/packages/design-system exports` resolves the path — test_type: unit (package resolver)
- 1.3 PENDING — target: `ui/packages/website/src/styles.css` — input: remove every `.z-btn*` / `.z-card*` rule now owned by the package — expected: file has zero `.z-btn` / `.z-card` rules — test_type: unit (grep assertion)
- 1.4 PENDING — target: `ui/packages/website/src/index.css` (or equivalent entry) — input: add `@import "@usezombie/design-system/components.css"` after the tokens import — expected: visual regression matches pre-migration screenshots — test_type: e2e (Playwright visual diff)

---

## 2.0 Button Rewrite — framework-agnostic + RSC-safe

**Status:** PENDING
**Workstream:** M26.2

**The framework-agnostic contract** (enforced by exports + TypeScript):

- ❌ No import from `react-router-dom`, `next/link`, `next/navigation`, `react-router`, `wouter`, or any other router package
- ❌ No import from `next/*` (App Router-specific), `react-server`, `next/headers`
- ❌ No `"use client"` directive at the top of files that should be usable in Server Components. (A component marked `"use client"` is RSC-safe but forces client-side rendering for the subtree — only use when a hook requires it, e.g. `useState` in an interactive composition.)
- ✅ Import `@radix-ui/react-slot` for `asChild` — routing-agnostic composition
- ✅ Import `class-variance-authority` for variant types
- ✅ Import `@usezombie/design-system/utils` `cn` helper (internal)
- ✅ Use only Layer-2 semantic Tailwind utilities (`bg-primary`, `text-destructive`, `from-primary to-primary-bright`, etc.) — backed by the app's / website's `globals.css` `@theme inline` block. Packages that aren't Tailwind-based get a `buttonClassName(variant)` function that returns the equivalent class string (for callers willing to apply their own CSS).

**The `asChild` escape hatch** is the single source of routing integration:

```tsx
// Next.js App Router (ui/packages/app)
import Link from "next/link";
import { Button } from "@usezombie/design-system";

<Button asChild variant="primary">
  <Link href="/dashboard">Go to dashboard</Link>
</Button>;

// Vite SPA (ui/packages/website)
import { Link } from "react-router-dom";
import { Button } from "@usezombie/design-system";

<Button asChild variant="primary">
  <Link to="/pricing">See pricing</Link>
</Button>;

// Non-React-Router HTML anchor
<Button asChild variant="ghost">
  <a href="https://docs.usezombie.com" target="_blank" rel="noopener noreferrer">
    Docs
  </a>
</Button>;
```

Radix `Slot` clones the single child element and merges props. The Button cares only that `children` is a single React element that renders a navigable element — it doesn't need to know which router produced it.

**Visual parity contract** — post-migration, every Button in both apps must render pixel-identically (within a 1-pixel AA-antialiasing tolerance) to its pre-migration screenshot. Any regression is a blocker.

**Dimensions:**

- 2.1 PENDING — target: `ui/packages/design-system/src/design-system/Button.tsx` — input: rewrite using `cva` + `Slot` + semantic Tailwind utilities — expected: zero `react-router-dom` import, zero `next/*` import, all existing variants render (`primary`, `ghost`, `double-border`) — test_type: unit (SSR snapshot) + grep (forbidden-import check)
- 2.2 PENDING — target: `ui/packages/design-system/src/design-system/Button.tsx` — input: `<Button asChild><Link to="/x">X</Link></Button>` (react-router) AND `<Button asChild><Link href="/x">X</Link></Button>` (Next.js) — expected: both SSR to a single `<a>` with Button classes merged onto it, no extra wrapper DOM — test_type: unit (SSR snapshot)
- 2.3 PENDING — target: `ui/packages/design-system/src/design-system/Button.tsx` — input: `<Button>Click</Button>` (no asChild) — expected: renders as `<button type="button">` with correct classes — test_type: unit
- 2.4 PENDING — target: `ui/packages/design-system/src/classes.ts` — input: export `buttonClassName(variant)` returning the class string for non-Tailwind callers — expected: function round-trips through `cn(...)` for all variants — test_type: unit
- 2.5 PENDING — RSC-safety — target: create a minimal Next.js RSC fixture under `ui/packages/app/app/(dev)/ds-button-rsc/page.tsx` that renders `<Button variant="primary">Hello</Button>` as a Server Component — expected: `bun run build` produces a static route (no `"use client"` hoisting) — test_type: build gate

---

## 3.0 Card / Terminal / Grid / Section / InstallBlock / AnimatedIcon / ZombieHandIcon Rewrite

**Status:** PENDING
**Workstream:** M26.3

**Scope rule (important):** `@usezombie/design-system` owns **generic UI primitives only** — Button, Card, Dialog, Input, Badge, Alert, Tooltip, Skeleton, Dropdown, Select, Tabs, Sheet, Toast and friends. Both `ui/packages/app` and `ui/packages/website` import these from the shared package — zero forking, zero drift.

**Domain compositions stay in the consuming package.** `ui/packages/app/components/domain/ActivityFeed.tsx` is zombie-specific and stays in the app. A website's `PricingHero` or `InstallBlock` is website-specific and stays in the website (or in the design-system ONLY if a second consumer also needs it). The rule for promotion: **a domain component moves to the shared package when a second consumer actually needs it**, not speculatively. Matches the assistant-ui reference layout (`packages/ui/src/components/ui/` shared, `packages/ui/src/components/assistant-ui/` domain) and shadcn's own `apps/v4` structure.

Today's `@usezombie/design-system` already carries Terminal / Grid / Section / InstallBlock / AnimatedIcon / ZombieHandIcon because the website is the only consumer. We keep them in the package (avoids breaking the website's imports) and give them the same framework-agnostic treatment. If any turn out to be website-only with no plausible second consumer, they can migrate to `ui/packages/website/src/components/domain/` as a follow-up — but that's explicitly out of M26 scope to avoid churn.

Each component gets the same treatment as Button: framework-agnostic imports only, semantic Tailwind utilities or exposed class-string helpers, `asChild` where a semantic wrapper makes sense (Card, Section), no `asChild` where the component owns structural markup (Terminal, Grid, AnimatedIcon — same rationale as M12's StatusCard).

**Dimensions — one per component, same shape:**

- 3.1 PENDING — Card rewrite + `asChild` support + SSR snapshot test
- 3.2 PENDING — Terminal rewrite (preserves keyframes in tokens.css) + SSR snapshot test
- 3.3 PENDING — Grid rewrite + SSR snapshot test
- 3.4 PENDING — Section rewrite + `asChild` support (lets pages wrap as `<main>` / `<article>` / etc.) + SSR snapshot test
- 3.5 PENDING — InstallBlock rewrite + SSR snapshot test
- 3.6 PENDING — AnimatedIcon rewrite (preserves `.z-animated-icon__glyph` keyframes) + SSR snapshot test
- 3.7 PENDING — ZombieHandIcon rewrite + SSR snapshot test

---

## 4.0 Website Migration

**Status:** PENDING
**Workstream:** M26.4

`ui/packages/website` currently imports design-system components AND relies on the website's own `styles.css` for the component CSS. After M26.1, the CSS moves to the package. After M26.2–3, the components change API slightly (e.g. `<Button to="/x">` → `<Button asChild><Link to="/x" /></Button>`).

**Dimensions:**

- 4.1 PENDING — target: `ui/packages/website/src/App.tsx` + every page file — input: migrate Button call-sites from `<Button to="/x">X</Button>` to `<Button asChild><Link to="/x">X</Link></Button>` — expected: all pages render, Playwright smoke passes — test_type: e2e
- 4.2 PENDING — target: website entry CSS — input: add `@import "@usezombie/design-system/components.css"` after tokens import — expected: `.z-btn` rules resolve from the package — test_type: unit (grep + cascade check)
- 4.3 PENDING — target: `ui/packages/website/src/tests/design-system/Button.test.tsx` + siblings — input: update test fixtures for the new `asChild` API — expected: all existing tests pass against the new components — test_type: unit
- 4.4 PENDING — Playwright visual regression — input: capture screenshots of `/`, `/pricing`, `/docs`, `/installation` on main before migration; re-capture after migration on the feat branch — expected: pixel diff under 1% per page (AA tolerance) — test_type: e2e

---

## 5.0 App Migration

**Status:** PENDING
**Workstream:** M26.5

`ui/packages/app` currently has its own `components/ui/button.tsx` — a shadcn-style Button written during M12_001 as a workaround for the package's unusability. After M26.2–3, the package's Button is RSC-safe + framework-agnostic, so the app's copy becomes redundant.

**Dimensions:**

- 5.1 PENDING — target: `ui/packages/app/components/ui/button.tsx` — input: delete the file, add a re-export `export { Button, buttonVariants } from "@usezombie/design-system";` (or update all call-sites to import directly from the package) — expected: build + typecheck green, zero call-site breakage — test_type: unit + build
- 5.2 PENDING — target: app entry CSS (`globals.css`) — input: add `@import "@usezombie/design-system/components.css"` after tokens import — expected: any `.z-btn` class (if any leaks into the app via design-system components) resolves — test_type: unit
- 5.3 PENDING — target: every Button call-site in `ui/packages/app/app/**` — input: convert `<Button onClick>` → unchanged, `<Button asChild><Link href="/x">...</Link></Button>` where a link is desired — expected: zero `<Link><Button>...</Button></Link>` nested pattern (invalid HTML) — test_type: grep assertion
- 5.4 PENDING — RSC gate — target: the existing (dashboard) route tree — input: `bun run build` produces the same static / dynamic route split as before the migration — expected: no regression in Server Component coverage (same or more routes statically rendered) — test_type: build diff
- 5.5 PENDING — Playwright visual regression — input: capture screenshots of `/workspaces` (existing v1 page) before migration; re-capture after — expected: pixel diff under 1% — test_type: e2e

---

## 6.0 Interfaces

**Status:** PENDING

### 6.1 Public package exports (post-M26)

```ts
// @usezombie/design-system
export { cn } from "./utils";
export { uiButtonClass, uiCardClass, buttonClassName, type UiButtonVariant } from "./classes";
export {
  Button,
  Card,
  Terminal,
  Grid,
  Section,
  InstallBlock,
  AnimatedIcon,
  ZombieHandIcon,
} from "./design-system";

// CSS exports
// @usezombie/design-system/tokens.css     — Layer 0 palette + typography primitives
// @usezombie/design-system/components.css — component CSS (new)
```

### 6.2 Public API change — Button

Before (M26.1 and earlier):

```tsx
<Button to="/x" variant="primary">Go</Button>
<Button to="https://docs.usezombie.com" external variant="ghost">Docs</Button>
<Button onClick={fn}>Click</Button>
```

After (M26.2 and later):

```tsx
<Button asChild variant="primary">
  <Link to="/x">Go</Link>
</Button>
<Button asChild variant="ghost">
  <a href="https://docs.usezombie.com" target="_blank" rel="noopener noreferrer">Docs</a>
</Button>
<Button onClick={fn}>Click</Button>   // unchanged
```

The breaking change affects every `<Button to={...}>` call in the website. Migration is mechanical (codemod candidate). Documented in release notes.

### 6.3 Forbidden imports (CI-enforced)

A new grep-based gate in `make lint` asserts that `ui/packages/design-system/src/**` contains zero imports from:

- `react-router-dom`, `react-router`
- `next/link`, `next/navigation`, `next/headers`, `next/server`
- `wouter`, `@tanstack/react-router`

The package is framework-agnostic by construction.

---

## 7.0 Implementation Constraints (enforceable)

**Status:** PENDING

| Constraint | Verify |
|---|---|
| Package contains no router imports | grep gate in `make lint` |
| Every component under `design-system/src/design-system/` has an SSR snapshot test | `find` + test coverage check |
| Zero `"use client"` in `design-system/src/**` unless a specific component needs a hook | grep gate |
| Components.css contains only token-referenced values (no raw hex, no inline color) | grep assertion |
| Both website and app pass Playwright visual regression within 1% pixel diff | CI gate |
| Build delta: app + website first-load bundle shrinks or stays flat vs pre-migration | `next build` + `vite build` output comparison |
| `.z-btn` / `.z-card` rules no longer in `website/styles.css` | grep assertion |
| All Button variants round-trip in both consumers | unit + e2e tests |

---

## 8.0 Execution Plan (Ordered)

| Step | Action | Verify |
|---|---|---|
| 1 | CHORE(open) — create worktree `../usezombie-m26-design-system`, move spec pending→active | `pwd` inside worktree, spec in active/ |
| 2 | M26.1 CSS migration — copy `.z-*` rules from website/styles.css to design-system/components.css, update package.json exports | dims 1.1–1.4 pass |
| 3 | M26.2 Button rewrite — cva + Slot + semantic tokens + `buttonClassName()` helper | dims 2.1–2.5 pass |
| 4 | M26.3 other components — Card, Terminal, Grid, Section, InstallBlock, AnimatedIcon, ZombieHandIcon | dims 3.1–3.7 pass |
| 5 | Website Playwright baseline — capture main's visual state before migration | screenshots checked in |
| 6 | M26.4 website migration — update call-sites, re-import CSS | dims 4.1–4.4 pass |
| 7 | M26.5 app migration — delete app/components/ui/button.tsx, re-export, update call-sites | dims 5.1–5.5 pass |
| 8 | Full verification — `make lint`, `bun run typecheck / lint / test / build` in both app and website, `bunx playwright test` in both | all green |
| 9 | CHORE(close) — spec DONE, move to done/, Ripley log, changelog entry, PR | PR open with visual-diff screenshots attached |

---

## 9.0 Acceptance Criteria

- [ ] `@usezombie/design-system` package is self-contained: importing only from `@usezombie/design-system` (JS + CSS) renders components correctly with no consumer-side CSS
- [ ] Zero `react-router-dom` / `next/*` imports anywhere under `ui/packages/design-system/src/**`
- [ ] `ui/packages/app/components/ui/button.tsx` is deleted; all call-sites import from `@usezombie/design-system`
- [ ] `ui/packages/website` call-sites use `<Button asChild><Link>...</Link></Button>` pattern
- [ ] Playwright visual regression ≤ 1% pixel diff on both app and website for every page that uses Button / Card / Section / etc.
- [ ] `make lint` grep gate enforces forbidden imports (see §6.3)
- [ ] Both apps build successfully, with bundle size flat or smaller
- [ ] All existing tests (website + app) pass against the new component API
- [ ] RSC fixture route in the app builds as a static Server Component (proves RSC-safety)
- [ ] Release notes document the `<Button to="/x">` → `<Button asChild><Link to="/x" /></Button>` migration

---

## 10.0 Out of Scope

- Introducing Panda CSS / Vanilla Extract / styled-components. M26 stays on Tailwind v4 + CSS variables.
- Adding new design-system components. M26 is a migration, not a feature milestone — new primitives go in M27 or later.
- Migrating shadcn-scope primitives I added in M12 (StatusCard, EmptyState, Pagination, DataTable, ConfirmDialog, ActivityFeed) into the design-system package. Those are app-local compositions; promoting them is a judgement call for M27+ when a second consumer needs them.
- Playwright visual-regression infra itself. M26 uses the existing Playwright setup + manual before/after screenshot comparison. A proper visual-diff CI gate is a separate DX milestone.

---

## 11.0 Risk Log

| Risk | Mitigation |
|---|---|
| Website visual regression on a page we didn't screenshot | Capture screenshots of *every* top-level route before starting, not just the landing page |
| `Slot`'s single-child constraint breaks a consumer pattern | Fall back to non-asChild Button in that one call-site; document the exception |
| Tailwind v4 `@source` needs to scan the package's new components.css | Verify `@source` directives in both app and website entry CSS cover the design-system package paths |
| Next.js RSC build silently client-hoists the whole tree due to a hidden import | The M26.2 dimension 2.5 fixture catches this — explicitly asserts a Server Component builds |
| Breaking change (`to=` → `asChild`) missed in a call-site | grep-gate for `<Button to=` — zero matches post-migration |

---

## Applicable Rules

Standard set. In addition: **RULE ORP (orphan sweep)** applies at CHORE(close) — delete every unused `.z-btn` reference, dead Button import, and legacy BEM class. **RULE FLL** on every touched file (350 lines) — Button, Card, etc. likely stay well under, but the new `components.css` might need splitting if it grows beyond.

---

## Eval Commands

```bash
# Forbidden-import gate
grep -rE "from ['\"](next/|react-router)" ui/packages/design-system/src/ && echo "FAIL: router import" || echo "OK"

# Zero .z-btn in website/styles.css post-migration
grep -E "^\.z-(btn|card)" ui/packages/website/src/styles.css && echo "FAIL: component CSS still in consumer" || echo "OK"

# RSC-safety (new app fixture route)
cd ui/packages/app && bun run build 2>&1 | grep "ds-button-rsc" | grep -E "○|●" && echo "OK: static RSC"

# Visual regression (captured manually pre-migration, compared post)
cd ui/packages/website && bunx playwright test tests/e2e/visual-regression.spec.ts
cd ui/packages/app     && bunx playwright test tests/e2e/visual-regression.spec.ts
```

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Website Playwright visual diff | `bunx playwright test --project=website-visual` | | |
| App Playwright visual diff | `bunx playwright test --project=app-visual` | | |
| `make lint` forbidden-import gate | see Eval Commands | | |
| Website bundle size | `du -h ui/packages/website/dist` | | |
| App bundle size | `du -h ui/packages/app/.next` | | |
| RSC static fixture | see Eval Commands | | |
| All unit tests | `bun run test` in both packages | | |

---

## Dead Code Sweep

At CHORE(close): delete `ui/packages/app/components/ui/button.tsx`, any `.z-btn`/`.z-card` rules left in `ui/packages/website/src/styles.css`, unused imports.
