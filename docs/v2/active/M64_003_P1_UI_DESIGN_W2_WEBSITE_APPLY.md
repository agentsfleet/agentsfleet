<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
-->

# M64_003: Apply Operational Restraint to ui/packages/website

**Prototype:** v2.0.0
**Milestone:** M64
**Workstream:** 003
**Date:** May 08, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — marketing surface is the highest-taste touchpoint; ships in PR #308 alongside W1 so `main` never receives a broken intermediate state.
**Categories:** UI
**Batch:** B2 — depends on W1 (M64_002) DONE; runs after W1's token + font surface lands.
**Branch:** feat/m64-002-design-w1 (stacked on W1)
**Depends on:** M64_002 (token swap + WakePulse primitive + design-system font self-hosting)

**Canonical architecture:** `docs/DESIGN_SYSTEM.md` — locked direction "Operational Restraint" (memorable thing: "It wakes"); mockup A is the marketing-hero canonical shape.

---

## Implementing agent — read these first

1. `docs/DESIGN_SYSTEM.md` — the spec, end-to-end. Type scale (§Typography), color tokens + the `--pulse` currency rule (§Color), spacing (§Spacing), layout grid + radius rules (§Layout), motion budget (§Motion), component principles (§Component principles), forbidden treatments. The doc-reads gate makes this mandatory on every `ui/packages/**` edit.
2. `~/.gstack/projects/usezombie/designs/design-system-20260508-0831/preview.html` — Mockup A is the marketing-hero canonical reference (top-bar + brand-mark pulse, hero eyebrow with WakePulse, mono headline, mono CLI demo block, dot-grid bg). Implementation is a direct evolution, not a different system.
3. `ui/packages/design-system/src/tokens.css` + `theme.css` — W1 surface; tokens we consume. tokens.css already self-hosts `@fontsource/commit-mono` + `@fontsource-variable/instrument-sans` via `@import`, so removing `@fontsource-variable/geist*` from `website/package.json` is safe — fonts arrive transitively when website imports `@usezombie/design-system/tokens.css` (which it already does).
4. `ui/packages/design-system/src/index.ts` — primitives the rebuild consumes (`Button`, `Card`, `Badge`, `InstallBlock`, `Terminal`, `Section`, `Grid`, `WakePulse`). UI Component Substitution Gate forbids raw `<section>`/`<button>`/`<input>` when a primitive exists.
5. `ui/packages/website/src/styles.css` — current 1866-line legacy file with 372 dead `var(--z-*)` references. Wholesale-rewritten (not migrated) to ~400L driven only by Layer 0/1 tokens + Tailwind utilities + design-system primitives.
6. The W1 spec at `docs/v2/done/M64_002_P0_UI_DESIGN_W1_TOKEN_SWAP.md` — for the patterns the design-system package now exposes and the rules under "Applicable Rules" we inherit.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal. Specifically **NDC** (no dead code at write time), **NLR** (touch-it-fix-it), **NLG** (no legacy framing pre-v2.0; current `VERSION` 0.34.x), **ORP** (cross-layer orphan sweep), **TST-NAM** (no milestone IDs in source).
- `docs/BUN_RULES.md` — TS/TSX file shape, const/import discipline, anti-patterns. Doc-read gate fires on every `*.tsx` / `*.ts` edit.
- `docs/DESIGN_SYSTEM.md` — doc-read gate fires on every file under `ui/packages/**` and on visual-token / motion / typography changes.
- File & Function Length Gate — file ≤ 350L, fn ≤ 50L, method ≤ 70L.
- UI Component Substitution Gate — raw HTML elements only via `asChild`; primitives from `@usezombie/design-system` first.
- Standard set otherwise.

---

## Anti-Patterns to Avoid (read this BEFORE drafting the spec)

N/A — spec authoring complete; the implementing agent (this same session) reads sections below as goal contract, not pseudocode.

---

## Overview

**Goal (testable):** Every page under `ui/packages/website/src/` renders against `docs/DESIGN_SYSTEM.md` tokens + Mockup A shape — zero `var(--z-*)` references remain in source, zero Geist references remain in source or `package.json`, the wake-pulse appears exactly where the spec says (live indicator on hero eyebrow + brand-mark in topbar), and `qa-website` Playwright smoke passes against the new shape.

