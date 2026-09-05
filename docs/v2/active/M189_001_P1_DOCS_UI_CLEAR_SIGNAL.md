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
| `ui/packages/website/public/fleet-workshop.webp` | ADD | Original generated hero illustration, optimized for delivery |
| `ui/packages/website/src/components/FleetPreview.tsx` | ADD | Clearly labelled incident illustration with human repair handoff |
| `ui/packages/website/src/components/AdoptionSections.tsx` | ADD | Founder and infrastructure audiences plus setup guidance |
| `ui/packages/website/src/components/AdoptionSections.test.tsx` | ADD | Audience, setup, and control-boundary regressions |
| `ui/packages/website/public/logos/elasticsearch.svg` | ADD | Locally served evidence-source mark |
| `ui/packages/website/src/components/*.tsx` | EDIT | Product hero, capabilities, pricing, and calls to action |
| `ui/packages/website/src/pages/*.tsx` | EDIT | Home composition, fleet catalog, and design gallery |
| `ui/packages/website/src/pages/About.tsx` and paired test | ADD | Minimal product explanation and tracked direct contact without a new form or service |
| `ui/packages/website/src/components/Footer.tsx` and paired test | EDIT | About and Contact links, Early access label, and bounded local layout helpers |
| `ui/packages/website/src/lib/marketing-copy.ts` | EDIT | Explain current product and remove free-run promises |
| `ui/packages/website/src/lib/rates.ts` and paired test | DELETE | Remove the unused website-only rate mirror after withdrawing numerical pricing |
| `ui/packages/website/src/lib/llms-text.ts` and paired test | EDIT | Keep generated public copy aligned with early-access terms |
| `ui/packages/website/scripts/prebuild.mjs` | EDIT | Stop supplying rate quotes to public text generation |
| `ui/packages/website/src/App.tsx` | EDIT | Navigation presentation |
| `ui/packages/website/tests/e2e/*.spec.ts` | EDIT | Responsive and accessibility regression scenarios |
| `ui/packages/website/tests/e2e/design-system-ownership.spec.ts` | ADD | Prove shared font and color edits propagate to rendered consumers |
| `ui/packages/website/tests/e2e/design-system-status-smoke.spec.ts` | ADD | Split gallery browser assertions into bounded suites |

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
- **Dimension 2.2 — IN_PROGRESS** — Navigation and forms remain usable on narrow screens → Test browser keyboard and responsive walkthrough.
- **Dimension 2.3 — DONE** — Empty, loading, and error states retain visible next actions → Test existing app state suites.
- **Dimension 2.4 — DONE** — Shared primitives own navigation, interface typography, and meter presentation; consumers retain semantic technical text → Test component contracts, app navigation suites, and browser token propagation.
- **Dimension 2.5 — DONE** — Design-token gate rejects consumer font definitions and display typography in the app → Test rejection and permitted technical token fixtures plus app primitive override checks.

### §3 — Website and pricing

Explain recurring work with a visible product example and provisional pricing.
Invite early users to try a real workflow and provide feedback before launch pricing is decided.

- **Dimension 3.1 — DONE** — Home presents product, working example, fleets, controls, and pricing → Test home section walkthrough.
- **Dimension 3.2 — DONE** — Early-access copy separates runtime and model usage without unapproved prices or credit promises → Test pricing content assertions.
- **Dimension 3.3 — DONE** — Theme switching and mobile navigation preserve readable content → Test website end-to-end smoke.
- **Dimension 3.4 — DONE** — About and direct Contact preserve existing footer destinations and click attribution → Test Footer and About suites.

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
| 3.4 | unit | Footer and About suites | About route, direct mailto, retained links without duplicates, and navigation attribution |
| 4.1 | package audit | app `bun outdated` | No eligible update remains; the accepted `typescript-jsapi` compatibility alias stays on TypeScript 6 |
| 4.2 | unit | app `bun run test:coverage` | Existing 100% thresholds pass after dependency resolution |
| 4.3 | static | app lint and typecheck | Oxlint and the TypeScript 7 native compiler complete without errors |
| 5.1 | package audit | CLI manifest and lockfile | Effect uses the current release candidate without reverting to the earlier stable major |
| 5.2 | unit / static | CLI `bun run test`, lint, and typecheck | Existing behavior passes and enforced function and line coverage reach 100% |

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
- **Early-access direction:** User said, “Since i need to get early users and tryout first before arriving at pricing”.
  This supersedes the numerical pricing-grid design. Paid terms must be confirmed before paid usage; no free-usage promise is introduced.
