# Clear Signal design history

Historical checkpoints below are preserved as recorded. The current decisions are in the active M189 spec and interface review.

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
