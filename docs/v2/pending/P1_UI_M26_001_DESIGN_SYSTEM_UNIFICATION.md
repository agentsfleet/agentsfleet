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

## Migration scope — at a glance

Both existing consumers migrate in M26. No one stays on the old package shape.

| Consumer | Framework | Migration workstream | What changes |
|---|---|---|---|
| `ui/packages/website` | Vite SPA + `react-router-dom` | **§4 Website Migration** (M26.4) | CSS moves from `website/styles.css` into the package; `<Button to="/x">X</Button>` becomes `<Button asChild><Link to="/x">X</Link></Button>`; all existing tests re-run against new API |
| `ui/packages/app` | Next.js 16 App Router + RSC | **§5 App Migration** (M26.5) | Delete `components/ui/button.tsx`; re-export / import from `@usezombie/design-system`; convert call-sites to `asChild + <Link>`; prove RSC-safety via fixture route |

Both packages also inherit the motion language (§5.5 / M26.6) and the load-performance budget (§5.6 / M26.7) the instant they upgrade. Zero follow-up PRs. Zero drift window.

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

## 5.5 Signature Motion + Atmospheric Dark Background

**Status:** PENDING
**Workstream:** M26.6 — runs after M26.5 (app migration) so motion is consistent across both consumers on day one.

