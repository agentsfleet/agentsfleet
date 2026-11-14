# M249_003 implementation validation

The implementation uses account-owned Plugins throughout the API, command-line interface (CLI), browser and current documentation.
Auth pages share the refreshed theme; Usage shows complete daily charges and recorded models.
The workstream remains active until its remaining verification completes.

## Candidate and baseline

| Item | Value |
|---|---|
| Branch | `feat/m249-plugin-vocabulary-ui-followup` |
| Base | `646fc536e9e504d0bda2db1742aab21e3e3896a8` |
| Dependencies | M249_001: `646fc536e`; M249_002: `baa26abed` |
| Candidate checkout | `/workspace/agentsfleet-m249` |
| Isolated baseline checkout | `/workspace/agentsfleet-m249-baseline`, detached at the exact base |
| Logs and screenshots | `/workspace/artifacts/m249-followup/` |

Continuous Integration (CI) did not run the full verification lanes on this base.
The local baseline therefore measures those lanes at the exact commit.
[The successful smoke run](https://github.com/agentsfleet/agentsfleet/actions/runs/43482399100) and [cache warm-up](https://github.com/agentsfleet/agentsfleet/actions/runs/43482250358) supply no full unit or integration counts.

## Repository checks

| Command | Exact base | Candidate | Evidence file |
|---|---|---|---|
| `make dry-app` | PASS: 478 files, 4,563 unit tests, 90 browser tests | PASS: 479 files, 4,575 unit tests, 94 browser tests | `baseline-dry-app.log`, `dry-app-final.log` |
| `make lint-all` | PASS | PASS through `orly gate verify` | `baseline-lint-all.log`, `gate-verify-complete.log` |
| `make test-unit-all` | Initial run failed in the container; all component reruns later passed separately | PASS: runner 1,731; daemon 172; libraries 3,674; four package coverage gates passed | `baseline-test-unit-all.log`, `baseline-test-unit-all-init.log`, `test-unit-all-schema-final.log` |
| `make test-unit-rustd-runner` | PASS: 1,731 tests with corrected mask | Passed within the full unit target | `baseline-rust-final.log`, `test-unit-all-schema-final.log` |
| `make test-coverage-all` | Four gates passed within the unit target | Four gates passed within the full unit target, each at 100% | `baseline-test-unit-all-init.log`, `test-unit-all-schema-final.log` |
| `make test-integration-rustd` | FAIL: 339 passed before one wall-stream timing failure; later suites not reached | PASS: 1,779 regular tests and 2 exclusive tests | `baseline-rust-final.log`, `test-integration-complete.log` |
| `make check-version` | PASS: all versions match 0.77.0 | PASS: all versions match 0.77.0 | `baseline-version.log`, `gate-verify-complete.log` |
| `orly gate work` | Not repeated | PASS: four commands declared | `gate-work-complete.log` |
| `orly gate verify` | Not repeated | PASS: configuration, documentation, lint and version | `gate-verify-complete.log` |
| `orly gate pr` | Not run | Not run: cloud shipping rule | Owner opens the Pull Request from the app |
| `make acceptance-local SUITE=cli` | Linux host cannot run the macOS adapter | Same host limitation | `acceptance-local.log` |
| `make acceptance-codex SUITE=cli` | BLOCKED: unexpected shared service | BLOCKED: unexpected shared service | `acceptance-codex.log`, `acceptance-codex-branch.log` |
| `gitleaks detect --redact --no-banner` | Not repeated | PASS: 8,108 commits; no leaks | `gitleaks.log` |
| `gitleaks protect --staged --redact --no-banner` | Not repeated | PASS: no leaks in the final staged candidate | `gitleaks-staged-final.log` |

The container's PID 1 does not reap orphaned processes.
The first baseline unit run failed two runner cleanup tests; both pass under `/usr/local/bin/docker-init -s` without changing tests.

A second environment mismatch produced mode `0400` where the toolbox test expects `0444`, because the inherited file-creation mask is `077`.
Final unit invocations use the init wrapper and mask `022` in both checkouts; no test changes apply.

The initial candidate coverage process received `SIGKILL` before returning a result; the container's memory counters recorded no out-of-memory kill.
The full candidate unit run uses a separate process session, with its process identifier and exit status recorded beside its log.
The interrupted `coverage-all.log` supplies no passing coverage claim.

The first candidate unit run passed 1,731 runner tests, then reused a baseline telemetry dependency from the shared Cargo directory.
Its daemon compile therefore rejected the new `PluginPublished` variant despite that variant existing in the candidate source.
Cargo cleaned all 59 workspace packages; each checkout now owns its build directory and reuses only copied external dependency outputs.

Baseline TypeScript coverage measured 4,563 app tests, 133 website tests, 2,336 CLI tests and 514 design-system tests.
The CLI reported 17 skipped tests; every configured coverage gate reported 100%.

The final candidate unit command exited successfully on November 14, 2026.
Its coverage gates measured 4,576 app tests, 133 website tests, 2,335 CLI tests and 514 design-system tests.
The CLI again reported 17 skipped tests; every configured coverage gate reported 100%.

The candidate's OpenAPI assertion found missing UUID formats on revision path and query parameters.
Both annotations now match the existing UUIDv7 validation; the targeted standards recheck found no issue.
The killed coverage process also left a lock; cleanup removed only that lock after confirming its owner had exited.

The generated-schema sweep then found the delete endpoint's missing Plugin identifier format.
That endpoint now uses the shared parameter type; the assertion checks all seven Plugin and revision identifiers across every operation.
The final full unit rerun passed, including the expanded assertion; this correction changes metadata rather than runtime validation.

Coverage then found an untested setup-charge label and malformed JSON path after the catalogue tests were removed.
The new checks verify the chart's setup label and exact amount, and refusal of malformed runner policy input before HTTP.
No coverage threshold or test guard changed.

The exact-base integration run timed out in `a_second_lag_inside_a_tick_re_announces_when_the_tick_ends`.
Its two-second wall-transition assertion failed; the candidate changes neither that test nor its stream helper.
The failed run does not establish a complete baseline integration count.

The candidate passed that same test and completed both integration groups.

The full repository lanes ran before the final test-only proof additions.
A fresh-schema focused run passed all ten Plugin API tests, including those additions.
The same run passed 11 authoring and 11 billing checks, for 32 focused checks in total.

Production code remains unchanged.

The first final gate found one Rust formatting mismatch and ten documentation language findings.
The corrections change formatting and prose only; the direct language scan now reports no findings.
The next gate required checked JSON access in the schema test; that assertion now uses `get` and `pointer`.

The final gate passed configuration, documentation, lint and version checks.
The separately rerun schema assertion also passed after its checked-access correction.

## Focused checks

These checks support individual changes; they do not replace repository lanes.

| Command and working directory | Result | Evidence file |
|---|---|---|
| `rustd/`: `cargo test -p afd_api --all-features --test tenant_plane authoring::plugin -- --ignored` | 10 passed, including the final proof additions | `plugin-integration-proof-final.log` |
| `rustd/`: `cargo test -p afd_authoring --all-features --test authoring_suite draft::plugin -- --ignored` | 11 passed | `plugin-integration-proof-final.log` |
| `rustd/`: `cargo test -p afd_billing --all-features --test integration_reports daily -- --ignored` | 3 passed | `plugin-integration-proof-final.log` |
| `rustd/`: `cargo test -p afd_api --all-features --test tenant_plane integration_billing_report -- --ignored` | 8 passed | `plugin-integration-proof-final.log` |
| `node /tmp/m249-grant-transition.mjs` | 8 transformation fixtures and 6 malformed-input checks passed; no live writes | `grant-transition.log` |
| `rustd/`: `cargo run --all-features --bin agentsfleetd -- --no-banner openapi` | PASS: four Plugin paths; seven UUID identifiers; required daily and fleet models | `openapi-complete.log`, `openapi-validation.log` |
| `rustd/`: `cargo test -p afd_api --all-features --test tenant_plane input_bounds::published_plugin_parameters_are_uuid_identifiers -- --exact` | 1 passed after checked-access correction | `openapi-assertion-checked-access.log` |
| `cli/`: `bun run lint` and `bun run build` | PASS | `cli-lint-build.log` |
| `cli/`: `bun run test` | PASS: 2,335 passed, 17 skipped; function and line coverage 100% | `cli-coverage-complete.log` |
| `ui/packages/app/`: `bun run lint` | PASS | `app-lint.log` |
| `/workspace/docs`: `make test` | 25 passed | `docs-test.log` |

The focused integration commands ran through a temporary make helper after `_reset-test-db` and `_migrate-test-db`, using make's exported test environment.

Plugin checks cover private account access, narrowed authority, removed routes, immutable revisions, conflicts and revoked permissions.
Daily billing checks cover midnight, late settlement, exact sums, cursor independence and historical model attribution.
The browser suite covers filtered Usage, both themes, narrow widths, exact large amounts, authoritative zero and unavailable reports.

## Visual review

Screenshots use production components at 1,480-pixel and 390-pixel widths in both themes.
The uploaded proposal supplies layout guidance; its sample numbers never become production defaults.

The sign-in copy emphasizes tasks, connected tools and reviewable results.
Public copy references include [Groq](https://groq.com), [Capy](https://capy.ai) and [OpenBots](https://openbots.ai), read on November 13, 2026.

| Surface | Evidence |
|---|---|
| Sign-in and signup | `sign-in-{1480,390}-{light,dark}.png`, `sign-up-{1480,390}-{light,dark}.png`, `auth-measurements.json` |
| Setup summary | `setup-summary-{1480,390}-{light,dark}.png`, `summary-measurements.json` |
| Usage | `usage-final-{desktop,mobile}-{light,dark}.png` |
| Team and Security | `team-{1480,390}-{light,dark}.png`, `security-{1480,390}-{light,dark}.png`, `settings-measurements.json` |
| Security Profile | `profile-section-{1480,390}-{light,dark}.png`; the design system requires this section |

## Reviews and vocabulary audit

The standards reviewer checked all 363 rules.
A final evidence check found inaccurate test names and missing assertions for saving a pinned draft and retaining installed copies after refused publication.
The test-only corrections add those assertions and cite the actual CLI tests and source scan.

The targeted standards recheck found no remaining issues.
Three native implementation reviews found no grounded production defects; the final pass covered every changed production file.

The final implementation reviewer summarized 127 test and fixture paths rather than reviewing every test line.
An outside-provider review was unavailable, and the core review remains incomplete while required verification is outstanding.

The core review stopped after three editing cycles without a complete pass requiring no edits.
It records zero unresolved findings; the corrected schema metadata and blocked acceptance checks prevent a completed review claim.

Gate review covered all 263 eligible hunks from the 288-hunk inventory, including the saved listing beyond the scanner's 50-hunk display limit.
The change weakens no hooks, coverage floors or assertions for retained behavior.

The frozen blast inventory records 967 files at the base, including unrelated model terminology and history.
The source audit classifies every remaining removed public spelling; none may remain an active caller or declaration.
The current scan classifies 194 matches, with no active callers or declarations and no unclassified matches.

See `M249_003_BLAST_INVENTORY.tsv` and `M249_003_SOURCE_AUDIT.tsv` beside this file.

## Outstanding environment and deployment work

The Linux acceptance adapter finds an older shared stack containing Dragonfly where the current adapter expects NATS.
That stack belongs to shared development state; this task has not removed it.

The required signed toolbox also cannot download: the network proxy refuses GitHub's blob destination with HTTP 403.
The cached toolbox supports version 0.73; the candidate requires 0.77.
These are incomplete acceptance checks, not waived checks.

Clerk grants remain unchanged.
The approved exact replacement still requires paused development traffic, verified token expiry and fresh-session authorization checks before deployment resumes.

The companion docs branch is `chore/m249-plugin-vocabulary-changelog` in `/workspace/docs`.
Its local schema checks pass, but Mintlify still loads the old remote OpenAPI document from main.

Repository instructions require committing those API pages after the application Pull Request merges.
This task keeps the companion branch together until that merge.

The owner withdrew the deleted fleet URL: "i will give you a nother one later".
No live-fleet success claim applies to that deleted draft.
Cloud shipping instructions prohibit pushing and running `orly gate pr` here; the owner opens the Pull Request from the app.

## Nov 14 account and fleet setup polish

The owner requested aligned Team counts and an identity card above keys and sessions, replacing the repeated Profile button and provider widget.
They also requested fleet-list expansion and a fuller fleet setup design.
The screenshots supply grouped-row inspiration; their environment-variable, privacy and VPN controls are not added to the product.

API Keys now shows avatar, name and primary email through the existing auth facade. The existing session reader, confirmation, recent verification and sign-out actions remain.

Team counts share the heading's center line. The Fleets disclosure retains loaded pages when closed and reopened.

The setup Summary opens a grouped review panel. It presents name, available source revision and runtime model, purpose, instructions, access, secret names, spending and timing.
Current saved schedules retain registration errors and retry controls separately from proposed triggers.

An older closed draft cannot supply the contents labelled as the current saved revision. A missing runtime-model field in a saved response is unavailable, not evidence of an unresolved default.

The implementation review found four projection/feedback issues and confirmed their corrections. The standards reviewer checked 220 applicable rules and closed with no remaining findings after the corrections.
These are targeted reviews of the polish diff; the earlier core-review limitation above remains.

Verification is recorded in `/workspace/artifacts/m249-followup/dry-app-polish-final.log`, `gate-work-polish.log` and `gate-verify-polish.log`.
`make dry-app` passed: 480 files, 4,582 app tests and 96 browser checks. A later coverage run includes the additional account-photo, stale-proposal and incomplete-proposal regression cases.

`make test-coverage-all` passed: app 4,585 tests, website 133, command-line client 2,335 passed with 17 existing acceptance skips, and design system 514. All four package coverage gates remain at 100%.
The fleet disclosure also passed a final rerun at 1480px and 390px through the native desktop rail and phone navigation dialog.

`orly gate work` and `orly gate verify` passed, including repository lint, version checks and documentation language checks.
The final chat/setup browser rerun passed all six checks after stabilizing fixture timestamps; see `polish-chat-final.log` beside the verification logs.

Reviewed screenshots are in `/workspace/artifacts/m249-polish/`, covering Team, API Keys and setup review in both themes and phone/desktop layouts, plus fleet expansion at both widths.
The browser fixture uses production components with sample identity and records; it does not prove authenticated development behavior.

The original proposal contains no `platform-priming` entry. A new fixture displays that key name but does not reproduce the owner's unspecified defect.

Dimension 5.6 was pending clarification at this review. The owner's later screenshot identifies the masked-key suffix and Last used fields.
Both need recorded backend metadata; the design proposal uses sample data and makes no implementation claim.

## Nov 14 model picker refinement

The owner requested removal of Show advanced models because it was absent from the approved design.
Create and edit now request the complete model catalogue when opened, and their picker includes all returned provider choices.
Paging, retry and selected identifiers retain their existing behavior.

The revised regression assertions failed on the parent implementation: four failed and 20 passed in `remove-advanced-red.log`.
`bun run --cwd ui/packages/app test tests/models- tests/model- tests/provider-model-select.test.tsx` then passed 199 tests across 23 files.
One duplicated retry case was removed because it differed only by the deleted switch state.

`bun run --cwd ui/packages/app test:e2e tests/e2e/resource-settings.spec.ts` passed six browser checks, covering both themes on desktop and phone.
Reviewed default-dialog captures are `/workspace/artifacts/m249-polish/desktop-create-model-default-dark.png` and `phone-create-model-default-light.png` beside it.

Logs are under `/workspace/artifacts/m249-followup/`, using the `remove-advanced-` prefix.
`orly gate verify` and `orly gate work` passed; their logs are `remove-advanced-verify.log` and `remove-advanced-work.log`.
The replacement fleet setup page still awaits the owner's requested design approval.

## Nov 14 compact chat errors and model-load diagnosis

The owner reported the live error disappeared after switching to a working model: "i flipped the model to a good one that works".
The original provider response was not captured. No claim identifies a missing credential, provider outage or deleted model.

The replacement URL reaches Vercel protection from this runtime; its task API credential receives `401 UZ-AUTH-002`.
That task-credential refusal does not establish the signed-in browser's failure.

Chat errors now flash for six seconds, pausing on hover or focus. Dismissal retains quiet recovery beside the composer and the current unsent text.
An uncertain save retains its original request key and body after the notice is dismissed.

Startup failures identify a failed model or chat-history read and retain only safe backend guidance.
Raw problem details remain hidden. Successful empty-model reads still show model configuration recovery; access refusals retain the private-data lock.

The initial regression run had three failed and eight passed checks in `chat-error-red.log`.
Targeted chat and transport checks then passed 631 tests across 65 files in `chat-error-unit.log`.
`make dry-app` passed 4,594 app tests and 98 browser checks in `chat-error-dry-app.log`.

The standards review found spacing, tooltip and pending-save proof gaps. Corrections use the existing spacing token and TooltipButton and extend the uncertain-save dismissal test.
Its targeted recheck closed without remaining findings across 220 applicable review-checked rules.

Final coverage, focused browser and gate results remain pending after those review corrections.
Logs are under `/workspace/artifacts/m249-followup/`, using the `chat-error-` prefix.

## Nov 14 final interface and shared-time verification

This batch supersedes the earlier grouped-review proposal and pending Summary approval. The owner approved the flat Summary, direct publication and live progress.
The latest Plugin capability reassessment is separate design work in M249_003 §7; these results do not establish capability implementation.

| Command | Result | Evidence under `/workspace/artifacts/m249-followup/` |
|---|---|---|
| `make test-unit-rustd` | 5,577 passed; 1,791 ignored integration cases | `final-unit-rustd.log` |
| `make test-integration-rustd` | 1,780 regular and 2 exclusive passed | `final-integration-rustd.log` |
| `make test-coverage-all` | App 4,631; website 133; CLI 2,335 passed, 17 existing skips; design system 516 | `final-coverage-all.log` |
| All four package coverage gates | Each package meets its configured 100% coverage floors | `final-coverage-all.log` |
| `ui/packages/app`: `bun run build` | Production build and type checks passed | `final-production-build.log` |
| Focused shared-time tests | 75 passed across three files | `final-shared-time-tests.log` |
| Focused native chat, schedule, typography, local-time and Usage browser checks | 44 passed across desktop and phone | `final-time-layout-browser.log` |
| Empty and invalid schedule browser checks | 2 passed, desktop and phone | `final-invalid-schedule-browser.log` |
| Focused send, retry, save, publication and ownership checks | 131 passed across ten files | `final-robust-recovery-tests.log` |
| Canonical proposal browser sweep | 16 checks passed across both themes and widths | `final-proposal-review.log` |

The final coverage correction removed an unreachable optional fallback from a required schedule array. No threshold, exclusion, assertion guard or harness changed.
Earlier stale assertions expected hidden memory rows or forced UTC labels; their replacements check the requested visible behavior.

The Rust lane uses the scoped init wrapper and file mask described above. Integration needed a task-owned runtime override for the cloud certificate and NATS proxy environment.
The normal localhost health probe returned `403` through that proxy; clearing the proxy for NATS returned `{"status":"ok"}`. No repository test or service configuration changed.

### Current behavior and edge cases

Setup Summary uses the separate 320px token, one-line package label and no internal internet-policy divider. Next wake-up appears only above the composer.
Compaction has one quiet progress row; routine memory bookkeeping stays outside conversation rows and disclosures. Run and Read use distinct terminal and open-book icons.

All visible clocks, date labels, ranges and relative labels use shared time utilities. Browser readiness prevents server-time flashes; wire timestamps and accounting month identities retain their established meanings.
The source audit found no page-local `Intl.DateTimeFormat` or forced UTC display outside the shared utility's accounting-month formatter.

Browser tests set Asia/Kolkata and America/New_York explicitly. They verify India's 7:00 PM wake-up, local billing activity and a date rollover near New York midnight.
The test labels describe browser timezone settings, not separate product display modes.

| Edge case | Confirmed behavior | Test source |
|---|---|---|
| Empty, whitespace-only, oversized and multibyte messages | Refused before writing; meaningful indentation remains unchanged | `tests/draft-client.test.ts` and draft input tests |
| Partial responses, cancelled reads and ownership changes | Original operation identity and scope remain authoritative | Draft request, recovery and ownership tests |
| Lost save or publication response | Retry retains the original request identity and body | Workspace saving and publication tests |
| Lost pause/resume acknowledgment | Retry reads committed state; one server write | `components/domain/schedules/WakeupStrip.test.tsx` |
| Empty or rejected schedule edit | No empty write; typed values and unsent chat survive refusal | `tests/e2e/chat-schedules.spec.ts` |
| Invalid dates, named timezones and daylight-saving boundaries | Shared fallback or correct local calendar result | Design-system time tests and timestamp server-render tests |

### Refactor and initial-load review

The Appsmith reference and installed Next.js guides informed feature boundaries and imports. Next dynamic imports defer inspection, history and selection editors until their controls open.
The final static import graph is `/workspace/artifacts/m249-refactor/imports-final.json`. WorkspaceChat reaches 52 local modules, versus 80 before; PluginWorkspace reaches 18, versus 49.
Those counts measure source dependencies, not transferred bytes or page-load time.

The production output emits Plugin inspection, revision history and selection editors into separate client chunks. The Plugin entry's initial chunk list excludes inspection and history.
The recorded chunk inventory is `/workspace/artifacts/m249-refactor/production-lazy-chunks.json`.

The domain audit examined 164 production files and found no byte-identical duplicate group. Remaining old chat-folder references outside implementation history describe historical review records.

The controlled four-fleet loader review returned its report in 40ms but waited 1,217ms when catalogue delay was 1,200ms.
Both reads started together; no per-fleet request loop occurred. These are injected local delays, not live Fly.io measurements.
See `billing-four-fleet-load-review.json`; M249_004 §2a specifies the single-call remedy and required live evidence.

The single visual reference remains `/workspace/artifacts/m249-proposal/visual-review-candidate-ed7c4032b-desktop-a68d0c18f6e545dd94f5ad4161786bd3/proposal.html`.
Latest native evidence includes `india-wake-up.png`, `india-usage.png`, `new-york-midnight.png` and `summary-policy-desktop.png` in the artifact directory.

Final repository gates and staged secret scanning remain pending after the new specification amendments. Independent final review could not complete because the review agents reached their usage limit.
Live authentication and acceptance limitations remain as recorded above. No development deployment or identity-grant mutation occurred.
