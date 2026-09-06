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
**Status:** DONE
**Priority:** P1 — readable product surfaces and accurate launch copy
**Categories:** DOCS, UI
**Batch:** B1 — independent of the outbound repair
**Branch:** `feat/m189-clear-signal`
**Test Baseline:** User directs full unit and integration suites to run only immediately before the PR. No passing baseline recorded.
**Depends on:** none for visual work; connector acceptance remains with its existing workstream
**Provenance:** LLM-drafted from the user's approved design proposal
**Canonical architecture:** `docs/DESIGN_SYSTEM.md`; `docs/architecture/billing_and_provider_keys.md`

## Overview

**Goal (testable):** App and website render readable sans typography, flat surfaces, mint actions, and honest early-access messaging in both themes.
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
| `docs/qa/clear-signal-ux-review.md` | ADD | Record every app and website route, scenario evidence, findings, and verified repairs |
| `ui/packages/app/lib/onboarding.ts` and paired test | EDIT | Point the credential checklist step to the existing Secrets route |
| `ui/packages/app/lib/auth/client.ts`, account settings route, and identity tests | EDIT / ADD | Supported account page and uploaded-avatar identity |
| `ui/packages/app/app/(dashboard)/settings/page.tsx`, billing and approvals pages, and paired tests | EDIT | Keep failed reads distinct from empty lists and explain restricted access |
| `ui/packages/app/public/brand.svg`, `ui/packages/app/public/user.svg` | ADD | Flat brand and account fallback marks for Clerk widgets |
| `ui/packages/app/app/not-found.tsx`, `ui/packages/website/src/pages/NotFound.tsx` and paired tests | ADD | Provide themed recovery for unknown routes |
| `package.json`, `ui/packages/website/package.json`, and `bun.lock` | EDIT | Keep the Playwright test runner and its core dependency on the same version |
| `ui/packages/app/tests/e2e/**/*.spec.ts` | EDIT / ADD | Verify rendered typography, flat surfaces, input states, table alignment, and responsive page behavior |
| `docs/v2/pending/M189_001_P1_DOCS_UI_CLEAR_SIGNAL.md` | MOVE | Track lifecycle and verification |
| `ui/packages/design-system/src/tokens.css` | EDIT | Color, typography, spacing, and motion values |
| `ui/packages/design-system/src/theme.css` | EDIT | Forward new font roles and token values |
| `ui/packages/design-system/package.json` | EDIT | Bundle display font locally |
| `bun.lock` | EDIT | Record font dependency |
| `cli/package.json` and `cli/bun.lock` | EDIT | Update Effect within the current release-candidate line |
| `audits/design-tokens.sh` | EDIT | Enforce flat fills and shared font ownership |
| `scripts/design_tokens_test.py` | ADD | Exercise font ownership, gradient, and palette rejection in isolated repositories |
| `audits/cross-tier-rates.sh` | EDIT | Pin the remaining rate consumers after removing website numerical prices |
| `docs/architecture/billing_and_provider_keys.md`, `docs/CHANGELOG_VOICE.md`, `dispatch/write_changelog.md` | EDIT | Remove references to the deleted website rate mirror |
| `ui/packages/design-system/src/design-system/*.tsx` | EDIT | Shared typography and interaction presentation |
| `ui/packages/design-system/src/design-system/eyebrow.ts`, `ui/packages/design-system/src/design-system/index.ts` | EDIT | Centralize interface typography and navigation exports |
| `ui/packages/design-system/src/index.ts` | EDIT | Expose shared navigation through the package entry point |
| `ui/packages/design-system/src/design-system/NavItem.tsx` and paired test | ADD | Own shared navigation item appearance and active state |
| `ui/packages/app/components/**/*.tsx` | EDIT | Align page widgets and remove local visual overrides |
| `ui/packages/design-system/src/design-system/DataTableModel.ts` | EDIT | Synchronize pagination before commit rather than cascading from an effect |
| `ui/packages/design-system/src/design-system/*.test.tsx` | EDIT | Keep component behavior covered |
| `ui/packages/design-system/src/tokens.css.test.ts` | EDIT | Font roles, contrast, and token mapping regressions |
| `ui/packages/app/app/globals.css` | EDIT | Remove ambient gradients and meter gradients |
| `ui/packages/app/lib/clerkAppearance.ts` | EDIT | Keep third-party auth actions on the same CTA token pair as shared buttons |
| `ui/packages/app/components/*.tsx` | EDIT | App shell and navigation typography |
| `ui/packages/app/app/**/*.tsx` | EDIT | Migrate visual consumers without changing data access |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/SourceDocumentPane.tsx` | ADD | Separate technical source rendering from source-edit state while keeping interface labels sans |
| `ui/packages/app/lib/avatarColor.ts` | RENAME / EDIT | Replace avatarGradient.ts with deterministic flat identity colors |
| `ui/packages/app/lib/avatarColor.test.ts` | RENAME / EDIT | Preserve identity and empty-seed behavior without gradients |
| `ui/packages/app/tests/*` | EDIT | Update visual assertions and preserve existing behavior |
| `ui/packages/app/tests/interface-typography.test.ts` | ADD | Enforce shared interface font defaults across app pages and components |
| `ui/packages/app/tests/fleet-library-paging.test.ts` | ADD | Split paging checks from install-entry checks and wait for async error rendering |
| `ui/packages/app/tests/app-shell-frame.test.ts`, `ui/packages/app/tests/admin-models-management.test.ts` | ADD | Split oversized suites by concern without dropping assertions |
| `ui/packages/app/.oxlintrc.json` | EDIT | Match compiler-only checks to the app's disabled React Compiler |
| `ui/packages/app/package.json` and `bun.lock` | EDIT | Update app runtime and development dependencies to current releases; retain the TypeScript 6 parser alias required by the bundle guard |
| `ui/packages/website/src/styles.css` | EDIT | Flat editorial layouts and responsive composition |
| `ui/packages/website/src/components/AgentIllustration.tsx` | ADD | Small shared vector replacing the workshop raster |
| `ui/packages/website/src/components/FleetPreview.tsx` | ADD | Clearly labelled incident illustration with human repair handoff |
| `ui/packages/website/src/components/AdoptionSections.tsx` | ADD | Founder and infrastructure audiences plus setup guidance |
| `ui/packages/website/src/components/AdoptionSections.test.tsx` | ADD | Audience, setup, and control-boundary regressions |
| `ui/packages/website/public/logos/elasticsearch.svg` | ADD | Locally served evidence-source mark |
| `ui/packages/website/src/components/*.tsx` | EDIT | Product hero, capabilities, pricing, and calls to action |
| `ui/packages/website/src/pages/*.tsx` | EDIT | Home composition, fleet catalog, and design gallery |
| `ui/packages/website/src/pages/About.tsx` and paired test | DELETE | Remove the redundant About route |
| `ui/packages/website/src/components/Footer.tsx` and paired test | EDIT | Direct Contact, Early access label, and bounded local layout helpers |
| `ui/packages/website/src/lib/marketing-copy.ts` | EDIT | Explain current product and remove free-run promises |
| `ui/packages/website/src/lib/rates.ts` and paired test | DELETE | Remove the unused website-only rate mirror after withdrawing numerical pricing |
| `ui/packages/website/src/lib/llms-text.ts` and paired test | EDIT | Keep generated public copy aligned with early-access terms |
| `ui/packages/website/scripts/prebuild.mjs` | EDIT | Stop supplying rate quotes to public text generation |
| `ui/packages/website/src/App.tsx` | EDIT | Navigation presentation |
| `ui/packages/website/tests/e2e/*.spec.ts` | EDIT | Responsive and accessibility regression scenarios |
| `ui/packages/website/tests/e2e/design-system-ownership.spec.ts` | ADD | Prove shared font and color edits propagate to rendered consumers |
| `ui/packages/website/tests/e2e/design-system-status-smoke.spec.ts` | ADD | Split gallery browser assertions into bounded suites |

The website `.size-limit.json` tightens the critical JavaScript budget to 120 kB.

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

- **Dimension 1.1 — DONE** — Both themes expose readable foregrounds on supported surfaces → Test `theme contrast pairs`.
- **Dimension 1.2 — DONE** — Display, UI, and code font roles resolve independently → Test `font role mappings`.
- **Dimension 1.3 — DONE** — Shared controls preserve keyboard, disabled, and error behavior → Test existing component suites.

### §2 — App clarity

Apply shared typography and flat surfaces to existing authenticated screens.

- **Dimension 2.1 — DONE** — App backgrounds and usage meters contain no gradients → Test `flat app surfaces`.
- **Dimension 2.2 — DONE** — Navigation and forms remain usable on narrow screens → Test browser keyboard and responsive walkthrough.
- **Dimension 2.6 — DONE** — Every app and website route receives source and rendered review across relevant roles, forms, empty/populated/error states, keyboard interactions, and responsive layouts → Evidence `docs/qa/clear-signal-ux-review.md`.
- **Dimension 2.3 — DONE** — Empty, loading, and error states retain visible next actions → Test existing app state suites and failed initial reads.
- **Dimension 2.4 — DONE** — Shared primitives own navigation, interface typography, and meter presentation; consumers retain semantic technical text → Test component contracts, app navigation suites, and browser token propagation.
- **Dimension 2.5 — DONE** — Design-token gate rejects consumer font definitions and display typography in the app → Test rejection and permitted technical token fixtures plus app primitive override checks.

### §3 — Website and pricing

Explain recurring work with a visible product example and provisional pricing.
Invite early users to try a real workflow and provide feedback before launch pricing is decided.

- **Dimension 3.1 — DONE** — Home presents product, working example, fleets, controls, and pricing → Test home section walkthrough.
- **Dimension 3.2 — DONE** — Early-access copy separates runtime and model usage without unapproved prices or credit promises → Test pricing content assertions.
- **Dimension 3.3 — DONE** — Theme switching and mobile navigation preserve readable content → Test website end-to-end smoke.
- **Dimension 3.4 — DONE** — Direct Contact preserves useful footer destinations and click attribution; About is removed → Test Footer and App suites.

### §4 — App dependency currency

Update dependencies declared by the app package to current stable releases without weakening its checks.

- **Dimension 4.1 — DONE** — Runtime and development dependencies resolve at their current stable releases → Verify `bun outdated` reports no eligible app update.
- **Dimension 4.2 — DONE** — The app retains 100% statement, branch, function, and line coverage after the upgrade → Test the app coverage lane.
- **Dimension 4.3 — DONE** — The current lint and native TypeScript compiler paths remain operational → Test app lint and typecheck.

### §5 — Effect dependency currency

Update the CLI's Effect dependency within its existing release-candidate line.

- **Dimension 5.1 — DONE** — The CLI resolves the current Effect release candidate → Verify package manifest and lockfile.
- **Dimension 5.2 — DONE** — CLI behavior and coverage remain intact → Test CLI build, lint, typecheck, and coverage enforcement.

## Interfaces

Backend API routes, CLI commands, billing rates, and authorization remain unchanged.
The website exposes agent resources at `/agents`; dashboard links open the app.
Mobile navigation exposes the same destinations as desktop navigation.
Account profile and security open at `/settings/account`.
Workspace creation accepts an omitted display name and uses the existing generated-name behavior.
Shared components add optional presentation and focus props. Existing callers remain valid.
Analytics events describe the action each link performs.

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
4. Public website copy quotes no unapproved rates, credits, or unlimited usage. Backend billing constants remain unchanged.

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
| 3.2 | unit | pricing content assertions | No unapproved prices, credits, or free-run promises; terms precede paid usage |
| 3.3 | e2e | website smoke | Light, dark, mobile, and reduced-motion states remain usable |
| 3.4 | unit | Footer and App suites | Direct mailto, removed About route, retained links, and navigation attribution |
| 4.1 | package audit | app `bun outdated` | No eligible update remains; the accepted `typescript-jsapi` compatibility alias stays on TypeScript 6 |
| 4.2 | unit | app `bun run test:coverage` | Existing 100% thresholds pass after dependency resolution |
| 4.3 | static | app lint and typecheck | Oxlint and the TypeScript 7 native compiler complete without errors |
| 5.1 | package audit | CLI manifest and lockfile | Effect uses the current release candidate without reverting to the earlier stable major |
| 5.2 | unit / static | CLI `bun run test`, lint, and typecheck | Existing behavior passes and enforced function and line coverage reach 100% |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|-----------|---------------------|----------|----------|-----------------|
| R1 | Conform | `make harness-verify` | exit 0 | P0 | PASS — make harness-verify through orly gate work; merge commit hook exit 0 |
| R2 | Lint | `make lint-all` | exit 0 | P0 | PASS — make lint-all; All lint checks passed |
| R3 | Unit behavior | `make test-unit-all` | exit 0 | P0 | PASS — make test-unit-all; Rust 2355, app 2527, website 155, CLI 1646, design system 559 passed |
| R4 | Integration regression | `make test-integration-rustd` | exit 0 | P0 | PASS — make test-integration-rustd; 377 passed |
| R5 | Version consistency | `make check-version` | exit 0 | P0 | PASS — make check-version; all versions match 0.28.0 |
| R6 | Browser smoke | `make dry-smoke` | exit 0 | P0 | PASS — make dry-smoke; website 13, app browser 4, smoke unit 1 passed |
| R7 | No secrets | `gitleaks detect` | exit 0 | P0 | PASS — gitleaks detect --redact; 5204 commits scanned, no leaks found |

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
- **Early-access direction:** User said, “Since i need to get early users and tryout first before arriving at pricing”.
  This supersedes the numerical pricing-grid design. Paid terms must be confirmed before paid usage; no free-usage promise is introduced.
- **Skill-chain outcomes:** Design consultation informed shared font roles and flat surfaces. Image generation supplied the original workshop illustration.
- **Deferrals:** The agent onboarding guide is recorded under Approved deferral below.
- **Effect refresh direction:** User said, “and can you update the effects packages to the latest as well”.
- **Dependency refresh direction:** User said, “update all the packages in the packaages/app as well to the lates”. All eligible app dependencies move to their current stable releases. The existing `typescript-jsapi` alias remains on TypeScript 6 because it supplies the parser API used by `intent-module-loader.test.ts`; the normal app compiler remains the current TypeScript 7 release. This preserves the earlier explicit decision to keep the parser bridge until TypeScript 7.1 supplies the needed official API.

## Current UX direction

The hero focuses on incident response with a small vector illustration. How it works owns the detailed examples.
Incident Response and Slack Teammate appear first. About is removed; direct Contact remains.
Dark surfaces are brighter. The website advertises an open-source runtime and describes self-hosting as planned.
The JavaScript size budget is 120 kB gzip. Expanded browser checks cover accessibility, enlarged text, and keyboard focus.
Account settings use Clerk's supported page mode. Header controls and runner actions have explicit alignment checks.
Mobile website navigation remains visible, with touch targets at least 44px.
Audience labels retain the eyebrow scale; lifted section fills separate audiences and early access.
Evidence-source marks use amber and the delivery mark uses blue from the shared semantic palette.
Internal workflow links use router navigation so the target section is visible after a page change.
Historical checkpoints are preserved in [the design history](../../qa/clear-signal-design-history.md).

## Approved deferral

The complete agent onboarding guide and executable incident-investigation walkthrough on `/agents` are deferred until after `docs/v2/pending/M187_001_P0_API_CLI_INFRA_UI_FLEET_END_TO_END_ACCEPTANCE.md`.
> Indy (2026-09-05): "1 - i will do it after docs/v2/pending/M187_001_P0_API_CLI_INFRA_UI_FLEET_END_TO_END_ACCEPTANCE.md" — context: complete agent onboarding guide and executable walkthrough.
This milestone changes the page heading, route, and navigation; it does not claim agent onboarding is end-to-end verified.

## Final integration evidence

The branch integrates M190 while retaining its live run summaries and immediate action feedback.
The UI review and all canonical verification lanes pass on the combined branch.
Logs: `/tmp/m189-merged-{unit-all,integration,lint-all,dry-smoke,check-version,gitleaks-history}.log`.
The merged production app passes 17 live acceptance tests and 8 true-touch scenarios.
Website browser evidence remains 157 Chromium tests, 206 Firefox/WebKit tests, and 56 rebuilt gallery/accessibility/smoke tests.
The exact reviewed states and evidence limits are recorded in `docs/qa/clear-signal-ux-review.md`.
Companion documentation: https://github.com/agentsfleet/docs/pull/186.
