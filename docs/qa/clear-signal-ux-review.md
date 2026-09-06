# Clear Signal interface review

Status: UX review and local repository verification complete. PR gates and CI are tracked separately.

## Scope and method

Reviewed the app and website from their rendered pages, route components, shared controls, and tokens.
The app review uses genuine Clerk sessions for isolated development accounts and existing read-only fleet and runner fixtures.
Desktop, 390px, and 320px checks cover navigation, forms, keyboard focus, long content, and accessible recovery.
The website also covers light mode, enlarged text, reduced motion, Firefox, and WebKit.

Screenshots and safe measurements are under `.gstack/qa-reports/m189-ux/`.
The website designer report and inspected before-and-after images are under `.gstack/design-reports/website-final/`.
[Earlier design decisions](clear-signal-design-history.md) remain separate from current evidence.

## Designer findings and fixes

| Finding | Result |
|---|---|
| Dark surfaces and monospace prose weakened hierarchy | Brighter graphite surfaces, Instrument Sans for UI/prose, and separate technical and display roles come from shared tokens |
| Website sections and workflow roles looked alike | Lifted audience/pricing surfaces, corrected eyebrow scale, amber evidence marks, and blue delivery marks distinguish the sections |
| Mobile website navigation was hidden | Visible navigation exposes home, agents, workflow examples, and docs; active routes use mint |
| Workflow navigation from another page missed the section | Router links preserve the hash transition; target-position checks pass |
| Touch controls and focused fleet tabs could be clipped | Shared controls have 44px coarse-pointer targets; focused navigation items reveal themselves within their scroll container |
| Workspace/account controls overflowed narrow headers | Narrow header controls remain visible; account settings use Clerk's supported page at `/settings/account` |
| Dialog focus, table alignment, and pagination were inconsistent | Shared dialog focus returns to the opener; Created columns align; page-size and row-count changes reset pagination correctly |
| Failed reads could resemble successful empty lists | Initial read errors reach recovery boundaries or explicit unavailable states; regression tests inject failures |
| Missing pages and restricted access lacked useful recovery | Themed recovery pages keep an appropriate route back and a single main landmark |
| Workspace creation rejected a name the API could generate | An omitted name uses the existing generated-name response |

The review retained a flat graphite-and-mint design, with brighter semantic accents and consistent spacing.
Color is supported by labels, focus rings, and status text.

## Route and state ledger

| Surface | Reviewed evidence |
|---|---|
| Sign-in/sign-up and CLI authentication | Desktop/mobile auth states, input hover/focus, invalid email, local routing, and invalid CLI-session recovery |
| Account profile/security | Menu entry, direct route, security tab, edit/cancel, reload, and keyboard access at 1440/390/320px |
| Header/sidebar/workspace/Getting started | Expanded mobile navigation, long workspace names, menu focus return, account visibility, current/completed onboarding steps |
| Fleet wall/install/detail | Empty onboarding, populated wall and installation entry; chat, events, memory, Skill, and Trigger views; missing/malformed fleet recovery |
| Approvals/events/integrations | List and detail captures, missing-record recovery, and failed-read assertions tied to the shared recovery boundary |
| Models/secrets/API Keys | Populated tables, trailing-column keyboard scrolling, date alignment, sorting, creation/editing dialogs, and focus return |
| Billing | Usage, Invoices, and Payment method tabs; empty transaction states; unavailable/rejected read tests |
| Admin catalogs/runners | Operator and restricted-access views, runner header/actions, lists, leases/activity, long creation dialogs, and validation |
| Library and runner dialogs | GitHub/upload source tabs, missing-field validation, expanded content, and reachable Create/Cancel controls at narrow widths |
| Website | Home, Agents, Privacy, Terms, gallery, recovery, redirects, both themes, links, FAQ/workflow keyboard behavior, and mobile reflow |

A captured state proves that state only. Populated chat/billing and runner mutations were not exercised as live end-to-end business flows.
Failed-read behavior is also verified in component/page tests; the review does not claim a live failure injection into every backend endpoint.
The executable agent onboarding guide remains deferred by the user's recorded decision in the milestone spec.

