# M79_001: Simplify the marketing landing — pricing, steps, headings, footer

**Prototype:** v2.0.0
**Milestone:** M79
**Workstream:** 001
**Date:** May 21, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the public marketing landing is the first-touch conversion surface; one flagged item (step card off-screen) is a real content-overflow bug.
**Categories:** UI
**Batch:** B1 — standalone; no parallel workstream shares this surface.
**Branch:** feat/m79-001-landing-simplify-polish
**Depends on:** M78_001 (DONE) — this edits the hero/pricing/footer surface M78 shipped and **supersedes M78 §5's trial-aware billing cards** (Option B removes that grid). No merge-ordering issue — M78 is on main.
**Provenance:** agent-generated (pre-spec) — Indy `/design-consultation`, May 21, 2026 (seven flagged polish issues; Indy chose pricing Option B, remove the replay link, left-rail the headings, build all).

**Canonical architecture:** `docs/DESIGN_SYSTEM.md` (§Layout left-rail / §Type scale / operational-restraint principle). This polish reinforces the documented left-rail identity; it adds no new architecture and reconciles no doc.

---

## Implementing agent — read these first

1. `docs/v2/done/M78_001_P1_UI_LANDING_HERO_ANIMATED_TERMINAL.md` — the immediate predecessor on this exact surface; §4 (footer) and §5 (trial-aware pricing cards) are what M79 trims. Read its Discovery for the `FREE_TRIAL_STAGE_DISPLAY` / `isWithinFreeTrial` provenance.
2. `ui/packages/website/src/components/Pricing.tsx` + `ui/packages/website/src/lib/rates.ts` — the convoluted five-block pricing and the `RATES_DISPLAY` constant source. `RATES_DISPLAY.STAGE_PLATFORM` / `STAGE_SELF_MANAGED` / `EVENT_RATE` are the rate VALUES the new table reuses verbatim.
3. `ui/packages/website/src/styles.css` (`.wrap` = `width:min(100%,1280px); margin-inline:auto`) + `ui/packages/design-system/src/theme.css` (`--container-measure` → `max-w-measure`) — why the FAQ/CTA headings read as centered: `max-w-measure` sits on the `.wrap` div, which is `margin-inline:auto` centered. The fix relocates that token onto the body content.
4. `ui/packages/website/src/components/OnboardingFlow.tsx` — the 4-card `lg:flex-row` + `FlowArrow` that overflows; §2 reshapes it to a responsive grid.
5. `ui/packages/website/src/marketing-spec.test.ts` — pins the Hero pillar tokens (`wake.on.event`, `long-lived runtime`, `replayable log`). Must pass **unchanged**; none of M79's edits touch those tokens.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Simplify landing — pricing, onboarding steps, section headings, footer
- **Intent (one sentence):** A first-time visitor sees a landing that reads simply — one clear pricing story (free now, one honest forward rate), onboarding steps that fit on screen, every section heading on the same left rail, no overpromised "replay" link, and a trimmed footer — without losing the rate values or the marketing pillars.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`; a mismatch against the Intent above → STOP and reconcile before any edit.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — always.
- **RULE UFS** — the new rate-table row labels and the free-trial lead line are semantic literals → named consts (ui/ UFS is manual; the audit skips `ui/`). Rate **values** render from `RATES_DISPLAY` — never inline `$0.001`/`$0.0001`.
- **RULE NDC** — delete the `BILLED_FLOW` and `EXTRAS` arrays and the removed markup cleanly; no commented-out blocks.
- **RULE NLR** — touch-it-fix-it: if `isWithinFreeTrial` / `FREE_TRIAL_STAGE_DISPLAY` (added by M78) orphan once the billing grid is gone, remove them in the same diff; if still referenced elsewhere, leave them.
- **RULE NLG** — pre-2.0 (`VERSION` 0.37.0): no "legacy"/"V2"/"old pricing" framing in code, comments, or copy.
- **RULE ORP** — orphan sweep after `BILLED_FLOW` / `EXTRAS` / the removed FAQ entry / the replay-link tracking call leave the tree.
- **RULE TST-NAM** — no milestone IDs (`M79`, `§3`, `dim 3.1`) in test names, test-file names, or source comments.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| DESIGN TOKEN | **yes** | New rate table + heading-alignment edits use existing token utilities only. Reuse the `max-w-measure` token (just relocated to the body element); no `text-[Npx]`, no `max-w-[...]` arbitrary, no raw hex. |
| UFS | **yes** | Rate-table labels + lead line as named consts; rate values from `RATES_DISPLAY` (see Rules). |
| File & Function Length (≤350 / ≤50 / ≤70) | **yes** | `Pricing.tsx` net-shrinks (three blocks removed). Keep every file ≤350 and the `Pricing` component fn ≤50 — split a `RateRow`/`RateTable` helper if it nears the cap. |
| MILESTONE-ID | **yes** | No `M79`/`§`/`dim` tokens in any source or test identifier or body (RULE TST-NAM). |
| UI Substitution | **no** | Gate scope is `ui/packages/app/` only; the marketing website is exempt (it already composes raw `<section>`/`<p>`/`<a>`). We still reuse design-system primitives (`Card`, `Button`, `List`, `Badge`, `DisplayLG`) where they exist. |
| PUB / Struct-Shape | **no** | No `*.zig` under `src/`; website TS only, no new public Zig surface. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA / ZIG | **no** | None of those surfaces touched. |
| Architecture Consult & Update | **minor** | M79 supersedes M78 §5's trial-aware billing cards. Not a `docs/architecture/` change; `DESIGN_SYSTEM.md` pins type/color/motion/layout, not the pricing billing grid. Recorded in Discovery as a deliberate supersede. |

---

## Overview

**Goal (testable):** The pricing section renders the free-trial lead line plus a clean three-row rate table (event = free, reasoning stage = `RATES_DISPLAY.STAGE_PLATFORM` with self-managed `RATES_DISPLAY.STAGE_SELF_MANAGED`, model tokens = your provider) and contains no struck-through dual-rate line, no EVENT→STAGE-N billing grid, and no "operational extras" list; the onboarding steps render in a responsive grid with no horizontal overflow at desktop width; the FAQ and closing-CTA headings sit on the same left rail as the hero; the hero shows no "view a real wake (replay)" link; and the footer tagline ends at "…own the outcome." — each asserted by a unit or e2e test, with `marketing-spec.test.ts` passing unchanged.

**Problem (user-facing):** the pricing section is five stacked blocks (struck dual rates, a billing-flow grid, a dashed "underneath every stage" box, prose, and an extras list) that bury the simple truth ("it's free right now"); the fourth onboarding step ("Steer your zombie") is clipped off the right edge of the viewport; the FAQ and closing headings float in a centered narrow column and read as centered against the otherwise hard-left page; the hero's "view a real wake (replay)" link overpromises a replay artifact that does not exist; the footer tagline carries "Self-managed. Open source." that Indy wants pulled for now.

**Solution summary:** Remove the hero replay link. Reshape the onboarding steps into a responsive grid (1 col → 2×2) and drop the inline arrow connectors so the row can no longer overflow. Replace the pricing body with Option B (free-trial lead + three-row rate table, rate values from `RATES_DISPLAY`), deleting the billing grid, the extras list, and the now-orphaned extras FAQ entry. Relocate `max-w-measure` off the FAQ/CTA `.wrap` onto their body content so the headings align to the left rail while prose keeps its reading measure. Trim the footer tagline. Sweep any helper that orphans.

---

## Prior-Art / Reference Implementations

- **UI** → existing website components + design-system primitives + `theme.css` tokens. The new rate table mirrors the existing `Card` + `SectionLabel` + token-class composition already in `Pricing.tsx`; no new component type. **Alignment:** reuse `RATES_DISPLAY`, `Badge`, `Button`, `Card`, `List`. **Divergence:** none — this is a subtractive/rearranging change within the established system.
- **Heading alignment** → the hero/pricing/onboarding sections already place `DisplayXL`/`DisplayLG` directly inside the full-width `.wrap`. §4 makes FAQ + CTA match that exact pattern (heading at the `.wrap` left edge; reading-measure applied to the body block only).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/website/src/components/Hero.tsx` | EDIT | Remove the "→ view a real wake (replay)" ghost link and its `trackNavigationClicked` call; the copy-row + terminal remain. |
| `ui/packages/website/src/components/Hero.test.tsx` | EDIT | Drop the `/view a real wake/i` assertion; assert the replay link is absent and the install copy-row + terminal still render. |
| `ui/packages/website/src/components/OnboardingFlow.tsx` | EDIT | Responsive grid (1 col → 2×2), remove `FlowArrow`, add `min-w-0` so terminals scroll internally instead of forcing row width. |
| `ui/packages/website/src/components/OnboardingFlow.test.tsx` | EDIT | Assert grid container + 4 step cards + no arrow connectors; all four step headings present. |
| `ui/packages/website/src/components/Pricing.tsx` | EDIT | Option B body: free-trial lead + three-row rate table; delete `BILLED_FLOW` grid, the dashed LLM box, `EXTRAS`, and the struck dual-rate line. Preserve the rate `data-testid`s. |
| `ui/packages/website/src/components/Pricing.test.tsx` | EDIT | Re-assert the simple table; remove the billing-grid + operational-extras assertions; keep the rate-value-from-constant assertions. |
| `ui/packages/website/src/lib/rates.ts` | EDIT (conditional) | Remove `isWithinFreeTrial` / `FREE_TRIAL_STAGE_DISPLAY` **only if** the orphan grep is clean. Rate VALUE constants and their cross-file sync are untouched. |
| `ui/packages/website/src/components/FAQ.tsx` | EDIT | Remove the "What does 'extras provisioned per workspace' mean?" entry; move `max-w-measure` off the `.wrap` onto the `Accordion` so the heading sits on the left rail. |
| `ui/packages/website/src/components/FAQ.test.tsx` | EDIT | Drop any assertion of the removed extras entry; assert the heading is not inside the measure-constrained element. |
| `ui/packages/website/src/components/CTABlock.tsx` | EDIT | Move `max-w-measure` off the `.wrap` onto the prose+button block so the heading sits on the left rail. |
| `ui/packages/website/src/components/CTABlock.test.tsx` | EDIT | Assert the heading is on the full-width rail (not inside the measure element); heading text unchanged. |
| `ui/packages/website/src/components/Footer.tsx` | EDIT | Trim the tagline to end at "…and own the outcome." (drop "Self-managed. Open source."). |
| `ui/packages/website/src/components/Footer.test.tsx` | EDIT | Update the tagline assertion: no "Self-managed. Open source."; the base sentence remains. |
| `ui/packages/website/tests/e2e/home.spec.ts` | EDIT | Add a desktop-width assertion that step 4 ("Steer your zombie") is within the viewport and the page has no horizontal overflow. |

