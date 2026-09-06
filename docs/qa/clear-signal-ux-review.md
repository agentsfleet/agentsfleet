# Clear Signal interface review

Status: IN_PROGRESS. This report does not claim a completed review or repository verification.

## Scope and method

Review every app route and website page in the M189 worktree.
Check the rendered interface alongside its route, child components, shared primitives, and tokens.
Use isolated development accounts with genuine Clerk sessions.

The app preview is `http://localhost:3100`; the website preview is `http://localhost:5174`.
The app offers dark mode. The website and gallery also require light-mode checks.
Screenshots and computed styles live in `.gstack/qa-reports/m189-ux/` and are local review evidence.

Each route requires desktop and mobile inspection, including relevant loading, empty, populated, error, and restricted-access states.
Forms require empty and invalid submission, readable validation, hover, keyboard focus, cancellation, and confirmation checks.
Review navigation, deep links, browser history, long content, dialogs, reduced motion, and enlarged text.

## Findings

| Finding | Evidence | State |
|---|---|---|
| App input and textarea boundaries have no hover treatment | Shared `Input.tsx`, `Textarea.tsx`, and captured creation dialogs | Repaired; mint hover and visible focus use shared tokens |
| Onboarding markers use unsupported accessible labels on spans | Initial captures report axe `aria-prohibited-attr` | Repaired with readable step status; component tests cover each state |
| Explanatory copy inherits monospace | `EventDetailsDialog`, `FleetPayloadDisclosure`, model hints, runner checks | Repaired; prose is sans and technical values retain mono |
| Auth browser tests assert the previous palette | `tests/e2e/auth-theme.spec.ts` | Updated; final browser rerun pending |
| Clerk resets field borders and overrides focus | Initial `auth-states.ts` capture measures a zero-width border | Repaired through the typed appearance adapter; hover and focus captured |
| Clerk adds gradients to buttons, pseudo-elements, and footers | Initial auth captures and account menu inspection | Repaired; final account captures report zero gradients |
| Mobile workspace trigger pushes the account button off-screen | Initial account trigger ends at 445px in a 390px viewport | Repaired; trigger now ends at 374px |
| The onboarding credential step opens a missing route | `lib/onboarding.ts` pointed to `settings/secrets` | Repaired; link now opens the existing Secrets route |
| Unknown website URLs display an empty main area | Initial unknown-route screenshots | Repaired with a themed recovery page; website tests cover navigation home |
| Gallery overflows at mobile width | Initial gallery capture and shared Section layout | Shared Section sizing repaired; final visual recheck pending |
| Failed reads can appear as successful empty lists | Approvals, events, billing, secrets, integrations, and runner state handlers | Repaired with honest error or unavailable states; focused tests cover failures |
| Created headings and cells differ in alignment or mobile visibility | Secrets and API Keys tables; shared DataTable header padding | Repaired; browser checks cover column order and alignment at desktop and mobile widths |
| Closing a dialog loses focus to the page | Shared Dialog and cells rendered as newly created component types | Repaired; six live Edit, Rename, and Delete checks pass across two widths |
| Empty workspace names are rejected by the form | Rust accepts an absent or blank name and generates one | Form and response validation now accept generated names; focused tests pass |
| Invalid fleet links display a generic failure page | Backend rejects malformed fleet IDs with HTTP 400 | Repaired; malformed and missing fleet links show the themed recovery page |
| Nested recovery pages duplicate the main landmark | Dashboard not-found capture | Repaired through shared recovery content; corrected captures report zero axe violations |
| Fleet event read failures display “No events yet” | Fleet detail's initial events promise discarded its error | Repaired; original errors reach the existing recovery boundary |
| Clerk profile modal lacks an accessible name | Initial account capture | Replaced by the supported account page at `/settings/account`; final live account tests pass |
| Clerk profile menu buttons lack required ARIA parents | Initial account captures | Supported account page and navigation pass accessibility checks; no rules suppressed |

## Completed observations

The authenticated onboarding preview uses Instrument Sans and a flat graphite background.
The earlier user screenshot shows monospace navigation and an ambient gradient absent from this preview.
The Create secret dialog displays separate inline errors for an empty name, field name, and field value.

The initial captures used `--bg: #0c1113`. Current tokens use the brighter graphite `--bg: #131d21`.
Current website screenshots confirm the lifted palette; older captures establish interaction history only.
`corrections.json` records the computed background, font, overflow, gradients, and accessibility findings for each corrected page.
Its 18 app and website captures contain no gradients, page overflow, or axe violations.

`tokens.css` owns font families, light and dark colors, overlay color, and avatar saturation and lightness.
Shared components and the Clerk adapter consume those roles.
The design rules and enforcement responsibilities are documented in `docs/DESIGN_SYSTEM.md`.

## Scenario evidence and remaining review

Screenshots demonstrate a captured state; they do not prove every interaction or a completed visual review.
All paths below are relative to `.gstack/qa-reports/m189-ux/` unless another location is stated.