- **Skill-chain outcomes:** Design consultation informed shared font roles and flat surfaces. Image generation supplied the original workshop illustration.
- **Deferrals:** None.
- **Effect refresh direction:** User said, “and can you update the effects packages to the latest as well”.
- **Dependency refresh direction:** User said, “update all the packages in the packaages/app as well to the lates”. All eligible app dependencies move to their current stable releases. The existing `typescript-jsapi` alias remains on TypeScript 6 because it supplies the parser API used by `intent-module-loader.test.ts`; the normal app compiler remains the current TypeScript 7 release. This preserves the earlier explicit decision to keep the parser bridge until TypeScript 7.1 supplies the needed official API.

## Working preview checkpoint

Implementation remains in progress. Design changes remain uncommitted; no PR or deployment has been made.
The branch merged `origin/main` through `359222520` and is zero commits behind that fetched revision.
The worktree is `/private/tmp/agentsfleet-m189-clear-signal`.
The website preview runs at `http://127.0.0.1:5189/`.

Package checks are inner-loop evidence, not repository VERIFY completion:

| Check | Observed result |
|---|---|
| Website `bun run test` | 175 tests passed before adding two example-run assertions |
| Updated HowItWorks suite | 7 tests passed, including both new example-run assertions |
| Selected shared-component and token suites | 137 tests passed |
| Selected app shell and avatar suites | 37 tests passed |
| Website production build | Vite build completed |
| App typecheck | Completed without errors |
| Design-system lint and typecheck | Completed without errors |
| Website lint | Completed without errors |
| Homepage accessibility scan | No WCAG A/AA violations reported in either theme |
| Browser widths | No horizontal overflow at 390, 768, or 1440px in either theme |
| Reduced motion | Example path computed animation name was `none` |
| Source sweep | No gradient functions or old avatar/surface names in production UI source |

Still required: authenticated app browser walkthrough, complete diff review, repository gates, and lifecycle closure.
Full unit and integration suites remain deferred until PR preparation under the user's instruction.

### Fleet catalogue content checkpoint

The user approved aligning the catalogue with current scenario docs.
Four cards now explain PR review, incident response, the Slack channel resident, and planned security review.
Each card identifies its trigger, output, and control boundary. All signup links say “Join the waitlist”.
The empty roadmap placeholder was removed. Security remains marked coming soon.
The Slack resident is not described as a library template or unattended worker.

Sources: `docs/architecture/scenarios/github-pr-reviewer.md`, `production-deploy-repair.md`, and `slack-channel-resident.md` in the same directory.
Security availability follows `docs/architecture/roadmap.md`.

| Changed unit | Required evidence | Observed result |
|---|---|---|
| Fleet catalogue and details | Rendered trigger, output, and control labels | Component regression tests |
| Waitlist links | Destination, label, and click attribution preserved | Component regression tests |
| Availability boundaries | No immediate-install claims; Slack limits; planned security status | Component regression tests |
| Homepage composition | No removed placeholder consumer | Home regression tests |
| Responsive catalogue | No horizontal overflow | Chrome at 390, 768, and 1440px |
| Section accessibility | WCAG A/AA scan | No violations reported |

Red-green evidence: the revised catalogue suite first reported five failures and four passes against the old content.
After implementation, website `bun run test` reported 181 passing tests across 24 files.
Website lint and production build completed successfully. These are package checks, not repository VERIFY completion.
No new transport, persistence, retry, concurrency, or backend behavior was introduced.

### Audience, illustration, and early-access checkpoint

The homepage now leads with practical work, followed by the catalogue, incident illustration, audience guidance, controls, memory, setup, and early access.
The incident illustration distinguishes the responder from the approval-gated repairer. Diagnosis alone never starts repair.
Source: `docs/architecture/scenarios/production-deploy-repair.md`.
The Elasticsearch mark comes from Simple Icons, served locally from `public/logos/elasticsearch.svg`.
Source: `https://raw.githubusercontent.com/simple-icons/simple-icons/develop/icons/elasticsearch.svg` (CC0; third-party trademarks remain with their owners).

Numerical rates and starter-credit promises were removed from the hero, pricing, FAQ, billing paragraph, and generated public text.
The website-only rate mirror and its obsolete display tests were removed. No backend billing file changed.
The retained early-access action still reports `pricing_early_access`; removed plan actions no longer emit events.
The billing paragraph remains draft copy requiring owner review before publication; no legal review is claimed.

