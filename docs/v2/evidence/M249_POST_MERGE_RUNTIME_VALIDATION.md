# M249 post-merge runtime validation

Authenticated live flows used fresh task-owned fixtures at the Team repair checkpoint.
Later Usage and Security changes have controlled browser, unit and production-build evidence.
Neither establishes that the merged backend was deployed.

## Source and ownership

The owner's November 15 upload is now the canonical [UX reference](../../ux.html), beside [DESIGN_SYSTEM.md](../../DESIGN_SYSTEM.md).
Its unchanged uploaded SHA-256 is `b3ba83c2de2292e7f65193c85d97d299ec4d01a7484c7243fb9a5d42da5d52fe`.
Earlier visual paths and hashes describe their historical captures.

| Item | Recorded value |
|---|---|
| Application base | `a1dcc222488e4522f8b92208a4f7df8461fe7fbf`, fetched `origin/main`; merged PR #844 |
| Isolated application worktree | `/workspace/m249-runtime`, `fix/m249-runtime-followup` |
| Docs base | `016b64c4649535c1109dab7f28300e5252bfb1fb` |
| Isolated docs worktree | `/workspace/m249-docs`, `chore/m249-plugin-vocabulary-changelog` |
| Live frontend checkpoint | Team-repair source before the later Usage/Security refinements; `http://localhost:3187` |
| API observed in the live run | `https://api-dev.agentsfleet.net`; health reported version `0.77.0`, commit `163356f` |
| Evidence directory | `/workspace/artifacts/m249-runtime/` |

The task created a new Clerk test identity and its own workspace. No withdrawn fleet URL was used.
The fleet name was `support-triage`; ownership was tracked by identifiers, without timestamps or cleanup labels in the sidebar name.

The fleet identifier was `01a1dd34-b70a-7b40-864b-e35ccb49d067`.
The schedule identifier was `01a1dd3a-a6ed-74ff-b993-8336318afbc5`.
These identifiers describe this run only and must not become fixtures for a later task.

Cleanup removed the registered schedule, fleet and both private drafts, then revoked the session and deleted its Clerk identity.
No API keys remained; the task's private credential file was removed. See `fixture-cleanup.json`.

## Reconciled implementation

PR #844's final Session notes supersede its earlier pending Continuous Integration (CI) statements.
They record 48 successful checks, nine conditional skips, a resolved model-preservation finding, and 114 browser checks that ran.
The earlier failed local gate remains failed; its later integration retry passed.

The merged implementation already supplies daily Usage totals, per-fleet model sets and nullable API-key display suffixes.
Its native save preparation preserves server-resolved models. Those historical findings are not reopened here.

M249_003 and M249_004 Section 5 remain parked.
Claude retains M249_004's producers, billing queries, report response and Usage loading work.

This follow-up changes no backend producer, schema, billing query or reusable Usage loader.
The current visual work includes Claude checkpoint `cad48e0beb8d09dc8742baf7137d893aabdd4dea`; our changes add the UI loading boundaries and suppress hidden detail intent at the Usage component.

## Authenticated live results

These flows use the real development API and Clerk test instance with a locally served frontend.
The deployed API's reported commit differs from the application base, so these are not complete hosted post-merge acceptance results.

