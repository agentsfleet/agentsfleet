<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.-->

# M227_002: Finish the carried website, PostHog, and branding batch

**Prototype:** v2.0.0
**Milestone:** M227
**Workstream:** 002
**Date:** Oct 14, 2026
**Status:** DONE
**Priority:** P1 — the carried website/branding work has scoped completion proof for the cutover PR
**Categories:** API, CLI, DOCS, UI
**Batch:** B1 — same product PR as M227_001
**Folded-into:** `M227_001`
**Branch:** feat/website-completion
**Baseline revision:** ad3ed8f92c739b59df71863dbeec81d621dd7d0a
**Test Baseline:** repository comparison remains in M227_001; scoped interface evidence is recorded in the completion review
**Baseline evidence:** `docs/reviews/workspace-setup/ui-completion-review.md`; exact package/browser command summaries and logs
**Depends on:** combined release remains gated by M227_001; website/branding completion is recorded separately by explicit user direction
**Provenance:** carried authorized website/analytics/branding work and owner's one-PR request
**Canonical architecture:** docs/architecture/product_analytics.md; docs/architecture/website-job-entrance.md

## Overview

Goal: the reviewed website journey, pricing, explicit PostHog clicks, shared brand assets and shell remain correct when the chat cutover ships. Indy confirmed the website visually; that is source-independent human evidence, not current unit/coverage evidence. External organization/personal profile branding PRs are already merged and outside this product PR.

## PR Intent & comprehension handshake

- **Intent:** keep the polished website and brand consistent across entry, app, emails, README, and shared assets while completing analytics wiring.
- **Handshake:** this work folds into the existing branch and owning PR, with no additional worktree. ASSUMPTIONS I'M MAKING: explicit clicks are intent events; deploy/app-dev verification belongs to Indy; current scoped proof is recorded below; repository and deployment checks remain in M227_001.

## Implementing agent — read these first

1. `docs/architecture/product_analytics.md` — actual event/property allowlists and privacy.
2. `docs/architecture/website-job-entrance.md` — approved website journey.
3. `ui/packages/website/src/analytics/posthog.ts` — capture initialization/failure behavior.
4. `branding/README.md` — brand asset sources and distribution.
5. `docs/DESIGN_SYSTEM.md` — fonts, tokens, reusable components.
6. `docs/reviews/workspace-setup/checks.md` — owner authorizations; this batch stays folded into the same PR.

## Files Changed (blast radius)

The exact carried paths below come from working-tree status on this branch. They are uncommitted; actions include additions, edits, deletions, and asset links already visible in the diff. Inspect each status before finishing it; do not recreate removed components. Expanded inventory is in `docs/reviews/workspace-setup/file-inventory.md`. Completion edits and pending-check transfers are recorded in `docs/reviews/workspace-setup/ui-completion-review.md`.