> **Untouched on purpose:** `lib/rates.ts` rate VALUE constants + their `tenant_billing.zig` / `rates.mdx` sync (display-only change); `marketing-spec.test.ts` (pillar tokens unaffected); `Home.tsx` "Self-managed key"/"Open source" feature cards (Indy scoped the removal to the footer); `Agents.tsx`.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five subtractive/rearranging slices — hero link, onboarding grid, pricing, heading alignment, footer — each independently testable and shippable.
- **Alternatives considered:** (a) pricing Option A (drop the platform-vs-self-managed gradient entirely) — rejected by Indy, who wants the gradient kept but presented simply; (b) pricing Option C (keep both cards, light declutter) — rejected as not simple enough; (c) intentionally centering the FAQ/CTA headings — rejected: it would fight the terminal left-rail identity.
- **Patch-vs-refactor verdict:** **patch** — edits website copy/markup and relocates one utility class; removes M78 additions cleanly. No architecture rewrite, no new component, no schema/API surface.

---

## Sections (implementation slices)

### §1 — Remove the hero replay link (issue 1)
The "→ view a real wake (replay)" ghost link routes to `/agents` and promises a replay artifact that does not exist; remove it and its tracking call. The install copy-row stays the single primary action.

- **Dimension 1.1** — the hero renders no replay link/text; the install copy-row and animated terminal still render → Test `test_hero_has_no_replay_link`

