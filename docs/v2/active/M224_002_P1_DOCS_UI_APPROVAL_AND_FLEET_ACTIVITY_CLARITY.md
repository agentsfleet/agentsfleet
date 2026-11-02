# M224_002: Keep approval decisions visible and make fleet activity readable

**Prototype:** v2.0.0
**Milestone:** M224
**Workstream:** 002
**Date:** Nov 01, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — remove reported approval flicker and unclear activity labels
**Categories:** DOCS, UI
**Batch:** B1 — owner-requested follow-up to merged Pull Request #795
**Branch:** feat/m224-ci-campaign
**Baseline revision:** 2f6daf23ffa013d2d92aeeece0ef10adb9a851e2
**Test Baseline:** application 4,628 tests across 462 files; design system 522 tests across 68 files; both source jobs passed
**Baseline evidence:** [application job](https://github.com/agentsfleet/agentsfleet/actions/runs/41302479018/job/124836127711) and [design-system job](https://github.com/agentsfleet/agentsfleet/actions/runs/41302479018/job/124836127538); candidate tree matches the comparison revision through deployment `41304302557`
**Depends on:** M224_001 owns fleet demonstrations and milestone closure; #795 supplies the existing chat implementation
**Provenance:** agent-drafted from Indy's reports and explicit change requests in this session
**Canonical architecture:** `docs/DESIGN_SYSTEM.md` — row actions and chat activity ownership

## Overview

**Goal:** An approval row stays visible while saving, confirmed actions use enclosed icons, and Slack activity opens readable message text.
**Problem:** Approval rows disappear and return after a decision. Setup checks repeat service names, and Slack activity exposes generic event wording and JSON details.
**Solution summary:** Retain the row during its request, use shared Lucide circle-check icons, distinguish readiness checks, and render Slack message text.
**Design flow:** A native decision updates the existing table after read-back. Recorded request fields supply the source label and message disclosure.

## PR Intent & comprehension handshake

- **PR title:** fix(ui): keep approvals visible and clarify fleet activity
- **Intent:** Explain confirmed setup and incoming work without disappearing rows or raw Slack event JSON.
- **Authoring handshake:** Indy requested consistent enclosed icons across pages, clearer Slack activity and source JSON matching public fleet access.
- **Implementer handshake:** Keep authorization, reviewed repository writes and chat ownership unchanged; record live proof separately from presentation tests.

## Implementing agent — read these first

1. `AGENTS.md`, `.orly/LOCAL.md` and `ui/packages/app/AGENTS.md` — repository and chat ownership rules.
2. `docs/DESIGN_SYSTEM.md` — shared row controls, accessible labels and activity colors.
3. `ui/packages/app/node_modules/next/dist/docs/01-app/03-api-reference/01-directives/use-client.md` — existing browser boundaries remain in place.
4. `playbooks/operations/acceptance/baselines/M224_001-ci-evidence.md` — concise native behavior, trace, model and regression ledger.

## Files Changed (blast radius)

| Files | Action | Reason |
| --- | --- | --- |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/approvals/components/{ApprovalsList,ApprovalsTable}.tsx`; `ui/packages/app/tests/approvals-list/resolve.test.ts` | EDIT | retain pending rows and replace duplicate actions with saving feedback |
| `ui/packages/app/components/domain/chat/{SetupChecks,SetupStatusFlap,DraftMessage}.tsx`; `ui/packages/app/tests/workspace-chat/setup-status.test.tsx` | EDIT | distinguish credentials, connections and approvals; use enclosed completion icons |
| `ui/packages/app/components/domain/{RefreshButton,FleetToolCalls}.tsx`; `ui/packages/app/app/(auth)/layout.tsx`; `ui/packages/app/app/(dashboard)/invites/components/InvitesView.tsx`; `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.tsx` | EDIT | replace bare action and success ticks across pages |
| `ui/packages/design-system/src/design-system/{CopyButton,Terminal}.tsx`; `ui/packages/app/tests/secrets-list.test.ts` | EDIT | use the same enclosed success icon in shared copy feedback |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/{RunnerChecksReport,RunnerIdentityLine}.tsx`; its `RunnerHeader.test.tsx` | EDIT | use enclosed result icons and readable capability status words |
| `ui/packages/app/components/domain/{FleetMessageRow,fleetMessageRenderers}.tsx`; `ui/packages/app/lib/events/{event-payload,event-summary}.ts` | EDIT | show Slack message wording and readable request text |
| `ui/packages/app/components/domain/FleetMessageRow.test.tsx`; `ui/packages/app/lib/events/{event-summary,event-summary.body}.test.ts`; `ui/packages/app/tests/fleet-thread/{role-activity,role-links,role-rows,malformed-metadata}.test.ts` | EDIT | verify labels, disclosure behavior and existing event fallbacks |
| `ui/packages/app/tests/e2e/chat-typography.spec.ts`; `ui/packages/app/tests/e2e/fixtures/chat-browser-probe.tsx`; `ui/packages/app/tests/e2e/acceptance/fleet-reply-parts.spec.ts` | EDIT | verify enclosed icons, desktop/mobile geometry and a Slack event through the real page stream |
| `tests/fixtures/fleets/{ci-responder,ci-repairer}/definition.json`; `tests/fixtures/fleets/ci-investigation/cases.json` | EDIT / CREATE | save approved public network defaults and the eight-case plan |
| `ui/packages/app/components/domain/chat/ChatFailure.tsx`; `ui/packages/app/components/domain/tool-call-activity{,.test}.ts`; `ui/packages/app/tests/fleet-tool-progress.test.tsx` | EDIT | use calm inline recovery notices, action-specific tool groups and enclosed command success icons |
| `rustd/crates/afr_tools/src/http_request/{mod,body_tests}.rs`; `docs/architecture/runner_fleet.md` | EDIT / CREATE | keep explicit truncation notices without sending the known-incomplete megabyte prefix to inference |
| `rustd/crates/afr_agent/src/trace/{mod,tests}.rs`; `rustd/crates/afd_fleet_runtime/tests/definition/corpus.rs` | EDIT | retain call inventory before previews and assert the approved public read-only fixture policy |
| `docs/architecture/*.md`; `docs/architecture/scenarios/*.md`; `docs/reviews/space-sandbox/{next-specs-handoff,orly-egm-jev-handoff-prompt}.md` | EDIT / CREATE | reconcile current behavior and proposed unified autonomy scope without separate review records |
| `docs/v2/pending/M236_001_P0_API_DOCS_INFRA_OBS_UI_GOAL_DRIVEN_DURABLE_REPAIR.md` | CREATE | draft the reviewed unified autonomy scope without claiming implemented behavior |
| `cli/src/lib/{http,http-retry}.ts`; `cli/src/services/http-client.ts`; `cli/src/errors/index.ts`; `cli/src/commands/{fleet_steer,fleet_steer_events,approvals,approvals_pending}.ts`; their focused CLI tests | EDIT / CREATE | stop cancelled requests and retries, bound event polling and report incomplete approval walks |
| `docs/reviews/development-autonomy/` | DELETE | remove separate review records at the owner's request; canonical decisions remain in the scenario and pending specification |
| This spec; `docs/v2/active/M224_001_P0_DOCS_INFRA_FIRST_PRODUCTION_LAUNCH.md`; `docs/DESIGN_SYSTEM.md`; the M224 launch ledger | EDIT / CREATE | retain requested behavior, actual evidence and open closure requirements |

## Applicable Rules

- TS-LAYOUT, TS-PRIMITIVE and TS-TOKEN in `.orly/standards/languages/typescript.md` require shared layout, controls and tokens.
- TCF, TEST-REDGREEN and TNM in `.orly/standards/tests.md` require meaningful failure assertions and behavior names.
- SPEC-DIM, SPEC-FAIL and SPEC-E2E in `.orly/standards/spec.md` require named proof, negative cases and a real application path.
- DOC-02 and DOC-03 in `.orly/standards/docs.md` bound new prose sentences and paragraphs.

Only confirmed results use success colors; a saving request remains pending.
The native API controls grants, repository writes and final decision outcomes.

## Applicable Gates

Run application and design-system lint, type checking and coverage for changed packages.
Run the desktop/mobile browser fixture for rendered changes and documentation checks for the specifications.
Repository gates retain their configured commands; package checks alone do not prove the whole repository.

## Prior-Art / Reference Implementations

`RefreshButton` and shared `CopyButton` already distinguish pending work from confirmed success.
`FleetActivityRow` provides the existing keyboard-accessible disclosure and relative timestamp.
The approval list's read sequence already rejects superseded page responses; retain that behavior.

## Sections (implementation slices)

### §1 — Stable approvals

Keep the row visible while its decision request or read-back is pending.
Replace its action controls with `Saving decision…`, preventing another click on that row.
Permit independent rows to save without hiding the table.

On failure, retain the row and show the actual error.
On success, use the server's refreshed status and decision metadata.
An already-resolved response reports the recorded outcome rather than assuming the requested decision won.

- **Dimension 1.1:** pending rows remain visible and errors remain actionable → Test `keeps an inbox row visible while saving and preserves it on failure`.
- **Dimension 1.2:** superseded reads cannot overwrite the current walk → Test `drops the older page when a refresh started after it`.

### §2 — Consistent enclosed status icons

Use Lucide `CircleCheckIcon` for approval, invitation, completion, refresh and copy success.
Retain existing sizes and semantic colors; provide text or an accessible control label.
Selection ticks in checkboxes and menus remain selection indicators.

Name GitHub and Slack checks by their purpose: credential, connection or approval.
Do not collapse distinct readiness checks into one apparent success.

- **Dimension 2.1:** check labels distinguish service prerequisites → Test `hides infrastructure subjects while retaining resource names and unknown checks`.
- **Dimension 2.2:** enclosed icons fit desktop/mobile layouts → Test `setup checks use enclosed icons and incoming messages have readable details`.

### §3 — Readable Slack activity

Label the source `Slack` and its chat event `Message received`.
Use `View message` for a recorded Slack request and show its message text instead of raw envelope JSON.
Keep other event data inspectable under `View event`, using a document icon rather than braces.

Preserve timestamps, failure outcomes, grouped events and keyboard interaction.
Do not replace the fleet's reply with the trigger summary or render private metadata as message text.

- **Dimension 3.1:** a Slack disclosure shows only its recorded message → Test `shows the received Slack message instead of raw event JSON`.
- **Dimension 3.2:** native chat events receive a readable headline → Test `test_slack_received_uses_readable_label_without_empty_details`.

### §4 — Public source defaults and honest acceptance records

Both CI reference Definitions declare `network.http_egress: public`, with `read_only: true` and no extra read POST paths.
The responder retains read-only repository access; the repairer retains the reviewed `dev` draft-repair scope.
Public destination reach does not grant write access or bypass private-address protections.

Save these defaults in repository JSON and reusable campaign JSON, then compare them with the accepted native Definitions.
The blocked-download case explicitly uses restricted reach to GitHub alone.
Update planned policy hashes whenever a reviewed configuration changes.

Record draft #6's unchanged regression result separately from missing native trace witnesses.
An omitted call, unread required source, failed stopping rule or incomplete campaign cannot become a passing grade.

- **Dimension 4.1:** source defaults match accepted public policies → Manual `public Definition comparison`, performed by the agent and recorded in the launch ledger.
- **Dimension 4.2:** passing regression results cannot hide missing evidence → Manual `draft regression and trace comparison`, reviewed by Indy against the recorded head and omission count.

## Interfaces

No API, command or grant interface changes.
The existing command is `agentsfleet approvals approve <gate_id>`; denial uses `agentsfleet approvals deny <gate_id>`.
The pending-table state is local presentation state and never resolves a native approval.

## Failure Modes

| Failure | Required behavior | Proof |
| --- | --- | --- |
| Decision refusal | row remains, error appears, actions return | `keeps an inbox row visible while saving and preserves it on failure` |
| Stale page response | existing sequence rejects it | `drops the older page when a refresh started after it` |
| Another person resolved the gate | show actual outcome and actor | `an already resolved gate stays removed and shows who resolved it` |
| Live request carries only an empty placeholder | no empty inspector appears | `omits the disclosure for a live empty request placeholder` |
| Page read fails | retain current rows and show the error | `keeps the rows it has when the read fails` |
| Live trace omits calls | retain incomplete evidence; no acceptance pass | Manual `draft regression and trace comparison` |

## Invariants

Preserve the 14px conversation token, alignment, accessibility and stream cancellation ownership from #795.
Codex does not author the demonstration's repair, merge its draft or deploy to production.
No instrumentation limit, workflow, permission or test assertion changes to obtain passing evidence.

## Metrics & Observability

Record changed-package test counts, coverage, browser projects and screenshot paths.
Record native setup revisions, event identities, omission counts and exact regression heads in the launch ledger.
No startup, allocation or rendering-speed improvement is claimed without a measurement.
No product analytics events, operator signals or monitoring rules change.

## Test Specification (tiered)

| Tier | Test | Concrete input and expected output |
| --- | --- | --- |
| Unit | `keeps an inbox row visible while saving and preserves it on failure` | deferred approval, then refusal → same visible row, saving feedback, then error and restored actions |
| Unit | `drops the older page when a refresh started after it` | old page settles after refresh → current walk stays visible |
| Unit | `an already resolved gate stays removed and shows who resolved it` | server reports earlier decision → actual resolver appears |
| Unit | `omits the disclosure for a live empty request placeholder` | received Slack event with `{}` → headline without a data disclosure |
| Unit | `keeps the rows it has when the read fails` | failed refresh → existing rows and error remain |
| Unit | `hides infrastructure subjects while retaining resource names and unknown checks` | blocked GitHub credential and unknown checks → distinct service label and safe recovery link |
| Unit | `shows the received Slack message instead of raw event JSON` | message plus channel metadata → only message text in the disclosure |
| Browser | `setup checks use enclosed icons and incoming messages have readable details` | desktop/mobile fixture → 14px circle checks, readable message disclosure and keyboard-expandable soft failure, with no horizontal overflow |
| Application | `test_slack_received_uses_readable_label_without_empty_details` | native event frame through the real signed-in page → Slack label and no empty inspector |
| Manual | `public Definition comparison` | source JSON and accepted revision 2 → identical public read-only network and unchanged repository boundaries |
| Manual | `draft regression and trace comparison` | original summary omits 17 rows; all 50 full records are recovered without truncation and include the passing regression/check read |
| Unit | `should_reclaim_old_previews_before_omitting_later_calls` | fifty calls with oversized optional previews → complete inventory and arguments, zero omissions, valid byte bound |
| Unit | `should_count_a_call_when_essential_rows_alone_exhaust_the_budget` | essential rows exceed byte capacity → honest omitted count and a valid bounded trace |
| Unit | `responder_definition_holds_no_write_reach` | native responder fixture → public read-only HTTP, empty allowlist and no write reach |
| Unit | `repairer_definition_is_write_bound_to_one_base` | native repairer fixture → public HTTP with reviewed repository/base restrictions intact |
| Unit | `summarizes only the group's own actions and preserves mixed outcomes` | reads, failed write and unknown tool → actual action labels, retained failure, no adjacent-text contamination |
| Manual | `native hello after diagnosis` | greeting after a diagnosed incident → short social reply, present empty call inventory and no incident recap |
| Manual | `native varied social replies` | casual/friendly/thanks wording → brief replies and present empty call inventories; no literal keyword routing |
| Unit | `should_omit_a_truncated_body_without_hiding_its_incomplete_status` | one-MiB truncated response → explicit status/type/truncation, bounded omission notice and no body prefix |
| Integration | `cancelling an accepted HTTP write stops waiting without replaying it` | real local server accepts POST, then caller aborts → one write and cancellation reported |
| Unit | `Effect interruption aborts the HTTP request owned by that fiber` | interrupt the request fiber → its fetch signal aborts |
| Unit | `event polling surfaces HTTP %s without another read` | 401, 403 or 404 → original error returned after one read |
| Unit | `event polling interrupts a stalled read at the overall deadline` | stalled read and virtual clock → timeout result and interrupted request |
| Unit | `event polling waits for an admitted event to reach the read store` | accepted event initially absent → bounded reads of the same identity until visible, with no resend |
| Unit | `event polling surfaces disappearance after observing the admitted event` | known event later disappears → exact native error after two reads |
| Unit | `approval page cap reports whether more gates remain: %s` | last allowed page with/without continuation → incomplete-list error or complete results |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded |
| --- | --- | --- | --- | --- | --- |
| R1 | Stable approval rows and truthful errors | `bun --cwd ui/packages/app run test -- tests/approvals-list` | exit 0 | P1 | PASS — included in application coverage lane |
| R2 | Enclosed icons and readable disclosures on desktop/mobile | `BASE_URL=http://127.0.0.1:1 bun --cwd ui/packages/app run test:e2e tests/e2e/chat-typography.spec.ts` | six tests pass | P1 | PASS |
| R3 | Slack message through the real application page | `bun --cwd ui/packages/app run test:e2e:acceptance --project=journeys tests/e2e/acceptance/fleet-reply-parts.spec.ts --grep test_slack_received_uses_readable_label_without_empty_details` | one target test and preflight pass | P1 | PASS — local production page with native development backend: 10 passed, including nine preflight checks |
| R4 | Public reference JSON matches native setups | Manual `public Definition comparison`: inspect both accepted revision-2 Definitions and compare their network and repository fields with the two fixture files | public read reach matches; repository access/base unchanged | P1 | PASS |
| R5 | Passing regression and complete native records agree | Manual `draft regression and trace comparison`: inspect draft #6 head, regression run and all 50 native records in the launch ledger | three assertions pass; complete records recovered; compact summary limitation stays explicit | P0 | PASS — full native record recovery recorded |
| S1 | Repository work gate | `orly gate work` | exit 0 | P0 | pending |
| S2 | Declared unit lane | `make test-unit-all` | exit 0 | P0 | pending |
| S3 | Declared lint lane | `make lint-all` | exit 0 | P0 | pending |
| S4 | Declared integration lane | `make test-integration-rustd` | exit 0 | P0 | pending |
| S5 | Version parity | `make check-version` | exit 0 | P0 | pending |
| S6 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | pending |

### Behaviour evals

The concise evidence ledger attributes each native behavior proof to its saved setup and retains earlier failed observations. Social follow-ups remain separately recorded; no unchanged statistical campaign is claimed.
M224_001 records the eight native behavior proofs, complete trace and memory witnesses; its owner-approved deferrals are canonical there. This workstream does not reinstate development rollout identity checks, full Linux acceptance, the incident demonstration or production launch as M224 blockers.

## Dead Code Sweep

Remove optimistic row hiding and its comments when the retained-row state replaces it.
Remove the braces icon and unused generic details label from activity rows.
Retain the generic JSON inspector for non-message events and tool inputs.

## Out of Scope

A general Effect-code refactor or performance claim requires its own concrete findings and measurements.
Original-PR authority and durable cross-request repair continuation remain proposed work.
M224_001 owns the separate development-evidence closure; this workstream grants no production-release authority.

## Product Clarity (authoring record)

1. **Successful user moment:** an approved row changes status in place, and a Slack activity opens the actual message.
2. **Preserved user behaviour:** existing approvals, denial confirmation, table paging, chat replies and private setup ownership keep working.
3. **Optimal-way check:** reuse the existing row, controls and event renderer; avoid replacing their data or authorization paths.
4. **Rebuild-vs-iterate:** iterate within the current components; no new query library or state framework.
5. **What we build:** retained saving rows, enclosed status icons, distinct readiness labels, message disclosures and public source defaults.
6. **What we do NOT build:** new repair authority, cross-request continuation or a general command-line refactor.
7. **Fit with existing features:** preserve native Review/Apply, recorded outcomes, 14px chat text and request cancellation.
8. **Surface order:** dashboard first for the reported presentation defects; existing command-line approval remains available.
9. **Dashboard restraint:** show pending until native confirmation; hide empty event placeholders and Slack envelope metadata.
10. **Confused-user next step:** inspect the row error, open its message or use `agentsfleet approvals show <gate_id>`.

## Decomposition & alternatives (patch vs refactor)

Retain existing React state, server actions, table controls and event renderers.
A query-library migration or new generic state framework would enlarge this repair without improving its stated proof.

## Discovery (consult log)

Indy reported: "once i approve, the table doesnt show that row and then the row comes back."
He requested: "The tick is used across other pages like Approval too, so you need to change that too."
He directed public fleet access and asked that every reusable JSON definition retain that choice.
He also requested that the specifications be written and pushed in the PR.

The approved behavior is concrete; no new product scope or model authority is inferred.
The owner-authorized CLI fixes preserve cancellation, distinguish transient errors and report incomplete approval walks; no performance claim is made.

## Approved completion amendment: Nov 01, 2026

Indy replied "Approve this finish plan" to the recorded plan: keep draft #6 regression and Slack delivery passed; fix trace retention and required-source stopping, complete the eight-case evaluation, prove native memory recall, then remove the bespoke CI-evidence machinery and obsolete callers/rubric commands in favor of a concise evidence ledger.
Runtime policy and repair tests remain required; missing or failing live evidence remains incomplete.

The owner also supplied a concrete conversational regression: `hello` triggered GitHub reads and a repeated diagnosis.
Both CI definitions now distinguish social replies from investigation, require short greetings without tool calls or incident recaps, and strengthen the required-read stop and completed-validation-log ordering.
Live setup updates must use native Improve/Review/Apply and preserve config, grants, repository/base, model and public read-only HTTP policy.

- **Dimension 5.1:** earlier previews cannot omit later call records when essential rows fit → Test `should_reclaim_old_previews_before_omitting_later_calls`.
- **Dimension 5.2:** essential rows over the existing byte/call caps remain honestly counted as omitted → Test `should_count_a_call_when_essential_rows_alone_exhaust_the_budget`.
- **Dimension 5.3:** responder retains public HTTP with empty allowlist and read-only restrictions → Test `responder_definition_holds_no_write_reach`.
- **Dimension 5.4:** repairer retains public HTTP and the exact repository/base write scope → Test `repairer_definition_is_write_bound_to_one_base`.
- **Dimension 5.5:** greeting after diagnosis returns a brief social reply and explicit zero-call trace → Manual `native hello after diagnosis`, retained in the launch ledger; a repeated incident summary fails clarity even with no calls.

The owner moved chat polish before the remaining M224 publication work and required social handling beyond a literal `hello`.
Collapsed tool groups now retain recorded action verbs, including job-log/status and branch-head reads, rather than replacing them with generic `Used tools` categories.
`ChatFailure` uses a polite inline status with an enclosed warning icon and expandable diagnostics; connection interruption does not imply the runner crashed.

- **Dimension 5.6:** settled groups preserve actual actions and mixed failures without reading full output → Test `summarizes only the group's own actions and preserves mixed outcomes`.
- **Dimension 5.7:** soft failure remains readable, 14px and keyboard-expandable on desktop/mobile → Test `setup checks use enclosed icons and incoming messages have readable details`.
- **Dimension 5.8:** casual/friendly/thanks wording does not invoke investigation tools → Manual `native varied social replies`, retained in the launch ledger.

- **Dimension 5.9:** known-truncated HTTP content stays explicitly incomplete without an oversized model payload → Test `should_omit_a_truncated_body_without_hiding_its_incomplete_status`.
The one-MiB capture cap, HTTP status, policy checks and complete-body behavior remain unchanged.
The deployed-runtime rerun names the truncated log and stops after six reads, with no source or memory call afterward. The earlier provider failure stays recorded.

## Owner-requested CLI correction

Indy requested: "fix the CLI fixes" and "remove the docs/reviews/development-autonomy folder".
The CLI work covers the three reported defects, with no new command or flag.
Cancellation stops local requests and backoff; an accepted remote message may still run.
Permanent event-read errors return directly, and the polling deadline also bounds an active read.
An approval walk with a remaining cursor at its page cap fails instead of reporting a complete list.

- **Dimension 6.1:** cancelling an accepted HTTP write stops local waiting without replay → Test `cancelling an accepted HTTP write stops waiting without replaying it`.
- **Dimension 6.2:** interrupting the owning Effect fiber aborts its fetch → Test `Effect interruption aborts the HTTP request owned by that fiber`.
- **Dimension 6.3:** permanent event-read failures surface immediately → Test `event polling surfaces HTTP %s without another read`.
- **Dimension 6.4:** the polling deadline interrupts an active stalled read → Test `event polling interrupts a stalled read at the overall deadline`.
- **Dimension 6.5:** an admitted event may wait for its first native read-store record → Test `event polling waits for an admitted event to reach the read store`.
- **Dimension 6.6:** a later disappearance of an observed event surfaces immediately → Test `event polling surfaces disappearance after observing the admitted event`.
- **Dimension 6.7:** a remaining approval cursor at the page cap reports an incomplete walk → Test `approval page cap reports whether more gates remain: %s`.

All in-scope Dimensions are DONE. The real-page Slack test passed with its preflight checks; final repository verification is recorded in PR #796 Session notes 2.
