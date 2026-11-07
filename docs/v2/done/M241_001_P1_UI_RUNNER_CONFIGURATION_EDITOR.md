# M241_001: Edit runner configuration in a grouped panel

**Prototype:** v2.0.0

**Milestone:** M241

**Workstream:** 001

**Date:** Nov 07, 2026

**Status:** DONE

**Priority:** P1, operators need readable runner settings and reachable save controls.

**Categories:** CLI, UI, auth

**Batch:** B1, one isolated presentation workstream.

**Depends on:** none.

**Provenance:** LLM-drafted (Codex, Nov 07, 2026).

**Canonical architecture:** `docs/DESIGN_SYSTEM.md`, settings and configuration surfaces.

## Overview

**Goal (testable):** An operator can edit runner settings in a responsive panel and save the complete assignment through the existing action.

**Problem:** The runner editor presents a long sequence of fields without clear grouping or a fixed save area.

**Solution summary:** Group execution, network access, durable tasks, and advanced settings in a right-side panel using shared components and tokens.

**Design flow:** Runner header → existing editor → shared policy fields → existing `updateRunnerAction` → existing PATCH → refresh after success.
`ui/packages/app/lib/api/runners.ts` owns the PATCH path.
`PolicyFields.tsx` owns assignment conversion and validation.
Existing API payloads and runner behavior stay authoritative.
The owner later approved an explicit trusted proxy for identity requests so Cloud acceptance can run.

## PR Intent & comprehension handshake