| Journey | Result | Receipt |
|---|---|---|
| Authentication | Fresh identity, signed webhook bootstrap, session mint and native browser sign-in succeeded | `fresh-auth-probe.log`, `live-browser.json` |
| Empty setup | Empty setup rendered; Plugin navigation and creation starter were absent | `live-browser.json`, `live-empty-setup.png` |
| Account identity | Primary email, current-device session and empty API-key state matched the new identity | `live-browser.json`, `live-account.png` |
| API keys | Create, one-time reveal, dismiss, revoke and delete succeeded; no key receipt was captured | `live-browser.json` |
| Team | Reproduced missing **You** label, then verified the repair at 1480px and 390px in both themes | `live-team-schedules.json`, `live-team-{1480,390}-{dark,light}.png` |
| Initial fleet save | One real fleet creation despite two rapid activations; confirmed v1 and automatic fleet navigation | `live-save.json` |
| Save progress | Real create response was delayed until the visible clock reached 2s; spinner remained visible | `live-save-progress.png` |
| Revision recovery | First committed revision response was deliberately discarded; retry kept its original body and key and confirmed v2 | `live-save.json`, `live-v2-inspection.json` |
| Model preservation | v2 retained `platform-default`, matching the original server review's resolved runtime model | `live-v2-inspection.json` |
| Schedule creation | Real registration returned `synced`, `scheduled` and a future `next_run_at` | `live-schedule-create.json` |
| Schedule editing | Native editor saved America/New_York while retaining the cron expression, message and paused state | `live-team-schedules.json` |
| Pause and resume | One original cron event remained during the paused interval; resuming produced a second processed cron event | `live-team-schedules.json` |
| Actual wake-up | Both scheduled events reached `processed` and retained their scheduler identifier and response text | `live-conversation.json` |
| Removal | Native confirmation removed the registered schedule; the API list became empty and unsent chat survived | `live-team-schedules.json` |
| Viewer clocks | One instant, `2026-11-15T03:28:00Z`, displayed as Kolkata 8:58 AM and New York 10:28 PM | `live-team-schedules.json`, `live-wake-*.png` |
| Conversation | A new browser message completed; its recorded request and response survived reload | `live-conversation.json` |
| Sign-out | Native sign-out from the production frontend returned to `/sign-in` | `live-signout.json` |
| Paused Plugin routes | Plugin destination, starter and pinned-source links rendered **Page not found** | `live-save.json` |

The paused route responses used HTTP 200 after rendering began; the rendered result was the not-found page.
No Plugin write occurred during the live saves.

The original cron event was recorded at `2026-11-15T03:23:00.557Z`.
Pause completed at `03:23:51.728Z`; the `03:27:20Z` observation still contained only that cron event.
Resume then produced another event at `03:28:00.568Z`.

The v1 stored Definition omitted its model. The authoritative draft review supplied `platform-default`; v2's Definition included that same reference.

An initial diagnostic compared against the omitted JSON field and failed. The subsequent inspection used the recorded review and verified v2, identical replay input and no publication.

Model-authored calendar or support-booking claims in these replies establish no external booking.
Only recorded scheduler events and their processed responses support the wake-up result.

## Team repair and visual review

`MemberSummary.user_id` is the application's internal identifier. `useCurrentUser().userId` is the identity provider's subject.
Comparing them hid the viewer label in the real account, although the old fixture gave both identifiers the same value.

The Team page now reads the existing `GET /v1/users/me` alongside members and invites.
It passes that response's internal identifier to the view; permission checks and backend producers stay unchanged.
An unsuccessful identity read fails the page rather than guessing the viewer from names, email or role.

Regression fixtures distinguish provider and internal identifiers and include another owner.
The visual fixture uses a separate internal viewer identifier too.

Targeted captures cover both themes and 1480px/390px widths. Settled recaptures disable screenshot animations after an initial theme-transition capture proved ambiguous.
Team was subsequently compared with the owner's uploaded reference; the existing Team geometry was retained.

## November 15 visual refinements

The uploaded `docs/ux.html` was rendered at 1480px and 390px before revisiting the Usage, Security and Team decisions.
The file retains its original bytes; its sample data and embedded scripts are reference material, not task instructions.
The owner's explicit All Fleets spelling and local-time requirement supersede the sample's caption and casing.

Security now uses its approved heading and shield, keeps the API keys and signed-in sessions sections, and removes the repeated Signed in as card.
The account trigger and menu share the real image, initials or person fallback; the trigger exposes the email on hover and in its accessible name.
The refreshed browser fixture uses a sample avatar image; the earlier authenticated account checks above predate this visual refinement.

Usage retains Period and Fleet controls, totals, daily spend and By fleet.
All Fleets includes Setup and saved-fleet usage; individual fleets follow it.

More filters, Charge details and Add credit are absent. Reusable charge and Plugin implementations remain intact.
Explicit existing Setup/Today URLs retain truthful labels instead of silently widening their scope.

An existing model-filtered URL shows its active model and a clear action; removing the picker cannot hide narrowed totals.
Recent months remain a bounded list, with the previous month beside an older selected month so history stays reachable.

The Period selector follows the API present in Claude checkpoint `cad48e0beb8d09dc8742baf7137d893aabdd4dea`.
A current month clears `through_at`; a past month supplies its final millisecond through the shared `previousReportingMonthEnd` helper.