### §2 — Onboarding steps responsive grid (issue 2)
Replace the single non-wrapping `lg:flex-row` of 4 cards + 3 `FlowArrow`s with a responsive grid (1 col mobile, 2×2 from the medium breakpoint up), and add `min-w-0` to the cards so a wide `Terminal` command scrolls inside its card instead of forcing the row past the `.wrap`. Drop `FlowArrow`. **Implementation default:** 2×2 grid (not a 4-wide row, which re-creates the overflow on mid widths) because four cards each holding a terminal need the width.

- **Dimension 2.1** — steps render in a grid container with all 4 step cards and no arrow connectors; "Steer your zombie" heading present → Test `test_onboarding_steps_render_as_grid`
- **Dimension 2.2** — at desktop width the page has no horizontal overflow and step 4 is within the viewport → e2e `home page steps fit without horizontal scroll`

### §3 — Pricing Option B + remove operational extras (issues 3, 4)
Replace the pricing body with: the free-trial lead line; a three-row rate table (event receipt = free; reasoning stage = `STAGE_PLATFORM`, self-managed `STAGE_SELF_MANAGED`; model tokens = your provider/your bill); the "a stage is one reasoning step…" line; the get-early-access CTA; the design-partner email note. Delete the struck dual-rate line, the EVENT→STAGE-N `BILLED_FLOW` grid, the dashed "underneath every stage" box, and the `EXTRAS` list. Preserve `data-testid` `pricing-rate-event` / `pricing-rate-stage-platform` / `pricing-rate-stage-self-managed` on the new table so `smoke.spec.ts` + `Home.test.tsx` rate assertions keep passing. **Implementation default:** render rate values straight from `RATES_DISPLAY` (no trial toggling) because Option B states "free now + future rate" declaratively, which removes the runtime trial branch.

