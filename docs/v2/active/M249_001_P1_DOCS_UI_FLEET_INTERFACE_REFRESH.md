# M249_001: Readable fleet conversations and account navigation

**Prototype:** v2.0.0
**Milestone:** M249
**Workstream:** 001
**Date:** Nov 11, 2026
**Status:** IN_PROGRESS
**Priority:** P1, approved customer interface refresh
**Categories:** DOCS, UI
**Batch:** B1, shared surfaces before account identity operations and public vocabulary migration
**Depends on:** none
**Provenance:** Agent-generated from Indy's approved visual review and implementation plan
**Canonical architecture:** `docs/DESIGN_SYSTEM.md`; `docs/architecture/context_lifecycle.md`

## Overview

**Goal (testable):** A user can create, read, manage, archive, and delete a fleet through the approved compact interface without losing history, drafts, or recovery actions.

**Problem:** The conversation needs clearer type, permanent Summary, discoverable navigation, and stable feedback after actions.

**Solution summary:** Apply the approved Manrope and Phosphor design to the existing React, Tailwind, and assistant-ui surfaces. Preserve their runtime and authority.

**Design flow:** Authenticated shell → selected workspace → setup chat → confirmed save → fleet conversation with inline Summary. Existing fleet and draft clients remain the request owners.

## PR Intent & comprehension handshake

- **PR title (eventual):** Refresh fleet conversations and account navigation
- **Intent (one sentence):** Make fleet work readable and its state and recovery actions available without opening a context drawer.
- **Authoring handshake:** Implement the approved mock using real source state. ASSUMPTIONS I'M MAKING: current platform-default CLI stays intact; account permissions stay intact; no capability-composition feature.
- **Implementer handshake:** The approved design is the target. Shared styles, navigation, conversation and resource pages change; actor identity, request ownership and recorded outcomes remain authoritative.

## Implementing agent — read these first

1. `docs/DESIGN_SYSTEM.md` — approved typography, layout, outcomes and lifecycle decisions.
2. `ui/packages/app/components/domain/chat/WorkspaceChat.tsx` — draft ownership and confirmed save state.
3. `ui/packages/app/components/domain/FleetThread.tsx` — assistant-ui runtime over fleet events.
4. `ui/packages/app/components/layout/ShellFrame.tsx` — authenticated shell and workspace navigation.
5. `ui/packages/app/components/domain/chat/WorkContextSections.tsx` — recorded changes and source facts.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `docs/v2/active/M249_001_P1_DOCS_UI_FLEET_INTERFACE_REFRESH.md` | CREATE | Approved implementation specification; move to active and done with lifecycle |
| `bun.lock` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `docs/DESIGN_SYSTEM.md` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/app/AGENTS.md` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/app/app/(auth)/layout.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/error.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/invites/components/InvitesView.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/ApiKeyList.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/settings/members/components/InviteDialog.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/settings/members/components/TeamTable.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsTable.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetConfig.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/KillSwitch.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/MemoryMenu.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/MemoryPanel.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/NewFleetEntrance.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/integrations/components/IntegrationsConnectors.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/integrations/components/connector-rows.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/AddSecretDialog.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/AddSecretForm.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/SecretValueNotice.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/secret-row-cells.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/defaults/page.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCells.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/security/page.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/globals.css` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/app/app/layout.tsx` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/app/components/domain/CollectionToolbar.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/EventDetailsDialog.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/EventsList.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/ExhaustionBadge.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/ExhaustionBanner.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/FleetFailedOutcome.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/FleetLibraryEmptyState.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/FleetMessageRow.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/MessageExpansion.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/ModelCells.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/RefreshButton.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/SteerComposer.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/ChatAvailability.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/ChatComposer.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/ChatFailure.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/ChatSummary.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/ChatThread.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/DraftChatHeading.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/DraftMessage.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/FleetSetupWorkspace.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/LibraryRemoval.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/NativeTaskPrBadge.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/SetupChecks.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/SetupStatusFlap.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/SetupSummary.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/TaskPanel.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/WorkActivity.tsx` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/app/components/domain/chat/WorkContextSections.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/WorkspaceChat.tsx` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/app/components/domain/fleetMessageRenderers.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/tool-call-presentation.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/layout/AccountMenuNavigation.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/layout/DraftSidebarItem.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/layout/FleetDeleteDialog.tsx` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/app/components/layout/NoWorkspaceEmptyState.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/layout/RecentChats.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/layout/ShellControls.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/layout/ShellFrame.tsx` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/app/components/layout/SidebarNavigation.tsx` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/app/components/layout/SidebarNavigationSections.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/layout/WorkspaceSwitcherMenu.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/layout/WorkspaceSwitcherTrigger.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/layout/account-menu-data.ts` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/layout/shell-sidebar-state.ts` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/app/components/layout/sidebar-navigation-data.ts` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/package.json` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/app/tests/dashboard-killswitch.test.ts` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/tests/dashboard-placeholder.test.ts` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/tests/dashboard-workspace.test.ts` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/tests/e2e/fixtures/chat-alignment-probe.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/tests/fleets-routes/harness.ts` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/tests/fleets.test.ts` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/tests/integrations-connectors.test.ts` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/tests/secrets-components.test.ts` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/tests/secrets-list.test.ts` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/tests/secrets-page.test.ts` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/design-system/package.json` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/design-system/src/design-system/Alert.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/design-system/src/design-system/CopyButton.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/design-system/src/design-system/DataTableRows.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/design-system/src/design-system/DataTableView.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/design-system/src/design-system/Dialog.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/design-system/src/design-system/Terminal.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/design-system/src/index.ts` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/design-system/src/theme.css` | EDIT | Apply the approved interface and preserve current request and scope ownership |
| `ui/packages/design-system/src/tokens.css` | EDIT | Apply the approved interface and preserve current request and scope ownership |