| Surface | Evidence available | Remaining work |
|---|---|---|
| Sign-in and sign-up | Desktop and mobile captures; input hover, focus, and pseudo-element inspection | Final tracked auth regression rerun |
| Account menu, profile, and security | `account-states.ts`; 1440px, 390px, and 320px captures | Resolve the two Clerk accessibility findings |
| Header, sidebar, workspace selector, Getting started | Authenticated route captures; mobile account boundary checks; step-state unit tests | Final expanded sidebar and narrow header visual pass |
| Fleet wall and installation | Empty onboarding, populated wall, install entry, and reduced-motion coverage | Close the route and scenario review ledger |
| Fleet detail | Chat, events, memory, SKILL, and TRIGGER captures at desktop and mobile widths | Final visual confirmation after shared table changes |
| Approvals, events, integrations | Empty and populated captures; failed-read and missing-record tests | Confirm remaining failure screenshots against their source states |
| Models, secrets, API Keys | Populated tables, creation dialogs, sorting, date alignment, and resource lifecycle checks | Rerun tracked date and focus checks after the final table correction |
| Billing | Usage, Invoices, and Payment method captures; unavailable and failed-read tests | Final visual pass through captured tabs |
| Admin catalogs and runners | Catalog tables, runner list, leases, activity, and restricted-access captures | Final long-dialog footer and expanded-field checks |
| Creation and editing dialogs | `popups.ts`: library GitHub/upload, catalog model, runner, model, secret, and workspace forms | Final popup pass; upload footer is reachable, runner footer needs explicit confirmation |
| Secret editing actions | Edit, Rename, and Delete; live Escape and focus assertions | Six checks pass in `/tmp/m189-table-focus-browser.log`; earlier popup focus failures are superseded |
| Recovery routes and CLI authentication | `corrections.ts`: unknown route, malformed fleet, missing approval, invalid CLI session, restricted access | Final image inspection |
| Website | Home, Agents, Privacy, Terms, gallery, and unknown-route captures; About removed | Final gallery, legal-page mobile, light-theme, and browser regression pass |

## Focused check results

The following commands ran in the M189 worktree on 2026-09-05.

| Command and working directory | Latest completed result | Log |
|---|---|---|
| `make lint-app lint-design-system lint-website` at repository root | All three lint and typecheck lanes pass | `/tmp/m189-ux-lint-latest.log` |
| `bun run test:coverage` in design-system | 58 files, 551 tests; 100% statements, branches, functions, and lines | `/tmp/m189-ux-design-system-coverage-latest.log` |
| `bun run test:coverage` in website | 24 files, 157 tests; 100% statements, branches, functions, and lines | `/tmp/m189-ux-website-coverage-final.log` |
| `bun run test:coverage` in app | 240 files, 2429 tests; 100% statements, branches, functions, and lines | `/tmp/m189-ux-app-coverage-latest.log` |
| `bun run build` in website | Passes after correcting the shared table header type | `/tmp/m189-ux-website-build-latest.log` |

The two app assertions matched a tooltip after closing a dialog and restoring focus to its opener.
They now check the dialog role and accessible name directly.
The shared focus regression tests fail before the rendering fix and pass afterward.

## Verification boundary

Browser exploration and package checks provide focused evidence.
The user deferred full repository unit and integration suites until immediately before PR preparation.
No repository-wide pass or completed UX review is claimed here.
Production app build, bundle budgets, final browser regressions, adversarial review, and checks over the staged diff remain pending.
No new commit, push, or PR was created during this review pass.

## Additional acceptance punch list

The user added these checks after reviewing deployed screenshots on 2026-09-05.

- Standardize every dialog’s title, description, fields, close control, cancellation, destructive action, and submit wording through shared components.
- Review both GitHub and folder-upload states in Create fleet library, including long content and reachable footer actions.
- Confirm Rust’s empty-name workspace behavior and make the creation form match that behavior.
- Review header branding, sidebar brightness, workspace selection, account menus, and their alignment at desktop and mobile widths.
- Review Getting started fonts, spacing, current-step emphasis, completed steps, and readable secondary text.
- Keep font families, backgrounds, and semantic colors in shared tokens. Remove gradients from application and Clerk surfaces, including pseudo-elements.
- Return keyboard focus to the opener after closing every dialog, including dialogs opened by table actions.


## Final website designer review

The website review now covers visual hierarchy, bright semantic colors, spacing, mobile navigation, and both color themes.
Phone navigation exposes home, agents, how it works, and docs. The active page uses mint.
Audience eyebrows retain their smaller scale. Brighter audience and pricing surfaces separate the page sections.
Amber evidence marks and a blue delivery mark distinguish workflow roles while preserving text labels.
Internal workflow navigation reaches the visible target section. Desktop anchors clear the header; mobile navigation scrolls with the page.
The recovery page describes its agent-resources destination accurately.

The detailed local report is `.gstack/design-reports/website-final/review.md`, with inspected before-and-after screenshots beside it.
Two coarse-pointer navigation tests pass at 320px and 390px; the original versions reproduced hidden navigation and the cross-page anchor failure.
Final Chromium, Firefox, and WebKit logs remain the authority for completion: `/tmp/m189-website-{full,cross}-final.log`.

## Final app review evidence

The app/shared diff review is recorded in `/tmp/m189-app-review-ledger.md`.
It covers failed initial reads, optional workspace names, account navigation, dialog focus, table focus, and pagination state.
A new test proves the original billing balance-read error reaches the recovery boundary. The focused suite passes 17 tests.
Eleven live account, header, and navigation scenarios pass in `/tmp/m189-app-ux-acceptance.log`.
True-touch runner controls measure 44px at 320px and 390px. Keyboard scrolling reaches hidden model-table columns.
A readonly source remains unchanged while ArrowDown scrolls its content after native resizing.
The final touch review found a partly clipped focused fleet tab at 390px.
The shared navigation item now scrolls only its overflowing navigation container when focused.
Tab alone reveals the whole label. The regression suite fails before the fix and passes all nine cases afterward.

The full website Chromium suite passes 157 tests in `/tmp/m189-website-full-final.log`.
Firefox and WebKit completion remains pending.
