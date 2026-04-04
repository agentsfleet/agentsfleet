# M3_009: Website Positioning And Brand System

**Prototype:** v1.0.0
**Milestone:** M3
**Workstream:** 009
**Date:** Mar 07, 2026
**Status:** DONE
**Priority:** P0 — launch-critical positioning and conversion clarity
**Depends on:** M3_002 (Website Execution Plan), M3_007 (Website Launch-Blocker Routes And CTAs), M3_008 (Website Enhancement)

---

## 1.0 Positioning And Hero Narrative

**Status:** DONE

Homepage messaging was refocused to concrete human-buyer value with route separation preserved (`/` humans-first, `/agents` machine-first).

**Dimensions:**
- 1.1 DONE Replaced generic hero framing with actionable value narrative
- 1.2 DONE Added explicit subhead from queued work to validated PR outcomes
- 1.3 DONE Kept product proof above the fold via CLI command surface and CTA proof
- 1.4 DONE Preserved human/agent route clarity and mode behavior

---

## 2.0 Mascot And Brand Asset System

**Status:** DONE

Initial mascot direction and supporting asset strategy were implemented as a reusable visual system, then adjusted per launch pass (hero image removal for current humans flow while retaining mascot asset for reuse in docs/brand surfaces).

**Dimensions:**
- 2.1 DONE Established undead operator mascot concept with broken-bucket direction
- 2.2 DONE Defined reusable placement intent for docs/marketing usage
- 2.3 DONE Kept stylization distinct enough to avoid direct IP copying
- 2.4 DONE Aligned mascot visual language with terminal/product aesthetic

---

## 3.0 Homepage Feature Architecture

**Status:** DONE

Humans route was rebuilt section-by-section with outcome-led framing and mission-control narrative while agents route retained machine-focused surfaces.

**Dimensions:**
- 3.1 DONE Rewrote feature sections around outcomes, not internal implementation jargon
- 3.2 DONE Added sectioned feature-flow format with credible product surfaces
- 3.3 DONE Aligned copy with GTM/use-case/architecture narratives
- 3.4 DONE Maintained proof-oriented, technical tone

---

## 4.0 Pricing Narrative And Conversion Surfaces

**Status:** DONE

Pricing moved to clearer conversion framing with revised plan narrative and CTA intent capture.

**Dimensions:**
- 4.1 DONE Updated plan narrative and naming for current launch direction (`Hobby`, `Scale`)
- 4.2 DONE Converted bullets to user-benefit framing with visual tick affordances
- 4.3 DONE Kept BYOK/BYOM and runtime/commercial constraints explicit
- 4.4 DONE Removed stale/redundant CTA/footer copy from revised humans flow

---

## 5.0 Acceptance Criteria

**Status:** DONE

- [x] 5.1 Homepage messaging contract implemented with revised hero/subhead/section order
- [x] 5.2 Mascot direction documented and implemented as reusable brand asset
- [x] 5.3 Homepage features rewritten around buyer outcomes
- [x] 5.4 Pricing structure and differentiators updated for current launch pass
- [x] 5.5 Human and agent audience separation preserved
- [x] 5.6 Footer/stale copy cleanup completed

---

## 6.0 Out of Scope

- Final long-term mascot art pack production files
- Full enterprise packaging and outbound campaign system
- Mission Control product UI implementation beyond website CTA routing

---

## 7.0 Verification Evidence

**Status:** DONE

- Lint: `bun run lint` passed
- Unit tests: `bun run test` passed (`163/163`)
- E2E: `bun run test:e2e` passed (`74 passed`)
- Coverage: `bun run test:coverage` passed (overall >95%)