- **Dimension 3.1** — pricing shows the free-trial lead + a three-row rate table; no struck dual-rate line and no billing-flow grid in the DOM → Test `test_pricing_shows_simple_rate_table`
- **Dimension 3.2** — the "operational extras" section and its list are absent → Test `test_pricing_has_no_operational_extras`
- **Dimension 3.3** — the displayed stage rates equal `RATES_DISPLAY.STAGE_PLATFORM` and `RATES_DISPLAY.STAGE_SELF_MANAGED` (proves display-only; constants intact) → Test `test_pricing_rate_values_come_from_constants`

### §4 — Left-rail headings + drop orphaned extras FAQ entry (issues 5, 4)
Move `max-w-measure` off the `.wrap` div in `FAQ.tsx` and `CTABlock.tsx` onto the body content (the `Accordion`; the prose+button block), so the `DisplayLG` headings align to the page left rail while the answers/prose keep their reading measure, left-anchored. Remove the now-orphaned "What does 'extras provisioned per workspace' mean?" FAQ entry (its subject was deleted in §3).

- **Dimension 4.1** — the FAQ "Common questions" heading is a direct child of the full-width wrap, not inside the measure-constrained element → Test `test_faq_heading_on_left_rail`
- **Dimension 4.2** — the closing-CTA "Stop chasing failed deploys." heading is on the full-width rail, not inside the measure element → Test `test_cta_heading_on_left_rail`
- **Dimension 4.3** — the extras FAQ entry is absent; the remaining entries render → Test `test_faq_has_no_extras_entry`

### §5 — Trim the footer tagline (issue 6)
Drop "Self-managed. Open source." from the footer tagline, leaving "Durable, markdown-defined agents that wake on your events and own the outcome." The product/community/legal columns are unchanged.

- **Dimension 5.1** — the footer tagline contains the base sentence and not "Self-managed. Open source." → Test `test_footer_tagline_trimmed`

---

## Interfaces

```
Pricing rate table (data-testids preserved so existing assertions hold):
  pricing-rate-event           -> RATES_DISPLAY.EVENT_RATE        (free)
  pricing-rate-stage-platform  -> RATES_DISPLAY.STAGE_PLATFORM    ($0.001)
  pricing-rate-stage-self-managed -> RATES_DISPLAY.STAGE_SELF_MANAGED ($0.0001)

lib/rates.ts:
  RATES_DISPLAY.* rate VALUE constants  — UNCHANGED (changelog-pinned across tenant_billing.zig / rates.ts / rates.mdx).
  isWithinFreeTrial / FREE_TRIAL_STAGE_DISPLAY — removed IFF orphaned after §3 (else left intact).
```