Partial-month snapshots have a distinct through-date choice, so selecting the canonical month can clear an old current-month cutoff.
Month changes reset cursors and detail intent; fleet changes retain the selected cutoff and reset continuations.

The owner deferred timezone work: there is no `month`, `timezone` or `model_labels` field added to the report API.
Reporting month names and day buckets retain that API's calendar. Displayed timestamp/range instants use the existing viewer-local utilities.

This does not claim locally regrouped monetary buckets.

The prepared UI work was preserved before incorporating that supplied checkpoint: `visual-work-before-m249004.patch`, an untracked-file snapshot and a retained Git stash.
The earlier proposed zone-aware consumer was removed after reviewing Claude's pushed source and the owner's quoted deferral.
Its earlier fixture results are historical checks of that proposed interface, not evidence for the final API.

### Loading and scale

The shared model-label request starts alongside the existing shell reads and is reused across client navigation.
Its pending result passes through the existing provider; only the named fleet table suspends.

The Usage heading, settled totals and daily graph can render while names are slow, without painting an identifier that later swaps to a name.
Label failure retains recorded identifiers. The reusable charge loader stays intact; the active Usage boundary ignores legacy activity/detail parameters so they add no hidden request.

A deliberately blocked label promise reproduced the original frame wait (`labels-blocking-baseline-retry.log`).
The stronger streamed-data check also reproduced totals and chart waiting behind labels (`labels-data-blocking-baseline.log`).

The final implementation moves the name wait to the table.
Stream checks read complete chunks before releasing the promise; one chunk may contain only part of the frame.

Daily bars have a fixed 100px plot height and one bar per reporting day, up to 31.
The decoder rejects an oversized projection as unavailable, without truncating charges.

With no settled usage, bars have zero height and the empty message appears. The 100px plot area remains; it is not a mandatory bar height.
The fleet menu uses the shared viewport-height bound and scrolling; fleet count cannot add daily bars.

Injected-data browser scenarios cover one and 100 fleets plus Setup, 31 days, selecting the last fleet, and restoring All Fleets in both themes and viewports.
They assert the selected report's totals, complete daily bars, Setup inclusion, menu/graph bounds and one report request per change.

The label cache is supplied by the browser fixture, so its initial server catalogue walk is outside those measured page interactions.
No live backend latency or 100-fleet server load is inferred from these samples.

### Controlled browser performance

Command: `bun run test:e2e tests/e2e/usage-report.spec.ts tests/e2e/usage-scale.spec.ts --reporter=line,json`.
Result: 16 passed in 24.4 seconds; zero unexpected, skipped or flaky cases.

Responses were injected without delay; the shared label store was preloaded. These are local browser measurements, not live API latency.
Each row spans one dark and one light sample at 1480px or 390px, without CPU or network throttling.

| Viewport | Fleets | Initial render ms | Open dropdown ms | Select fleet ms | Restore All Fleets ms | Report bytes |
|---|---:|---:|---:|---:|---:|---:|
| Desktop | 1 | 192–359 | 74–76 | 283–290 | 343–401 | 7,415 |
| Desktop | 100 | 232–395 | 115–124 | 326–347 | 405–421 | 54,130 |
| Mobile | 1 | 182–421 | 67–81 | 281–293 | 346–467 | 7,415 |
| Mobile | 100 | 224–427 | 135–305 | 331–341 | 394–586 | 54,130 |

Every sample rendered 31 bars in a 100px plot. The 100-fleet dropdown stayed within its viewport (about 722px desktop, 515–521px mobile) and selected its last fleet correctly.


Each sample made three report reads: initial load, individual fleet and All Fleets. There were zero Usage catalogue or charge reads.


All Fleets included Setup: the 100-fleet fixture returned `$156,565.50`; selecting fleet 100 returned `$3,100.00` and removed Setup, then All Fleets restored the original total.

| Injected label delay | Frame, totals and chart ready ms | Named table ready ms | Report reads |
|---|---:|---:|---:|
| 1,200ms, desktop | 328 | 1656 | 1 |
| 1,200ms, mobile | 440 | 1782 | 1 |