- **PR title (eventual):** Improve runner configuration editing
- **Intent (one sentence):** Make runner configuration understandable and comfortable to edit within the existing product theme.
- **Authoring handshake:** Implement the approved runner editor first. ASSUMPTIONS I'M MAKING: existing assignment semantics stay authoritative; chat follows later; unsupported network controls stay absent.
- **Implementer handshake:** Same intent. Preserve mounts, durable-task settings, decision model, and untouched labels when saving another field.

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/admin/runners/components/EditPolicyDialog.tsx` — save, reset, and error behavior.
2. `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyFields.tsx` — shared enrollment and edit assignment schema.
3. `ui/packages/app/lib/api/runners-types.ts` — supported isolation and network policies.
4. `ui/packages/design-system/src/design-system/Dialog.tsx` — focus ownership and accessible dialog primitives.
5. `ui/packages/app/tests/e2e/fixtures/chat-probe.ts` — browser fixture serving real components with an HTTP test boundary.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| This spec, active and done paths | CREATE / MOVE | Opening, decisions, and proof record |
| `docs/DESIGN_SYSTEM.md` | EDIT | Document configuration panel composition and its design-history entry |
| `docs/AUTH.md` | EDIT | Document the explicit identity proxy and unchanged verification |
| `docs/development.md` | EDIT | Show command-scoped Cloud acceptance proxy configuration |
| `rustd/crates/afd_identity/src/transport.rs` | CREATE | Explicit direct or trusted-proxy transport without ambient proxy discovery |
| `rustd/crates/afd_identity/src/lib.rs` | EDIT | Export the typed transport |
| `rustd/crates/afd_identity/src/jwks/http.rs` | EDIT | Accept the explicit transport for signing-key reads |
| `rustd/crates/afd_identity/src/provider.rs` | EDIT | Accept it for capability reads |
| `rustd/crates/afd_identity/src/metadata/mod.rs` | EDIT | Accept it for metadata writeback |
| `rustd/crates/afd_identity/tests/direct_client.rs` | EDIT | Prove direct ignores ambient proxies and explicit proxy routes requests |
| `rustd/crates/afd_identity/tests/{http_key_set,provider_claims,provider_metadata}.rs` | EDIT | Pass the explicit transport to the single constructor |
| `rustd/crates/agentsfleetd/src/preflight/config.rs` | EDIT | Carry the validated transport |
| `rustd/crates/agentsfleetd/src/preflight/knobs.rs` | EDIT | Name the optional proxy setting |
| `rustd/crates/agentsfleetd/src/preflight/read.rs` | EDIT | Read and validate the setting once without logging its value |
| `rustd/crates/agentsfleetd/src/preflight/mod.rs` | EDIT | Export the setting and include its tests |
| `rustd/crates/agentsfleetd/src/preflight/identity_proxy_tests.rs` | CREATE | Prove explicit opt-in, invalid configuration refusal, and redacted diagnostics |
| `rustd/crates/agentsfleetd/src/identity/mod.rs` | EDIT | Pass the validated transport to all identity clients |
| `rustd/crates/agentsfleetd/src/identity/tests.rs` | EDIT | Retain seam failure coverage with the explicit transport |
| `ui/packages/app/app/(dashboard)/admin/runners/components/EditPolicyDialog.tsx` | EDIT | Right-side editor, stable header and save footer, preserved action behavior |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyFields.tsx` | EDIT | Compose the shared grouped fields without changing conversion semantics |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicySection.tsx` | CREATE | Reusable local section heading and description |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyIsolationField.tsx` | CREATE | Isolation choices separated from unrelated settings |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyNetworkFields.tsx` | CREATE | Group supported network policies and registry domains |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyWorkerField.tsx` | CREATE | Worker count and restart guidance beside the control |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyEngineField.tsx` | CREATE | Restore the supported Bubblewrap engine display above isolation |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyDecisionField.tsx` | EDIT | Name TypeSafe Jev and resolve its enabled catalogue entry as the initial choice |
| `ui/packages/app/app/(dashboard)/admin/runners/components/usePolicyDecisionCatalogue.ts` | CREATE | Resolve the actual default and report pending setup without inventing a model identity |
| `ui/packages/app/app/(dashboard)/admin/runners/components/AddRunnerDialog.tsx` | EDIT | Wait for the default decision selection before enrollment |
| `ui/packages/app/app/(dashboard)/admin/runners/components/AddRunnerDialog.test.tsx` | EDIT | Enroll with the configured TypeSafe Jev default |
| `ui/packages/app/tests/runners-decision-model.test.tsx` | EDIT | Prove default selection, catalogue failures, and retained overrides |
| `ui/packages/app/tests/helpers/catalogue-fixtures.ts` | EDIT | Share a valid reviewed TypeSafe Jev model across the form and browser tests |
| `ui/packages/app/app/(dashboard)/admin/runners/components/EditPolicyDialog.test.tsx` | EDIT | Preserve save, error, pending, and reset guarantees |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyFields.test.tsx` | EDIT | Exercise grouped network choices and existing conversion guarantees |
| `ui/packages/app/tests/runners-create-dialog.test.ts` | EDIT | Scope isolation assertions to its group after network choices become radios |
| `ui/packages/app/tests/e2e/runner-configuration.spec.ts` | CREATE | Rendered desktop and mobile editing journey |
| `ui/packages/app/tests/e2e/fixtures/runner-configuration-probe.tsx` | CREATE | Render the production editor and enrollment fields |
| `ui/packages/app/tests/e2e/fixtures/runner-configuration-action.ts` | CREATE | Existing action replaced at the browser test HTTP boundary |
| `ui/packages/app/tests/e2e/fixtures/runner-configuration-build.ts` | CREATE | Build the browser probe using the established fixture pattern |
| `ui/packages/app/tests/e2e/fixtures/runner-configuration.ts` | CREATE | Serve browser fixture and shared stylesheet |
| `cli/test/acceptance/steer-live.spec.ts` | EDIT | Prove a fleet conversation produces readable terminal output without the JSON flag |
| `cli/src/commands/chat/render.ts` | EDIT | Render setup configuration and access as readable sections instead of inline JSON |
| `cli/test/chat/render.test.ts` | EDIT | Verify text review, empty settings, terminal control escaping, and unchanged explicit JSON |
| `cli/test/chat/actions.test.ts` | EDIT | Capture section headings through the existing output test double |
| `ui/packages/app/lib/api/runners-types.ts` | EDIT | Clarify the approved-destination network label and its package-site relationship |
| `ui/packages/design-system/src/interactions.css` | EDIT | Reuse the running animation for shared chat text sheen with reduced-motion and forced-color fallbacks |
| `ui/packages/app/components/domain/FleetReplyFigures.tsx` | EDIT | Apply the shared treatment to active waiting labels |
| `ui/packages/app/components/domain/FleetThought.tsx` | EDIT | Apply the shared treatment to Thinking |
| `ui/packages/app/components/domain/FleetToolCalls.tsx` | EDIT | Apply it to running action labels |
| `ui/packages/app/components/domain/FleetToolCallHeader.tsx` | EDIT | Apply it to each recorded running command |
| `ui/packages/app/components/domain/chat/DraftMessage.tsx` | EDIT | Use the same setup progress line without duplicate icons |
| `ui/packages/app/components/domain/ModelCells.tsx` | EDIT | Color the existing platform-managed model lock |
| `ui/packages/app/app/(dashboard)/admin/models/components/CatalogueList.tsx` | EDIT | Color default-model, pending, and price-context icons |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCells.tsx` | EDIT | Reuse existing badges for active and keyless model icons |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog.tsx` | EDIT | Identify stored-key context in the existing badge |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/secret-row-cells.tsx` | EDIT | Identify stored secrets with the existing key icon |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/SecretValueNotice.tsx` | EDIT | Use the shared information color for the existing help icon |
| `ui/packages/app/tests/e2e/chat-typography.spec.ts` | EDIT | Measure shared loading motion and reduced-motion behavior |
| `ui/packages/app/tests/e2e/fixtures/chat-browser-probe.tsx` | EDIT | Render the existing setup conversation in its actual pending state |
| `ui/packages/app/components/domain/chat/SetupChecks.tsx` | EDIT | Distinguish blocked, ready, and pending check icons with existing status colors |
| `ui/packages/app/components/domain/chat/SetupSummary.tsx` | EDIT | Add colored context icons to the existing summary headings |
| `ui/packages/app/components/domain/chat/WorkContextSections.tsx` | EDIT | Color existing source/output icons and identify recorded changes |
| `ui/packages/app/components/domain/chat/TaskPanel.tsx` | EDIT | Show waiting and paused work with the existing attention color |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/integrations/components/connector-rows.tsx` | EDIT | Color connector icons from current connection and pending/error state |