| Check | Latest observed result |
|---|---|
| Website `bun run test` | 156 passed across 24 files |
| Website `bun run lint` | Completed without errors |
| Website `bun run build` | TypeScript and Vite completed; main JavaScript 387.08 kB, gzip 121.01 kB |
| Homepage WCAG A/AA scans | Zero violations in dark and light themes |
| Responsive browser checks | No horizontal overflow at 390, 768, or 1440px in either theme |
| Reduced-motion check | Incident wire animation name is `none` |
| `git diff --check` | No whitespace errors |

Test count fell because the rate mirror and three-plan presentation were removed.
Replacement tests cover provisional pricing, waitlist links, model-key limits, founder and infrastructure guidance, setup requirements, and repair boundaries.
These package checks do not satisfy repository VERIFY or indicate PR readiness.
`make harness-verify` exited successfully against an empty staged scope. It did not grade these unstaged source changes and must be rerun before commit.

### Homepage repetition cleanup

The user requested removal of “Your first fleet” and “Operational knowledge” because they repeat other sections.
Both homepage sections were removed, along with the unused knowledge component, paired tests, copy constants, and illustration styles.
Audience guidance, the fleet catalogue, incident illustration, and core memory capability remain.
Home tests now assert that neither removed section appears and that capabilities precede early access.
Focused Home and audience tests passed all 16 cases. Website lint and typecheck completed without errors.

Website `bun outdated` found newer Vite, React Router, PostHog, and testing-tool releases. No dependency upgrades were made during this cleanup.

### About, navigation, and footer cleanup checkpoint

The homepage places How it works before Meet the fleet. Header and footer use Early access for the retained `/#pricing` destination.
The lazy `/about` page explains the product and provides direct contact through the existing support address.
About and Contact appear in the footer's copyright row. No team credentials, contact form, or backend service were added.

Footer rendering now composes local brand, column, and copyright/contact helpers.
The fragment around the columns preserves the existing grid children. Link destinations, external-link attributes, CSS classes, and navigation event payloads are unchanged by this extraction.
`Footer.tsx` contains 105 lines; component functions contain 14, 12, 40, and 17 lines, all below the 50-line function cap.

| Changed unit | Existing regression evidence |
|---|---|
| Footer shell and brand | Rendered brand, tagline, and current-year assertions |
| Navigation columns | Product, resource, community, and legal destinations; external-link attributes; no duplicate links |
| Copyright and contact row | About and mailto destinations plus both navigation event payloads |

The focused Footer, App, About, and Home suites passed all 41 tests before and after extraction.
The final coverage run enforced 100% thresholds for statements, branches, functions, and lines on `src/components/Footer.tsx`:
14/14 statements, 14/14 lines, 6/6 functions, and 0 conditional branches (reported as 100%).
No new tests or coverage exclusions were needed for this structural change.

Run from `ui/packages/website`:

```sh
bun run test src/components/Footer.test.tsx src/App.test.tsx src/pages/About.test.tsx src/pages/Home.test.tsx --coverage --coverage.include=src/components/Footer.tsx --coverage.thresholds.statements=100 --coverage.thresholds.branches=100 --coverage.thresholds.functions=100 --coverage.thresholds.lines=100
```

Website `bun run lint` and `bun run typecheck` also completed without errors.
These checks establish package-level evidence for the footer cleanup, not repository VERIFY or coverage of the full M189 diff.
Authenticated app visuals, the full adversarial review, and repository gates remain pending.

### Hero image production record

Output: `ui/packages/website/public/fleet-workshop.webp`, 1254 × 1254 pixels, 122872 bytes.
The built-in image generator created the image. WebP encoding preserved the scene for website delivery.
The final editing prompt was:

> Edit this illustration for production website use. Replace the entire checkerboard background with a perfectly uniform solid graphite #0C1113 background. No checkerboard and no transparency. Preserve the workshop and robot composition. Make the illustration rigorously flat screen-print artwork: every surface filled with one solid color, hard-edged two-tone planes only, no gradients, no soft shadows, no glow, no glossy highlights. Use cyan-mint #5EEAD4, ivory, and graphite solid fills. Keep all objects inside canvas with clear margins. No text or watermark.

### Review and package validation checkpoint

The repository-wide verification rubric remains pending until PR preparation, per the quoted user instruction above.
The authenticated walkthrough remains incomplete: the local environment lacks the Clerk secret and publishable keys.
The app root returned HTTP 500 with Clerk's missing-publishable-key error. No authentication bypass was added.

