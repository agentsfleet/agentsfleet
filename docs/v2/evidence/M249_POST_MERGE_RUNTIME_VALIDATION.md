# M249 post-merge runtime validation

Current frontend validation uses fresh task-owned fixtures. It does not establish that the merged backend was deployed.

## Source and ownership

| Item | Recorded value |
|---|---|
| Application base | `a1dcc222488e4522f8b92208a4f7df8461fe7fbf`, fetched `origin/main`; merged PR #844 |
| Isolated application worktree | `/workspace/m249-runtime`, `fix/m249-runtime-followup` |
| Docs base | `016b64c4649535c1109dab7f28300e5252bfb1fb` |
| Isolated docs worktree | `/workspace/m249-docs`, `chore/m249-plugin-vocabulary-changelog` |
| Local frontend | Current application source, with the Team repair; `http://localhost:3187` |
| Live API | `https://api-dev.agentsfleet.net`; health reports version `0.77.0`, commit `163356f` |
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
This follow-up changes no backend producer, schema, billing query or Usage loader.

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
The approved layout and the existing canonical proposal were not redesigned.

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

Final repository and documentation gate receipts are recorded in the associated PR's Make section.
The logs above use the `focused-unit`, `targeted-browser` and `revision-browser-retry` prefixes.

The first complete local PR gate failed before completing its native test lanes.
CMake was missing, QStash's package download lacked the injected trust chain, and evidence-only edits did not satisfy `docs.updated`.
A subsequent infrastructure preflight exposed proxy interception of NATS's local health check.

Task-local CMake, a trust bundle and a Compose environment override repaired those environment prerequisites.
The corrected native infrastructure preflight passed with the repository's commands and health checks unchanged.

The canonical authentication guide now documents Team's application identity.
These corrections do not turn the earlier failed gate into a passing result.

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
Its one-report Usage and producer work remain separate; this run supplies no four-fleet query plan or live Usage performance measurement.
Recorded Outputs must not infer external publication or bookings from model-written text.

The data handoff stays in this requested evidence record.
Automatic approval review rejected a separate GitHub issue containing deployment and authentication details; no issue was created.