| File | Action | Why |
|---|---|---|
| `README.md` | REVIEW CARRIED DIFF | Existing source/assets; complete/validate within owner PR |
| `branding/README.md`; `branding/agentsfleet-dark.png`; `branding/agentsfleet-dark.svg`; `branding/agentsfleet-light.png`; `branding/agentsfleet-light.svg`; `branding/agentsfleet-mark-glow.png`; `branding/agentsfleet-mark-glow.svg`; `branding/agentsfleet-mark-transparent.png`; `branding/agentsfleet-mark-transparent.svg`; `branding/agentsfleet-mark.svg`; `branding/avatar-1024.png`; `branding/avatar-512.png`; `branding/favicon.ico`; `branding/favicon.svg`; `branding/github-avatar.png`; `branding/github-social.png`; `branding/github-social.svg` | REVIEW CARRIED DIFF | Existing source/assets; complete/validate within owner PR |
| `cli/README.md` | REVIEW CARRIED DIFF | Existing source/assets; complete/validate within owner PR |
| `docs/DESIGN_SYSTEM.md` | REVIEW CARRIED DIFF | Existing source/assets; complete/validate within owner PR |
| `docs/architecture/product_analytics.md`; `docs/architecture/website-job-entrance.md` | REVIEW CARRIED DIFF | Existing source/assets; complete/validate within owner PR |
| `rustd/crates/afd_mail/templates/invite.html`; `rustd/crates/afd_mail/templates/layout.html` | REVIEW CARRIED DIFF | Existing source/assets; complete/validate within owner PR |
| `rustd/crates/afd_mail/src/invite/snapshots/afd_mail__invite__tests__invite_html.snap` | REVIEW CARRIED DIFF | Existing tests; complete/validate within owner PR |
| `ui/packages/app/app/(auth)/layout.tsx`; `ui/packages/app/app/favicon.ico`; `ui/packages/app/app/icon.svg`; `ui/packages/app/app/layout.tsx`; `ui/packages/app/components/layout/MobileNavigationDialog.tsx`; `ui/packages/app/components/layout/ShellControls.tsx`; `ui/packages/app/components/layout/SidebarNavigation.tsx`; `ui/packages/app/components/layout/SidebarNavigationSections.tsx`; `ui/packages/app/components/layout/sidebar-navigation-data.ts`; `ui/packages/app/public/apple-touch-icon.png`; `ui/packages/app/public/brand.svg` | REVIEW CARRIED DIFF | Existing source/assets; complete/validate within owner PR |
| `ui/packages/app/components/domain/island-dynamic/intent-shell-controls.test.tsx`; `ui/packages/app/tests/app-components.test.ts`; `ui/packages/app/tests/app-pages.test.ts`; `ui/packages/app/tests/app-shell-navigation.test.ts`; `ui/packages/app/tests/e2e/wake-pulse.spec.ts` | REVIEW CARRIED DIFF | Existing tests; complete/validate within owner PR |
| `ui/packages/design-system/assets/brand.svg`; `ui/packages/design-system/src/assets/favicon.ico`; `ui/packages/design-system/src/assets/favicon.svg`; `ui/packages/design-system/src/assets/logo-dark.svg`; `ui/packages/design-system/src/assets/logo-light.svg`; `ui/packages/design-system/src/design-system/BrandMark.tsx`; `ui/packages/design-system/src/design-system/Terminal.tsx`; `ui/packages/design-system/src/design-system/index.ts`; `ui/packages/design-system/src/index.ts`; `ui/packages/design-system/src/theme.css`; `ui/packages/design-system/src/tokens.css` | REVIEW CARRIED DIFF | Existing source/assets; complete/validate within owner PR |
| `ui/packages/design-system/src/design-system/BrandMark.test.tsx`; `ui/packages/design-system/src/design-system/Terminal.test.tsx` | REVIEW CARRIED DIFF | Existing tests; complete/validate within owner PR |
| `ui/packages/website/index.html`; `ui/packages/website/public/apple-touch-icon.png`; `ui/packages/website/public/favicon.ico`; `ui/packages/website/public/favicon.svg`; `ui/packages/website/public/social.png`; `ui/packages/website/src/App.tsx`; `ui/packages/website/src/analytics/posthog.ts`; `ui/packages/website/src/components/AdoptionSections.tsx`; `ui/packages/website/src/components/AgentIllustration.tsx`; `ui/packages/website/src/components/CTABlock.tsx`; `ui/packages/website/src/components/CliQuickstart.tsx`; `ui/packages/website/src/components/FAQ.tsx`; `ui/packages/website/src/components/FleetPreview.tsx`; `ui/packages/website/src/components/Footer.tsx`; `ui/packages/website/src/components/Hero.tsx`; `ui/packages/website/src/components/HowItWorks.tsx`; `ui/packages/website/src/components/IncidentFlow.tsx`; `ui/packages/website/src/components/JobComposer.tsx`; `ui/packages/website/src/components/JobSuggestions.tsx`; `ui/packages/website/src/components/PrebuiltFleets.tsx`; `ui/packages/website/src/components/Pricing.tsx`; `ui/packages/website/src/components/SignalField.tsx`; `ui/packages/website/src/components/StoryMessage.tsx`; `ui/packages/website/src/components/use-chat-scroll.ts`; `ui/packages/website/src/components/use-motion-environment.ts`; `ui/packages/website/src/components/use-scene-motion.ts`; `ui/packages/website/src/config.ts`; `ui/packages/website/src/entrance.css`; `ui/packages/website/src/incident-flow.css`; `ui/packages/website/src/lib/chat-story.ts`; `ui/packages/website/src/lib/example-jobs.ts`; `ui/packages/website/src/lib/llms-text.ts`; `ui/packages/website/src/lib/marketing-copy.ts`; `ui/packages/website/src/pages/Home.tsx`; `ui/packages/website/src/styles.css`; `ui/packages/website/src/workbench.css` | REVIEW CARRIED DIFF | Existing source/assets; complete/validate within owner PR |
| `ui/packages/website/src/App.test.tsx`; `ui/packages/website/src/analytics/landing-clicks.test.tsx`; `ui/packages/website/src/analytics/posthog.test.tsx`; `ui/packages/website/src/components/CTABlock.test.tsx`; `ui/packages/website/src/components/FAQ.test.tsx`; `ui/packages/website/src/components/Footer.test.tsx`; `ui/packages/website/src/components/Hero.test.tsx`; `ui/packages/website/src/components/HowItWorks.test.tsx`; `ui/packages/website/src/components/PrebuiltFleets.test.tsx`; `ui/packages/website/src/components/Pricing.test.tsx`; `ui/packages/website/src/components/StoryMessage.test.tsx`; `ui/packages/website/src/components/motion.test.tsx`; `ui/packages/website/src/components/use-chat-scroll.test.ts`; `ui/packages/website/src/config.test.ts`; `ui/packages/website/src/lib/llms-text.test.ts`; `ui/packages/website/src/marketing-spec.test.ts`; `ui/packages/website/src/pages/Home.test.tsx`; `ui/packages/website/src/pages/Terms.test.tsx`; `ui/packages/website/src/test-setup.ts`; `ui/packages/website/tests/e2e/content-accessibility.spec.ts`; `ui/packages/website/tests/e2e/home.spec.ts`; `ui/packages/website/tests/e2e/humans-fleets-links.spec.ts`; `ui/packages/website/tests/e2e/navigation.spec.ts`; `ui/packages/website/tests/e2e/smoke.spec.ts` | REVIEW CARRIED DIFF | Existing tests; complete/validate within owner PR |
| `ui/packages/design-system/src/design-system/Pagination{,.test}.tsx`; `ui/packages/website/src/pages/DesignSystemGallery.tsx`; `ui/packages/website/src/components/IncidentFlow.tsx`; website analytics and browser tests | EDIT / TEST | Distinct pagination landmarks, keyboard transcript focus, final outgoing analytics privacy and current removed-route expectations |
| `docs/reviews/workspace-setup/ui-completion-review.md`; spec and review/architecture links | CREATE / EDIT / MOVE | Reviewed scope closure and preserved release checks |