| Scope | Command | Result |
|---|---|---|
| App | `bun run test:coverage` | 237 files; 2,410 tests passed; all four coverage measures 100% |
| Shared components | `bun run test:coverage` | 56 files; 542 tests passed; all four coverage measures 100% |
| Website | `bun run test:coverage` | 24 files; 156 tests passed; all four coverage measures 100% |
| CLI | `bun run test` | Build succeeded; 1,624 passed, 13 skipped, zero failures; enforced function and line coverage 100% |
| Website browser | `BASE_URL=http://127.0.0.1:5174 bun run test:e2e` | 104 passed; both themes, accessibility, reduced motion, links, and shared gallery |
| Footer final layout | `bunx vitest run src/components/Footer.test.tsx` | Eight tests passed; mobile screenshot inspected |
| Static checks | Package lint and typecheck | App, website, shared components, and CLI completed successfully |
| Conform | `make harness-verify` | All staged gates green |
| Flat fills | `bash audits/design-tokens.sh --all` | Named utilities and flat fills verified |
| Rate parity | `bash audits/cross-tier-rates.sh` | One rate constant agrees across three remaining consumers |
| Secrets | `gitleaks protect --staged --redact --no-banner` | No leaks found |

Review fixes remove stale pricing links, preserve diagnosis-only outcomes, and keep clipboard clicks out of signup analytics.
The footer uses interface typography, valid heading levels, and two mobile link columns.
Inline prose links remain underlined. Accessibility checks wait for theme transitions before measuring contrast.
Oversized source functions and test files are split by concern; no assertions were dropped except one duplicate assertion.
Pagination and toast state changes satisfy the updated lint checks while preserving existing interaction tests.
A new pagination regression preserves a valid page when client pagination starts with an already-selected page size.
The generated replacement hero image remains outside the repository; the installed workshop illustration is unchanged.

The test ledger covers clipboard failure and success, footer navigation attribution, rate-copy removal, flat avatar colors,
pagination configuration changes, toast fade cancellation, and shared-control keyboard behavior.
No backend input/output contract changed. Live datastore testing remains at the PR boundary.

### Shared ownership and enforcement checkpoint

Merged `origin/main` at `1b87e2a91` into this worktree with merge commit `49fa999ac`.
The pending alignment edits were restored without conflicts.

Shared `EYEBROW_CLASS` now follows interface typography. `NavItem` owns destination
styling for the sidebar, fleet sections, and runner sections. `UsageBar` owns its
solid fill without app CSS. Clerk actions consume the same CTA token pair as Button.
Human conversation text, recovery copy, onboarding labels, and metric labels use sans;
technical identifiers, source text, timestamps, and technical input values retain mono.

The design-token gate rejects consumer font definitions, arbitrary font utilities,
and app display typography. The app's syntax-tree regression check rejects local font
overrides on shared interface primitives, including aliased imports and local constants.
Four isolated-repository script tests cover permitted token references and rejected
font, palette, and gradient bypasses. ShellCheck passes without suppressions.

| Required test | Evidence |
|---|---|
| Shared active/inactive navigation and router composition | Three NavItem tests, including import through the public package entry |
| App primitive font ownership | Three syntax-tree tests, including every app page and component |
| Tokens propagate to rendered consumers | Three browser tests: both themes, changed sans token, changed pulse token, unchanged technical font |
| Gallery styling and accessibility | 43 browser checks passed |
| Source editing after extraction | Existing SkillEditor cases retained; technical panes extracted to keep the source file bounded |
| Async paging and dialog transitions | Assertions wait for the resulting error or dialog instead of reading before the transition commits |

The gallery was inspected at desktop width in dark and at narrow width in light.
This proves shared component rendering, not the authenticated app walkthrough.
Local Clerk publishable and secret keys remain absent. The app currently normalizes
its own theme to dark; shared-token light support does not imply an app light-mode switch.
The authenticated walkthrough and repository-wide PR verification remain outstanding.

Package validation after alignment: app `bun run test:coverage` passed 2,413 tests
across 239 files with 100% statements (6,021), branches (3,645), functions (1,624),
and lines (5,375). Shared components passed 545 tests across 57 files with all four
coverage measures at 100%. Website passed 156 tests across 24 files with all four
coverage measures at 100%. App production build succeeded. App, website, and shared
component Make lint targets passed. `make harness-verify` passed staged conformance;
gitleaks found no staged secrets. These are package and conformance claims, not a
replacement for the deferred repository verification commands.

Scoped review repaired the missing public navigation export, preserved the sidebar's
mint active state, and retained mono on lease identifiers while removing it from
surrounding labels. No new data-access, authorization, or backend contract was introduced.