**Problem:** W1 swapped the design-system tokens but the website still loads `@fontsource-variable/geist`, still references 372 `var(--z-*)` calls in 1866-line `styles.css` (all resolving to `unset`), still ships a centred-everything orange-era hero with multi-stop linear-gradient CTAs, animated particle field, three radial-gradient body decorations, and `border-radius: 9999px` buttons — every one forbidden by the Operational Restraint spec. Visiting `usezombie.com` today is a half-migrated site: design-system primitives use the new palette, page chrome is broken / orange-era / both.

**Solution summary:** Delete Geist from `package.json`, drop the 1866-line `styles.css` and replace with a ~300-450 line file that defines only page-specific layout (top-bar, page rhythm, dot-grid) using new tokens + Tailwind utilities. Rebuild Home / Pricing / Agents around Mockup A's mono-headline, mono-CLI, single-pulse posture; sweep Privacy + Terms for typography only. Delete every component file no longer composed (HeroIllustration, FeatureFlow's orange-era code, CTABlock's gradient pill, etc.) — RULE NDC governs. The `<WakePulse live={true}/>` primitive from W1 fires exactly twice on the marketing site: brand-mark in topbar and hero eyebrow's "LIVE — N wakes" indicator.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/website/package.json` | EDIT | Remove `@fontsource-variable/geist` + `@fontsource-variable/geist-mono` deps. Fonts arrive transitively via design-system's tokens.css `@import`. |
| `ui/packages/website/src/styles.css` | REWRITE | 1866L → ~80-120L (Tailwind/shadcn utility-first). Keeps only what utilities can't express: dot-grid `body` bg, `@keyframes pulse` re-export, `prefers-reduced-motion` overrides, `[data-theme="light"]` mirror, `.wrap` page-shell width, top-bar sticky shell. All page chrome (`.mc-header`, `.feature-section`, `.pricing-card`, `.cta`, `.hero-proof-card`, `.scanline`, `.particle-field`, etc.) collapses into utility classes on the JSX. Zero `var(--z-*)`. Zero `linear-gradient` outside the dot-grid radial. Zero `border-radius: 9999px`. |
| `ui/packages/website/src/App.tsx` | EDIT | Top-bar gets `<WakePulse live>` brand-mark + design-system `<Button>` nav; site-shell drops the orange-era `::before` / `::after` blob decorations (deleted from CSS). |
| `ui/packages/website/src/pages/Home.tsx` | REBUILD | Mockup A shape: eyebrow + LIVE pulse → mono headline → lede → CTA pair → mono CLI block → features grid → CTA. |
| `ui/packages/website/src/pages/Pricing.tsx` | REBUILD | Mono pricing-card chrome, no gradient pill featured-card border, lead-form rebuilt with design-system Input/Button/Card. |
| `ui/packages/website/src/pages/Agents.tsx` | REBUILD | Terminal-aesthetic page rebuilt around `<Terminal>` + `<Card>` primitives. The `.scanline` overlay is forbidden under spec §Motion (no decorative scanlines); replaced with mono-CLI evidence list. |
| `ui/packages/website/src/pages/Privacy.tsx` | EDIT | Typography sweep — body text uses Instrument Sans (default), section heads Commit Mono. No layout rebuild. |
| `ui/packages/website/src/pages/Terms.tsx` | EDIT | Same typography sweep. |
| `ui/packages/website/src/components/Hero.tsx` | REBUILD | Mockup A hero. Eyebrow + WakePulse live indicator, mono `<h1>` with two-line break, body lede in Instrument Sans, two-button CTA row (primary + ghost), inline `<Terminal>` showing the install transcript. |
| `ui/packages/website/src/components/HeroIllustration.tsx` | DELETE | Zero importers (audited). Orange-era illustration (zombie mark inside multi-stop radial-gradient panel). Mockup A replaces with CLI block. RULE NDC. |
| `ui/packages/website/src/components/FeatureFlow.tsx` | REBUILD | Mockup-A grid-2 evidence/code rows. Mono code block, no orange-rail decoration. |
| `ui/packages/website/src/components/FeatureSection.tsx` | REBUILD | Mono number eyebrow + mono title + sans body. Borders > shadows. No orange rail. |
| `ui/packages/website/src/components/HowItWorks.tsx` | REBUILD | 3-step mono cards, no counter pseudo-element decoration, no hover orange glow. |
| `ui/packages/website/src/components/Footer.tsx` | EDIT | Token swap + Commit Mono nav labels. |
| `ui/packages/website/src/components/CTABlock.tsx` | REBUILD | Mono headline + sans lede + button pair. No animated shimmer pill. |
| `ui/packages/website/src/components/FAQ.tsx` | EDIT | Token swap + chevron transition uses `--motion-duration-snap`/`--easing-snap`. |
| `ui/packages/website/src/components/ProviderStrip.tsx` | DELETE | Zero importers (audited). Provider strip pattern not in Mockup A; if a future spec needs it, rebuild from scratch. RULE NDC. |
| `ui/packages/website/src/components/ProviderStrip.test.tsx` | DELETE | Component deleted. |
| `ui/packages/website/tests/e2e/design-system-smoke.spec.ts` | EDIT | Update assertions to new shape (mono `<h1>` family, no linear-gradient on CTAs, dot-grid bg present, brand-mark animation-name `pulse`). |
| `ui/packages/website/src/components/CTABlock.test.tsx` / `FAQ.test.tsx` / `FeatureSection.test.tsx` / `Footer.test.tsx` / `Hero.test.tsx` / `HowItWorks.test.tsx` / `ProviderStrip.test.tsx` | EDIT | Re-align unit-test class-string assertions to rebuilt component shape. |
| `ui/packages/website/src/components/HeroIllustration.test.tsx` | DELETE | Component deleted. |
| `docs/v2/active/M64_003_P1_UI_DESIGN_W2_WEBSITE_APPLY.md` | CREATE | This spec. |

---

## Sections (implementation slices)

### §1 — Font + dependency surface

Remove `@fontsource-variable/geist` and `@fontsource-variable/geist-mono` from `ui/packages/website/package.json`. Run `bun install` so the lockfile drops the entries. Verify `bun pm ls | grep -i geist` returns no rows. Fonts arrive via `@usezombie/design-system/tokens.css` (already imported by `styles.css`).

**Implementation default:** the website does not add its own fontsource imports. All font loading is the design-system's responsibility per W1.

### §2 — `styles.css` wholesale rewrite

Delete the 1866L file's contents. Author a fresh `styles.css` containing only:
- `@import "@fontsource-variable/geist"` removed; `@import "@usezombie/design-system/tokens.css"` + `theme.css` retained.
- The page-shell wrapper width / padding (`.wrap` analogue, 1280px max).
- The top-bar (sticky, `var(--bg)` 90% mix, backdrop-blur, `1px solid var(--border)` bottom).
- The hero dot-grid background (radial-gradient at 8% opacity per spec §Layout — fixed, not animated, with light-mode mirror under `[data-theme="light"]`).
- `@media (prefers-reduced-motion: reduce)` — block any motion the rebuilt components add (route-fade etc.).
- Section vertical rhythm (`section { padding: 96px 0; border-top: 1px solid var(--border) }` per Mockup A).

No `.cta`, `.feature-section`, `.pricing-card`, `.hero-proof-card`, `.pricing-lead-card`, `.scanline`, `.particle-field`, `.site-shell::before`, `.site-shell::after`, `.feature-flow-row`, `.mode-switch`, animated keyframes (`z-cta-shimmer`, `z-glow-pulse`, `z-grid-shift`, `z-float-up`, `z-drift`, `z-rise-in`, `z-cta-shimmer`). They're all replaced by design-system primitives + Tailwind utilities at the component level.

**Implementation default:** keep the file under 350 lines. If the rewrite drifts higher, split repeated patterns into design-system primitives (consult before crossing the package boundary; W1 is a closed surface).

### §3 — App.tsx + top-bar

App.tsx renders the top-bar with: brand-mark `<WakePulse live={true} size={12} />`, "usezombie" wordmark in Commit Mono 500, primary nav (Home, Agents, Pricing, Docs) in Commit Mono 12px muted, single primary `<Button>` "→ install" CTA on the right. Drops:
- The existing centred `.site-shell::before`/`::after` blob decorations (deleted from CSS).
- The legacy `.mode-switch` pill (mode toggle not part of Mockup A — defer until DESIGN_SYSTEM.md commits to a UI affordance).
- The `<AnimatedIcon><ZombieHandIcon /></AnimatedIcon>` Mission Control button hover treatment. Mockup A's topbar ships a plain "→ install" Button. Net effect: zero `ZombieHandIcon` / `AnimatedIcon` imports in website source after rebuild. Both primitives stay in design-system because W3 (app) still imports them.

**Implementation default:** dark is canonical; no in-page light/dark toggle. `[data-theme="light"]` works (via OS preference) but is not driven from the UI in this spec.

### §4 — Hero (Mockup A canonical)

`Hero.tsx` ships the canonical marketing hero:
- Eyebrow row: `<WakePulse live size={6}>` + Commit Mono 12px uppercase letter-spaced label "LIVE — wake.on.event" (placeholder count text — actual N comes from a future telemetry slice in W3+).
- `<h1>` Commit Mono 500, two-line break, copy from Mockup A (or the spec's "Memorable thing" voice — direct, evidenced).
- Lede paragraph in Instrument Sans 18/1.5, max-width 640px.
- Two-button CTA row: primary `<Button>` ("→ install in Claude Code") + default `<Button>` ("view a real wake (replay)").
- Inline `<Terminal>` block underneath rendering the install transcript (mockup A's CLI block).

The hero gets the dot-grid background applied at the `body` level via `styles.css` so the entire page shares the texture; `prefers-reduced-motion` does not affect the static dot-grid.

### §5 — Home page composition

`Home.tsx` composes: `<Hero />`, then a `<Section>` with `<FeatureSection>` cards in a 4-up `<Grid columns="four">` (the four "Markdown-defined / BYOK / Approval gating / Open source" capabilities — copy survives unchanged), then `<HowItWorks />` (3 mono numbered steps), then `<CTABlock />` ("install platform-ops" → docs link). No marketing-era `.features-grid` decoration; layout is strict 12-col grid per spec §Layout.

**Implementation default:** copy preserved from current `Home.tsx`'s `features` array — voice rewrite is out of scope for this spec; only chrome changes.

### §6 — Pricing page

`Pricing.tsx`: `<Section>` with mono-headline + Instrument Sans lede; `<Grid columns="two">` of `<Card>`-shaped pricing tiers (Free + Pro at v2 pricing, mono price values with tabular-nums, `<Badge>` for "Popular" not a `::before` pseudo-pill, `<Button variant="primary">` CTA per card); a lead-form `<Card>` with design-system `<Input>` + `<Button>` + status `<Badge>` for success/error states; mono FAQ rows (`<FAQ>` component rebuilt with `<details>` + design-system styling).

### §7 — Agents page

`Agents.tsx`: terminal-aesthetic page rebuilt around `<Terminal>` and mono `<Card>` rows. Existing `.scanline` overlay is removed — spec §Motion forbids decorative scanlines. The page shows a plain-text-evidenced "what platform-ops does" walkthrough with line-numbered evidence rows in mono blocks. Voice/copy survives; chrome changes.

### §8 — Privacy + Terms typography sweep

`Privacy.tsx` + `Terms.tsx`: replace the legacy `.legal-page` class chrome with semantic `<article>` (or design-system primitive if `Section` accepts the role) + Instrument Sans body via Tailwind `font-sans`. Headings get Commit Mono via `font-mono`. No layout rebuild — these stay single-column ~68ch measure per spec §Layout.

### §9 — Test alignment

`design-system-smoke.spec.ts` assertions update to new shape:
- `<h1>` family contains `Commit Mono` (computed-style font-family check).
- Hero CTA `background-image` is `none` (no linear-gradient).
- Brand-mark animation-name is `pulse` (not the deleted `z-icon-wave`).
- Body's background-image is the dot-grid radial-gradient (asserts `radial-gradient` substring).

Per-component `.test.tsx` files update class-string + computed-style assertions. RULE TST-NAM forbids milestone IDs in test names — describe behaviour, not workstream lineage.

---

## Interfaces

No HTTP / CLI / RPC surface changes. Public component interfaces preserved:
- `<Hero />` — no props (currently no props; preserved).
- `<FeatureSection number={string} title={string} description={string} />` — preserved.
- `<HowItWorks />` — no props; preserved.
- `<CTABlock />` — no props; preserved.
- `<Footer />`, `<FAQ />`, `<ProviderStrip />` — preserved.
- `<HeroIllustration />` — DELETED. No callers after Hero rebuild.

The `<InstallBlock>` consumer call from `Home.tsx` survives unchanged (W1 ships the new shape).

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Geist still loads | Browser cache or a missed import | `grep -rn -i geist ui/packages/website/` returns zero non-historical hits; CI verified by `bun pm ls` step. |
| `--z-*` ref leaks back | Author copy-pastes legacy snippet | `grep -rn 'var(--z-' ui/packages/website/src/` returns zero hits — verified at HARNESS VERIFY. |
| `--pulse` used decoratively | Author drops it on a non-live element | Pre-PR grep: every `var(--pulse)` / `#5EEAD4` is on a `live`-data-driven element OR is the brand-mark in topbar. RULE: surfaced if violated. |
| Buttons go pill again | `rounded-full` survives a copy-paste | `grep -rn 'rounded-full\|border-radius: 9999' ui/packages/website/src/` returns zero hits. |
| Aurora gradient sneaks in | Author mistakes the dot-grid for permission-to-decorate | `grep -rn 'linear-gradient\|radial-gradient' ui/packages/website/src/` audited — every hit must be the dot-grid radial OR a single-stop overlay; aurora multi-stop forbidden. |
| Test asserts old shape | Smoke spec runs against new components and fails | §9 alignment captured in same commit; `qa-website` green is acceptance criterion. |
| Dot-grid hurts LCP | Background-image is decoded on every paint | Implemented as `radial-gradient` (CSS), not `<img>` — LCP unaffected. |