## Applicable Rules

NDC/NLR/NLG and ORP: delete superseded components and their orphans. UFS: event/source allowlists and fixed text. UI/DESIGN TOKEN: existing primitives, token fonts/colors/motion. Read `.orly/docs/greptile-learnings/RULES.md`, `.orly/docs/DOCUMENTATION_RULES.md`, `.orly/docs/LOGGING_STANDARD.md`, and triggered TypeScript/interface dispatch pages. Product ownership and privacy follow the owning spec.

## Applicable Gates

UI/token/length/export gates cover shared components and assets. Product-analytics capture must keep input/clipboard/query/hash out of payloads. Unit/coverage, browser behavior, repository lint/integration/version and conform all run at the owner's final PR boundary, not in this docs rewrite.

## Prior-Art / Reference Implementations

Existing `ui/packages/website/src/analytics/posthog.ts`, `landing-clicks.test.tsx`, and app event catalog. Shared BrandMark/Terminal use the design-system exports. No second brand/font or event catalog is introduced.

## Sections (implementation slices)

### §1 — Website journey and motion

Retain approved appearance/pricing, real CLI/docs links, chat entrance, accessible motion/reduced-motion, and mobile layout. Correct behavioral copy and its journey diagram: ordinary Save reports fleet creation only; Save to library is a separate explicit action. The story now reports fleet creation and a scheduled check; explicit Library publication remains separate (`src/lib/chat-story.ts`, current browser proof in the review).
- **Dimension 1.1** — website entry/navigation/pricing and keyboard/mobile/reduced-motion behavior → Test `test_website_entry_and_accessible_motion` — DONE (current browser proof in the completion review)

