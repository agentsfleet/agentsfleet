# M4_006: Marketing + App Design System Unification (shadcn)

**Prototype:** v1.0.0
**Milestone:** M4
**Workstream:** 006
**Date:** Mar 08, 2026
**Status:** ✅ DONE
**Priority:** P0 — v1 release consistency gate
**Depends on:** M3_007_WEBSITE_LAUNCH_BLOCKERS.md, M3_009_WEBSITE_POSITIONING_AND_BRAND_SYSTEM.md

---

## 0.0 Prompt Folder Structure (LLM Execution Contract)

**Status:** DONE

Specification prompt artifacts for this workstream must live under one deterministic folder so execution prompts, baselines, and evidence are reviewable.

**Dimensions:**
- 0.1 ✅ DONE Keep canonical spec at `docs/done/v1/M4_006_MARKETING_SHADCN_UNIFICATION.md`
- 0.2 ✅ DONE Create prompt pack root at `docs/done/v1/prompts/M4_006_MARKETING_SHADCN_UNIFICATION/`
- 0.3 DONE Store prompt files as ordered steps (`01_*.md`, `02_*.md`, ...) so execution sequence is deterministic
- 0.4 DONE Store visual baseline references and Playwright evidence under `docs/done/v1/prompts/M4_006_MARKETING_SHADCN_UNIFICATION/evidence/`

---

## 1.0 Shared Design System Foundation (`ui/packages/website` + `ui/packages/app`)

**Status:** DONE

Unify `ui/packages/website` and `ui/packages/app` to one shadcn-style system with one source of truth for tokens and component primitives.

**Dimensions:**
- 1.1 DONE Create shared UI package at `ui/packages/design-system` and consume from both `ui/packages/website` and `ui/packages/app`
- 1.2 DONE Standardize shared component primitives and variants (button, card, terminal, grid, section, install block)
- 1.3 DONE Remove duplicated website-local design-system implementation and consume shared package only
- 1.4 DONE Verify desktop/mobile parity for shared components in both surfaces

---

## 2.0 Token Contract (Recommendation Locked)

**Status:** DONE

Use one token source that produces both runtime CSS variables and typed TS exports for component logic.

**Dimensions:**
- 2.1 DONE Define canonical token schema (colors, spacing, radius, typography, motion)
- 2.2 DONE Emit CSS variables for styling (`:root` + theme scopes) from shared package `@usezombie/design-system/tokens.css`
- 2.3 DONE Emit TS token exports for component usage/tests from the same schema
- 2.4 DONE Validate no hand-maintained token drift between CSS and TS outputs

---

## 3.0 Migration Strategy (Design System First)

**Status:** DONE

Build/lock shared design system first, then migrate surfaces to it (not page-by-page ad-hoc cloning).

**Dimensions:**
- 3.1 DONE Migrate by component category first (button/card/terminal/grid/section/install block), then update pages
- 3.2 DONE Apply shared typography scale and semantic color usage across homepage, pricing, login-entry CTAs, and dashboard shell touchpoints
- 3.3 DONE Replace legacy ad-hoc CTA/button/link styles with shared variants only
- 3.4 DONE Keep existing functionality and routes unchanged while refactoring visuals/system code

---

## 4.0 CTA Contract: `Login` -> `Mission Control`

**Status:** DONE

After shared system migration, enforce CTA naming and style consistency for app-entry actions on marketing.

**Dimensions:**
- 4.1 DONE Rename marketing app-entry `Login` label to `Mission Control`
- 4.2 DONE Use shared orange primary button variant for `Mission Control` CTA
- 4.3 DONE Pass accessibility contrast and visible focus-state checks for the CTA variant
- 4.4 DONE Keep route target behavior unchanged unless explicitly specified in this spec

---

## 5.0 Visual Verification And Playwright Gate

**Status:** DONE

Capture current visual baseline first, then assert post-migration parity/intentional deltas with Playwright screenshots and CI pass.

**Dimensions:**
- 5.1 DONE Capture baseline screenshots before migration for key routes/components in `ui/packages/website` and `ui/packages/app`
- 5.2 DONE Add/extend Playwright visual assertions for key shared UI states (default/hover/focus/mobile)
- 5.3 DONE Store before/after evidence and diff artifacts in this workstream prompt evidence folder
- 5.4 DONE Gate completion on passing Playwright suite in CI/local (`make qa-smoke` minimum, `make qa` preferred)

---

## 6.0 Acceptance Criteria

**Status:** DONE

- [x] 6.1 `ui/packages/website` and `ui/packages/app` consume one shared shadcn-style UI source (`ui/packages/design-system`)
- [x] 6.2 Token contract is single-source and outputs both CSS vars and TS exports
- [x] 6.3 Marketing `Mission Control` CTA is live with shared orange primary style and accessible focus/contrast
- [x] 6.4 Playwright visual and smoke coverage passes with evidence captured for baseline vs migrated UI
- [x] 6.5 Existing behavior still works after migration (no route/functionality regressions)

---

## 7.0 Out of Scope

- None. This is a v1 working scope and must ship fully working.
