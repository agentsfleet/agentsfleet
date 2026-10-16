---
type: explanation
audience: contributor
verified: 2026-10-16
product_version: 0.61.0
executable: false
---

# Website and app completion review

## What it is

This records reviewed user interface (UI) work for one combined Pull Request (PR).
[M227_002](../../v2/done/M227_002_P1_CLI_DOCS_UI_WEBSITE_BRANDING_BATCH.md) covers the website and branding.
[M227_001](../../v2/active/M227_001_P0_API_CLI_DOCS_UI_WORKSPACE_CHAT_CUTOVER.md) keeps the backend, live journeys and release checks.

M227_002 is DONE and moved to `docs/v2/done/`; all 135 browser checks pass.
The app setup and API alignment updates are complete at package scope. Indy signed off the website and app changes.

## Why it exists

Completed interface work needs evidence without implying that the combined release is ready.
Indy approved moving pending runner-label checks into M227_001 and closing the reviewed website scope.
Indy also confirmed: “since M227_001 and M227_002 will go in the same pr”.
His visual signoff covers both surfaces: “yes signed off website, app changes”.
The signoff approves the interface changes; it does not waive the remaining technical or release evidence.

## How it behaves

### Reviewed implementation

| Scope | Result | Source and proof |
|---|---|---|
| Website entry and motion | Real links, private composer, full transcript, evidence, pause/resume, reduced motion and mobile navigation remain covered. Ordinary Save does not claim Library publication. | Website story, components and all 135 browser checks below. |
| Keyboard and gallery | Actual transcript viewport has the log role and keyboard focus. Gallery pagination has distinct navigation labels. | `IncidentFlow.tsx`, design-system `Pagination.tsx`, `DesignSystemGallery.tsx`; one approved inline exception keeps keyboard scrolling reachable. |
| Analytics privacy | Final outgoing events lose URL/referrer/campaign/element/person properties. Unused flags/config requests are disabled. | Website `posthog.ts`; enriched-event and init-config regressions; scoped source review reports no findings. |
| Failure isolation | Failed analytics loading/capture and clipboard access preserve the visitor action. | Website analytics and landing-click tests. |
| Branding | Four icon consumers match the canonical 487-byte mark. App metadata serves the branded icon. | Asset digest and dimension audit; earlier production icon response HTTP200, `image/svg+xml`. |
| Installer | Installs the command-line interface (CLI) only; no product skill installation remains. | Served installer, installer smoke tests and website Fleets copy. |
| First login | Empty workspaces render the fleet dashboard and New fleet chat entrance. The obsolete five-step checklist and isolated state are removed. | App route and component tests; matching-backend first-login journey remains open. |
| App setup | Required nonempty description, sole definition name in `config.name`, config edits and only `replace /description`. | Four production modules, shared/acceptance fixtures and regression checks; app make results below. |
| Runner interface | Edit/Test controls and plain Open cue match the approved interface; placement labels prefill, clear and save through the existing runner PATCH. | Earlier screenshots in `/tmp/m227-ui-resume/handoff.md`; new transport/action/dialog regressions in the app run below. |
| App API alignment | Optional policy/labels edit fields preserve untouched values; removed key/event members and dead list calls are gone; stream identifier/kind/status are required. | Fourteen production modules and 46 tests/helpers/fixtures listed in [file inventory](file-inventory.md#app-api-alignment-completion). |

The earlier transcript wrapper separated the log from its scroll state and broke two unit checks.
Restoring the log on the actual viewport fixes those checks without changing their assertions.
Indy approved exactly one new inline lint exception, then confirmed ‘approved’.
A temporary focusable sibling still failed lint while the log line was suppressed; the sibling was removed.
The final source search has exactly two suppressions: the existing PostHog test and the transcript comment.

### Package and browser evidence

Commands below describe scoped evidence, not repository-wide verification.
Final approved-comment checks are in `/tmp/m227-transcript-focus/`. Full logs also live under `/tmp/m227-spec-review/`, `/tmp/m227-setup-shape/` and `/tmp/m227-cli-only/`.

| Command | Real result | Log |
|---|---|---|
| `make lint-app` | App source clean; 10 checker tests OK; app lint passed. | `/tmp/m227-api-trigger/lint-app-final.log` |
| `make test-unit-app` | 369 files; 3430 tests passed. | `/tmp/m227-api-trigger/unit-app-final.log` |
| App `bun run test:coverage --maxWorkers=4` | 395 source files; statements 9936/9936, branches 6162/6162, functions 2721/2721, lines 8508/8508; all 100%. | `/tmp/m227-api-trigger/coverage-app.log` |
| App `bun run build` | Compiled successfully in 15.4s; TypeScript and 5/5 page generation passed. | `/tmp/m227-api-trigger/build-app.log` |
| App focused randomized unit run | 5 files; 82 tests passed with shuffle seed 227. | `/tmp/m227-api-trigger/randomized.log` |
| `make test-unit-website` | 20 files; 134 tests passed. | `/tmp/m227-transcript-focus/unit-website.log` |
| Website `bun run test:coverage` | 28 source files; statements 411/411, branches 171/171, functions 137/137, lines 354/354; all 100%. | `/tmp/m227-spec-review/website-coverage-close.log` and package coverage summary |
| `make test-unit-design-system` | 62 files; 495 tests passed. | `/tmp/m227-spec-review/ui-unit-final.log` (design-system result only) |
| Design-system `bun run test:coverage` | 69 source files; statements 517/517, branches 541/541, functions 190/190, lines 470/470; all 100%. | `/tmp/m227-spec-review/design-system-coverage.log` and package coverage summary |
| `make lint-design-system` | Design-system lint passed. | `/tmp/m227-spec-review/ui-checks-final.log` (design-system result only) |
| Website `bun run build` | 2191 modules transformed; build passed. | `/tmp/m227-spec-review/website-build-close.log` |
| Website `bun run size` | Landing 108.67 kB gzipped / 128 kB limit; CSS 14.67 kB / 20 kB limit. | `/tmp/m227-spec-review/website-size-close.log` |
| `bash ui/agentsfleet.dev/install_test.sh` | 28 passed, 0 failed. | `/tmp/m227-cli-only/installer-green.log` |
| `make lint-website` | PASS: Oxlint and TypeScript; website lint passed. | `/tmp/m227-transcript-focus/lint-website.log` |
| `make _dry_website` | 135 passed (2.4m); website dry pass passed. | `/tmp/m227-transcript-focus/browser-website.log` |

The app description regressions first failed in three checks; the final app suite passed afterward.
The flags regression and two transcript regressions first failed; all website unit checks now pass.
No thresholds, exclusions, retries or skips changed. Fresh app coverage passes all four configured 100% thresholds. The first run exposed one unreachable missing-timing fallback; it was removed, and the full app unit/coverage, lint, build and final source review passed afterward.

### API-trigger removal and webhook editing

The [follow-up inventory](file-inventory.md#api-trigger-follow-up) records 20 app files, including one new validator and no deletions.
Defaults use no external triggers; readers accept an omitted list; the empty view explains chat and steer access.
The editor validates webhook slugs, reserved sources, supported providers or explicit Svix secret references, and removed signature keys.
Mention sources retain their existing behavior. Invalid edits remain visible without a write; daemon-safe 400 messages remain intact.
Save parsing retains source-specific webhook addresses, and the browser acceptance helper uses the returned address.
The D13-owned opening comment in `events.spec.ts` remains unchanged.

Focused red/green proof: 21 failed and 47 passed before the fix; five files and 82 tests passed with shuffle seed 227 afterward.
The first full app run exposed one stale route assertion: one failed and 3429 passed. Its expected text now matches the new empty view.
The final `make test-unit-app` passed 369 files and 3430 tests; app lint and production build passed.
The eleven-row follow-up ledger is `/tmp/m227-api-trigger/required-test-ledger.md`; live matching-backend delivery remains assigned to Claude.
All nine production hashes passed native source review with NO_FINDINGS; tests/fixtures were reviewed in summary mode only.
App coverage after the follow-up: 395 source files; statements 9936/9936, branches 6162/6162, functions 2721/2721, lines 8508/8508.
The removed-variant search keeps five hits, each deliberate:

| Hit | Reason |
|---|---|
| `lib/api/fleets.test.ts:85` | Negative response-decoding regression rejects the deleted variant. |
| `tests/workspace-chat/editor.test.tsx:81` | Negative editor regression rejects the removed timestamp-header key. |
| `app/cli-auth/[session_id]/page.tsx:156` | Clerk sign-in token template named `api`; unrelated to triggers. |
| `tests/cli-auth-approval-transport.test.ts:54` | Asserts that same token template. |
| `tests/e2e/acceptance/fixtures/constants.ts:5` | Documents the same sign-in template boundary. |

### Runner edit and stream review

Runner edits use `PATCH /v1/fleets/runners/{runner_id}` with policy, labels or both in one request. Labels replace the whole set; `[]` clears it. The reply includes nullable policy, labels and self-test request time. The dialog preserves the backend's 400 sentence and retains edits after failure.

The final source review caught two regressions: policy-only saves reparsed untouched labels, and labels-only saves included policy, which the backend refuses on revoked runners. Both are fixed. Eight dialog regressions failed before the fix; all twenty dialog tests passed afterward. Unchanged label input is omitted; changing labels alone omits policy; changing both sends one combined body. Intentional label entry retains the requested comma-separated `parseLabels` format.

The live tool-start wire type has no outcome; completion and saved tool-call types require status. The pending/no-result renderer remains because a started call can still be waiting, and stream delivery is best-effort. Required fields are checked by frame readers; malformed frames are dropped. Optional result fields remain optional.

The requested removed-field search has zero hits. Dead Library listing functions and their tests are removed; `removeWorkspaceLibraryEntry` and its live DELETE caller remain. No files were deleted in this app alignment; two test files were added.

The resolved eleven-row test ledger, exact removed-field search and scoped report are under `/tmp/m227-app-wire/`. Earlier setup-shape and installer reports remain supporting evidence.

The preview was rebuilt and restarted from this worktree at `http://localhost:3100`; the updated dialog title is **Edit runner**, with **Labels** below Sandbox mounts. The agent browser reached the sign-in page, so authenticated dialog rendering was not independently re-observed. Indy's explicit app signoff is recorded above; matching-backend journeys remain separate.

### App test proof ledger

The app make run supplies the package result. Live backend assertions remain explicitly separate.

| Changed behavior | Required proof | State |
|---|---|---|
| One runner edit PATCH and nullable reply | Transport checks for policy-only, labels-only, clear and both; exact reply and no second request | PASS: runners-edit.test.ts; full app make run |
| Runner label prefill and replacement | Stored values shown, clear sends [], both edits one Save | PASS: EditPolicyDialog.test.tsx; 20 dialog tests |
| Unchanged fields preserved | Untouched comma/Unicode/space/empty labels omitted; refreshed props do not replace labels; labels-only omits unchanged policy | PASS: regression run red 8 failed/12 passed, green 20 passed |
| Runner write authorization and refusal | Write-scope refusal, authentication, 400 sentence retained with open dialog and no implicit retry | PASS: runners-actions.test.ts, runners-edit.test.ts and dialog tests |
| Label validation | Invalid grammar, 65-byte ASCII label, 33 labels; blank and dedup | PASS: runners-edit.test.ts and dialog tests; unchanged stored values bypass entry parser |
| API key activity removal | Creation/revocation remain, removed activity copy absent | PASS: api_keys.test.ts, ApiKeyList.test.tsx and API key fixtures |
| Removed event member | Final row reader and fixtures omit removed member | PASS: event/stream tests; exact search has no hits |
| Required live stream members | Missing/invalid identifier, status and text kind dropped; repeated/late completion remains keyed by call | PASS: fleet-stream-required.test.ts and stream suites |
| Saved tool calls and pending display | Saved status required; optional output absent is allowed; live starts retain pending state until completion | PASS: saved-trace and settling suites; unchanged pending component retained |
| Dead Library list removal | List symbols have no callers or remaining match; live DELETE still called | PASS: fleet-library.test.ts and library action tests; exact search has no hits |
| Required type fixture updates | Positive fixtures supply identifier/status/kind; malformed reader tests deliberately omit required fields | PASS: make lint-app and full unit run; browser acceptance fixture execution remains pending in M227_001 |

### Search and asset review

The exact requested `skill` word search under `ui/packages/app`, excluding `node_modules`, returned no matches after the setup update.
No generated OpenAPI DefinitionSkill or FleetDefinition types exist under that app directory.
The backend owns public schema generation.

The earlier removal audit found twelve unrelated avatar or Continuous Integration (CI) report-upload references.
Every remaining hit and deleted file is classified in `/tmp/m227-cli-only/report.md`.
No served installer match remains. Removed files went through `trash`.

Canonical icon digest: `1876f1ce392f87ec2e4322b205776bb0123bbcddc839c0da9d7abd21a3a651d4`.
App icon, app public brand, website favicon and design-system favicon each match its 487 bytes.
Brand avatar sizes are 1024×1024 and 512×512 pixels; social cards are 1280×640 pixels; touch icons are 180×180 pixels.
The asset audit is `/tmp/m227-spec-review/assets.json`.

### Source review

Native source re-review found no remaining issue in the scoped analytics changes.
It verified the final event filter after enrichment and the installed library's disabled flags/config request path.
Tests and fixtures were reviewed in summary mode only by that reviewer.

The website native review record is `/tmp/m227-transcript-focus/native-privacy-review.md`.
The fresh app review is `/tmp/m227-app-wire/native-final-review.md`: NO_FINDINGS after both runner fixes, all fourteen production file hashes matched. Its tests/fixtures were reviewed in summary mode only. The earlier finding report and both fresh-token receipts are retained in that directory.
It does not claim whole-branch or outside-model approval.
Final combined review stays with M227_001.

## Limits

M227_001 remains active. Its live database runner checks and next-poll placement proof remain open as Dimensions 8.4 and 8.5.
All original runner validation, read-back, atomicity, replay and placement assertions remain in its test table.
The transferred 8.1–8.3 results retain their previous verification record; no fresh Rust run is claimed here.

Matching-backend draft/Save/improve, joined-member recovery and first-login browser journeys remain open.
Repository conform, lint, unit/coverage, integration, version and coordinated release/recovery checks remain required in M227_001.
Rust email template source was reviewed; its final runtime checks remain in that combined gate.
App-dev analytics observation remains a required human check after deployment.

Five durable server funnel event definitions and their privacy tests now exist in Rust source.
Earlier handoff notes claiming those events were absent are superseded; backend runtime results still need verification.
React Compiler comparable on/off measurement remains parked by Indy.

No staging, commits, stashes, resets or reverts were performed.
The 1248 staged paths retain cached binary-diff digest `3368bd83c65e7cc1cade7874890f68ce67da7ede475a3b2583294ab604f7c9d3`.

## Related pages

- [Product analytics](../../architecture/product_analytics.md)
- [Workspace setup review packet](review-packet.md)