### §2 — Explicit analytics wiring

Verify signup_started/navigation_clicked/interaction_clicked and stable source/surface/target/path properties for every cataloged control. Failure does not stop action; private composer input never enters capture. Existing app-dev/provider configuration is referenced, not duplicated.
- **Dimension 2.1** — complete click catalog, no private fields, SDK/clipboard failure preserves action → Test `test_website_posthog_clicks_and_privacy` — DONE (click/failure tests and final enriched-payload privacy tests)

### §3 — Branding distribution

Consistent outlined Instrument Sans/Bricolage assets, avatar/social/icon files, dark/light marks, app shell/design-system/email/readme use; verify links, file contents and dimensions.
- **Dimension 3.1** — deployed asset imports/exports and accessible branded surfaces → Test `test_brand_assets_and_shared_surfaces` — DONE (asset byte/dimension checks, package/build/browser evidence; mail source reviewed, Rust verification retained in M227_001)

Runner-label §4 is carried intact into M227_001 §8. Former4.1–4.3 keep their recorded DONE results; former4.4/4.5 remain IN_PROGRESS as8.4/8.5, with full live database/placement assertions. This document closes the reviewed website/branding scope only.

## Interfaces

Website emits the current documented explicit event catalog, not signup completion or server truth. Brand assets live in branding and shared design-system assets; consumers reuse them. Account/workspace chat behavior comes from M227_001. External profile branding stays in its already merged repositories.

## Failure Modes

Analytics SDK/clipboard refusal must not block navigation/copy UI;2.1. Reduced motion/mobile overflow has accessible fallback;1.1. Missing/symlinked public assets fail build/smoke checks;3.1. Private free text in capture is rejected;2.1. Runner-label refusal, atomicity, read-back and placement requirements remain in M227_001 §8.

## Invariants

Typed allowlists enforce event props, composer capture exclusion protects private text, shared tokens/exports enforce visual reuse, browser assertions prove links/focus/layout. Human visual approval does not replace these proofs.

## Metrics & Observability

Preserve signup_started, navigation_clicked, interaction_clicked with the existing source list in product_analytics.md. No prompt/clipboard/query/hash or secret values. Test2.1 enumerates cataloged controls and capture-failure isolation. App-dev deployment events are verified by Indy after deployment, not claimed observed here.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
|1.1|e2e|`test_website_entry_and_accessible_motion`|Existing browser lane proves entry/navigation/pricing, viewport/focus/reduced-motion; ordinary Save claims only fleet creation and Library publication requires its separate explicit action.|
|2.1|unit|`test_website_posthog_clicks_and_privacy`|Posthog/landing-clicks tests prove mapping, failure isolation and the final before_send filter after SDK enrichment; only fixed attribution, pathname and required ingest/identity fields remain.|
|3.1|unit|`test_brand_assets_and_shared_surfaces`|BrandMark/Terminal/app package proofs, byte-identical icons, PNG dimensions and website asset/build/browser checks; email templates inspected in source, final Rust test lane remains in M227_001.|

## Acceptance Rubric (single scoring surface)

