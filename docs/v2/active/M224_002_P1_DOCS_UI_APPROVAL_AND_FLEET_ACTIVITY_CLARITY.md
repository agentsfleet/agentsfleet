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
4. `playbooks/operations/acceptance/ci_evidence_format.md` — complete trace, model and regression proof requirements.

## Files Changed (blast radius)

| Files | Action | Reason |
| --- | --- | --- |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/approvals/components/{ApprovalsList,ApprovalsTable}.tsx`; `ui/packages/app/tests/approvals-list/resolve.test.ts` | EDIT | retain pending rows and replace duplicate actions with saving feedback |
| `ui/packages/app/components/domain/chat/{SetupChecks,SetupStatusFlap,DraftMessage}.tsx`; `ui/packages/app/tests/workspace-chat/setup-status.test.tsx` | EDIT | distinguish credentials, connections and approvals; use enclosed completion icons |
| `ui/packages/app/components/domain/{RefreshButton,FleetToolCalls}.tsx`; `ui/packages/app/app/(auth)/layout.tsx`; `ui/packages/app/app/(dashboard)/invites/components/InvitesView.tsx`; `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.tsx` | EDIT | replace bare action and success ticks across pages |
| `ui/packages/design-system/src/design-system/CopyButton.tsx`; `ui/packages/app/tests/secrets-list.test.ts` | EDIT | use the same enclosed success icon in shared copy feedback |
| `ui/packages/app/components/domain/{FleetMessageRow,fleetMessageRenderers}.tsx`; `ui/packages/app/lib/events/{event-payload,event-summary}.ts` | EDIT | show Slack message wording and readable request text |
| `ui/packages/app/components/domain/FleetMessageRow.test.tsx`; `ui/packages/app/lib/events/{event-summary,event-summary.body}.test.ts`; `ui/packages/app/tests/fleet-thread/{role-activity,role-links,role-rows,malformed-metadata}.test.ts` | EDIT | verify labels, disclosure behavior and existing event fallbacks |
| `ui/packages/app/tests/e2e/chat-typography.spec.ts`; `ui/packages/app/tests/e2e/fixtures/chat-browser-probe.tsx`; `ui/packages/app/tests/e2e/acceptance/fleet-reply-parts.spec.ts` | EDIT | verify enclosed icons, desktop/mobile geometry and a Slack event through the real page stream |
| `tests/fixtures/fleets/{ci-responder,ci-repairer}/definition.json`; `tests/fixtures/fleets/ci-investigation/cases.json` | EDIT / CREATE | save approved public network defaults and the eight-case plan |
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
| Browser | `setup checks use enclosed icons and incoming messages have readable details` | desktop/mobile fixture → 14px circle checks, readable disclosure and no horizontal overflow |
| Application | `test_slack_received_uses_readable_label_without_empty_details` | native event frame through the real signed-in page → Slack label and no empty inspector |
| Manual | `public Definition comparison` | source JSON and accepted revision 2 → identical public read-only network and unchanged repository boundaries |
| Manual | `draft regression and trace comparison` | passing head and 17 omitted calls → regression pass retained, complete-trace acceptance incomplete |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded |
| --- | --- | --- | --- | --- | --- |
| R1 | Stable approval rows and truthful errors | `bun --cwd ui/packages/app run test -- tests/approvals-list` | exit 0 | P1 | PASS — included in application coverage lane |
| R2 | Enclosed icons and readable disclosures on desktop/mobile | `BASE_URL=http://127.0.0.1:1 bun --cwd ui/packages/app run test:e2e tests/e2e/chat-typography.spec.ts` | six tests pass | P1 | PASS |
| R3 | Slack message through the real application page | `bun --cwd ui/packages/app run test:e2e:acceptance --project=journeys tests/e2e/acceptance/fleet-reply-parts.spec.ts --grep test_slack_received_uses_readable_label_without_empty_details` | one test passes | P1 | INCOMPLETE — supported acceptance host required |
| R4 | Public reference JSON matches native setups | Manual `public Definition comparison`: inspect both accepted revision-2 Definitions and compare their network and repository fields with the two fixture files | public read reach matches; repository access/base unchanged | P1 | PASS |
| R5 | Passing regression cannot hide missing trace evidence | Manual `draft regression and trace comparison`: inspect draft #6 head, regression run and native repair event in the launch ledger | three assertions pass; omitted calls remain 17; complete-trace grade incomplete | P0 | PASS — limitation recorded |
| S1 | Repository work gate | `orly gate work` | exit 0 | P0 | pending |
| S2 | Declared unit lane | `make test-unit-all` | exit 0 | P0 | pending |
| S3 | Declared lint lane | `make lint-all` | exit 0 | P0 | pending |
| S4 | Declared integration lane | `make test-integration-rustd` | exit 0 | P0 | pending |
| S5 | Version parity | `make check-version` | exit 0 | P0 | pending |
| S6 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | pending |

### Behaviour evals

This workstream adds no model-behavior pass.
M224_001 still requires its eight actual-model cases, complete trace and memory witnesses, full Linux acceptance, incident prerequisites and production gates.

## Dead Code Sweep

Remove optimistic row hiding and its comments when the retained-row state replaces it.
Remove the braces icon and unused generic details label from activity rows.
Retain the generic JSON inspector for non-message events and tool inputs.

## Out of Scope

A general Effect-code refactor or performance claim requires its own concrete findings and measurements.
Original-PR authority and durable cross-request repair continuation remain proposed work.
This PR neither grants production-release authority nor closes M224_001.

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
The command-line Effect code review remains a recorded follow-up, not a demonstrated performance repair.