## Applicable Rules

- `code.md`: NDC, NRC, NLG, HLP, GRD, FLL, CODE-SEAM.
- `languages/typescript.md`: TS-ANY, TS-CAST, DID, ASE, TS-PRIMITIVE, TS-TOKEN, TS-LAYOUT.
- `tests.md`: TCF, TEST-OUTCOME, TEST-NEG, TEST-PUBLIC, TEST-INJECT, TEST-REPORT, TEST-SKIP.
- `docs.md`: DOC-02, DOC-03, DOC-05, DOC-S6.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `orly gate work` | yes, changed source | Split field concerns and use shared components |
| `orly gate verify` | yes, user surface and documentation | Update the design reference and run declared non-test checks |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | Split rendering by policy concern |
| Browser dry lane | yes, rendered page change | Desktop and mobile geometry, keyboard, network, and save checks |

## Prior-Art / Reference Implementations

- `ui/packages/app/components/domain/chat/DraftChatHeading.tsx` — right-side configuration dialog with bounded content.
- `ui/packages/design-system/src/design-system/OptionCard.tsx` — accessible selected policy choices.
- `ui/packages/app/tests/e2e/fixtures/chat-browser-build.ts` — browser fixture build and HTTP action boundary.

## Sections (implementation slices)

### §1 — Group the supported policy controls

Keep the assignment schema and conversion in their existing module.
Extract field rendering by concern and compose named sections for both enrollment and editing.
Isolation remains an assignment, rather than an engine selector.
The network interface offers only the three policies declared in `runners-types.ts`.
Registry hosts retain their existing meaning and empty-list defaults.

- **Dimension 1.1** — Supported network choices update the assignment → Test `saves each supported network policy` (unit): select each network mode → matching wire enum.
- **Dimension 1.2** — Invalid worker input cannot save → Test `rejects worker counts outside the supported range` (unit): zero or 65 workers → error and no action call.

### §2 — Keep editing and saving comfortable