| Outcome | Verify | Expected | Graded |
|---|---|---|---|
| Conform | `make harness-verify` | Exit 0 | Not run here; combined release gate retained in M227_001 |
| Lint | `make lint-all` | Exit 0 | Not run here; combined release gate retained in M227_001 |
| Unit/coverage | `make test-unit-all` | Exit 0; configured coverage bars | Not run here; combined release gate retained in M227_001 |
| Integration | `make test-integration-rustd` | Exit 0 for whole owning PR | Not run here; combined release gate retained in M227_001 |
| Version | `make check-version` | Exit 0 | Not run here; combined release gate retained in M227_001 |
| Website lint | `make lint-website` | Exit 0 | PASS: Oxlint and TypeScript; website lint passed |
| Website unit | `make test-unit-website` | Exit 0 | PASS: 20 files, 134 tests passed |
| Website browser | `make _dry_website` | Exit 0; website behavior/copy assertions, including explicit-publication separation | PASS: 135 passed (2.4m); website dry pass passed |
| Deployment analytics | Manual: Indy app-dev click/event check after deployment | Each cataloged source emits intended fields and no private input | Not observed; manual requirement retained in M227_001 |

## Dead Code Sweep

Inspect deleted marketing components/test/import/export/CSS references and link assets; no duplicated font/brand/event catalogs. No public profile worktree or extra feature worktree is retained for this folded work.

## Out of Scope

New visual variants, unrelated profile edits, analytics account provisioning, deploying now, and coverage claims without a fresh declared lane run.

## Product Clarity (authoring record)

1. Successful moment: visitor understands the product, chooses a real entry, sees consistent brand.
2. Preserved behavior: approved visual/content flow and correct links.
3. Optimal-way check: shared assets and cataloged clicks are direct and bounded.
4. Rebuild-vs-iterate: finish the existing feature batch.
5. Build: website, analytics wiring, brand distribution and proofs.
6. Exclude: unrelated profiles/infrastructure; already handled elsewhere.
7. Fit: feeds the workspace-chat entry without exposing prompts to analytics.
8. Surface order: website and app surfaces share the owning cutover PR.
9. Restraint: no fabricated conversion/results or unmeasured quality claims.
10. Next step: real signup/install/docs link and reliable action failure handling.

## Decomposition & alternatives (patch vs refactor)

Focused interface patch folded into M227_001 per owner request. Additional worktrees/atomic-task PRs are rejected because the user requested one working tree and cutover PR. Shared brand assets avoid a second visual system.

## Discovery (consult log)

Indy's visual signoff: “yes signed off website, app changes”. Remaining technical/release evidence stays in M227_001.

- **Consults:** existing carried status and event/brand source inspection; Indy confirmed website visually in the conversation.
- **Metrics review:** catalog checked; code wiring exists locally, deploy/app-dev event observation is future human evidence.
- **Review outcome:** source review and scoped behavior evidence are recorded in `docs/reviews/workspace-setup/ui-completion-review.md`; whole-branch review, repository checks and PR follow-up remain with M227_001.
- **Deferrals:** owner deferred full repository checks/commit/PR until workspace-chat completion; same quote recorded in M227_001. External profile PRs were merged by Indy.
- **Runner labels (§4):** Indy: “Add operator-authorized updates to runner labels using the existing registration validation and deduplication rules. Keep labels separate from sandbox policy.” Enrolment had no collapse before this; the carried implementation adds it to enrolment too, so both writers store alike. Placement compares by subset (`required_tags <@ labels`), which a repeat cannot change. Filed here because M227_001 sits at the template's line cap; same PR. Indy rejected a separate labels URL: “the Runners > Click Single Runner > Edit will allow edit of a label, and that API must be improved. And the GET must display the label back.” He also asked that “a sanity check must be done to ensure the label is read by rust runner as well” — that check is now M227_0018.5.

- **Closure direction:** Indy: “Carry pending checks into M227_001 and close the reviewed website/branding scope (Recommended)”. All runner-label requirements and source/test inventory moved to M227_001 §8; original test names stay unchanged. Repository/release/deployment requirements remain at that combined boundary, with no passing result inferred from package checks.

- **Transcript exception:** Indy approved exactly one new inline `jsx-a11y/no-noninteractive-tabindex` suppression for keyboard scrolling. A temporary sibling still failed lint, then was removed; the final source has exactly two website suppressions. Fresh scoped privacy review reports `NO_FINDINGS`; final lint, unit and browser outputs are in `/tmp/m227-transcript-focus/`.