---

## Invariants

1. **Zero `var(--z-*)` in `ui/packages/website/src/`.** Enforced by `scripts/audit-doc-reads.sh` and a one-line `grep -rn 'var(--z-'` step in CI's `qa-website` job — non-zero matches fail the build.
2. **Zero Geist in `ui/packages/website/`.** Enforced by `grep -rn -i geist ui/packages/website/` — non-zero matches in non-historical files fail the build. `package.json` lockfile audited via `bun pm ls`.
3. **`--pulse` only on live signals.** Enforced by code-review grep: every `var(--pulse)` / `#5EEAD4` callsite is either the brand-mark in `App.tsx` or a `live={true}`-prop'd `<WakePulse>` instance. Surfaced in PR Discovery.
4. **No `border-radius: 9999px` on buttons.** Enforced by `grep -rn 'rounded-full\|border-radius: 9999' ui/packages/website/src/` — non-zero fails CI.
5. **File length gate.** `styles.css` ≤ 350L; component files ≤ 350L; functions ≤ 50L. Enforced by File & Function Length Gate.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `design-system-smoke / hero / mono headline` | `getComputedStyle(h1).fontFamily` contains "Commit Mono". |
| `design-system-smoke / hero / no gradient CTA` | Primary CTA `background-image` is "none". |
| `design-system-smoke / topbar / brand-mark animates` | Brand-mark `animation-name` is "pulse". |
| `design-system-smoke / body / dot-grid present` | `getComputedStyle(body).backgroundImage` contains "radial-gradient". |
| `design-system-smoke / pricing / mono price` | Pricing-card price element font-family contains "Commit Mono". |
| `design-system-smoke / agents / no scanline` | `[class*="scanline"]` count is 0 on the /agents route. |
| `Hero.test / renders pulse on eyebrow` | A `<WakePulse>` with `live={true}` prop is rendered inside the eyebrow row. |
| `Hero.test / no orange-era classes` | Rendered DOM contains no `.cta-row`, `.hero-cta-primary`, `.hero-illustration`, `.hero-proof-grid`. |
| `Pricing.test / renders two pricing cards` | Two `<Card>` instances render under the pricing grid. |
| `FeatureSection.test / mono number eyebrow` | Number element class string contains `font-mono`. |
| `Footer.test / no orange link colors` | All `<a>` elements resolve `color` to `var(--text-muted)` or `var(--pulse)` — never to `--z-orange`. |
| `qa-website-smoke / route renders` | `/`, `/pricing`, `/agents`, `/privacy`, `/terms` each return 200 + render `<main>`. |