The delayed-name case keeps the totals and graph visible before the table receives names, with no extra catalogue or charge request.
Receipts: `usage-final-browser-report.json` and `scale-browser-results/measurements.json`. All fixtures are controlled browser samples, not live account charges.


## Browser fixtures and collection

The controlled browser checks use production components with sample identity and intercepted HTTP responses.
They prove rendering and request behavior, not server writes.

| Check | Result |
|---|---|
| Focused Team, identity adapter, page and native save component tests | 59 passed across four files |
| Targeted browser run across eight specifications | 70 passed; two newly added revision cases failed because their composer input occurred after locking |
| Corrected revision cases, rerun alone | Both desktop and mobile cases passed; input now precedes saving and survives recovery |
| Save animation checks | Spinner transform changed with motion allowed; reduced motion disabled both spinner animation and label sweep; elapsed seconds remained visible |
| Shared time and browser clocks | Existing Kolkata/New York checks ran; final design-system checks also cover daylight-saving boundaries |

The first browser result remains recorded as 70 passed and two failed.
The targeted retry supplies the two corrected results; this document does not relabel the earlier run as green.
No acceptance collection result is presented as a completed run.

Earlier visual refinement checks ran 18 targeted browser cases, then eight after viewport/control-width changes.
At that checkpoint package coverage recorded 4,664 app and 516 design-system tests at 100 percent.

The superseded zone-aware consumer ran 75 unit checks and 16 browser checks.
Those mocks required fields absent from the final API; they do not validate its behavior.

The speculative broad coverage run was interrupted when the actual checkpoint arrived (exit 130), and is not a completed result.

After incorporating Claude's checkpoint, the first browser attempt failed to launch all 24 cases because its updated Playwright browser executable was missing.
The native browser installation repaired that prerequisite. The next run passed 22 cases; two new model-filter cases failed on an obsolete fixture-helper reference, which was removed.

The one/100-fleet, Security and timezone-display cases passed in that run.
The focused final retry passed all 16 affected Usage/scale cases and saved the JSON performance attachments above.

`stream-and-bar-limit-final-unit.log` records 28 passed for streamed-data availability and oversized-report refusal.
The final stale-cutoff repair passed 32 report-view units and both desktop/mobile captured-month browser cases (`captured-month-unit.log`, `captured-month-browser.log`).

Final repository and documentation gate receipts are recorded in the associated PR's Make section.
The logs above use the `focused-unit`, `targeted-browser` and `revision-browser-retry` prefixes.

The first complete local PR gate failed before completing its native test lanes.
CMake was missing, QStash's package download lacked the injected trust chain, and evidence-only edits did not satisfy `docs.updated`.
A subsequent infrastructure preflight exposed proxy interception of NATS's local health check.

Task-local CMake, a trust bundle and a Compose environment override repaired those environment prerequisites.
The corrected native infrastructure preflight passed with the repository's commands and health checks unchanged.

The canonical authentication guide now documents Team's application identity.
These corrections do not turn the earlier failed gate into a passing result.

The next native gate exposed missing Clang resource headers; a task-local matching resource directory repaired that prerequisite.
The subsequent complete gate remained red when the known two-second wall-tick timeout recurred.

The daemon-library linker also received a bus error with the filesystem full.
The runner unit shard passed 1,731 tests and the daemon unit shard passed 172; the daemon-library result remains unverified locally.

The unchanged native integration lane was retried through `make test-integration-rustd`: 1,780 passed plus two exclusive tests, exit 0 (`integration-final-retry.log`).
No Rust source or test was changed. This passing retry does not relabel the prior whole gate as green.

## Final integrated gate checkpoint

The pushed `576e11213` candidate ran the complete repository PR gate; it exited 1.
Rust unit shards passed: runner 1,731, daemon 172 and daemon libraries 3,680.

All 4,690 app tests across 492 files passed. Branch coverage was 99.98 percent because the server page's initial query branch had no case.
The new case verifies an initial period/fleet link makes exactly one scoped report read.

The prose checker found long paragraphs and sentences. They were split without changing API or accounting decisions.

Integration never began: pulling `mirror.gcr.io/library/postgres:19rc2-alpine` returned a Google registry upstream-read quota error (`artifactregistry.googleapis.com`).
The parent moved the image from `19rc1`; only that older image was cached locally.
A same-mirror pull retry and a manifest request returned the same quota refusal; no different registry or image was substituted.