The Files Changed table expands with each grounded implementation slice before editing its additional callers or tests.
The owner's approval covers the described surfaces; it does not authorize unrelated changes.

## Applicable Rules

- `code.md`: NDC, NRC, NLG, HLP, ORP, GRD, TGU, ECL.
- `languages/typescript.md`: TS-SHAPE, TS-TYPEIMPORT, TS-SCHEMA, TS-FINALLY, DID, ASE.
- `tests.md`: TNM, TCF, TEST-OUTCOME, TEST-ERROR, TEST-NEG, TEST-PUBLIC.
- `docs.md`: DOC-06, DOC-09, DOC-11, DOC-33, DOC-43.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| Source lint and type checking | Yes | Shared tokens, direct imports, typed states, existing make lanes |
| Browser checks | Yes | Render real components in both themes and at 390px, 768px and 1480px |
| File and function length | Yes | Keep stateful behavior in focused modules with callers in the same change |
| Work and verify gates | Yes | Run the declared gates locally; retain exact failures and evidence |

## Prior-Art / Reference Implementations

- `ui/packages/app/components/domain/chat/WorkActivity.tsx` supplies recorded activity; preserve its event model.
- `ui/packages/app/lib/api/draft-continuation.ts` owns save and activation recovery; reuse that path.
- `ui/packages/design-system/src/tokens.css` and `theme.css` remain the shared token source.
- The approved local preview at `/workspace/artifacts/fleet-refresh-v2/proposal.html` supplies visuals, not backend behavior.

## Sections (implementation slices)

### §1 — Shared type, icons, colors and borders

Use Manrope 13px/20px regular for desktop reading, 12px/16px metadata, and 16px/26px phone reading. Controls use 13px/20px medium; phone typing uses 16px. Keep code monospace.
Use Phosphor 20px rail, 16px action and 18px composer glyphs; preserve 48px desktop rail and 44px phone targets. Use 10px, 16px and 22px rounded surfaces and zero tracking.
Keep the website's explicit display roles separate from application reading roles. Retain Radix unless a concrete primitive defect justifies a separate change.

- **Dimension 1.1** — Browser-computed fonts and sizes match approved roles → Test `approved type and icon scale` (browser): desktop/phone and light/dark render expected sizes.
- **Dimension 1.2** — Labels and focus targets remain reachable at 200 percent zoom → Test `compact controls remain reachable` (browser): long names wrap and no controls overflow.