---

## Acceptance Criteria

- [ ] `grep -rn 'var(--z-' ui/packages/website/src/` returns 0 matches.
- [ ] `grep -rn -i geist ui/packages/website/` returns 0 non-historical matches.
- [ ] `grep -rn 'rounded-full\|border-radius: 9999' ui/packages/website/src/` returns 0 matches.
- [ ] `grep -rn 'linear-gradient' ui/packages/website/src/` returns only single-stop overlays (audited).
- [ ] `bun --filter @usezombie/website run test` passes.
- [ ] `bun --filter @usezombie/website run lint` passes.
- [ ] `bun --filter @usezombie/website run build` produces a CSS file under ~80kB raw.
- [ ] `qa-website` CI job passes (Playwright smoke spec + every-route-renders check).
- [ ] `make lint` clean.
- [ ] No file over 350 lines added/edited.
- [ ] PR description includes 2-3 hero screenshots (dark canonical + light secondary).
- [ ] `gitleaks detect` clean.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Zero --z-* in website source
grep -rn 'var(--z-' ui/packages/website/src/ && echo "FAIL" || echo "PASS"

# E2: Zero Geist in website package
grep -rn -i geist ui/packages/website/ --include="*.json" --include="*.css" --include="*.tsx" --include="*.ts" && echo "FAIL" || echo "PASS"