A design-system without a signature motion language and a distinctive background feels generic. M26.6 gives UseZombie both — a *voice* for motion and an *atmosphere* for the canvas — without reaching for a heavyweight runtime (Framer Motion's ~50KB gzipped). Everything GPU-accelerated (`transform`, `opacity` only), everything gated behind `prefers-reduced-motion: reduce`, everything tokenized so future rebrands touch one file.

### 5.5.1 Motion tokens (Layer 1 extension of globals.css)

Add a motion token block alongside the color tokens. Every duration + easing curve referenced by a component must resolve to a CSS variable — no inline `200ms ease-out` in component files.

```css
:root {
  /* Duration — tuned for perceived snappiness on interactive paths. */
  --motion-duration-instant:  80ms;   /* hover feedback */
  --motion-duration-short:    160ms;  /* focus ring, small transitions */
  --motion-duration-medium:   280ms;  /* dialog enter/exit, toast */
  --motion-duration-long:     560ms;  /* page stagger, hero reveal */

  /* Easing — "zombie" feels the interaction curve. */
  --motion-ease-standard: cubic-bezier(0.22, 0.61, 0.36, 1);
  --motion-ease-emphasized: cubic-bezier(0.4, 0, 0.2, 1);
  --motion-ease-decelerate: cubic-bezier(0, 0, 0.2, 1);
  --motion-ease-spring: linear(0, 0.006 1.6%, 0.023 3.2%, 0.054 4.8%, 0.18 8.2%, 0.372 12%, 0.569 16%, 0.734 20%, 0.859 24%, 0.944 28%, 0.994 32%, 1 34%, 0.998 38%, 0.988 42%, 0.987 46%, 0.999 60%, 1);
}

@theme inline {
  --animate-glow-pulse: z-glow-pulse 2.8s var(--motion-ease-standard) infinite;
  --animate-shimmer: z-shimmer 1.6s linear infinite;
  --animate-scanline: z-scanline 6s var(--motion-ease-standard) infinite;
  --animate-drift-slow: z-drift 22s linear infinite;
  --animate-float-up: z-float-up 15s linear infinite;
  --animate-enter: z-enter var(--motion-duration-medium) var(--motion-ease-decelerate);
}
```

### 5.5.2 Signature animations — the brand motion vocabulary

Six named animations form the vocabulary. Every component picks from these, never inlines its own.

| Name | When to use | Keyframes | Fallback when reduced |
|---|---|---|---|
| `z-glow-pulse` | Active CTA idle state (primary Button), pending kill-switch confirmation dialog | Orange glow shadow cycles 0.08 → 0.22 opacity, 2.8s infinite | Static shadow at 0.1 opacity |
| `z-shimmer` | Skeleton loaders (replaces shadcn default `pulse`) — gradient traverses left-to-right | 200% background width, `background-position` 0% → 100%, 1.6s linear | Static muted bar at 0.12 opacity |
| `z-enter` | Dialog / Popover / Tooltip / Toast enter | `opacity 0 + translateY(4px)` → `opacity 1 + translateY(0)`, 280ms emphasized | No transform, `opacity 1` immediately |
| `z-scanline` | Hero / empty-state decorative overlays — horizontal line traverses element | Vertical `translateY` from -2% → 102%, 6s infinite | Hidden entirely |
| `z-drift` | Atmospheric aurora blob on dark background + AnimatedIcon idle wobble | Existing keyframe in tokens.css, `22s linear infinite` | Paused at neutral position |
| `z-float-up` | Background particles — tiny dots drift from bottom to top | Existing keyframe in tokens.css, `15s linear` stagger | Hidden |

### 5.5.3 Signature interaction — Button hover "trace"

When a user hovers or focuses the primary Button, a subtle orange radial trace animates outward from the cursor (on hover) or from the element center (on focus). Uses CSS `radial-gradient` + `@property --trace-x/--trace-y` registered custom properties, all GPU-accelerated, zero JS.

- Adds ~0.4KB of CSS to the package
- `@property` fallback: browsers without registered-property support get a flat glow (no animation), no breakage
- Respects `prefers-reduced-motion`

### 5.5.4 Atmospheric dark background

Replace the current flat `--z-bg-0: #05080d` with a layered composition. Each layer lives in `components.css` so both app and website inherit it automatically.

**Layer stack (bottom → top):**

1. **Base gradient** — `radial-gradient(ellipse at 50% 0%, var(--z-bg-1) 0%, var(--z-bg-0) 60%)`. Gives the viewport a subtle glow from above, like a CRT monitor or a spotlight on a desk.
2. **Grid overlay** — 1px lines at 2% opacity, 64×64 cell, slow horizontal drift via `z-grid-shift` (30s linear). Already have the keyframe. Masked with a radial vignette so lines fade at edges.
3. **Aurora glow** — two large radial blobs using `--primary-glow` (orange) and a faint `--info` (cyan), positioned at (top-left, bottom-right), `filter: blur(120px)`, `z-drift 22s`. Opacity capped at 0.14 so they're felt more than seen.
4. **Particle layer (optional)** — 12 tiny 2px dots, `position: absolute`, random horizontal positions, `z-float-up 15s` with staggered delays. Opacity 0.3, color `--primary-bright`. **Disabled** when `prefers-reduced-motion: reduce` OR `prefers-reduced-data: reduce` OR viewport < 640px (perf on mobile).
5. **Scanline (decorative, rare use)** — single horizontal line animated via `z-scanline` 6s. Only rendered in the app's `/dashboard` hero area (terminal/agent vibe). Not global.

Layers 1–3 run continuously on every page in both app and website. Layer 4 is opt-in via `<AtmosphericBackground particles />`. Layer 5 is page-specific.

**GPU + accessibility constraints (non-negotiable):**

- All animations use `transform` / `opacity` / `filter` only. No `top`/`left`/`width` transitions (layout thrash).
- `will-change: transform` applied to drifting layers — browser opts them onto the GPU composite layer.
- `@media (prefers-reduced-motion: reduce)` selector disables every `animation` property. Hardcoded in `components.css`.
- Layers use `pointer-events: none` and `aria-hidden="true"` so screen readers ignore them.
- Total background overhead: ≤ 3KB CSS, zero JS, zero layout thrash.

### 5.5.5 Dimensions — tests per animation + background layer

- 5.5.1 PENDING — target: `ui/packages/design-system/src/tokens.css` — input: add motion-duration + motion-ease tokens per §5.5.1 — expected: variables resolve in computed style, `@theme inline` emits `--animate-*` utilities — test_type: unit (grep + computed style)
- 5.5.2 PENDING — target: `ui/packages/design-system/src/components.css` — input: add 6 keyframes (`z-glow-pulse`, `z-shimmer`, `z-enter`, `z-scanline`, keep `z-drift`/`z-float-up`) — expected: keyframes defined, every component references them by name — test_type: unit (grep)
- 5.5.3 PENDING — target: `Button.tsx` primary variant — input: hover over Button — expected: `animation-name: z-glow-pulse` on computed style at idle, radial-gradient trace on hover — test_type: Playwright (hover state screenshot + getComputedStyle check)
- 5.5.4 PENDING — target: Skeleton component — input: render default — expected: `animation: var(--animate-shimmer)` in computed style, shimmer traverses left-to-right in screenshots at 0ms / 800ms / 1600ms — test_type: Playwright (multi-frame capture)
- 5.5.5 PENDING — target: `AtmosphericBackground` component (new, ships in design-system) — input: render on dashboard — expected: 3 layers visible in DOM (gradient + grid + aurora), pointer-events:none, aria-hidden, < 3KB CSS footprint — test_type: Playwright visual + unit (SSR snapshot)
- 5.5.6 PENDING — target: Dialog enter — input: open dialog — expected: `z-enter` animation runs once, duration 280ms, opacity ends at 1 — test_type: Playwright (animation start/end listener)
- 5.5.7 PENDING — target: `prefers-reduced-motion: reduce` — input: Playwright emulated media query — expected: all 6 signature animations have `animation: none` in computed style; particles layer hidden — test_type: Playwright
- 5.5.8 PENDING — target: GPU safety audit — input: every keyframe in `components.css` — expected: only `transform`, `opacity`, `filter`, `background-position` animated; grep gate catches `top`/`left`/`width`/`height` in any `@keyframes` block — test_type: unit (grep CI gate)
- 5.5.9 PENDING — target: SSR snapshot — input: every design-system component under `renderToStaticMarkup` — expected: no motion-specific runtime errors, no `undefined` in animation attributes — test_type: unit
- 5.5.10 PENDING — target: `AtmosphericBackground` responsive — input: Playwright at viewports 375px / 768px / 1440px — expected: particle layer hidden at 375, visible at ≥ 768, visual-regression delta < 1% per viewport — test_type: Playwright

### 5.5.6 New design-system export

```ts
// @usezombie/design-system
export {
  AtmosphericBackground,     // <AtmosphericBackground particles scanline={false} />
  type AtmosphericBackgroundProps,
} from "./design-system";
```

Both consumers wrap their root layout with it:

```tsx
// ui/packages/app/app/layout.tsx (RSC)
<body>
  <AtmosphericBackground particles />
  {children}
</body>

// ui/packages/website/src/App.tsx
<AtmosphericBackground particles scanline />
```

One component, two consumers, identical visual atmosphere.

---

## 5.6 Load Performance — snappier by construction

**Status:** PENDING
**Workstream:** M26.7 — applies during and after each per-consumer migration. Gate is enforced before each migration workstream's acceptance.

Motion only feels good when the page is already fast. M26.7 sets explicit load budgets, bakes the optimizations into the package + consumers' build pipelines, and makes regressions a CI-visible failure rather than a thing someone notices three weeks later.

### 5.6.1 Core Web Vitals targets (per consumer, 75th percentile)

Measured on throttled 4G, mid-tier mobile device profile (CPU 4x slowdown), Lighthouse CI + WebPageTest. These are the bars; a regression triggers a build-log warning and blocks merge.

| Metric | Target | Website landing | App `/workspaces` | Why this bar |
|---|---|---|---|---|
| **LCP** (Largest Contentful Paint) | < 2.0s | < 1.6s | < 2.0s | Google's "good" threshold; marketing depends on it |
| **FCP** (First Contentful Paint) | < 1.2s | < 1.0s | < 1.2s | User sees brand within a blink |
| **INP** (Interaction to Next Paint) | < 150ms | — | < 150ms | Replaces FID in 2024; kill-switch click feels instant |
| **CLS** (Cumulative Layout Shift) | < 0.05 | < 0.02 | < 0.05 | No jank — dashboard must never jump |
| **TBT** (Total Blocking Time) | < 200ms | < 150ms | < 200ms | Main-thread discipline |
| **Initial JS** (gzipped, first load) | — | ≤ 90 KB | ≤ 180 KB (RSC offloads most) | Package migration must shrink or hold bundles |

### 5.6.2 Package-level optimizations (shipped once, benefits both consumers)

These land in `@usezombie/design-system` and automatically improve every downstream consumer.

1. **`"sideEffects": false` in package.json** + per-file marker for CSS imports — unlocks tree-shaking. Importing `{ Button }` must not pull in `AtmosphericBackground` or its CSS.
2. **ESM-only + individual entry points** — `@usezombie/design-system/button`, `@usezombie/design-system/atmospheric-background` — so consumers can import surgically without the barrel file pulling everything.
3. **No React context providers at the root of the package.** Context forces a client boundary in RSC. If a component truly needs state, it carries its own `"use client"` and stays leaf-level.
4. **CSS is a separate artifact**, not inlined by JS. `components.css` ships as a raw CSS file (see §1.0). Consumers `@import` it in their entry stylesheet — zero CSS-in-JS runtime.
5. **Self-hosted, preloaded fonts.** Geist + Geist Mono ship as `.woff2` from the package (or from Vercel's CDN via `next/font` in the app). Replace the current `@import url("https://fonts.googleapis.com/...")` — that's a render-blocking third-party request on every page load. Use `font-display: swap` + `<link rel="preload">` for the above-the-fold weight.
6. **Lazy-load heavy optional components.** `Mermaid`/`SyntaxHighlighter`-class components (not currently shipped but anticipated) land behind `React.lazy()` or route-level code-splitting. The Button must never pull in a markdown parser.
7. **Critical CSS inlined for above-the-fold.** Website uses `@vercel/critical` or a Vite plugin to inline the CSS for the landing hero; the rest streams. App relies on Next.js's automatic critical-CSS extraction (already on by default in App Router).
8. **View Transitions API where supported.** Replaces JS-driven page transitions with a browser-native one-liner — zero JS cost, no layout thrash. Progressive enhancement: older browsers get the instant transition.

### 5.6.3 Consumer-level optimizations (per-app, during their migration)

**Website (M26.4):**

- Vite's `build.cssCodeSplit: true` + `build.reportCompressedSize: true` in config — verifiable CSS chunk output
- `vite-plugin-imagemin` on the `public/` assets if not already configured
- `<link rel="preconnect">` to `clerk.usezombie.com` (if used) and the API domain
- `<link rel="preload" as="image">` on the landing hero image

**App (M26.5):**

- Next.js 16 Turbopack is on by default; verify it for dev — production build stays on webpack unless confirmed stable
- `next/image` with explicit `width`/`height` on every image (prevents CLS)
- `next/font` for Geist/Geist Mono (auto-handles `font-display: swap` + preload)
- Route segment config: `export const dynamic = "force-static"` on the landing `/workspaces` page if it's truly cacheable; `revalidate: 60` on the dashboard
- Server Components default for data fetches — client components explicitly marked. Verifies in the build output's Route (app) table: `○` static + `ƒ` dynamic markers, minimize `ƒ`

### 5.6.4 Measurement + CI gate

| Gate | Tool | When it runs | Threshold |
|---|---|---|---|
| Lighthouse CI | `@lhci/cli` against a preview deployment | On every PR that touches `ui/packages/**` | LCP ≤ target, CLS ≤ target, TBT ≤ target — fail build if any regress by > 10% |
| Bundle size | `size-limit` or `bundlesize` in each package | On every PR | Initial JS gzipped must stay within the §5.6.1 budget per consumer |
| CSS size | Package build asserts `components.css` ≤ 12 KB gzipped | On every build of `@usezombie/design-system` | Hard fail if over |
| Runtime FPS during motion | Playwright with `page.evaluate(() => performance.now())` + frame timing | Motion suite | Every signature animation sustains 60 FPS on the test runner (≥ 55 FPS floor) |
| INP on dashboard interaction | Playwright + `web-vitals` library injected | M27 acceptance (dashboard pages ship) | Kill-switch click → status-updated within 150ms, measured |

### 5.6.5 Dimensions

- 5.6.1 PENDING — target: `@usezombie/design-system/package.json` — input: add `"sideEffects": false` + `"exports"` map for per-component entry points — expected: `import { Button }` tree-shakes to < 4 KB gzipped in isolation — test_type: bundle (`rollup --config rollup.analyze.js` or `size-limit`)
- 5.6.2 PENDING — target: `components.css` — input: final build of components.css at CHORE(close) — expected: gzipped size ≤ 12 KB — test_type: build (`gzip -9 | wc -c`)
- 5.6.3 PENDING — target: Geist font loading — input: migrate from `@import url(googleapis)` to self-hosted woff2 + preload — expected: zero third-party requests in network waterfall; FCP improvement measurable via Lighthouse diff — test_type: Playwright (request interception count) + Lighthouse CI
- 5.6.4 PENDING — target: Website landing — input: Lighthouse CI on preview deployment — expected: LCP < 1.6s, CLS < 0.02, TBT < 150ms at mobile profile — test_type: Lighthouse CI
- 5.6.5 PENDING — target: App `/workspaces` (existing v1 page) — input: Lighthouse CI on preview deployment — expected: LCP < 2.0s, INP < 150ms on any interaction, CLS < 0.05 — test_type: Lighthouse CI
- 5.6.6 PENDING — target: Bundle size (per consumer) — input: `bun run build` output — expected: website ≤ 90 KB gzipped first-load, app ≤ 180 KB gzipped first-load (baseline captured pre-M26, must not regress by > 5%) — test_type: size-limit CI gate
- 5.6.7 PENDING — target: Signature motion FPS — input: Playwright frame-timing trace during Button hover + Skeleton shimmer + Dialog enter — expected: ≥ 55 FPS sustained on each — test_type: Playwright performance trace
- 5.6.8 PENDING — target: View Transitions API — input: navigation between `/zombies` list and `/zombies/[id]` detail (when M27 ships those) — expected: transition uses `document.startViewTransition` on supporting browsers, falls back to instant on others — test_type: Playwright (feature-detect + visual)
- 5.6.9 PENDING — target: Third-party script discipline — input: `grep -r "googleapis.com\|googletagmanager\|cdn.jsdelivr" ui/packages/**/src` — expected: zero matches (all fonts + scripts self-hosted or package-shipped) — test_type: unit (grep CI gate)
- 5.6.10 PENDING — target: Initial-render layout stability — input: Playwright capturing CLS over first 2.5s of both consumers' landing pages — expected: CLS < 0.05 per route — test_type: Playwright (PerformanceObserver → layout-shift)

---

## 5.8 Website-Specific Interactive Components

**Status:** PENDING
**Workstream:** M26.8 — lands after M26.6 (motion vocabulary established) and M26.4 (website migration complete).
**Scope:** website-only. These are marketing / demo surface components for `ui/packages/website`. They live in `ui/packages/website/src/components/domain/`, NOT in the shared `@usezombie/design-system` package. The dashboard (`ui/packages/app`) does not consume them; it uses `<AtmosphericBackground />` (§5.5.4) instead.

### 5.8.1 Why these are website-domain, not shared primitives

- Landing pages need visual drama (beams, explosions) that would distract from operational tasks on the dashboard.
- The dashboard already has `<AtmosphericBackground />` (calm, continuous, low-distraction). Mixing paradigms on one surface is worse than two well-scoped surfaces.
- The demo terminal is landing-page copy — a way to show the CLI without a user having it installed. Not operational.
- YAGNI applies: promote to shared only if a second consumer actually needs these. Today, only the website does.

### 5.8.2 Motion library — scope exception with guardrails

The shared design-system bans heavy motion libraries (§5.6.2, rationale: `motion`/`framer-motion` is ~50 KB gzipped, too much for a package consumed by every page in both apps). The two components below genuinely need `motion`'s state machine + `AnimatePresence` for collision-triggered explosion lifecycle management and declarative variants. Implementing the same with raw CSS + `requestAnimationFrame` is doable but adds meaningful code complexity for no user-facing benefit.

**Exception:** `motion` (the package, formerly `framer-motion`) is allowed **in website-domain components only**, subject to:

| Guardrail | Threshold | Enforcement |
|---|---|---|
| Lazy-loaded via `React.lazy()` + `<Suspense>` | Motion chunk loads only when the /agents route mounts | Webpack/Vite chunk analysis — CI gate |
| Isolated to website | Zero `motion` imports under `ui/packages/app/**` or `ui/packages/design-system/**` | grep gate in `make lint` |
| Chunk budget | ≤ 40 KB gzipped for the motion+components split chunk | size-limit CI gate |
| Respects `prefers-reduced-motion` | All animations have static fallbacks | Playwright emulated media query |
| Does not affect landing-page LCP | Beams load *after* LCP, via dynamic import or below-the-fold Intersection trigger | Lighthouse CI (LCP target preserved) |
| Package.json import | `"motion": "^11"` pinned in `ui/packages/website/package.json` only | Dependency CI gate (see §7.0) |

### 5.8.3 `<BackgroundBeamsWithCollision />`

**Location:** `ui/packages/website/src/components/domain/background-beams-with-collision.tsx`

**What it does:** Vertical gradient beams fall from above the viewport. When any beam's bottom edge intersects a collision floor element, a particle explosion animates at the collision point (20 particles, random velocities, 0.5–2s opacity fade). After 2s the beam resets and re-fires with a repeat delay. Variable per-beam speed, position, height, and delay produce a layered, organic feel.

**Brand variant:** beams use `from-[var(--primary)] via-[var(--primary-bright)] to-transparent` (UseZombie orange). Explosions use `from-[var(--info)] to-[var(--primary)]` (cyan → orange gradient — faster than the "indigo/violet" reference). Container background: `bg-gradient-to-b from-[var(--background)] to-[var(--z-bg-1)]` (pure zombie dark, no neutral-100 fallback).

**Intended placement:** `/agents` route hero section on the website. Wraps an `<h2>` with messaging about zombie agents. Single instance per page — multiple stacked would tank perf.

**Architecture:**

- `BackgroundBeamsWithCollision` — orchestrator component. Fixed array of ~7 beam configs. Renders `<CollisionMechanism>` instances in a parent container with a `containerRef` pointing at the bottom floor element.
- `CollisionMechanism` — one per beam. Uses `motion.div` for the falling animation (`translateY` -200px → 1800px, `transition.repeat: Infinity`). A `useEffect` polling `getBoundingClientRect` at 50ms intervals checks whether the beam's bottom has crossed the floor's top. On collision, setState flips the explosion on, beam animation key increments to restart the animation after a 2s cooldown.
- `Explosion` — 20 `motion.span` particles with randomized `directionX` / `directionY`, `opacity: 1 → 0`, staggered durations 0.5–2s.

**Perf invariants (measurable):**

- `requestAnimationFrame`-based collision check instead of `setInterval(50ms)` — cleaner, browser-throttled when tab hidden. Replace the reference sample's `setInterval` with `rAF`.
- Beams use `will-change: transform, opacity`.
- `AnimatePresence` unmounts explosions after exit — no DOM growth over time.
- `pointer-events: none` on beams + floor so they never intercept user clicks.
- `aria-hidden="true"` + `role="presentation"` — screen readers ignore the decoration.
- Single IntersectionObserver pauses the animation when the section scrolls out of view (CPU idle when not visible).

**Dimensions:**

- 5.8.1 PENDING — target: `background-beams-with-collision.tsx` — input: default 7-beam config on `/agents` route — expected: beams render, each on its own vertical path, collision-on-impact particle explosions visible, reset every 2s — test_type: Playwright (visual at 0/3/6/9 seconds) + unit (SSR snapshot for beam array length)
- 5.8.2 PENDING — target: collision detection — input: Playwright `page.evaluate` checking for `data-collision-detected="true"` on at least one beam within 10s of mount — expected: collision fires for every beam at least once in 30s steady state — test_type: Playwright
- 5.8.3 PENDING — target: `prefers-reduced-motion: reduce` — input: Playwright emulated media query — expected: beams render statically (no `translateY` animation), explosions disabled, page still visually complete — test_type: Playwright
- 5.8.4 PENDING — target: CPU idle on hidden tab — input: `visibilitychange` → `hidden` simulated — expected: `motion` animations paused within 100ms, `performance.getEntriesByType("measure")` shows no animation frames after pause — test_type: Playwright
- 5.8.5 PENDING — target: accessibility tree — input: `axe-core` sweep on `/agents` — expected: zero violations; beams contribute zero nodes to a11y tree (`aria-hidden` + `role="presentation"`) — test_type: Playwright + @axe-core/playwright
- 5.8.6 PENDING — target: LCP preservation — input: Lighthouse CI on `/agents` before and after component mount — expected: LCP ≤ 2.0s (within §5.6.1 website target); beams load AFTER LCP via dynamic import or below-the-fold trigger — test_type: Lighthouse CI
- 5.8.7 PENDING — target: chunk budget — input: Vite build of `/agents` route — expected: motion + beams chunk ≤ 40 KB gzipped, loaded only when /agents navigates — test_type: size-limit CI gate

### 5.8.4 `<AnimatedTerminal />`

**Location:** `ui/packages/website/src/components/domain/animated-terminal.tsx`

**What it does:** macOS-styled terminal chrome. When scrolled into view, types out a configurable command list with realistic keystroke timing + optional per-keystroke sound + bash syntax highlighting + multi-step output rendering + blinking cursor. Phase state machine: `idle → typing → executing → outputting → pausing → (next command) → done`.

**Brand variant:** prompt uses `--z-cyan` for username + `--primary` for `$`. Syntax highlighter:
- `command` tokens → `text-success` (was emerald-400)
- `flag` tokens → `text-info` (was sky-400)
- `string` → `text-warning` (was amber-300)
- `path` → `text-info/80` (was cyan-300)
- `default` → `text-muted-foreground` (was neutral-300)

Window chrome: title bar with traffic lights stays neutral (the macOS convention). Body background: `bg-card` so it sits nicely on the beams container.

**Intended placement:** `/agents` route, below the `<BackgroundBeamsWithCollision />` hero, showing the `zombiectl` install + run happy-path. Dimensions referenced below use a canonical demo script:

```
zombiectl login
zombiectl zombie install --template lead-collector
zombiectl zombie up lead-collector --watch
# → [ready] lead-collector awaiting triggers
```

**Architecture:**

- `AnimatedTerminal` — the public component. Props: `commands: string[]`, `outputs?: Record<number, string[]>`, `username?: string`, `typingSpeed?: number`, `delayBetweenCommands?: number`, `initialDelay?: number`, `enableSound?: boolean`, `className?: string`.
- `useInView(ref, once)` — minimal `IntersectionObserver` hook. Trigger once, 0.1 threshold.
- `useAudio(enabled)` — Web Audio API hook. Fetches `/sounds/keystroke.ogg` (a sprite; key-specific offsets map in a static record). Disabled silently if autoplay blocked, file fails to fetch, or `enabled=false`. No JS shipped unless `enableSound` is truthy.
- `tokenizeBash(text)` — pure function, unit-testable. Splits on whitespace, classifies each word as `command | flag | string | number | operator | path | variable | comment | default`.
- `SyntaxHighlightedText` — renders `tokenizeBash` output into `<span>` elements with semantic-token color classes.

**Perf invariants (measurable):**

- Typing animation driven by `setTimeout` chains (existing pattern), not reflow-inducing layout writes. Each typed character appends to a state string that renders once — React batches.
- Audio is optional. If `enableSound={false}` or audio fails to load, the component behaves identically minus sound.
- No global `<audio>` element — Web Audio API via `AudioContext`, cleaned up on unmount.
- Output container `scrollTop = scrollHeight` on each line — single layout write, no thrash.
- Respects `prefers-reduced-motion: reduce` → commands appear instantly (no character-by-character), cursor stops blinking, audio silent.

**Dimensions:**

- 5.8.8 PENDING — target: `tokenizeBash()` pure function — input: `"zombiectl zombie install --template lead-collector # deploys"` — expected: tokens `[command="zombiectl", default=" ", default="zombie", default=" ", default="install", default=" ", flag="--template", default=" ", default="lead-collector", default=" ", comment="# deploys"]` — test_type: unit
- 5.8.9 PENDING — target: `tokenizeBash()` — paths + strings + numbers + operators + variables + comments — expected: correct classification per token type — test_type: unit (table-driven, one case per TokenType)
- 5.8.10 PENDING — target: `<AnimatedTerminal>` — input: 4-command script — expected: phase state machine transitions in sequence: idle → typing → executing → outputting → pausing → (next) → ... → done. Each state's render asserted via Playwright — test_type: Playwright (step-through)
- 5.8.11 PENDING — target: `useInView` hook — input: mount below the fold, then scroll into view — expected: animation starts on first intersection; does NOT restart on subsequent intersection — test_type: Playwright (scroll + assertion on line count)
- 5.8.12 PENDING — target: `prefers-reduced-motion: reduce` — input: Playwright emulated media query — expected: all commands + outputs render instantly, cursor stops blinking, audio silent — test_type: Playwright
- 5.8.13 PENDING — target: audio gating — input: `enableSound={false}` — expected: zero network requests to `/sounds/*`, no `AudioContext` constructor called (verified via Playwright `page.on("request")`) — test_type: Playwright
- 5.8.14 PENDING — target: audio failure path — input: `enableSound={true}`, mock `/sounds/keystroke.ogg` to 404 — expected: component still renders + types, no JS error thrown, silent fallback — test_type: Playwright
- 5.8.15 PENDING — target: SSR render — input: `renderToStaticMarkup(<AnimatedTerminal commands={[...]} />)` — expected: initial chrome + prompt render, no hydration mismatch, no runtime errors from `AudioContext`/`IntersectionObserver` in SSR (guarded behind `typeof window !== "undefined"` checks) — test_type: unit
- 5.8.16 PENDING — target: a11y — input: `axe-core` on a page containing the terminal — expected: zero violations; terminal semantic wrapper uses `role="region" aria-label="Interactive terminal demonstration"`; output text is live-announced (`aria-live="polite"`) so screen readers get the completed command output — test_type: Playwright + @axe-core/playwright

### 5.8.5 Bundle + performance budget (both components combined)

| Asset | Budget | How |
|---|---|---|
| `motion` library chunk | ≤ 40 KB gzipped | Vite chunk split on dynamic import of the /agents route |
| `background-beams-with-collision.tsx` source | ≤ 5 KB gzipped | Component code stays lean; beam configs are data, not logic |
| `animated-terminal.tsx` source | ≤ 5 KB gzipped | Tokenizer + audio + phase machine combined |
| `/sounds/keystroke.ogg` (audio sprite) | ≤ 50 KB, lazy-fetched, only when `enableSound={true}` | Route-level code split + fetch on first render |
| Initial `/agents` route JS (gzipped) | Unchanged vs pre-M26.8 baseline — these components chunk-split out | Lighthouse CI diff |

### 5.8.6 Audio sprite provenance

The keystroke sound sprite (`/sounds/keystroke.ogg`) is royalty-free or self-recorded. Track the source in `ui/packages/website/public/sounds/README.md` (license, author, generation command). No third-party CDN — bundled with the website's static assets.

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
  AtmosphericBackground,
  type AtmosphericBackgroundProps,
} from "./design-system";

// CSS exports
// @usezombie/design-system/tokens.css     — Layer 0 palette + typography + motion primitives
// @usezombie/design-system/components.css — component CSS + signature keyframes + atmospheric background
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
| Every keyframe under `components.css` animates only `transform` / `opacity` / `filter` / `background-position` | grep gate (§5.5.8 dimension) |
| Every signature animation has a `prefers-reduced-motion: reduce` fallback | Playwright emulated media query (§5.5.7) |
| Background overhead ≤ 3KB CSS, zero JS | measured against `components.css` + bundle analyzer |
| Motion tokens (`--motion-duration-*`, `--motion-ease-*`) referenced only through `var()` — no inline durations in component `style` or `className` | grep gate |
| `@usezombie/design-system/package.json` has `"sideEffects": false` + per-component exports map | JSON assertion + tree-shake test |
| `components.css` ≤ 12 KB gzipped at ship time | build gate (`gzip -9 \| wc -c`) |
| Zero third-party font/script imports (`googleapis`, `jsdelivr`, `cdn.*`) in any package source | grep gate in `make lint` |
| Lighthouse CI: LCP ≤ target, CLS ≤ target, TBT ≤ target on both consumers; no metric regresses > 10% vs baseline | LHCI CI gate per PR |
| Initial-JS gzipped budget: website ≤ 90 KB, app ≤ 180 KB first-load (pre-M26 baseline captured; ≤ 5% regression allowed) | size-limit CI gate |
| Every signature animation sustains ≥ 55 FPS during Playwright frame-timing trace | Playwright perf trace |
| Zero `motion` / `framer-motion` imports under `ui/packages/app/**` OR `ui/packages/design-system/**` (website-only exception, §5.8.2) | grep gate in `make lint` |
| Website `/agents` route bundle chunk-splits motion library — loaded only on route entry, not on `/` | Vite build chunk analysis CI gate |
| `<BackgroundBeamsWithCollision />` + `<AnimatedTerminal />` live in `ui/packages/website/src/components/domain/`, not in the shared package | `find` gate |
| Audio sprite for terminal is bundled locally (no CDN) and lazy-fetched only when `enableSound={true}` | grep gate + Playwright network assertion |

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
| 8 | M26.6 Signature Motion + Atmospheric Background — motion tokens, 6 keyframes, Button hover trace, Skeleton shimmer, `<AtmosphericBackground />`, Playwright hover/reduced-motion tests | dims 5.5.1–5.5.10 pass |
| 9 | M26.7 Load Performance — `sideEffects: false` + per-component exports, self-hosted Geist fonts replacing googleapis import, critical-CSS, Lighthouse CI + size-limit + Playwright FPS trace wired as CI gates | dims 5.6.1–5.6.10 pass |
| 10 | M26.8 Website interactive demos — `<BackgroundBeamsWithCollision />` + `<AnimatedTerminal />` land in `ui/packages/website/src/components/domain/`, `motion` chunk lazy-loaded on `/agents` route, audio sprite bundled, unit tests for tokenizer + phase machine + SSR, Playwright tests for collision / reduced-motion / LCP / chunk budget | dims 5.8.1–5.8.16 pass |
| 11 | Full verification — `make lint`, `bun run typecheck / lint / test / build` in both app and website, `bunx playwright test` in both (including motion + perf + M26.8 demo suites), Lighthouse CI green, bundle budgets green | all green |
| 12 | CHORE(close) — spec DONE, move to done/, Ripley log, changelog entry, PR | PR open with visual-diff screenshots + motion recordings + Lighthouse report deltas + /agents route demo capture attached |

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
- [ ] Motion tokens (`--motion-duration-*`, `--motion-ease-*`) + 6 signature keyframes land in the package; component files reference them via `var()` only
- [ ] `<AtmosphericBackground />` renders in both app and website layouts, with 3 base layers (gradient + grid + aurora), opt-in particles, and `prefers-reduced-motion` honoured
- [ ] Playwright motion suite (§5.5.3–5.5.10) green — hover trace, shimmer frames, dialog enter, reduced-motion disables all animations, GPU-safety grep
- [ ] Unit-test SSR coverage for every motion-using component (no runtime errors, no `undefined` in animation attributes)
- [ ] Lighthouse CI: website LCP < 1.6s, app LCP < 2.0s, CLS < 0.05 on both, TBT within target, measured on mobile throttled profile
- [ ] Initial JS bundle: website ≤ 90 KB gzipped, app ≤ 180 KB gzipped first-load (baseline captured pre-M26, ≤ 5% regression tolerance)
- [ ] `components.css` ≤ 12 KB gzipped at ship
- [ ] Zero third-party font/script imports (`googleapis`, `jsdelivr`, `cdn.*`) — Geist self-hosted from the package or via `next/font`
- [ ] `@usezombie/design-system/package.json` has `"sideEffects": false` + per-component exports; `import { Button }` tree-shakes in isolation
- [ ] Every signature animation sustains ≥ 55 FPS in Playwright frame-timing trace
- [ ] `export const dynamic` / `revalidate` tuned on app routes — max static coverage in Next.js build output
- [ ] `<BackgroundBeamsWithCollision />` ships on website `/agents` route — beams fall + collide + explode + reset; brand gradients (orange→bright, explosions cyan→orange) — zero indigo/violet
- [ ] `<AnimatedTerminal />` ships on website `/agents` route — types canonical `zombiectl` install+run script with bash syntax highlighting; phase state machine tested end-to-end
- [ ] `motion` library chunk lazy-loaded on `/agents` only; zero imports under `app/**` or `design-system/**`
- [ ] Both components pass `@axe-core/playwright` with zero violations
- [ ] Audio opt-in via `enableSound`; zero network requests to `/sounds/*` when disabled or not in viewport
- [ ] Reduced-motion fallback: beams static, terminal renders instantly, cursor stops blinking, audio silent
- [ ] `/agents` route LCP stays within §5.6.1 budget (≤ 2.0s) — demos load post-LCP

---

## 10.0 Out of Scope

- Introducing Panda CSS / Vanilla Extract / styled-components. M26 stays on Tailwind v4 + CSS variables.
- Adding new *shared* design-system components. M26 is a migration, not a feature milestone for the shared package. New primitives in the shared package go in M27 or later.
- Migrating shadcn-scope primitives I added in M12 (StatusCard, EmptyState, Pagination, DataTable, ConfirmDialog, ActivityFeed) into the design-system package. Those are app-local compositions; promoting them is a judgement call for M27+ when a second consumer needs them.
- Playwright visual-regression infra itself. M26 uses the existing Playwright setup + manual before/after screenshot comparison. A proper visual-diff CI gate is a separate DX milestone.
- Consuming `@aceternity/ui` or other third-party animated component libraries. M26.8 reimplements the beam-collision + typed-terminal patterns under our own code with UseZombie branding — structural technique (`getBoundingClientRect` collision, `IntersectionObserver` trigger, `AudioContext` sprite playback) is generic web platform usage, not copyrighted.
- Installing a `motion` library in the shared design-system or app packages. §5.8.2 carves an explicit website-only exception with six measurable guardrails; the ban holds everywhere else.
- Replacing `<AtmosphericBackground />` with `<BackgroundBeamsWithCollision />` on the dashboard. The dashboard is operational UI that needs calm, continuous ambient motion — beams would be distracting during a kill-switch decision. Two surfaces, two atmospheres.

---

## 11.0 Risk Log

| Risk | Mitigation |
|---|---|
| Website visual regression on a page we didn't screenshot | Capture screenshots of *every* top-level route before starting, not just the landing page |
| `Slot`'s single-child constraint breaks a consumer pattern | Fall back to non-asChild Button in that one call-site; document the exception |
| Tailwind v4 `@source` needs to scan the package's new components.css | Verify `@source` directives in both app and website entry CSS cover the design-system package paths |
| Next.js RSC build silently client-hoists the whole tree due to a hidden import | The M26.2 dimension 2.5 fixture catches this — explicitly asserts a Server Component builds |
| Breaking change (`to=` → `asChild`) missed in a call-site | grep-gate for `<Button to=` — zero matches post-migration |
| Signature animation looks like a "novelty" rather than a signature | Keep durations short (≤ 560ms), opacity low (0.08–0.22 for glow), grounded in existing brand tokens. Review recordings at M26.6 before merging. |
| Background particles tank perf on low-end devices | Auto-disable via `prefers-reduced-data`, `prefers-reduced-motion`, and viewport `< 640px`. Cap DOM nodes for particles at 12. |
| Playwright visual regression flakes on animated elements | Capture static screenshots with animations paused (`animation-play-state: paused` via test hook). Motion coverage uses frame captures at specific offsets (0ms / 800ms / 1600ms) rather than pixel-perfect diffs on an in-flight animation. |
| A consumer imports from `"@usezombie/design-system"` but forgets the CSS import, shipping unstyled | Package `exports` field + TypeScript barrel doesn't expose runtime checks. Mitigation: docs note + a runtime `console.warn` in dev-mode if `document.adoptedStyleSheets` doesn't contain the package's `z-btn` selector within 50ms of first component mount. |
| Lighthouse CI flakes on a slow runner — falsely fails a PR | Use `@lhci/cli` `numberOfRuns: 3` + take median; require > 10% regression before blocking, not 1%. |
| Bundle size grows when a second consumer is added | `size-limit` runs per-package, so the per-consumer budget is independent. The package's own budget (≤ 12 KB CSS, tree-shakable JS) is the invariant. |
| Self-hosting Geist breaks the website's current visual — subtle weight/spacing diffs vs Google Fonts version | Capture pre-migration reference screenshots of text-heavy pages; verify the self-hosted font matches at the same weight/axis. Use Vercel's npm `geist` package which ships the same binaries Google Fonts serves — known parity. |
| A motion improvement regresses CLS (e.g. dialog enter animation changes layout height) | Every motion keyframe is constrained to `transform` / `opacity` / `filter` (§5.5.8 grep gate). Layout-triggering animations are a build failure, not a runtime surprise. |
| `motion` library leaks from website-domain into the shared package or app | grep gate in `make lint` catches the import before PR merges; size-limit catches the bundle bloat as a second line of defence |
| Beam collision check `getBoundingClientRect` runs on the main thread and stalls the page | Replace the reference sample's `setInterval(50ms)` with `requestAnimationFrame` — browser throttles it when tab hidden; measured in §5.8.4 |
| Explosions pile up if `AnimatePresence` exit logic misbehaves | Playwright test §5.8.1 captures DOM node count at 0s / 30s — must stay within 5% of starting count |
| Terminal audio autoplay blocked by browser policy | Component silently falls back to no-sound (§5.8.14). User gesture requirement handled by Web Audio API `resume()` on first play, already in the reference implementation. |
| Audio sprite fetch fails (404 / CORS) | §5.8.14 covers — component behaves identically without audio. No console errors to the user. |
| `/agents` LCP degrades due to animated components | Dimensions §5.8.6 gate this — beams + terminal load AFTER LCP (via dynamic import + IntersectionObserver trigger). Lighthouse CI is the enforcement. |

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