Render a full-height right panel with a scrollable body and fixed header and footer.
Keep the current reset, error, label-only, and whole-assignment save behavior.
Retain pending protection against closing or duplicate submission.
Show heartbeat timing once and worker restart guidance beside its control.

- **Dimension 2.1** — Hidden assignment fields survive edits → Test `carries a stored bind through an edit that never touches it` (unit): change workers → retain mount data.
- **Dimension 2.2** — Failed saves keep the edit available → Test `surfaces a refused re-assignment instead of closing` (unit): 404 result → visible error and open dialog.

### §3 — Prove the rendered editing journey

Render the actual editor with shared fonts and tokens through the browser fixture pattern.
Inject save results at the HTTP test boundary without reaching a deployed account.
Measure the body and footer at desktop and mobile sizes, then capture the resulting panel.
Ask Astra to review the actual rendered implementation.

- **Dimension 3.1** — Editing works on desktop and mobile → Test `edits runner configuration with a reachable save footer` (e2e): open, scroll, edit, save → recorded complete assignment and refreshed values.
- **Dimension 3.2** — Network failure preserves edits for retry → Test `keeps runner edits after a failed save` (e2e): failed save → retained input, visible error, successful retry.

### §4 — Check the fleet conversation in a terminal

Use the existing live CLI fleet fixture and its one-shot conversation command.
Exercise both the established JSON path and an ordinary invocation without that flag.
Assert a readable final reply and human completion receipt, without a serialized event envelope.
Select any production repair from the observed failure rather than changing unrelated command output.

- **Dimension 4.1** — Ordinary fleet conversation is readable → Test `steer without the JSON flag renders a readable fleet conversation` (e2e): send a short acknowledgement request → final reply and human completion receipt.
- **Dimension 4.2** — Explicit JSON remains scriptable → Test `steer <id> <message> --json returns a processed terminal result` (e2e): explicit flag → processed JSON envelope.

### §5 — Align existing activity indicators

Reuse the shared running animation for a moving highlight on active chat labels.
Apply it to Fleet setup and Fleet conversation, retaining actual action text.
Honor reduced-motion and forced-color preferences; completed activity stays static.
Color existing status and context icons across setup, summaries, runners, connectors, secrets, and models.

- **Dimension 5.1** — Loading text stays readable without layout movement → Test `setup and fleet activity share a stable loading sheen with accessible motion fallbacks` (e2e): running → moving paint; reduced motion and forced colors → readable static text; completion → no loading marker.
- **Dimension 5.2** — The configured default preserves deliberate overrides → Tests in `runners-decision-model.test.tsx` (unit): enabled Jev → selected; missing default → blocked; stored override → retained; stale lookup after closing → ignored.

### §6 — Explicit trusted identity proxy

Resolve `IDENTITY_HTTP_PROXY_URL` once through preflight and pass the typed transport to all three identity clients.
Keep direct access as the default, redact diagnostics, preserve verification, and refuse invalid configuration.
- **Dimension 6.1** — Proxy use requires explicit opt-in → preflight tests: absent or blank setting with ambient proxies → Direct; valid setting → trusted route; invalid setting → startup fault.
- **Dimension 6.2** — Requests follow the configured route → `direct_client.rs`: ambient dead proxy → direct success; explicit live proxy → remote-host success; explicit dead proxy → failure without direct fallback.

## Interfaces

Existing `EditPolicyDialog` props and `updateRunnerAction` payload semantics stay unchanged.
The local field components take the existing `Control<PolicyFormValues>`.
The API still accepts the same `AssignedPolicy` and optional labels.
The field composition reports pending decision-model setup to enrollment and editing.
Unassigned forms prefer the enabled TypeSafe `jev-latest` entry; existing selections remain intact.
An explicit None choice retains enrollment without model catalogue access and never guesses a model identifier.
No catalogue identity or provider credential is hardcoded into a production form.
`IDENTITY_HTTP_PROXY_URL` explicitly selects a trusted proxy for the daemon's identity clients.
Unset remains direct; ambient proxy variables remain ignored. Invalid values refuse boot.

## Failure Modes