The parent Rust 1.100 pin also triggers Clippy at `afr_sandbox/examples/kernel_lane/toolbox.rs:185`.
It requests `.map_or_default(Vec::as_slice)` in place of `.map(Vec::as_slice).unwrap_or_default()`.
This is outside the UI changes; the fix was requested from Claude, preserving his Rust ownership.

The gate's version check passed at `0.77.0`. Its native log is `final-integrated-pr-gate.log`.

The failed whole gate remains failed. The targeted frontend lint retry passed source rules, Oxlint and TypeScript checks.

`make test-coverage-all` then passed: app 4,691, website 133, CLI 2,335 with 22 live cases skipped, and design system 516.

All four package coverage gates reached 100 percent. The native prose diagnostic returned zero findings.

The final app production build passed. Its receipts are `final-package-coverage-retry.log`, `final-frontend-lint.log` and `final-production-build.log`.
The CLI skips are not live acceptance; the blocked registry setup still leaves the new parent integration run unverified.

## Authorized draft checkpoint

The owner supplied parent `ab4fe8937`, which includes the requested Rust 1.100 Clippy repair.
It also reverts the earlier aggregate optimization; those SQL timings are now marked historical in the source inventory.
The parent was incorporated without changing the UI or its reporting API fields.

The owner authorized the remaining draft-review exception:

> "Authorize the documented override and open a draft PR"

The question named the failed native checks, unverified parent baseline and incomplete full review.
A branch-scoped `cmd.verify.integration` override records the configured Google mirror's upstream-read quota blocker.
The final gate is rerun after the parent update; its measured outcome belongs in the PR's Session notes.

The earlier failed whole gate stays failed. The override permits draft review and establishes no passing integration result.
Hosted merged-backend acceptance, complete parent baseline counts and the full review remain unverified or incomplete as described above.

## Deployment and access blockers

Development deployment [43669040645](https://github.com/agentsfleet/agentsfleet/actions/runs/43669040645) failed in `fly / deploy-fly`.
Its migration guard refused version `20261108120700` with `UZ-STARTUP-005`: the applied file had changed.
That is the API-key schema slot; this task neither changed it nor reset migration history.

Production acceptance [43669103980](https://github.com/agentsfleet/agentsfleet/actions/runs/43669103980) failed during tenant bootstrap with `ENOTFOUND api.agentsfleet.net`.
The smoke job in that workflow passed. The acceptance failure does not describe a completed product flow.

The hosted development dashboard redirects to Vercel protection. This environment has no bypass secret.

The configured task API key receives `401 UZ-AUTH-002`; fresh task-owned Clerk sessions succeeded independently.
Those two credential outcomes are kept separate.

Hosted acceptance against the merged backend remains **unverified** until deployment and host access are available.
Native sign-out first encountered the local Next.js development overlay; a production-build retry passed without changing the sign-out implementation.
Remote-session sign-out has controlled browser evidence; a separate live remote-session revocation is not claimed.

## Companion documentation and data handoff

The earlier chat records 13 uncommitted pages on `chore/m249-plugin-vocabulary-changelog` in another cloud environment.
Neither that local branch nor its patch was present here, and no remote branch contained it.
The original `/workspace/docs` checkout stayed clean and unchanged.

This task recreated the recorded branch name in an isolated worktree from current docs main.
Its update covers those same 13 prepared page paths and the actual merged behavior.
Review added a fourteenth page: the linked running guide still described existing schedule activity as coming soon.

Preservation of the unavailable original patch remains **unverified**; no claim says its exact text was recovered.

The companion update documents fleet-only saving, automatic navigation, local display clocks, daily Usage, safe suffixes, and retained account-owned Plugin APIs and commands.
It removes active platform-publication guidance while preserving historical changelog entries.

M249_004 must distinguish the deployed schema blocker from a missing suffix implementation.
Its backend producer work remains separate; this run supplies no live Usage performance measurement or new query plan.

Claude's source inventory records his separate query-plan and equivalence measurements; those are not this browser benchmark.
Recorded Outputs must not infer external publication or bookings from model-written text.

The data handoff stays in this requested evidence record.
Automatic approval review rejected a separate GitHub issue containing deployment and authentication details; no issue was created.
