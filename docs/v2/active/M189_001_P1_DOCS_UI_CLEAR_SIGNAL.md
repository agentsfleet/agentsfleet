<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M189_001: Clear typography and flat surfaces across agentsfleet

**Prototype:** v2.0.0
**Milestone:** M189
**Workstream:** 001
**Date:** Sep 05, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — readable product surfaces and accurate launch copy
**Categories:** DOCS, UI
**Batch:** B1 — independent of the outbound repair
**Branch:** feat/m189-clear-signal
**Test Baseline:** User directs full unit and integration suites to run only immediately before the PR. No passing baseline recorded.
**Depends on:** none for visual work; connector acceptance remains with its existing workstream
**Provenance:** LLM-drafted from the user's approved design proposal
**Canonical architecture:** `docs/DESIGN_SYSTEM.md`; `docs/architecture/billing_and_provider_keys.md`

## Overview

**Goal (testable):** App and website render readable sans typography, flat surfaces, mint actions, and accurate pricing in both themes.
**Problem:** Small mono controls and ambient gradients weaken hierarchy. Website copy promises free runs despite metered billing.
**Solution summary:** Update shared tokens and components, migrate visual consumers, and rebuild website sections around product work and evidence.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(ui): clarify typography and refresh the product website
- **Intent:** Make agentsfleet easier to read, navigate, and understand before launch.
- **Handshake:** Preserve mint, remove gradients, establish shared typography, and apply the design across app and website.
- **ASSUMPTIONS I'M MAKING:** Dark remains primary. Light receives equal accessibility checks. Billing policy and backend behavior stay unchanged.
- **Golden path:** Website explains fleets and costs. Existing links open signup or documentation. Authenticated users navigate existing workspace screens with shared components.

## Implementing agent — read these first

1. `docs/DESIGN_SYSTEM.md` — existing decisions and the approved replacement surface.
2. `ui/packages/design-system/src/tokens.css` — tokens and bundled fonts.
3. `ui/packages/design-system/src/theme.css` — utility mappings.
4. `ui/packages/website/src/lib/marketing-copy.ts` — copy and fleet availability.
5. `docs/architecture/billing_and_provider_keys.md` — billing semantics; runtime constants resolve conflicting prose.

## Files Changed (blast radius)

The user approved all three UI packages. Rows name grouped visual consumers; exact changed paths are recorded before review.

| File | Action | Why |
|------|--------|-----|
| `docs/DESIGN_SYSTEM.md` | EDIT | Define Clear Signal, typography, flat surfaces, and component rules |
| `docs/v2/pending/M189_001_P1_DOCS_UI_CLEAR_SIGNAL.md` | MOVE | Track lifecycle and verification |
| `ui/packages/design-system/src/tokens.css` | EDIT | Color, typography, spacing, and motion values |
| `ui/packages/design-system/src/theme.css` | EDIT | Forward new font roles and token values |
| `ui/packages/design-system/package.json` | EDIT | Bundle display font locally |
| `bun.lock` | EDIT | Record font dependency |
| `ui/packages/design-system/src/design-system/*.tsx` | EDIT | Shared typography and interaction presentation |
| `ui/packages/design-system/src/design-system/*.test.tsx` | EDIT | Keep component behavior covered |
| `ui/packages/design-system/src/tokens.css.test.ts` | EDIT | Font roles, contrast, and token mapping regressions |
| `ui/packages/app/app/globals.css` | EDIT | Remove ambient gradients and meter gradients |
| `ui/packages/app/components/*.tsx` | EDIT | App shell and navigation typography |
| `ui/packages/app/app/**/*.tsx` | EDIT | Migrate visual consumers without changing data access |
| `ui/packages/app/lib/account-avatar.ts` | EDIT | Flat account identity if this module owns the gradient |
| `ui/packages/app/tests/*` | EDIT | Update visual assertions and preserve existing behavior |
| `ui/packages/website/src/styles.css` | EDIT | Flat editorial layouts and responsive composition |
| `ui/packages/website/src/components/*.tsx` | EDIT | Product hero, capabilities, pricing, and calls to action |
| `ui/packages/website/src/pages/*.tsx` | EDIT | Home composition, fleet catalog, and design gallery |
| `ui/packages/website/src/lib/marketing-copy.ts` | EDIT | Explain current product and remove free-run promises |
| `ui/packages/website/src/lib/rates.ts` | EDIT | Describe starter allowance without changing rates |
| `ui/packages/website/src/App.tsx` | EDIT | Navigation presentation |
| `ui/packages/website/tests/e2e/*.spec.ts` | EDIT | Responsive and accessibility regression scenarios |