| Mode | Kind | Cause | Handling | Test |
|------|------|-------|----------|------|
| Invalid workers | failure | Count outside 1–64 | Inline field error; no write | `rejects worker counts outside the supported range` |
| Runner unavailable | failure | Action returns 404 | Keep dialog open and show existing error sentence | `surfaces a refused re-assignment instead of closing` |
| Network failure | failure | Save action fails | Retain edit; allow deliberate retry | `keeps runner edits after a failed save` |

## Invariants

1. Complete assignments preserve untouched policy fields — `formFromPolicy` and `policyFromForm`, covered by stored-mount tests.
2. Untouched labels never enter a policy-only PATCH — original label comparison, covered by existing stored-label tests.
3. Pending writes cannot close the editor — transition state guards both dismissal and controls.
4. Unsupported network controls cannot write invented settings — typed policy enums and the existing schema remain authoritative.

## Metrics & Observability

N/A: no product or operator signal changes.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|-----------|---------------------|----------|----------|-----------------|
| R1 | Existing assignment and edit guarantees pass | `make test-unit-app` | exit 0 | P0 | |
| R2 | Rendered editor supports desktop, mobile, and retry | `make dry-app` | exit 0 | P0 | |
| R3 | Local browser and terminal acceptance pass | `make acceptance-codex` | exit 0; both acceptance suites pass | P0 | OWNER-ACCEPTED DEFERRAL: one proxy-enabled attempt stopped at bootstrap HTTP 401; suites not reached |
| R4 | TypeScript packages meet the 100% coverage gates | `make test-coverage-all` | exit 0; app, CLI, and design-system coverage 100% | P0 | Initial pass: app 4,771; website 133; CLI 2,028; design-system 522; all configured measures 100%. Final review fixes await the final check. |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit checks | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration checks | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version alignment | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

The owner explicitly instructed pushing the PR, addressing Greptile, and merging after all PR checks are green.
The owner explicitly accepted remaining acceptance failures after one retry and requested closure rather than extending this spec.

## Dead Code Sweep

N/A: no files deleted. Field rendering moves out of `PolicyFields` without compatibility exports or duplicate implementations.

## Out of Scope

- Runner chat, conversation layout changes, task-summary redesign, and fleet setup navigation follow later under the approved priority.
- New endpoints, harness changes, engine switching, independent package-domain toggles, logs, and benchmarks are not part of this editor.

## Product Clarity (authoring record)

1. **Successful user moment** — Save a runner change while understanding its effect and preserving other settings.
2. **Preserved user behaviour** — Enrollment, labels, decision models, mounts, and pending/error behavior keep their existing semantics.
3. **Optimal-way check** — A grouped panel with an always-visible save action directly addresses the long editor.
4. **Rebuild-vs-iterate** — Iterate on the shared field and dialog components.
5. **What we build** — Policy sections, supported network choices, stable save controls, and rendered browser proof.
6. **What we do NOT build** — A new runner backend or assistant transport.
7. **Fit with existing features** — Enrollment shares the same fields; fleet chat transport remains untouched.
8. **Surface order** — UI first, using existing APIs, as the owner requested.
9. **Dashboard restraint** — Show settings the API actually supports; omit speculative engine and network controls.
10. **Confused-user next step** — Read the nearby field guidance or the existing runner policy documentation link.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Separate field composition, editor layout, and browser proof.
- **Alternatives considered:** A wider centered modal still requires a long scroll. A new settings route adds navigation and duplicates editing ownership.
- **Patch-vs-refactor verdict:** A focused refactor because grouping field rendering lets enrollment and editing improve together without changing policy semantics.

## Discovery (consult log)

- **Consults** — Source revision `7c2c3167b`; existing runner types and action remain the source of supported settings.
- **Model reviews** — Earlier Astra reviews covered the conversation prototype. The actual editor still needs its rendered review.
- **Open blockers** — None at opening.
- **Deferrals and transfers** — Conversation work follows later under the owner's explicit priority.

> Owner (Indy, Nov 07, 2026): "Also since you have everthing you could implement the edit of the runner config, as per you earlier design?" — context: runner editor implementation.

> Owner (Indy, Nov 07, 2026): "the chat can be changed later" — context: runner editor comes first.

### Session Notes