Contract: this PR changes pricing **display** only. No rate value moves; no testid that an existing smoke/Home assertion depends on is dropped.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Narrow viewport (mobile) | small screen | onboarding grid stacks to 1 column; no horizontal overflow; all 4 steps stacked and readable. |
| Wide terminal command in a step | long `gh api` command | the `Terminal` scrolls inside its `min-w-0` card; the card does not force the grid wider than `.wrap`. |
| Rate value drift | someone hardcodes `$0.001` in markup | UFS + `test_pricing_rate_values_come_from_constants` fail; values must come from `RATES_DISPLAY`. |
| Stale testid | rate testid renamed during the rewrite | `smoke.spec.ts` / `Home.test.tsx` rate assertions fail; the testids are pinned in Interfaces. |
| SSR / no-JS snapshot | static render | all pricing copy, steps, headings, and footer text are present in the DOM (static components; no client gating). |

---

## Invariants

1. Pricing rate values render from `RATES_DISPLAY` constants, never hardcoded — enforced by `test_pricing_rate_values_come_from_constants` + UFS (no inline rate literal).
2. The Hero pillar tokens (`wake.on.event`, `long-lived runtime`, `replayable log`) remain — enforced by `marketing-spec.test.ts` passing unchanged.
3. The `DisplayLG` section headings in FAQ + CTA are not descendants of the `max-w-measure` element — enforced by `test_faq_heading_on_left_rail` / `test_cta_heading_on_left_rail`.
4. The rate `data-testid`s in Interfaces survive the pricing rewrite — enforced by `smoke.spec.ts` + `Home.test.tsx` (run unchanged).
5. No milestone IDs in test/source identifiers — RULE TST-NAM (audited).

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_hero_has_no_replay_link` | rendered Hero: no element with text matching `/view a real wake/i`; install copy-row + terminal present |
| 2.1 | unit | `test_onboarding_steps_render_as_grid` | rendered OnboardingFlow: grid container, 4 step cards, zero `onboarding-flow-arrow`; "Steer your zombie" heading present |
| 2.2 | e2e | `home page steps fit without horizontal scroll` | at 1280px width, `document.documentElement.scrollWidth <= clientWidth`; step 4 card in viewport (Playwright) |
| 3.1 | unit | `test_pricing_shows_simple_rate_table` | rendered Pricing: free-trial lead + three rate rows; no `pricing-flow-billed` grid, no struck dual-rate node |
| 3.2 | unit | `test_pricing_has_no_operational_extras` | rendered Pricing: no `pricing-extras` list; "operational extras" text absent |
| 3.3 | unit | `test_pricing_rate_values_come_from_constants` | `pricing-rate-stage-platform` = `RATES_DISPLAY.STAGE_PLATFORM`; `…self-managed` = `RATES_DISPLAY.STAGE_SELF_MANAGED` |
| 4.1 | unit | `test_faq_heading_on_left_rail` | the "Common questions" heading's ancestor chain has no `max-w-measure`; the `Accordion` wrapper does |
| 4.2 | unit | `test_cta_heading_on_left_rail` | the "Stop chasing failed deploys." heading is outside the `max-w-measure` element; prose/buttons inside it |
| 4.3 | unit | `test_faq_has_no_extras_entry` | no FAQ trigger matching `/extras provisioned per workspace/i`; other entries still render |
| 5.1 | unit | `test_footer_tagline_trimmed` | footer tagline contains "own the outcome." and not `/Self-managed\. Open source\./` |

**Regression:** existing Hero/OnboardingFlow/Pricing/FAQ/CTABlock/Footer/Home unit tests + `smoke.spec.ts` are updated where copy/structure changed and otherwise pass unchanged; `marketing-spec.test.ts` and `marketing-no-pr-validator-framing.test.ts` pass **unchanged**. **Idempotency:** N/A (static presentational components).

---

## Acceptance Criteria

- [ ] Hero has no replay link — verify: `vitest run Hero`
- [ ] Onboarding steps grid; no desktop horizontal overflow — verify: `vitest run OnboardingFlow` + e2e home spec
- [ ] Pricing = free-trial lead + 3-row table; no billing grid, no extras — verify: `vitest run Pricing`
- [ ] Rate values come from `RATES_DISPLAY`; testids preserved — verify: `vitest run Pricing smoke`
- [ ] FAQ + CTA headings on the left rail; extras FAQ entry gone — verify: `vitest run FAQ CTABlock`
- [ ] Footer tagline trimmed — verify: `vitest run Footer`
- [ ] `marketing-spec.test.ts` passes unchanged — verify: `vitest run marketing-spec`
- [ ] `make lint` clean · website `make test` passes · e2e home spec passes
- [ ] `gitleaks detect` clean · no file over 350 lines added · design-token audit clean

---

## Eval Commands (post-implementation)

```bash
# E1: website unit tests
(cd ui/packages/website && bun run test) && echo PASS || echo FAIL
# E2: lint
make lint 2>&1 | grep -E "✓|FAIL"
# E3: design-token audit (no arbitraries where a token exists)
bash scripts/audit-design-tokens.sh 2>&1 | tail -5
# E4: marketing invariants untouched + green
git diff --name-only origin/main | grep -q marketing-spec.test.ts && echo "TOUCHED-INVESTIGATE" || echo "untouched OK"
# E5: rate constants untouched (display-only change)
git diff origin/main -- ui/packages/website/src/lib/rates.ts | grep -E '^\+' | grep -E '0\.001|0\.0001|FREE_TRIAL_END' && echo "RATE-VALUE-CHANGED-INVESTIGATE" || echo "rate values untouched OK"
# E6: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E7: gitleaks
gitleaks detect 2>&1 | tail -3
# E8: orphan sweep — removed arrays/helpers gone from the tree
grep -rn "BILLED_FLOW\|EXTRAS\b\|operational extras" ui/packages/website/src | head
grep -rn "isWithinFreeTrial\|FREE_TRIAL_STAGE_DISPLAY" ui/packages/website/src | head
```

---

## Dead Code Sweep

**1. Orphaned files** — N/A — no files deleted; this is in-file removal of arrays/markup.

**2. Orphaned references**

| Removed | Grep | Expected |
|---------|------|----------|
| `BILLED_FLOW` (pricing billing grid) | `grep -rn "BILLED_FLOW" ui/packages/website/src` | 0 matches |
| `EXTRAS` (operational extras) | `grep -rn "EXTRAS" ui/packages/website/src` | 0 matches |
| replay-link tracking source | `grep -rn "hero_secondary_replay" ui/packages/website/src` | 0 matches |
| `isWithinFreeTrial` / `FREE_TRIAL_STAGE_DISPLAY` | `grep -rn "isWithinFreeTrial\|FREE_TRIAL_STAGE_DISPLAY" ui/packages/website/src` | 0 → remove from `rates.ts` (RULE NDC); ≥1 → leave intact |

---

## Discovery (consult log)

> Empty at creation. Append consults, skill outcomes, Indy-acked deferrals as work proceeds.

- **Design consultation (pre-recorded), May 21, 2026:** Indy ran `/design-consultation` on the live landing and flagged seven items. Decisions captured: pricing → **Option B** (free-trial lead + clean three-row rate table; keep the platform-vs-self-managed gradient, drop the struck rates / billing grid / extras); hero replay link → **remove**; FAQ + CTA headings → **left-align to the page rail** (Indy deferred to design judgment; rationale: the whole site is a single left rail / terminal identity, centering one heading reads as an accident, centering everything fights the identity); scope → **build all seven now** as this milestone.
- **Supersede note:** §3 removes the trial-aware billing grid that M78_001 §5 shipped. Deliberate, at Indy's request (Option B). `FREE_TRIAL_STAGE_DISPLAY` / `isWithinFreeTrial` were M78 additions; swept here iff orphaned.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Clean; iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Clean OR every finding dispositioned (fixed / deferred-with-quote / rejected-with-reason). |
| After `gh pr create` | `/review-pr` | Comments addressed (fixup/amend) before human review. |
| After every push | `kishore-babysit-prs` | Greptile reviews walked + triaged; final report in Discovery. |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Website unit | `make test-unit-website` | {paste} | |
| Lint | `make lint-website` | {paste} | |
| Design-token audit | `scripts/audit-design-tokens.sh` | {paste} | |
| marketing-spec untouched | `git diff --name-only origin/main \| grep marketing-spec` | {paste} | |
| Rate values untouched | Eval E5 | {paste} | |
| Live render (no overflow) | headless Chromium `localhost:5173` | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |
| Playwright e2e | `make _e2e` (home spec) | {paste} | |

---

## Out of Scope

- `Home.tsx` "Self-managed key" / "Open source" feature cards — Indy scoped the removal to the footer only.
- Rate VALUE changes or the `tenant_billing.zig` / `rates.mdx` rate sync — this is display-only.
- `Agents.tsx` machine-surface copy — unchanged.
- Any new design-system component or animation — none needed; this is subtractive/rearranging within the existing system.