## Applicable Rules

- `docs/greptile-learnings/RULES.md`: UFS, NDC, NLR, NLG, ORP, TCF, DID, NCC, TWS, and GRD.
- `dispatch/write_ts_adhere_bun.md`: reusable primitives, typed props, and named utilities.
- `dispatch/write_any.md`: bounded files, no stale consumers, and named literals.
- `docs/DOCUMENTATION_RULES.md`: DOC-02, DOC-05, DOC-06, DOC-11, and DOC-14b.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UI | yes | Compose existing primitives |
| DESIGN TOKEN | yes | Define each new role in tokens and forward through theme |
| UFS | yes | Name repeated semantic values |
| LENGTH | yes | Keep source files below 350 lines and functions below their limits |
| MILESTONE ID | yes | Keep identifiers in this spec only |
| GREPTILE | yes | Review CSS mappings, consumers, and billing claims |
| LOGGING / ERROR / SCHEMA | no | No changes to these surfaces |

## Prior-Art / Reference Implementations

- Existing shared components and the website design gallery supply composition and interactions.
- The supplied Morph billing screenshot supplies hierarchy references. Its colors and layouts are not copied.
- `https://www.morphllm.com/pricing` supplies the principle of explicit billing units.

## Sections (implementation slices)

### §1 — Shared visual foundation

Publish the visual rules and their implementation together.

- **Dimension 1.1** — Both themes expose readable foregrounds on supported surfaces → Test `theme contrast pairs`.
- **Dimension 1.2** — Display, UI, and code font roles resolve independently → Test `font role mappings`.
- **Dimension 1.3** — Shared controls preserve keyboard, disabled, and error behavior → Test existing component suites.

### §2 — App clarity

Apply shared typography and flat surfaces to existing authenticated screens.

- **Dimension 2.1** — App backgrounds and usage meters contain no gradients → Test `flat app surfaces`.
- **Dimension 2.2** — Navigation and forms remain usable on narrow screens → Test browser keyboard and responsive walkthrough.
- **Dimension 2.3** — Empty, loading, and error states retain visible next actions → Test existing app state suites.

### §3 — Website and pricing

Explain recurring work with a visible product example and explicit costs.

- **Dimension 3.1** — Home presents product, working example, fleets, controls, and pricing → Test home section walkthrough.
- **Dimension 3.2** — Pricing distinguishes starter credit, runtime, and model costs → Test pricing content assertions.
- **Dimension 3.3** — Theme switching and mobile navigation preserve readable content → Test website end-to-end smoke.

## Interfaces

No API, CLI, billing rates, authorization, or data shapes change.
Component props remain stable. Font roles may add named utilities.
Existing navigation links and analytics events keep their meaning.

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Unreadable muted text | Surface and text colors conflict | Contrast assertions cover every supported pair |
| Missing font | Bundled font request fails | Fallback preserves readable content and layout |
| Mobile clipping | Long names or narrow viewport | Wrapping and overflow rules preserve actions |
| Reduced-motion mismatch | Animation preferences ignored | Browser check requires static state under reduced motion |
| Misleading billing | Old free-run copy survives | Content assertions reject unbounded free-run promises |
| Keyboard regression | Restyled control hides focus | Existing behavior tests and browser walkthrough verify focus |

