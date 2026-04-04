# M3_008: Website Enhancement — Content Depth, Visual Polish, Dual-Domain

Date: Mar 05, 2026
Status: ✅ DONE
Priority: P1 — required before launch
Depends on: M3_007 (static website baseline)

---

## Problem

The v1 website (M3_007) shipped the structural skeleton — routes, mode switch, machine-readable assets — but lacked the content depth, visual polish, and conversion elements needed for a credible launch. Compared to peers (usegitai.com, factory.ai, actors.dev), the site was too brief and visually flat.

## Solution

Enhanced the existing website with richer content, background effects, a proper footer, stronger CTAs, and a terminal aesthetic for the agent surface. No new dependencies beyond Geist fonts.

## Scope

### usezombie.com (Human-facing)

| Section | Gap | Fix | Status |
|---------|-----|-----|--------|
| Hero | Weak CTAs, no terminal preview | Added "Start free" + "Book team pilot" CTAs, terminal command block | ✅ DONE |
| Features | 4 brief cards only | 5 numbered feature sections with descriptions | ✅ DONE |
| BYOK strip | No social proof of provider support | "Bring your own LLM keys" with Anthropic/OpenAI/Google/Mistral/Groq | ✅ DONE |
| How it works | No lifecycle visualization | 3-step: Queue spec, Agent pipeline, Validated PR | ✅ DONE |
| Pricing preview | Only on /pricing route | Preview cards on home, link to /pricing | ✅ DONE |
| CTA block | Missing | Full-width conversion block before footer | ✅ DONE |
| Footer | Missing entirely | 4-column: Product, Community, Legal, Brand | ✅ DONE |
| /pricing | Sparse bullet points | Richer feature lists, FAQ accordion, BYOK explainer | ✅ DONE |
| Background | Plain flat dark | CSS dot-grid overlay + radial hero glow | ✅ DONE |
| Typography | Space Grotesk + IBM Plex | Geist Sans + Geist Mono | ✅ DONE |

### usezombie.sh (Agent-facing)

| Section | Gap | Fix | Status |
|---------|-----|-----|--------|
| Aesthetic | Same style as human site | Terminal green/cyan, scanline overlay, full monospace | ✅ DONE |
| Content | 2 brief cards | Full contract table, API operations table, webhook examples | ✅ DONE |
| Bootstrap | Single command | curl + npx commands with copy buttons | ✅ DONE |
| Safety | Brief list | Expanded with details | ✅ DONE |
| Footer | Missing | Minimal terminal-style footer | ✅ DONE |

### Visual System Updates

| Token | Before | After | Status |
|-------|--------|-------|--------|
| `--font-sans` | Space Grotesk | Geist Sans | ✅ DONE |
| `--font-mono` | IBM Plex Mono | Geist Mono | ✅ DONE |
| `--neon-orange` | `#FF5A2D` | `#FF6B35` (warmer) | ✅ DONE |
| `--neon-orange-bright` | `#FF7A3D` | `#FF8C42` | ✅ DONE |
| `--surface-1` | (none) | `#161E2B` (elevated cards) | ✅ DONE |
| `--glow-orange` | (none) | `rgba(255,107,53,0.15)` | ✅ DONE |
| `--glow-strong` | (none) | `rgba(255,107,53,0.3)` | ✅ DONE |
| `--terminal-green` | (none) | `#39FF85` (agent page only) | ✅ DONE |
| Background | Flat gradient | Dot-grid overlay + radial glow | ✅ DONE |
| Card hover | None | Border glow + shadow transition | ✅ DONE |
| Hero headline | `clamp(1.9rem,6vw,3.4rem)` | `clamp(2.2rem,5.5vw,4rem)` | ✅ DONE |

## New Components

| Component | File | Purpose | Status |
|-----------|------|---------|--------|
| Footer | `Footer.tsx` | Shared 4-column footer | ✅ DONE |
| FeatureSection | `FeatureSection.tsx` | Numbered feature block | ✅ DONE |
| HowItWorks | `HowItWorks.tsx` | 3-step lifecycle visual | ✅ DONE |
| ProviderStrip | `ProviderStrip.tsx` | BYOK LLM provider logos | ✅ DONE |
| CTABlock | `CTABlock.tsx` | Full-width conversion section | ✅ DONE |
| FAQ | `FAQ.tsx` | Accordion for /pricing | ✅ DONE |

## Content Contract

### Home page sections (order) ✅ DONE