### §2 — Navigation and resource pages

Default to the compact rail with a square-and-pencil New fleet action. Keep fleets reachable without an expanded permanent sidebar.
Place Plugins, Secrets and Connected apps in navigation; Models, Team, Usage and Security remain discoverable from the account menu.
Team remains account-wide. Preserve Owner/Member roles, pending invitations and delivery recovery. Usage remains settled ledger data, with unknown values distinct from zero.
Account Models keeps one form for new or saved provider credentials and optional Make active. Reuse a saved credential without replacing its bytes; preserve partial-success retries.
Platform defaults stay under the existing `agentsfleet platform keys` commands. No Admin page is created.

- **Dimension 2.1** — New fleet is reachable from empty and active views → Test `new fleet remains discoverable` (browser): writing-pad action opens the setup route.
- **Dimension 2.2** — Resource pages retain actual scope → Test `account and workspace navigation preserve scope` (unit/browser): account Team and workspace secrets use their existing routes.
- **Dimension 2.3** — Saved-key reuse and partial creation preserve resources → Test `model creation retries the failed step` (unit): registration failure retains the key reference; activation failure retains the registered model.

### §3 — Inline Summary and continuous conversation

Summary is always inline, 248px on desktop with a compact phone presentation. Show relevant approvals, changes, outputs, sources and next wake-up; omit Environment and empty groups.
Keep recorded command, edit, integration and schedule outcomes in their existing message stream. Distinguish running, succeeded, failed, interrupted and unconfirmed states.
After confirmed fleet creation, replace the setup URL with the fleet URL without an extra Open fleet click. Do not replay authoring text as a runtime request.
Keep setup and runtime drafts separate. Creator-private authoring history never becomes shared conversation history.

- **Dimension 3.1** — Summary needs no toggle or drawer → Test `summary is inline at every viewport` (browser): relevant content is visible in the page flow.
- **Dimension 3.2** — Saved identity drives navigation → Test `confirmed save opens the same fleet` (unit): partial activation failure keeps the created fleet and retry identity.
- **Dimension 3.3** — State labels remain evidence-based → Test `failed and unconfirmed work stays explicit` (unit/browser): incomplete work never receives a succeeded label.

### §4 — Archive, deletion and feedback

Archive permanently stops the fleet, retains history, and offers no Restore. Permanently Delete is independent and available directly on active or archived fleets.
Compose the current guarded terminal stop and deletion internally, preserving authorization, ETags and uncertain-response reconciliation.
If stop succeeds and deletion fails, retain the fleet and offer Retry deletion only. Never claim deletion based on a failed read.
On confirmed success, remove the active-list row and move an affected open fleet to New fleet. Keep another selected fleet in place.
Use the approved compact notice: Fleet archived with View history, or Fleet permanently deleted. No Undo; history remains reachable after dismissal.

- **Dimension 4.1** — Direct deletion uses one user confirmation → Test `active fleet deletion handles terminal stop internally` (unit/integration): stop then delete completes without another dialog.
- **Dimension 4.2** — Partial deletion remains recoverable → Test `deletion failure retains archived fleet` (unit): stop success/delete failure shows durable recovery and retries only delete.
- **Dimension 4.3** — Notices announce confirmed outcomes → Test `lifecycle notice survives navigation` (browser): polite announcement, no focus theft, pause dismissal on focus/hover, read-only history.

### §5 — Verification and visual finish

Run targeted tests for changed behavior, then repository work/verify checks. Run authenticated browser flows against real data before claiming production readiness.
Review spacing, borders, fonts, icon scale, long names, empty states and failures at the approved widths, both themes and 200 percent zoom.
The prototype's passing checks do not substitute for implementation checks.

- **Dimension 5.1** — Converted surfaces match the approved visual scale → Test `fleet refresh visual review` (browser/manual): screenshots and computed styles record final evidence.
- **Dimension 5.2** — Existing request ownership survives → Test `fleet and setup recovery journeys` (browser): reload, cancel, uncertain sends, scope switch and retries preserve the original operation.

## Interfaces