PLAN: Implement the approved runner editor using existing actions and assignment validation.
ASSUMPTIONS I'M MAKING: UI presentation is the scope; chat follows later; no harness or backend changes.
`orly doctor` passed before opening the worktree at `7c2c3167b`.

Indy requested `acceptance-codex` after implementation and identified fleet conversation output in the terminal as another concern.
Investigate that output through the existing CLI acceptance suite before selecting a code change.
Astra reviewed the actual editor source and desktop/mobile screenshots: one accessibility finding fixed, no remaining visual blockers.
The first unit invocation inherited absent environment-file paths and disabled analytics.
Clearing those invocation variables produced 491 passing files and 4,762 passing tests through `make dry-app`.

Indy requested the earlier Bubblewrap engine display and TypeSafe Jev as the default decision model.
He confirmed Landlock isolation and enabled durable tasks as new-runner defaults.
The engine enum in `rustd/crates/agentsfleet_runner/src/engine.rs` contains only Bubblewrap.
Decision credentials come from the platform-admin workspace's encrypted provider bag through `afd_credential::Providers`.
An absent decision-model assignment does not select Jev in the backend.

> Owner (Indy, Nov 07, 2026): "Approve this scope" — context: Runner defaults, existing-widget icon colors, targeted CLI investigation, and final acceptance.

> Owner (Indy, Nov 07, 2026): "yes lets use that" — context: shared chat activity line with moving text highlight, actual progress labels, and reduced-motion support from the attached video.

> Owner (Indy, Nov 07, 2026): "ensure the typescript changes (design-system, cli, app) all have 100% coverage" — context: run the declared package coverage gates without reducing their floors.

> Owner (Indy, Nov 08, 2026): "Okay signed off on the loading treatment, it looks good." — context: rendered shared setup and fleet chat loading video.

Companion documentation is in `/workspace/runner-configuration-docs` on `chore/m241-runner-edit-changelog`.
It updates `runners.mdx`, `cli/agentsfleet.mdx`, and `changelog.mdx` for the visible behavior.

`make acceptance-codex` built the daemon, runner, CLI, and app, then stopped at bootstrap with HTTP 503.
The daemon log records failed Clerk metadata and signing-key requests.
`afd_identity/src/direct.rs:24` explicitly disables proxies for these clients.
Fresh inherited-proxy probes returned HTTP 401 from the unauthenticated Clerk Backend API and HTTP 200 for its public signing keys.
The published environment therefore has working proxy connectivity; native identity connectivity remains blocked.
No environment-publishing changes are included in this workstream.

> Owner (Indy, Nov 08, 2026): "yes add the code fix for explicit trusted-proxy for the identity client (can it be done on test) but have or injecting the proxy env via some variable so it reaches reqwest or so" — context: explicit transport opt-in, retaining verification and the direct default.

> Owner (Indy, Nov 08, 2026): "Oky try once, or else i would start a local computeer use codex session later. So lets keep moving to a PR" — context: one acceptance retry after the proxy change, then report any remaining environment blocker for local follow-up.

> Owner (Indy, Nov 08, 2026): "I prefer we move this spec to done, even if the proxy test fails (one) that you plan to do since we cant just keep dragging specs. And indy will open a new cloud session to fix this later. And indy is okay in taking a hit on the accepatance-e2e or acceptance-cli failing." — context: accepted acceptance deferral and explicit authorization to close this workstream after its single retry.

Closure: the single proxy-enabled `make acceptance-codex` attempt stopped at bootstrap with HTTP 401.
The browser and CLI suites were not reached. Acceptance remains pending under the owner's accepted deferral.
Final code checks and PR review continue without keeping this spec active.

> Owner (Indy, Nov 08, 2026): "Indy is going to bed, can you ensure the PR is sent, CI jobs are green and the greptile feedback are replied and resolved. And when all are green, merge the PR." — context: explicit authorization to push, open, update, and merge this PR only after green checks and resolved review.

`make test-unit-rustd-daemon` passed 157 tests; `make test-unit-rustd-daemon-libs` passed 3,450.
`make test-unit-all` stopped in two unchanged `afr_executor` process-group cleanup tests; no tests or gates were relaxed.