## Invariants

1. Token forwards never reference themselves; token tests enforce this.
2. App and website decoration uses no gradient functions; source checks enforce this.
3. UI changes preserve existing disabled and accessible control behavior; component suites enforce this.
4. Pricing continues to use canonical rate constants; pricing tests enforce this.

## Metrics & Observability

No product or operator signals are added. Existing navigation and signup events remain attached to their actions.
No analytics or funnel playbook update is required because action meanings remain unchanged.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | unit | theme contrast pairs | Muted and primary text remain readable on all intended surfaces |
| 1.2 | unit | font role mappings | UI, display, and mono utilities resolve without cycles |
| 1.3 | unit | existing shared component suites | Disabled controls refuse activation; keyboard interactions work |
| 2.1 | unit | flat app surfaces | Gradient functions are absent from visual source |
| 2.2 | e2e | browser responsive walkthrough | Long content does not hide navigation or actions |
| 2.3 | unit | existing app state suites | Empty and failed requests retain their recovery UI |
| 3.1 | e2e | home section walkthrough | Product story and links appear in logical order |
| 3.2 | unit | pricing content assertions | Free-run promises are absent; rates come from constants |
| 3.3 | e2e | website smoke | Light, dark, mobile, and reduced-motion states remain usable |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|-----------|---------------------|----------|----------|-----------------|
| R1 | Conform | `make harness-verify` | exit 0 | P0 | |
| R2 | Lint | `make lint-all` | exit 0 | P0 | |
| R3 | Unit behavior | `make test-unit-all` | exit 0 | P0 | |
| R4 | Integration regression | `make test-integration-rustd` | exit 0 | P0 | |
| R5 | Version consistency | `make check-version` | exit 0 | P0 | |
| R6 | Browser smoke | `make qa-smoke` | exit 0 | P0 | |
| R7 | No secrets | `gitleaks detect` | exit 0 | P0 | |

## Dead Code Sweep

Remove ambient-glow styles and their class references together.
Replace gradient meters and avatar fills at their owner and update visual tests.
Remove superseded website composition and its unused imports.

## Out of Scope

- Backend behavior, connector acceptance, billing policy, and new payment features.
- Merge, deployment, and changes to the other agent's worktree.
- CLI styling; web typography changes do not alter terminal output.

## Product Clarity (authoring record)

1. **Successful user moment:** A user identifies live work, its next action, and its cost without decoding tiny labels.
2. **Preserved user behaviour:** Navigation, forms, approvals, authentication, and billing data retain their current meaning.
3. **Optimal-way check:** Shared tokens and primitives distribute visual changes consistently.
4. **Rebuild-vs-iterate:** Refactor presentation; retain working data and interaction code.
5. **What we build:** A design guide, tokens, shared components, app styling, and a refreshed website.
6. **What we do NOT build:** New backend capabilities or payment controls.
7. **Fit with existing features:** Fleet activity, approvals, and costs gain hierarchy without changed semantics.
8. **Surface order:** Shared foundation, app, then website; the request explicitly targets visual surfaces.
9. **Dashboard restraint:** Show actions supported by current data and handlers.
10. **Confused-user next step:** Visible helper text, existing documentation links, and recovery controls.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** One visual workstream with three independently checked Sections.
- **Alternatives considered:** Token-only editing would leave explicit mono styles and gradients in consumers.
- **Patch-vs-refactor verdict:** Presentation refactor because typography and hierarchy span shared components and both applications.

## Discovery (consult log)

- **Consults:** User approved Clear Signal and a separate worktree. M186 remains with another agent, per the user's instruction.
  > Indy (2026-09-05): "dont run these make test-integration, test-unit-all until you are about to send the PR"
  This instruction replaces the baseline cadence. Focused UI checks run during implementation.
- **Metrics review:** Existing analytics meanings remain unchanged.
- **Skill-chain outcomes:** Pending implementation and verification.
- **Deferrals:** None.