Keep existing fleet, draft, message, event, model registry, secret, membership and billing interfaces for this UI work.
The coordinated public Plugins and archived-status vocabulary changes have their own implementation specification.
Custom profile and browser-session operations have a separate auth specification; UI components consume app-owned shapes.

| `ui/packages/design-system/src/app-tokens.css` | CREATE | Share approved app type, color, icon and spacing tokens without changing website display type |
| `ui/packages/app/next.config.ts` | EDIT | Optimize Phosphor imports |
| `ui/packages/app/components/domain/FleetWorkSidebar.tsx` | EDIT | Render a single inline Summary tree across breakpoints |
| `ui/packages/app/components/domain/chat/DraftConversation.tsx` | EDIT | Mount the setup Summary without a toggle provider |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetHeader.tsx` | EDIT | Remove Summary toggle and simplify fleet actions |
| `ui/packages/app/components/domain/chat/ChatSummary.test.tsx` | EDIT | Prove Summary is inline and the composer survives layout changes |
| `ui/packages/app/components/layout/shell-sidebar-state.test.tsx` | EDIT | Verify compact default navigation and explicit expansion |
| `ui/packages/app/tests/e2e/chat-typography.spec.ts` | EDIT | Check approved computed font sizes and mobile typing |
| `ui/packages/app/tests/e2e/chat-alignment.spec.ts` | EDIT | Check actual responsive spacing and icon bounds |
| `ui/packages/app/tests/app-shell-navigation.test.tsx` | EDIT | Check navigation labels and destinations |
| `ui/packages/app/tests/app-shell-frame.test.ts` | EDIT | Check shell ownership with refreshed layout |
| `ui/packages/app/tests/task-history-sidebar.test.tsx` | EDIT | Verify a single Summary reader and preserved task ownership |

## Failure Modes

| Mode | Kind | Cause | Handling | Test |
|---|---|---|---|---|
| Uncertain save | failure | Lost creation response | Reconcile saved identity; preserve retry identity | `confirmed save opens the same fleet` |
| Registration after key save fails | failure | Registry rejects request | Retain key reference; retry registration only | `model creation retries the failed step` |
| Delete after stop fails | failure | Conflict or unavailable backend | Retain history, show Retry deletion | `deletion failure retains archived fleet` |
| Creator transcript disclosure | security | Shared member opens a fleet | Read authorized runtime data, never another creator's draft | `fleet and setup recovery journeys` |
| Narrow viewport | capacity | Long content or zoom | Wrap and scroll without hiding actions | `compact controls remain reachable` |

## Invariants

1. Existing account, workspace, creator and fleet boundaries remain authoritative.
2. A UI notification is never evidence that an operation succeeded; server-confirmed state drives it.
3. Archive is terminal and cannot be restored; deletion is a separate user choice.
4. Plugins reuse complete setups; no capability attachment or runtime mention is added.
5. Platform administration stays in the CLI with existing scopes.

## Metrics & Observability

Preserve existing coarse product events and server outcome records. Add no credential, prompt or private-history logging.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | UI behavior and recovery (§1–§4) | `make test-unit-app` | exit 0 | P0 | |
| R2 | Shared components (§1) | `make test-unit-design-system` | exit 0 | P0 | |
| R3 | Rendered flows and spacing (§5) | `make dry-app` | exit 0 and saved visual evidence | P0 | |
| S1 | Work gate | `orly gate work` | exit 0 | P0 | |
| S2 | Verification gate | `orly gate verify` | exit 0 | P0 | |
| R4 | Repository lint | `make lint-all` | exit 0 | P0 | |
| R5 | Repository unit tests | `make test-unit-all` | exit 0 | P0 | |
| R6 | Backend integration | `make test-integration-rustd` | exit 0 | P0 | |
| R7 | Version consistency | `make check-version` | exit 0 | P0 | |
| S4 | PR gate (CI/ship; cloud stops before this) | `orly gate pr` | recorded before merge | P0 | |
| S3 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Review requirements before later sections

- Summary mounts exactly one task reader and one responsive tree; no hidden duplicate mobile reader. Existing task provider owns the stream.
- Models must resolve the account primary vault by the backend's `(created_at, id)` ordering and authenticated account ID, excluding joined accounts. Keep explicit secret-write uncertainty, saved-key reference, registration reconciliation by exact model/reference, and activation-only retry. Matching secret metadata does not prove matching bytes.
- Save handoff uses confirmed fleet identity independently of readiness failure. Setup history rehydrates through creator-filtered context; keep setup/runtime buffers separate and clear them on identity/access loss. Never copy a setup prompt into runtime messages.
- Deletion retains confirmed-stop state, reads current authorized state after uncertain writes, and handles missing responses without claiming success. A denied or unavailable read cannot prove deletion. Preserve billed attribution and settled runner retention.
- The separate vocabulary spec must map public Plugins names to immutable historical representations without rewriting frozen revisions, and translate only existing library write authority without widening narrowed identities. The separate auth spec must preserve current subject-based actor/cache identity during adapter changes.

## Dead Code Sweep

Remove the Summary drawer/toggle callers and Lucide application imports after replacements are wired. Keep historical spec records unchanged.

## Out of Scope

- Platform-default UI, per Indy's explicit correction; preserve existing CLI behavior.
- Flow 2 capability composition, recorded for a future M239 discussion.
- Readable environment variables; existing Secrets remain the supported feature.
- A second chat runtime or speculative virtualization.

## Product Clarity (authoring record)

1. **Successful user moment** — Read and manage a fleet with its Summary and reliable action feedback visible.
2. **Preserved user behaviour** — Messages, approvals, scheduled work and recovery retain their current scope and runtime.
3. **Optimal-way check** — Reuse request owners and data readers while replacing presentation.
4. **Rebuild-vs-iterate** — Iterate on the current React, Tailwind, Radix and assistant-ui stack.
5. **What we build** — Shared visual tokens, rail, conversation, Summary, resource pages and lifecycle feedback.
6. **What we do NOT build** — Platform-default UI, capability composition, env resources or a second chat framework.
7. **Fit with existing features** — Existing model, vault, membership and fleet APIs retain authority.
8. **Surface order** — UI first; coordinate public naming changes separately before release.
9. **Dashboard restraint** — Show recorded outcomes and real data; no sample-backed product controls.
10. **Confused-user next step** — Durable failures explain retry or access recovery beside the failed operation.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Shared styling, navigation, conversation, lifecycle and verification slices.
- **Alternatives considered:** Svelte migration, second chat runtime and universal capability plugins were rejected as unnecessary scope.
- **Patch-vs-refactor verdict:** A refactor of presentation with focused lifecycle composition; preserve existing runtime state owners.

## Discovery (consult log)

- **Consults** — Fresh `origin/main` at `bfcfe5bac`; the merged platform CLI change removed operator pages. `orly doctor` passed both installation and rule checks.
- **Model reviews** — Native EGM implementation-readiness review completed against `bfcfe5bac`: 1 blocker for the separate vocabulary migration, 6 majors, 1 nit. No blocker to shared visual work. Report: `/workspace/artifacts/fleet-refresh-v2/implementation-readiness-current-main.md`. Missing Summary owners and focused test paths are now included. The remaining model, handoff, deletion, vocabulary and identity findings must be resolved in their corresponding sections/specifications before those edits.
- **Open blockers** — None in approved product direction; implementation details and exact callers are checked before each slice.
- **Deferrals and transfers** — Only user-excluded work below. Auth and public vocabulary are separately specified approved work, not dropped scope.

> Owner (Indy, Nov 11, 2026): “Yes i approve the changes you gave me.”
> Owner (Indy, Nov 11, 2026): “i set the admin platform default model via cli today, that is fine to me. I donot need a ui for it.”
> Owner (Indy, Nov 11, 2026): “Yes the access scope must be retained.”
> Owner (Indy, Nov 11, 2026): “Permanent stop, retain history, no Restore”.
> Owner (Indy, Nov 11, 2026): “i think you need to allow me to either Archive or Permanently Delete, not on a step level like Archive -> then Permanently Delete”.
> Owner (Indy, Nov 11, 2026): “Yes flow 1”.
> Owner (Indy, Nov 11, 2026): “I dont want to build this, but note it down with the elephant context details somewhere in our usual spot” — Flow 2 deferred to M239's revisit note.