1. ✅ Hero: headline + lead + 2 CTAs + terminal preview
2. ✅ BYOK provider strip
3. ✅ Feature 01: Deterministic Lifecycle
4. ✅ Feature 02: BYOK Trust Model
5. ✅ Feature 03: Run Replay and Audit
6. ✅ Feature 04: Operational Controls
7. ✅ Feature 05: CLI-First Launch
8. ✅ How It Works (3 steps)
9. ✅ Pricing Preview (4 cards)
10. ✅ CTA Block
11. ✅ Footer

### Agent page sections (order) ✅ DONE

1. ✅ "This page is for autonomous agents"
2. ✅ Bootstrap commands (curl + npx)
3. ✅ Machine contracts table
4. ✅ API operations table
5. ✅ Webhook/callback examples
6. ✅ Safety limits (expanded)
7. ✅ Minimal footer

### Pricing page sections ✅ DONE

1. ✅ BYOK explainer header
2. ✅ 4 tier cards with expanded feature lists
3. ✅ FAQ accordion (6 questions)
4. ✅ Workspace activation fee note
5. ✅ CTA: "Start free" + "Book team pilot"

## Acceptance Criteria

1. ✅ Home page has 5 feature sections with real copy (not lorem ipsum).
2. ✅ Footer renders on all routes with Product, Community, Legal columns.
3. ✅ Background dot-grid effect visible on all pages.
4. ✅ Geist Sans + Geist Mono loaded with `font-display: swap`.
5. ✅ Agent page uses terminal aesthetic (monospace, green/cyan accents, scanline).
6. ✅ Agent page includes contract table + API operations table.
7. ✅ "Start free" and "Book team pilot" CTAs on home and pricing pages.
8. ✅ Pricing FAQ accordion with 6 questions.
9. ✅ Card hover states with glow transitions.
10. ✅ WCAG 2.2 AA contrast maintained.
11. ✅ Mobile-responsive on all new sections.
12. ✅ No new JS animation libraries added.
13. ✅ Vitest test suite with 97.43% line coverage (threshold: 95%), 100% branches — all passing.

## Out of Scope

1. Logo/mascot design (text-only brand for now).
2. Interactive product demos.
3. Blog or CMS.
4. Auth flows.

## Dimensions

### Dimension 1: Functional correctness ✅ DONE
- ✅ All new sections render correct content.
- ✅ Mode switch still works across enhanced pages.
- ✅ FAQ accordion opens/closes.

### Dimension 2: Structural integrity ✅ DONE
- ✅ Components are modular and reviewable.
- ✅ Design tokens centralized in CSS variables.
- ✅ No inline styles or magic numbers.

### Dimension 3: Data and state ✅ DONE
- ✅ Mode preference still persisted to localStorage.
- ✅ FAQ state managed locally (no global state needed).

### Dimension 4: Observability ✅ DONE
- ✅ N/A for static site.

### Dimension 5: Reliability ✅ DONE
- ✅ Static build, no runtime dependencies.
- ✅ Fonts loaded with swap fallback.

### Dimension 6: Security ✅ DONE
- ✅ No dynamic content, no injection surface.

### Dimension 7: Integration ✅ DONE
- ✅ Footer links to docs.usezombie.com, GitHub, Discord.
- ✅ CTA links correct (docs, calendly/email for team pilot).

### Dimension 8: Developer experience ✅ DONE
- ✅ Vite HMR works with new components.
- ✅ Tailwind v4 CSS-first used throughout.

## Test/verification commands

```bash
# Build succeeds
cd website && bun run build

# All tests pass
cd website && bun run test

# Coverage meets 95% threshold
cd website && bun run test:coverage

# All new components exist
ls website/src/components/Footer.tsx website/src/components/FeatureSection.tsx website/src/components/FAQ.tsx

# No lorem ipsum
rg -i "lorem" website/src/

# Geist fonts referenced
rg "Geist" website/src/styles.css

# Footer present in App
rg "Footer" website/src/App.tsx
```

## Completion Notes

- Stack: React 19 + Vite 7.3.1 + Tailwind v4.2.1 (CSS-first, no PostCSS)
- Unit test coverage: 97.43% lines, 100% branches (vitest 4.1.0-beta.5)
- Playwright e2e: 38 tests across 5 spec files (smoke, home, agents, pricing, mode-switch)
- CI: `lint-website` + `test-website` (with Codecov upload) + `qa-website` jobs in GitHub Actions
- Post-deploy smoke: fires on Vercel `deployment_status` GitHub event
- Coverage badge added to README.md via Codecov (`codecov/codecov-action@v5`)