# E3: No pill buttons
grep -rn 'rounded-full\|border-radius: 9999' ui/packages/website/src/ && echo "FAIL" || echo "PASS"

# E4: Build
cd ui/packages/website && bun run build 2>&1 | tail -3

# E5: Tests
cd ui/packages/website && bun run test 2>&1 | tail -5

# E6: Lint
cd ui/packages/website && bun run lint 2>&1 | tail -3

# E7: 350-line gate
git diff --name-only origin/main -- ui/packages/website/ | grep -v '\.md$' | xargs -I{} sh -c 'wc -l "{}"' | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E8: Gitleaks
gitleaks detect --redact 2>&1 | tail -3
```

---

## Dead Code Sweep

**1. Files deleted:**

| File to delete | Verify deleted |
|----------------|----------------|
| `ui/packages/website/src/components/HeroIllustration.tsx` | `test ! -f ui/packages/website/src/components/HeroIllustration.tsx` |
| `ui/packages/website/src/components/HeroIllustration.test.tsx` | `test ! -f ui/packages/website/src/components/HeroIllustration.test.tsx` |
| `ui/packages/website/src/components/ProviderStrip.tsx` | `test ! -f ui/packages/website/src/components/ProviderStrip.tsx` |
| `ui/packages/website/src/components/ProviderStrip.test.tsx` | `test ! -f ui/packages/website/src/components/ProviderStrip.test.tsx` |

**2. Orphaned references:**

| Deleted symbol or import | Grep command | Expected |
|--------------------------|--------------|----------|
| `HeroIllustration` import | `grep -rn 'HeroIllustration' ui/packages/website/src/` | 0 matches |
| `ProviderStrip` import | `grep -rn 'ProviderStrip' ui/packages/website/src/` | 0 matches |
| `ZombieHandIcon` / `AnimatedIcon` in website | `grep -rn 'ZombieHandIcon\|AnimatedIcon' ui/packages/website/src/` | 0 matches in non-fixture (DesignSystemGallery may still import for the gallery surface — gated separately) |
| `--z-*` (any) | `grep -rn 'var(--z-' ui/packages/website/src/` | 0 matches |
| `Geist` font ref | `grep -rn -i geist ui/packages/website/ --include="*.ts" --include="*.tsx" --include="*.css" --include="*.json"` | 0 matches |
| `.cta` / `.feature-section` / `.pricing-card` / `.scanline` / `.particle-field` legacy classes | `grep -rn '"\(cta\|feature-section\|pricing-card\|scanline\|particle-field\)' ui/packages/website/src/` | 0 matches |
| Animated keyframes (`z-cta-shimmer`, `z-glow-pulse`, `z-grid-shift`, `z-float-up`, `z-drift`, `z-rise-in`) | `grep -rn 'z-cta-shimmer\|z-glow-pulse\|z-grid-shift\|z-float-up\|z-drift\|z-rise-in' ui/packages/website/src/` | 0 matches |

---

## Discovery (consult log)

Empty at creation. Populated as Legacy-Design / Architecture consults fire during implementation.

---

## Notes

- **Voice / copy:** preserved unless the existing copy contradicts spec voice ("operational, not SaaS-marketing, the daemon already knows why"). Where Mockup A's copy is materially better than current site copy (e.g., hero "Your deploy failed. The daemon already knows why."), adopt mockup voice. Pricing tier prose, FAQ prose, Agents page prose all preserved.
- **Light mode:** parity works via `[data-theme="light"]` from W1 + this spec's dot-grid mirror. Not driven from in-page UI in this spec — Mockup A topbar has a "⌘ toggle mode" button in its preview, but spec §Color says "Light mode is a polite afterthought" — defer toggle to a later spec when DESIGN_SYSTEM.md decides where the affordance lives.
- **Bundle size:** the styles.css rewrite drops ~70kB raw / ~8kB gzip from the website bundle — incidental win, not a goal.