## Completed checks before the main-branch integration

| Command | Result | Local log |
|---|---|---|
| `make test-unit-all` | All lanes passed: Rust 2351; app 2431; website 155; CLI 1646; design system 559 | `/tmp/m189-resumed-unit-all.log` |
| `make test-integration-rustd` | 365 passed against Postgres and Redis | `/tmp/m189-resumed-integration.log` |
| `make lint-all` | All lint checks passed | `/tmp/m189-resumed-lint-all.log` |
| `make dry-smoke` | Website 13, app browser 4, app smoke unit 1 passed | `/tmp/m189-resumed-dry-smoke.log` |
| Website full Chromium Playwright suite | 157 passed | `/tmp/m189-website-full-final.log` |
| Website Firefox/WebKit suite | 206 passed | `/tmp/m189-website-cross-final.log` |
| Website rebuilt gallery/accessibility/smoke checks | 56 passed | `/tmp/m189-website-final-build-browser.log` |
| Auth desktop/mobile suite | 10 passed | `/tmp/m189-auth-final-browser.log` |
| App account/header/navigation suite | 11 passed | `/tmp/m189-app-ux-acceptance.log` |
| Final visual capture script | 34 captures; no overflow, gradients, or Axe violations | `/tmp/m189-final-visual-resumed.log` |
| NavItem regression | Red reproduced; 9 tests passed after the fix | `/tmp/m189-navitem-green.log` |

App, website, and design-system coverage reached 100% of statements, branches, functions, and lines.
The CLI gate passed with 100% lines; its report marked functions ungraded because it emitted no per-function records.
These are the exact reported limits, not a claim of exhaustive behavioral coverage.

## Main-branch integration and final verification

M190 added immediate action feedback and live run summaries while this review was finishing.
The merge retains those changes and the Clear Signal presentation, focus, and failure-handling fixes.
The malformed/missing fleet and failed-history regressions now live in M190's split fleet-route suites.
Its exact pending-approval count replaces the earlier separate approval-list read.

The combined source passes 199 focused app tests and `make lint-app`.
The production app build passes; shared JavaScript is 266.53/276.48 kB gzip, fleet detail 61.36/102.4 kB, and runner detail 50.73/102.4 kB.
Website JavaScript remains 118.59/120 kB gzip and CSS 12.63/20 kB; the merge does not change website code.
Final canonical evidence on the merged branch:

| Command | Result | Local log |
|---|---|---|
| `make test-unit-all` | Rust 2355, app 2527, website 155, CLI 1646, design system 559 passed; all unit lanes passed | `/tmp/m189-merged-unit-all.log` |
| `make test-integration-rustd` | 377 passed; real Postgres and Redis | `/tmp/m189-merged-integration.log` |
| `make lint-all` | All lint checks passed | `/tmp/m189-merged-lint-all.log` |
| `make dry-smoke` | Website 13, app browser 4, app smoke unit 1 passed | `/tmp/m189-merged-dry-smoke.log` |
| `make check-version` | All versions match 0.28.0 | `/tmp/m189-merged-check-version.log` |
| `gitleaks detect --redact` | 5204 commits scanned; no leaks found | `/tmp/m189-merged-gitleaks-history.log` |
| Merged production app acceptance | 17 passed | `/tmp/m189-merged-app-acceptance.log` |
| Merged true-touch scenarios | 8 passed at 390px and 320px | `/tmp/m189-merged-app-touch.log` |

Merged app coverage: statements 6203/6203, branches 3784/3784, functions 1677/1677, lines 5531/5531.
The website and design-system coverage floors also remain at 100%. CLI line coverage passes; its function count remains ungraded by the runner.
The updated screenshots for runner controls, account access, and focused fleet navigation were inspected after the merge.

Companion documentation is [docs PR 186](https://github.com/agentsfleet/docs/pull/186).
It passes `make lint`, including 22 checker tests, Mintlify validation, and link checks.
No merge or deployment is part of this review.
