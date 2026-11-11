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
| `ui/packages/design-system/src/design-system/Input.test.tsx` | EDIT | Check semantic control shape |
| `ui/packages/design-system/src/design-system/Input.tsx` | EDIT | Use semantic control radius while preserving website defaults |
| `ui/packages/design-system/src/utils.test.ts` | EDIT | Preserve font family, weight, size and foreground when utilities merge |
| `ui/packages/design-system/src/utils.ts` | EDIT | Preserve font family, weight, size and foreground when utilities merge |
| `ui/packages/app/tests/workspace-chat/setup-summary.test.tsx` | EDIT | Preserve navigation and private-history assertions in the inline layout |
| `ui/packages/app/tests/workspace-chat/save-continuation.test.tsx` | EDIT | Preserve private editor history and rebase reads while handing off confirmed writes |
| `ui/packages/app/tests/app-components.test.ts` | EDIT | Preserve navigation and private-history assertions in the inline layout |
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
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetConfig.tsx` | DELETE | Remove the separate killed-only deletion button after the shared action menu is wired |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/KillSwitch.tsx` | DELETE | Replace the obsolete Kill presentation with FleetActions |
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
| `ui/packages/app/components/domain/FleetMessageRow.tsx` | EDIT | Restore useful turn timestamps through the shared message layout |
| `ui/packages/app/components/domain/MessageExpansion.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/ModelCells.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/RefreshButton.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/SteerComposer.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/ChatAvailability.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/ChatComposer.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/ChatFailure.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/domain/chat/ChatSummary.tsx` | DELETE | Remove obsolete toggle state |
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
| `ui/packages/app/components/layout/FleetDeleteDialog.tsx` | DELETE | Replace the blind two-write flow with shared lifecycle recovery |
| `ui/packages/app/components/layout/NoWorkspaceEmptyState.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/layout/RecentChats.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/layout/ShellControls.tsx` | EDIT | Use the shared Phosphor icon family without changing action semantics |
| `ui/packages/app/components/layout/ShellFrame.tsx` | EDIT | Match the approved persistent rail and rounded working surface without the old global header |
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
| `ui/packages/app/tests/e2e/fixtures/chat-alignment-probe.tsx` | EDIT | Review production shell alignment without a parallel fake navigation layout |
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
| `ui/packages/design-system/src/app-tokens.css` | CREATE | Share approved dashboard sizes, colors, icons and spacing while preserving website display sizes |
| `ui/packages/app/components/layout/MobileNavigationDialog.tsx` | DELETE | Remove the unused drawer now that the compact rail stays visible on phones |
| `ui/packages/app/components/domain/island-dynamic/intent-shell-controls.test.tsx` | EDIT | Replace obsolete drawer-loader checks with persistent-rail control behavior |
| `ui/packages/app/components/layout/WorkspaceSwitcher.tsx` | EDIT | Render the account initials from the existing auth facade without waiting for menu activation |
| `ui/packages/app/components/domain/FleetTimestamp.tsx` | EDIT | Use the approved metadata scale for relative conversation time |
| `ui/packages/app/tests/e2e/fixtures/visual-auth-fixture.tsx` | CREATE | Supply explicit sample identity only inside browser component fixtures |
| `ui/packages/app/tests/e2e/fixtures/chat-browser-build.ts` | EDIT | Bind fixture identity at the auth boundary while using the production shell |
| `ui/packages/app/tests/fleet-tool-calls.test.tsx` | EDIT | Verify steady running glyphs and outcome colors |
| `ui/packages/app/tests/e2e/fixtures/refresh-visual-probe.tsx` | CREATE | Render current production conversation and Summary with compact review chrome and sample activity states |
| `ui/packages/app/tests/e2e/fixtures/chat-browser-probe.tsx` | EDIT | Expose the current visual review through the existing production-component fixture |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/actions.ts` | EDIT | Reconcile uncertain registration and activation through the existing APIs |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/model-creation.ts` | CREATE | Typed saved-step snapshots and credential compatibility |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/model-creation-operations.ts` | CREATE | Run confirmed-step creation and reconciliation through existing scoped APIs |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/CreateModelForm.tsx` | CREATE | Compose provider, credential, model and activation controls with one submit action |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/use-model-creation.ts` | CREATE | Preserve confirmed storage, registration and activation; reconcile uncertain results |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCredentialFields.tsx` | CREATE | Provider-filtered saved-key selection and explicit new-key fields |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/use-stored-secrets.ts` | EDIT | Drop stale workspace reads and preserve lazy secret loading |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable.tsx` | EDIT | Preserve the existing workspace parameter for model dialogs |
| `ui/packages/app/tests/model-creation-recovery.test.tsx` | CREATE | Saved-key reuse, scoped cleanup and each confirmed or uncertain recovery step |
| `ui/packages/app/tests/models-registry-add.test.tsx` | EDIT | Verify the new form and prevent implicit rotation |
| `ui/packages/app/tests/models-registry-add-key.test.tsx` | EDIT | Verify explicit new-key creation |
| `ui/packages/app/tests/models-registry-add-custom.test.tsx` | EDIT | Preserve compatible endpoints and optional credentials |
| `ui/packages/app/tests/models-registry-created-activation.test.tsx` | EDIT | Exercise real action/API boundaries and partial success |
| `ui/packages/app/tests/models-registry-secrets.test.tsx` | EDIT | Exercise workspace secret metadata and recovery |
| `ui/packages/app/tests/models-provider-loading.test.tsx` | EDIT | Preserve catalogue loading behavior through the authenticated form boundary |
| `ui/packages/app/components/domain/chat/FleetTaskProvider.tsx` | EDIT | Share existing approval counts and saved schedules with the single Summary reader |
| `ui/packages/app/components/domain/chat/WorkWakeups.tsx` | CREATE | Show recorded scheduler registration and next-run state without inventing success |
| `ui/packages/app/components/domain/chat/WorkWakeups.test.tsx` | CREATE | Cover scheduled, unsynced, paused and unavailable wake-ups |
| `ui/packages/app/components/domain/FleetThread.tsx` | EDIT | Pass the existing live approval count and saved schedules into Summary |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/ChatView.tsx` | EDIT | Keep Summary approval counts on the existing fleet stream |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` | EDIT | Forward schedules already present in the fleet detail |
| `ui/packages/app/components/domain/FleetReplyParts.tsx` | EDIT | Keep an explicit safe fallback for unhandled content parts |
| `ui/packages/app/components/domain/FleetReplyBody.tsx` | EDIT | Pass recorded timestamps through the shared reply layout |
| `ui/packages/app/components/domain/chat/ChatComposerFrame.tsx` | EDIT | Share the approved composer spacing and left-model/right-send placement |
| `ui/packages/app/components/domain/FleetReplyBody.test.tsx` | EDIT | Verify unsupported parts cannot disappear or inject content |
| `ui/packages/app/components/domain/chat/TaskPanel.test.tsx` | EDIT | Preserve single-reader authorization and failed-read recovery with the smaller Summary |
| `ui/packages/design-system/src/design-system/Time.tsx` | EDIT | Format recorded wake-ups in their named schedule timezone |
| `ui/packages/design-system/src/design-system/time-utils.ts` | EDIT | Preserve shared formatting while supporting explicit timezones |
| `ui/packages/design-system/src/design-system/Time.test.tsx` | EDIT | Verify timezone labels agree with canonical instants |
| `ui/packages/app/lib/fleet-lifecycle.ts` | CREATE | Compose guarded archive and deletion with a confirmed-stop checkpoint |
| `ui/packages/app/lib/fleet-lifecycle.test.ts` | CREATE | Cover lost stop/delete responses, permission loss and deletion-only retries |
| `ui/packages/app/components/layout/FleetLifecycleDialog.tsx` | CREATE | Share direct Archive and Permanently Delete confirmation and durable recovery |
| `ui/packages/app/components/layout/FleetNoticeProvider.tsx` | CREATE | Keep approved polite lifecycle notices across navigation with paused dismissal |
| `ui/packages/app/components/layout/FleetNoticeProvider.test.tsx` | CREATE | Check dismissal timing, focus/hover pauses and account cleanup |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetActions.tsx` | CREATE | Offer Archive and independent permanent deletion beside Stop/Resume |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetConfig.test.tsx` | DELETE | Move deletion guarantees to shared lifecycle tests |
| `ui/packages/app/components/domain/FleetThreadViewport.tsx` | EDIT | Keep archived history readable without an active composer |
| `ui/packages/app/components/domain/FleetReplyFailure.tsx` | EDIT | Omit retry writes from archived history |
| `ui/packages/app/tests/helpers/dashboard-app-mocks.tsx` | EDIT | Exercise guarded lifecycle actions through the existing scoped action boundary |
| `ui/packages/app/components/layout/RecentChats.test.tsx` | EDIT | Preserve version reads, cancellation and another selected fleet during deletion |
| `ui/packages/app/tests/fleet-lifecycle-boundary.test.tsx` | CREATE | Verify the dialog through actual server actions, authentication and HTTP preconditions |
| `ui/packages/app/tests/e2e/acceptance/fixtures/lifecycle.ts` | EDIT | Use the approved Archive and Permanently Delete labels |
| `ui/packages/app/tests/e2e/acceptance/login-install-lifecycle.spec.ts` | EDIT | Exercise archive navigation and read-only history |
| `ui/packages/app/tests/e2e/acceptance/signup-lifecycle.spec.ts` | EDIT | Exercise archive navigation and read-only history |
| `ui/packages/app/tests/e2e/acceptance/operator-journey.spec.ts` | EDIT | Exercise archive navigation and read-only history |
| `ui/packages/app/tests/e2e/acceptance/workspace-fleet-lifecycle.spec.ts` | EDIT | Exercise archive navigation and read-only history |
| `ui/packages/app/tests/workspace-chat/save-handoff.test.tsx` | CREATE | Verify one confirmed save opens the same fleet without replaying authoring text |
| `ui/packages/app/tests/workspace-chat/saving.test.tsx` | EDIT | Assert automatic conversation navigation after a confirmed save |
| `ui/packages/app/tests/workspace-chat/fleet-editor.test.tsx` | EDIT | Keep revision saves on the confirmed fleet destination |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/ChatView.test.tsx` | EDIT | Verify live approvals flow into Summary without restoring the footer metrics strip |
| `ui/packages/app/tests/models-registry-pagination.test.tsx` | EDIT | Supply the current auth boundary while preserving paging race checks |
| `ui/packages/app/tests/models-registry-activation.test.tsx` | EDIT | Supply the current auth boundary while preserving activation checks |
| `ui/packages/app/tests/workspace-switcher-recovery.test.tsx` | EDIT | Supply identity through the auth facade while preserving one-time refresh checks |
| `ui/packages/app/tests/work-context-sections.test.tsx` | EDIT | Omit empty sources while retaining saved-policy provenance |
| `ui/packages/app/tests/fleet-tool-progress.test.tsx` | EDIT | Verify steady activity glyphs without losing disclosure identity |
| `ui/packages/app/tests/fleet-thread/role-senders.test.ts` | EDIT | Preserve named senders once for assistive readers in the new metadata position |
| `ui/packages/app/tests/fleet-thread/role-turns.test.ts` | EDIT | Verify compact timestamps outside message bodies |
| `ui/packages/app/tests/fleet-thread/role-links.test.ts` | EDIT | Preserve account-identifier redaction with visible sender labels |
| `ui/packages/app/tests/fleet-thread/role-rows.test.ts` | EDIT | Verify compact sender metadata and canonical timestamps |
| `ui/packages/app/tests/fleet-thread/malformed-metadata.test.ts` | EDIT | Keep malformed content fallback separate from timestamp metadata |
| `ui/packages/app/components/domain/FleetMessageRow.test.tsx` | EDIT | Check the composed message body and shared bubble surface |
| `ui/packages/app/tests/fleet-setup-recovery.test.tsx` | EDIT | Locate setup editing in the approved fleet menu |
| `ui/packages/app/tests/app-shell-team.test.tsx` | EDIT | Verify billing reaches account navigation without restoring the removed header |
| `ui/packages/app/tests/fleets-routes/detail-header.test.ts` | EDIT | Verify the simplified header and Summary approval link |
| `ui/packages/app/tests/fleets-routes/detail-lifecycle.test.ts` | EDIT | Verify archived history without killed-only inline delete controls |
| `ui/packages/app/tests/fleets-routes/detail-views.test.ts` | EDIT | Verify scoped chat data and menu ownership across failed server reads |
| `ui/packages/app/tests/fleets-routes/page-parts.ts` | CREATE | Inspect server-to-client chat data without pretending lazy menu contents are server-rendered |
| `ui/packages/app/tests/use-stored-secrets.test.ts` | EDIT | Verify workspace loading and stale results |
| `ui/packages/app/tests/helpers/models-registry-add-fixtures.tsx` | EDIT | Supply scoped identity and model action boundaries |
| `ui/packages/app/tests/helpers/models-registry-fixtures.tsx` | EDIT | Supply workspace secret metadata and model action results |
| `ui/packages/app/app/(dashboard)/settings/billing/page.tsx` | EDIT | Use the shared Manrope role instead of the local system font |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingSpending.tsx` | EDIT | Use shared Manrope typography |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingActivityDetails.tsx` | EDIT | Use shared Manrope typography |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingFilters.tsx` | EDIT | Use shared Manrope typography |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingActivities.tsx` | EDIT | Use shared Manrope typography |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingReportView.tsx` | EDIT | Use shared Manrope typography |
| `ui/packages/design-system/src/assets/logo-dark.svg` | EDIT | Use the shared Manrope family in the wordmark |
| `ui/packages/design-system/src/assets/logo-light.svg` | EDIT | Use the shared Manrope family in the wordmark |
| `ui/packages/website/src/pages/DesignSystemGallery.tsx` | EDIT | Label the shared Manrope display and reading roles accurately |
| `ui/packages/website/src/pages/Privacy.tsx` | EDIT | Remove stale font documentation |
| `ui/packages/website/tests/e2e/design-system-ownership.spec.ts` | EDIT | Verify Manrope ownership in the website |
| `ui/packages/website/tests/e2e/design-system-status-smoke.spec.ts` | EDIT | Verify Manrope in rendered shared controls |
| `ui/packages/website/tests/e2e/content-accessibility.spec.ts` | EDIT | Verify keyboard entry through the actual home-to-example route, responsive layout and zoom |
| `ui/packages/app/tests/e2e/fleet-output.spec.ts` | EDIT | Require the shipped Manrope font to load |
| `ui/packages/app/tests/e2e/acceptance/fleet-composer.spec.ts` | EDIT | Assert the approved shared font family |
| `ui/packages/app/tests/e2e/auth-theme.spec.ts` | EDIT | Verify authentication controls inherit Manrope |
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
| `ui/packages/design-system/src/design-system/PageTitle.tsx` | EDIT | Share approved type roles and Phosphor controls |
| `ui/packages/design-system/src/design-system/Button.tsx` | EDIT | Share approved type roles and Phosphor controls |
| `ui/packages/design-system/src/design-system/Textarea.tsx` | EDIT | Share approved type roles and Phosphor controls |
| `ui/packages/design-system/src/design-system/Select.tsx` | EDIT | Share approved type roles and Phosphor controls |
| `ui/packages/design-system/src/design-system/Accordion.tsx` | EDIT | Share approved type roles and Phosphor controls |
| `ui/packages/app/tests/work-context.test.tsx` | EDIT | Verify always-visible Summary and single reader ownership |
| `ui/packages/app/tests/native-task-pr-badge.test.tsx` | EDIT | Verify always-visible Summary and single reader ownership |
| `ui/packages/app/components/domain/FleetThreadViewport.test.tsx` | EDIT | Verify inline Summary without a drawer |
| `ui/packages/app/components/domain/FleetThread.flush.test.tsx` | EDIT | Verify inline Summary without a drawer |
| `ui/packages/app/tests/workspace-chat/setup-menu.test.tsx` | EDIT | Verify inline Summary without a drawer |
| `ui/packages/app/tests/e2e/acceptance/fleet-stream-transport.spec.ts` | EDIT | Verify inline Summary without a drawer |
| `ui/packages/app/tests/helpers/dashboard-mocks.tsx` | EDIT | Verify the approved shared icon and type roles |
| `ui/packages/app/tests/api-keys-components.test.ts` | EDIT | Verify the approved shared icon and type roles |
| `ui/packages/design-system/src/design-system/Button.test.tsx` | EDIT | Verify the approved shared icon and type roles |
| `ui/packages/design-system/src/design-system/PageTitle.test.tsx` | EDIT | Verify the approved shared icon and type roles |
| `ui/packages/design-system/src/interactions.css` | EDIT | Match approved working-status motion and compact glyph scale |
| `ui/packages/app/components/domain/FleetToolCallHeader.tsx` | EDIT | Match approved working-status motion and compact glyph scale |
| `ui/packages/app/components/domain/FleetToolCalls.tsx` | EDIT | Match approved working-status motion and compact glyph scale |
| `ui/packages/app/components/domain/chat/ChatMessageRow.tsx` | EDIT | Match approved working-status motion and compact glyph scale |
| `ui/packages/design-system/src/design-system/DropdownMenu.tsx` | EDIT | Match approved working-status motion and compact glyph scale |

| `schema/20261111230000_model_tenant_models_workspace.sql` | CREATE | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_db/src/migration.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_db/tests/integration_migrate/mod.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_db/tests/integration_migrate/model_workspace.rs` | CREATE | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/mod.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/managed.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/selection/mod.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/selection/tests.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/activate/mod.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/entries/mod.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/entries/page.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/entries/write.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/entries/sql/mod.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/sql/mod.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/sql/select_primary_workspace.sql` | DELETE | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/sql/select_workspace_exists.sql` | CREATE | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/sql/select_tenant_selection.sql` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/sql/lock_credential_for_reference.sql` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/sql/lock_credential_for_activation.sql` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/sql/insert_tenant_model_if_absent.sql` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/sql/activate_tenant_model.sql` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/entries/sql/insert_entry.sql` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/entries/sql/select_entry.sql` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/entries/sql/select_first_page.sql` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/entries/sql/select_page_after.sql` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/entries/sql/update_entry_model.sql` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/registered/sql/entry.sql` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/src/provider/registered/sql/lock_entry_credential.sql` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/tests/integration_rotation/main.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/tests/integration_rotation/activation.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/tests/integration_rotation/activation_ceiling.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/tests/integration_rotation/provider_resolution.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/tests/integration_rotation/registry.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/tests/integration_rotation/registry_page.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/tests/integration_rotation/registry_tenancy.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/tests/integration_rotation/selection_pointer.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/tests/integration_rotation/resolution_locks.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_credential/tests/integration_rotation/workspace_credentials.rs` | CREATE | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_vault/src/describe.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_vault/src/lib.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_vault/src/delete.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_vault/src/sql/mod.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_vault/src/sql/select_scoped_secret_descriptors.sql` | CREATE | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_vault/src/sql/lock_entries.sql` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_vault/tests/support/lane.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_api_wire/src/tenant_model_entry.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_api_wire/src/tenant_provider.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_api_wire/tests/tenant_provider_shapes.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/model_entry/write.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/model_entry/render.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/provider/mod.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/provider/write.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/provider/tests.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_api/tests/integration_tenant_provider.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_api/tests/integration_tenant_models/mod.rs` | CREATE | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_api/tests/harness/stubs_provider.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `ui/packages/app/lib/api/tenant_provider.ts` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `ui/packages/app/lib/api/tenant_provider.test.ts` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `ui/packages/app/lib/api/tenant_model_entries.ts` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `ui/packages/app/lib/api/tenant_model_entries.test.ts` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `ui/packages/app/lib/api/tenant-model-schemas.ts` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `ui/packages/app/lib/types.ts` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `cli/src/commands/tenant/tenant.ts` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `cli/src/program/tree/workspace.command.ts` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `cli/test/tenant-provider-read-write.unit.test.ts` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `public/openapi.json` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |

| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/EditModelEntryDialog.tsx` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `ui/packages/app/tests/models-registry-edit-remove.test.tsx` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `ui/packages/app/tests/helpers/models-registry-edit-fixtures.tsx` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `cli/test/helpers-tenant-effect.ts` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `ui/packages/app/tests/e2e/acceptance/models-pagination-race.spec.ts` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `ui/packages/app/tests/e2e/acceptance/fleet-model.spec.ts` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `ui/packages/app/tests/e2e/acceptance/provider-credential-reference.spec.ts` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `ui/packages/app/tests/e2e/acceptance/fixtures/preflight.ts` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `rustd/crates/daemon/afd_api/tests/authoring/seed.rs` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `rustd/crates/daemon/afd_authoring/tests/draft/seed.rs` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `rustd/crates/daemon/afd_api/tests/authoring/saving/revisions/model_changes.rs` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `rustd/crates/daemon/afd_api/tests/integration_connector_status/mod.rs` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `rustd/crates/daemon/afd_connector/tests/integration_connect_roundtrip/disconnect/mod.rs` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `rustd/crates/daemon/main/tests/integration_runner_stream/authoring/seed.rs` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |
| `rustd/crates/daemon/afd_fleet/tests/lease_renewal/seat.rs` | EDIT | Preserve the exact credential namespace through model clients and integration fixtures |

| `rustd/crates/daemon/afd_http/src/services/provider.rs` | EDIT | Pass an explicit typed workspace/name reference across the HTTP store seam |
| `rustd/crates/daemon/afd_http/src/services/model_entry.rs` | EDIT | Pass an explicit typed workspace/name reference across the HTTP store seam |

| `ui/packages/app/tests/models-source-reconciliation.test.ts` | CREATE | Verify explicit model credential namespaces through real client and HTTP boundaries |
| `rustd/crates/daemon/afd_api/tests/integration_tenant_models.rs` | EDIT | Verify explicit model credential namespaces through real client and HTTP boundaries |
| `rustd/crates/daemon/afd_api/tests/tenant_model_entry_input.rs` | EDIT | Verify explicit model credential namespaces through real client and HTTP boundaries |
| `rustd/crates/daemon/afd_api/tests/tenant_model_entry_route.rs` | EDIT | Verify explicit model credential namespaces through real client and HTTP boundaries |
| `rustd/crates/daemon/afd_api/tests/tenant_provider_route.rs` | EDIT | Verify explicit model credential namespaces through real client and HTTP boundaries |
| `rustd/crates/daemon/main/tests/integration_tenant_registry.rs` | EDIT | Verify explicit model credential namespaces through real client and HTTP boundaries |

| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/page.tsx` | EDIT | Reuse existing cached workspace names solely to label each saved model source |

| `ui/packages/app/tests/models-registry-dialogs.test.tsx` | EDIT | Distinguish stored, unavailable and explicitly keyless model credentials |
| `ui/packages/app/tests/model-details-dialog.test.tsx` | EDIT | Distinguish stored, unavailable and explicitly keyless model credentials |

| `ui/packages/app/tests/models-registry-table.test.tsx` | EDIT | Verify explicit source-workspace forwarding at model action boundaries |

| `ui/packages/app/tests/provider-actions.test.ts` | EDIT | Verify explicit source-workspace forwarding at model action boundaries |

| `ui/packages/app/components/domain/fleet-chat-elements.ts` | CREATE | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/components/domain/fleet-chat-adapters.ts` | CREATE | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/components/domain/FleetChatElements.tsx` | CREATE | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/components/domain/FleetActivityElements.tsx` | CREATE | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/components/domain/fleetMessageReaders.ts` | EDIT | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/components/domain/fleetMessageReaders.test.ts` | EDIT | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/components/domain/fleetFailureCopy.tsx` | EDIT | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/components/domain/fleet-reply-elements.ts` | CREATE | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/components/domain/FleetToolInputElements.tsx` | CREATE | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/components/domain/FleetToolCallBody.tsx` | EDIT | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/components/domain/FleetToolCallDetails.tsx` | EDIT | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/lib/events/event-grouping.ts` | EDIT | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/tests/fleet-chat-elements.test.tsx` | CREATE | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/tests/fleet-tool-rendering.test.tsx` | EDIT | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/tests/fleet-thread/role-reply-parts.test.ts` | EDIT | Normalize recorded chat manifestations into typed models and dispatch through static renderer registries |
| `ui/packages/app/tests/fleet-reply-elements.test.tsx` | CREATE | Exercise safe default reply content, code and specialized parts |
| `ui/packages/app/tests/fleet-tool-elements.test.tsx` | CREATE | Verify typed tool input previews and recorded outcomes through the transcript |
| `ui/packages/app/components/domain/FleetReplyBody.test.tsx` | EDIT | Keep reply-context imports at their canonical typed data module |
| `ui/packages/app/tests/e2e/chat-elements.spec.ts` | CREATE | Review typed code, tool previews, links and fallback content in both themes and viewport sizes |
| `ui/packages/app/components.json` | EDIT | Resolve official assistant-ui registry components for the existing Radix style and Phosphor icons |
| `ui/packages/app/components/domain/FleetThread.tsx` | EDIT | Install native render-only assistant-ui tool registration |
| `ui/packages/app/components/domain/FleetToolRegistry.tsx` | CREATE | Register existing backend tools with assistant-ui Tools and preserve generic rendering |
| `ui/packages/app/components/domain/FleetReplyActions.tsx` | EDIT | Use ActionBarPrimitive for message-copy availability and state |
| `ui/packages/app/components/domain/FleetMarkdown.tsx` | EDIT | Use the official Markdown code-header extension with shared appearance |
| `ui/packages/app/components/assistant-ui/CodeHeader.tsx` | CREATE | Adapt the official Markdown code-header composition to shared buttons and icons |
| `ui/packages/app/components/assistant-ui/TerminalBlock.tsx` | CREATE | Adopt the official terminal component with recorded outcome semantics |
| `ui/packages/app/components/assistant-ui/terminal-ansi.ts` | CREATE | Keep the upstream terminal text parser separate from presentation |
| `ui/packages/app/components/assistant-ui/CodeDiff.tsx` | CREATE | Adopt the official diff component with shared tokens and accessible side labels |
| `ui/packages/app/components/assistant-ui/surfaces.ts` | CREATE | Map reused official element surfaces to approved app tokens |
| `ui/packages/app/tests/fleet-tool-registry.test.tsx` | CREATE | Prove native tool registration and generic fallback without browser execution |

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
- `ui/packages/design-system/src/tokens.css` owns shared default values; `app-tokens.css` overrides dashboard values without changing website typography or radii. Only `theme.css` maps values to Tailwind utilities. The dashboard profile adds no JavaScript runtime or second utility registry.
- The approved local preview at `/workspace/artifacts/fleet-refresh-v2/proposal.html` supplies visuals, not backend behavior.

## Sections (implementation slices)

### §1 — Shared type, icons, colors and borders

Use Manrope 13px/20px regular for desktop reading, 12px/16px metadata, and 16px/26px phone reading. Controls use 13px/20px medium; phone typing uses 16px. Keep code monospace.
Use Phosphor 20px rail, 16px action and 18px composer glyphs; preserve 48px desktop rail and 44px phone targets. Use 10px, 16px and 22px rounded surfaces and zero tracking.
Use Manrope across website and app, including display headings; retain Commit Mono for technical content. Import Manrope once through shared `tokens.css` and remove Instrument Sans and Bricolage packages. Keep website display sizes separate from application reading sizes. Retain Radix unless a concrete primitive defect justifies a separate change.

- **Dimension 1.1** — Browser-computed fonts and sizes match approved roles → Test `approved type and icon scale` (browser): desktop/phone and light/dark render expected sizes.
- **Dimension 1.2** — Labels and focus targets remain reachable at 200 percent zoom → Test `compact controls remain reachable` (browser): long names wrap and no controls overflow.

### §2 — Navigation and resource pages

Default to the compact rail with a square-and-pencil New fleet action. Keep fleets reachable without an expanded permanent sidebar.
Place Plugins, Secrets and Connected apps in navigation; Models, Team, Usage and Security remain discoverable from the account menu.
Team remains account-wide. Preserve Owner/Member roles, pending invitations and delivery recovery. Usage remains settled ledger data, with unknown values distinct from zero.
Account Models keeps one form for new or saved provider credentials and optional Make active. Reuse a saved credential without replacing its bytes; preserve partial-success retries.
Platform defaults stay under the existing `agentsfleet platform keys` commands. No Admin page is created. The served credit balance remains available beside Usage in the account menu after removal of the global header; opening the menu must not fetch it again.

- **Dimension 2.1** — New fleet is reachable from empty and active views → Test `new fleet remains discoverable` (browser): writing-pad action opens the setup route.
- **Dimension 2.2** — Resource pages retain actual scope → Test `account and workspace navigation preserve scope` (unit/browser): account Team and workspace secrets use their existing routes.
- **Dimension 2.3** — Saved-key reuse and partial creation preserve resources → Test `model creation retries the failed step` (unit): registration failure retains the key reference; activation failure retains the registered model.

#### Models implementation decisions

Use the existing API ownership and the explicit workspace ID already supplied to the form. Do not introduce a primary-vault product concept, enumerate workspaces to choose storage, copy keys between workspaces or add a vault-selection abstraction. The API remains authoritative for account model registration and workspace secret access.

Create model offers Provider, Model, Add new key / Use saved key, optional Make active, and one Create model action. Saved choices come from the selected workspace's existing secret API and are filtered by provider and classified kind. A saved custom endpoint retains its stored URL. Add new key uses POST with a free name. A held name offers saved reuse or another name; creation never rotates or replaces a secret. Rotation remains explicit in the existing editor/Secrets flow.

Use React 19 Actions (`useActionState`) for submission state and pending feedback. Keep the existing Effect-backed API transport for timeouts and retries. A synchronous guard excludes duplicate dispatches before pending paints. Keep an immutable submitted snapshot of workspace, credential reference, model and activation choice. Store no credential bytes in progress state or persistent caches. Clear the form's key after storage settles, and unmount scoped form state on workspace or identity loss.

Retain completed steps. Registration failure retries registration using the confirmed key. Activation failure keeps the registered model visible and retries activation only. Lost registration or activation responses offer Check status through the existing API. Registration reconciliation follows pages with repeated-cursor protection and matches the exact model/reference pair. Failed or incomplete reads stay unconfirmed. Unknown storage offers an explicit credential choice; matching metadata never proves that submitted secret bytes were saved.

Negative proofs belong to the model recovery and existing dialog/action-boundary tests: saved reuse performs no secret write; held names never rotate implicitly; optional keys on compatible endpoints remain supported; confirmed steps are not repeated; lost acknowledgements are reconciled before retry; duplicate clicks and late results after unmount do not create another operation.

The backend ownership defect was confirmed and repaired at the API/storage boundary: `afd_api_tenant/src/handler/tenant/model_entry/write.rs` delegates registration with only tenant, model and secret name, and `afd_credential/src/provider/sql/lock_credential_for_reference.sql` resolves that name in the first account workspace. The form correctly writes to its explicit workspace. Repair this at the API/storage boundary as follows:

- A configured model stores its credential's `workspace_id` beside `secret_ref`. Models and the active choice remain account-scoped. Switching the selected workspace does not relocate a configured model or its key.
- Registration takes `workspace_id`, `model_id` and `secret_ref`. Self-managed activation takes `workspace_id`, `secret_ref` and the existing optional model. Platform reset needs no workspace. The server proves workspace/account ownership in the same statement that locks the credential. Another account's workspace is indistinguishable from a missing credential.
- Registry and active-selection responses carry the stored workspace. Reconciliation matches workspace, model and secret name together. Activation of a listed row uses that row's source workspace, never the currently selected page's workspace.
- Add a forward migration. Backfill existing entries once to the workspace the old implementation actually used, preserving entry IDs, selections, setup references and credential bytes. Rows with no recoverable workspace remain visible but unavailable; they never resolve by guessing a different workspace. A composite workspace/account foreign key prevents cross-account storage. Workspace removal clears only the source reference so existing model/setup history remains readable.
- Replace the old unique key with `(tenant_id, workspace_id, model_name, secret_ref)`. Two workspaces may register the same model and same key name independently. Registry metadata batches explicit workspace/name pairs in one read, without per-row queries or decryption.
- Runtime resolution, activation, registration and secret-deletion guards use the exact stored workspace/name pair. Preserve the credential-first lock order. Deleting an unused same-named key in another workspace must remain possible; deleting the referenced key must remain refused.
- The coordinated model fix updates the dashboard, CLI, generated OpenAPI and companion documentation together. `workspace_id` becomes required for model registration and self-managed activation; older callers must supply the workspace that holds the key. This requirement is part of Indy's instruction to fix the incorrect storage boundary, and must be explicit in release notes.

Do not add a frontend storage resolver, workspace enumeration, key copying or implicit rotation. Add live backend cases for two workspaces with same-named keys, activation and resolution after workspace switching, cross-account refusal, exact deletion guards, migration preservation and compatible keyless endpoints. Unit form fixtures alone cannot prove this repair.

### §3 — Inline Summary and continuous conversation

Summary is always inline, 248px on desktop with a compact phone presentation. Show relevant approvals, changes, outputs, sources and next wake-up; omit Environment and empty groups.
Approval counts come from the existing fleet stream and link to the fleet-filtered approval inbox; decisions retain that inbox's owner and authorization. Wake-ups use schedules already carried by the fleet detail. Neither surface creates another stream or poller. A transient task refresh failure retains confirmed Summary data. Access loss clears all Summary metadata. Scheduler registration failures stay explicit; a scheduled instant that has passed is not presented as a future run.
Keep recorded command, edit, integration and schedule outcomes in their existing message stream. Distinguish running, succeeded, failed, interrupted and unconfirmed states.
After confirmed fleet creation, replace the setup URL with the fleet URL without an extra Open fleet click. Do not replay authoring text as a runtime request.
Keep setup and runtime drafts separate. Creator-private authoring history never becomes shared conversation history.
Handoff reads `savedSetupRevision` and the returned fleet identity from the existing DraftClient. A preview or pending save is not sufficient. Once its owner finishes the save request, a confirmed new fleet or newer applied revision replaces the setup URL with that fleet’s conversation URL exactly once. A fleet page refreshes its current route instead. A later transient history or activation failure does not discard the created fleet; its existing recovery surface keeps the same identity. Authorization loss still blocks the handoff. No authoring message is submitted to runtime, and the runtime composer starts from its own buffer.

- **Dimension 3.1** — Summary needs no toggle or drawer → Test `summary is inline at every viewport` (browser): relevant content is visible in the page flow.
- **Dimension 3.2** — Saved identity drives navigation → Test `confirmed save opens the same fleet` (unit): partial activation failure keeps the created fleet and retry identity.
- **Dimension 3.3** — State labels remain evidence-based → Test `failed and unconfirmed work stays explicit` (unit/browser): incomplete work never receives a succeeded label.

Working-status text starts muted and a darker tone moves left to right across the glyphs. Use one paint-only sweep, no bounce, pulse in font size, or bright flash. Only active recorded status labels animate. Completed, failed, interrupted and unconfirmed labels are static. Reduced-motion and forced-color modes keep a readable solid label. Check both themes.
The working icon remains steady while the text sweeps. Use a short still interval between sweeps and a smooth transit, keeping the text bounds unchanged. The visual review includes desktop/phone screenshots and a short motion capture before further resource-page work.

- **Dimension 3.4** — Working text has a muted-to-darker sweep without layout shifts → Existing `chat-typography.spec.ts` (e2e): computed gradient and motion samples, stable bounds, no animation after completion, reduced-motion and forced-colors fallbacks.

Failed chat work stays beside its original item with a quiet underlined Try again control and optional details. A compact dismissible error notice is supplemental; it never removes the durable inline failure. Retain draft text and operation identity, gate duplicate retries, and use a sign-in/access/configuration action when repeating the operation cannot resolve the error. Existing `tests/chat-failure.test.tsx` verifies refusal, in-flight exclusion and disabled recovery; browser review checks the compact styling.

#### Typed chat element implementation

The row-layout split is complete. The explicit typed outcome layer remains unfinished until these paths are wired and verified. Do not describe the row split alone as completing the Appsmith-inspired approach.

Use assistant-ui's native part pipeline and render-only backend tool registration. Keep a thin typed adapter for recorded fleet events; remove the overlapping reply registry. Reuse official Elements for terminal output, code diffs and code headers, restyled through the shared design tokens. GroupedParts owns adjacent reasoning/tool grouping, ActionBar owns message-copy state, and the existing ExternalStoreRuntime remains the runtime owner.

Owner approval (Nov 11, 2026): "Can you build that then" / "as opposed to just hand rolling the gala" — implement the reviewed assistant-ui reuse proposal. Do not introduce client-side tool execution. The current API inventory has no general chat attachment upload endpoint; runner checkpoint uploads are unrelated. Attachment sending requires an actual supported transport and persistent representation. Structured form submission must use the current scoped backend behavior rather than invent an in-memory success.

Recorded elements cover operator turns, replies, integration activity, scheduled wake-ups, approval events, activity groups and a generic activity default. Keep the existing event order, stable IDs, private actor naming, source links and durable outcomes. Group metadata must be validated rather than cast to an event array. Invalid groups retain the parent activity text and outcome with a quiet details-unavailable note; never expose the malformed metadata.

Reply-part registration keeps assistant-ui's reasoning/tool grouping, clocks, streaming markdown, safe unknown-part fallback and existing request owners. Tool-input registration keeps separate command, diff, plan, clipped and generic presentations. Unknown tools stay readable and neutral. Recorded success, failure, interruption and unavailable output remain distinct; parsing input must never prove a side effect succeeded.

A renderer presents data and existing scoped actions. It cannot run model-provided code, invent an approval ID, fetch credentials, or send a notification. Schedule details retain the current fleet scope; approval navigation uses the current fleet-filtered inbox and leaves decisions with its existing permission checks. A part without a specialized view uses validated text or a safe resource label/link. If no readable fields exist, show a quiet details-unavailable note. Never serialize arbitrary part objects or execute embedded markup. This does not add attachment sending or a new tool capability.

Owner review (Nov 11, 2026): "would there be a default?" and "how about command that run, edit of files, reading files, links" / "What about code" — implement readable defaults and demonstrate command, file, diff, code and link behavior through the actual assistant-ui transcript. Activity actions use the existing shadcn-style Button link variant and shared class merging. Appsmith's WidgetFactory and FormControlFactory supply the registration reference; its AI panel is not a ready-made typed outcome runtime.

Required proofs: `mixed recorded chat elements use their registered renderers`, `malformed grouped metadata is safe and visible`, `unknown parts never execute markup`, and the existing `fleet-tool-rendering` regression showing text chunks do not recompute unchanged tool activity. Re-run real transcript browser checks before claiming the layer complete.

### §4 — Archive, deletion and feedback

Archive permanently stops the fleet, retains history, and offers no Restore. Permanently Delete is independent and available directly on active or archived fleets.
Compose the current guarded terminal stop and deletion internally, preserving authorization, ETags and uncertain-response reconciliation.
If stop succeeds and deletion fails, retain the fleet and offer Retry deletion only. Never claim deletion based on a failed read.
On confirmed success, remove the active-list row and move an affected open fleet to New fleet. Keep another selected fleet in place.
Use the approved compact notice: Fleet archived with View history, or Fleet permanently deleted. No Undo; history remains reachable after dismissal.

Lifecycle recovery keeps a typed checkpoint: read current state, confirm terminal stop, delete, or reconcile an uncertain write. A confirmed stop is retained when deletion fails, and Retry deletion never repeats the stop. An uncertain stop first reads state; a nonterminal result requires an explicit retry. An uncertain DELETE succeeds only after a successful authenticated read confirms absence; 401, 403, timeout, malformed responses and server errors never count as deletion. The fleet GET handler extracts `WorkspaceContext` and `PersonIdentity` before reading the resource (`afd_api_tenant/src/handler/fleet/native/read.rs`); an unauthorized read cannot become a successful missing-resource result. Keep every mutation's observed ETag. Canceling the mounted owner discards late results and prevents the next write. Account changes clear notice state. Archived conversations expose history without a composer or Restore.

- **Dimension 4.1** — Direct deletion uses one user confirmation → Test `active fleet deletion handles terminal stop internally` (unit/integration): stop then delete completes without another dialog.
- **Dimension 4.2** — Partial deletion remains recoverable → Test `deletion failure retains archived fleet` (unit): stop success/delete failure shows durable recovery and retries only delete.
- **Dimension 4.3** — Notices announce confirmed outcomes → Test `lifecycle notice survives navigation` (browser): polite announcement, no focus theft, pause dismissal on focus/hover, read-only history.

### §5 — Verification and visual finish

Current implementation checkpoint, Nov 11, 2026: `make lint-app lint-design-system test-unit-app test-unit-design-system` passes. App: 445 files / 4,234 tests. Design system: 62 files / 491 tests. The desktop and phone `chat-alignment.spec.ts` and `chat-typography.spec.ts` run passes all 22 browser cases, covering approved Manrope sizes, actual shell alignment, inline Summary, activity outcomes, scrolling and motion fallbacks. The final authenticated journeys, complete resource-page visual review and remaining repository-wide lanes remain due. These local results do not mark the implementation ready.

Run targeted tests for changed behavior, then repository work/verify checks. Run authenticated browser flows against real data before claiming production readiness.
Review spacing, borders, fonts, icon scale, long names, empty states and failures at the approved widths, both themes and 200 percent zoom.
The prototype's passing checks do not substitute for implementation checks.

- **Dimension 5.1** — Converted surfaces match the approved visual scale → Test `fleet refresh visual review` (browser/manual): screenshots and computed styles record final evidence.
- **Dimension 5.2** — Existing request ownership survives → Test `fleet and setup recovery journeys` (browser): reload, cancel, uncertain sends, scope switch and retries preserve the original operation.

## Interfaces

Keep existing fleet, draft, message, event, secret, membership and billing interfaces. The model-reference repair changes these fields without adding routes:

| Surface | Change |
|---|---|
| `POST /v1/tenants/me/models` | Required `workspace_id` identifies the credential namespace alongside `model_id` and `secret_ref` |
| `PUT /v1/tenants/me/provider` | Self-managed mode requires `workspace_id`; platform mode remains unchanged |
| Model registry, stored-entry and provider responses | Add stable `workspace_id` metadata, omitted when the historical source is unavailable or platform-managed |
| `tenant provider create` | Add `--workspace`; otherwise use the already selected workspace |

All callers and companion documentation must move together. Existing credential bytes, model IDs, scope checks and setup/model decision records remain unchanged.
The coordinated public Plugins and archived-status vocabulary changes have their own implementation specification.
Custom profile and browser-session operations have a separate auth specification; UI components consume app-owned shapes.

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
- Models follows existing API scopes and explicit workspace identity. Do not introduce client-side vault selection. Keep explicit secret-write uncertainty, saved-key reuse, exact registration reconciliation and activation-only retry.
- Save handoff uses confirmed fleet identity independently of readiness failure. Setup history rehydrates through creator-filtered context; keep setup/runtime buffers separate and clear them on identity/access loss. Never copy a setup prompt into runtime messages.
- Deletion retains confirmed-stop state, reads current authorized state after uncertain writes, and handles missing responses without claiming success. A denied or unavailable read cannot prove deletion. Preserve billed attribution and settled runner retention.
- The separate vocabulary spec must map public Plugins names to immutable historical representations without rewriting frozen revisions, and translate only existing library write authority without widening narrowed identities. The separate auth spec must preserve current subject-based actor/cache identity during adapter changes.

## Approved follow-up: scope-aware CLI discovery

Indy requested permission-aware masking of platform commands on November 11, 2026. This is approved CLI work in the coordinated command/API specification, with no platform UI reintroduced. Keep the backend permission checks unchanged.

The installed Effect CLI supplies `Command.unlisted` in `effect/src/cli/Command.ts`. It hides a command from parent help, generated shell completions and unknown-command suggestions while preserving exact-name parsing. Use that public combinator, with `Command.withSubcommands` and command factories, rather than filtering rendered help text or maintaining a second parser. Build a fresh tree per invocation; never mutate a module-global command's visibility.

Resolve effective scopes through the existing `GET /v1/users/me` identity read (`cli/src/lib/me-ping.ts`, already used by `whoami`). For discovery, decode `scopes` strictly as an array of strings; a missing/non-array field or any malformed member makes permissions unknown and hides Platform. The current general identity decoder filters bad members and must not be reused unchanged for visibility. Unknown but well-formed scope strings confer no recognized capability. CLI credentials are opaque; do not decode them as Clerk JWTs or call Clerk from the CLI. Reuse environment-key precedence, selected `--api` endpoint and stored-credential target protections. One read per help/discovery invocation, bounded to two seconds with no retry loop; do not log or persist credentials. Signed-out help, login, logout and version remain available without this read. An unavailable or expired identity read hides platform discovery and reports that permissions could not be loaded; it does not claim that the user has no permissions. Do not reuse an elevated result across identities, API endpoints or invocations.

| Effective permission | Advertised platform operations |
|---|---|
| None / unknown | No Platform entry in root help, suggestions or newly generated completion scripts |
| `platform-key:read` | `platform keys list` |
| `platform-key:admin` | `platform keys list`, `set`, `delete` |
| `model:read` | `platform models list`, `show` |
| `model:admin` | Model read commands plus `create`, `update`, `delete` |
| `runner:read` | Runner `list`, `show`, `events`, `leases` |
| `runner:enroll` | `platform runners enroll`, plus only read rights included in the server's effective scope closure |
| `runner:write` | Runner mutation commands and the read rights included in the existing scope closure |

A parent group is advertised only when at least one retained child is advertised. Independent scope families never imply one another. Setting a platform default from an existing key requires `platform-key:admin`; `--api-key-stdin` additionally requires existing secret-write authority and workspace ownership. Static visibility cannot prove ownership of a workspace supplied later. Keep that additional requirement in flag help and enforce it through the existing API.

Explicitly typing an unlisted command still parses and reaches the existing authorization path; a refusal names the required permission without suggesting that the user create a different account. `Command.unlisted` does not hide an exact command's own `--help`; that help can explain the requirement. This is discovery filtering, not a new authorization mechanism.

Explicitly enable Effect’s `GlobalFlag.Completions` alongside the existing Help/Version/LogLevel built-ins in `cli/src/cli.ts`, and document `agentsfleet --completions <shell>` for Bash, Zsh, Fish and PowerShell. Current source does not enable this flag.

Effect's generated completion scripts are static snapshots. Generate them from the same scoped tree and document regeneration after an identity or permission change. Do not claim an already-installed script changes immediately when access is revoked. Live dynamic completion is not required for this change; every actual operation remains checked on the server.

Implementation owners: `cli/src/cli.ts`, `cli/src/program/tree/` composition and platform factories, the existing identity reader, and help/completion tests under `cli/test/`. Preserve the existing 80-column formatter, JSON stdout discipline, credential-redaction behavior and exact-name command execution. Remove the platform Fleet-library group as separately approved; do not preserve it merely because a scope exists.

Required proofs before completion: ordinary identity hides Platform; read-only identity sees list/show but no writes; each independent admin family reveals only its own commands; parent hiding follows children; root and nested help, suggestions, Bash/Zsh/Fish/PowerShell generation agree; signed-out/offline/malformed/expired reads remain usable and unprivileged; environment API key overrides stored login; target/account switches cannot reuse scopes; direct hidden command still receives the backend's refusal; a refreshed backend authority rejects a revoked command even when an older completion file suggests it. The existing backend capability cache refreshes after one minute and permits up to fifteen minutes of stale authority during provider outage; preserve and document that policy instead of claiming immediate revocation. Add no CLI permission cache that extends those semantics. No new backend endpoint or Clerk coupling is needed.

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

## EGM finding disposition

The fresh review reported eight findings. Recording a requirement is not resolution. This table separates changes already made from work still requiring source-grounded design and tests.

| Finding | Status | Resolution / remaining proof |
|---|---|---|
| 1: Summary owner | Partly resolved | Actual layout, setup caller, header and tests now listed; drawer/toggle removed and one responsive reader wired. Still pin approval/schedule readers and finish resize/scope browser proofs. |
| 2: Verification | Partly resolved | Repository lanes and affected focused tests listed. Remaining sections need exact negative-test owners and completed lane evidence. |
| 3: Model credential scope/recovery | Implemented; authenticated browser proof due | Explicit workspace references now govern registration, activation, resolution and deletion guards. The three new live credential cases and forward-migration preservation case pass within `make test-integration-rustd` (1,706 regular + 2 exclusive tests). Focused app tests: 154; CLI tests: 14. No frontend storage resolver or key copying exists. |
| 4: Delete reconciliation | Implemented; authenticated browser proof due | Typed stop/delete checkpoints, uncertain-response reads, access-loss handling and paused notices pass 34 focused tests. |
| 5: Setup handoff | Implemented; authenticated browser proof due | The 204 setup tests pass, including save-key reuse, one confirmed handoff, transient failure and authorization loss. Existing authoring history stays private. |
| 6: Stored vocabulary/authority | Open in coordinated CLI/API spec | Preserve immutable review records, existing platform pins and narrowed capabilities; no migration edits until specified. |
| 7: Identity mapping | Open in separate auth spec | Preserve current subject-based chat identity; define neutral profile/session adapter shapes and ownership. |
| 8: Reference/type consistency | Partly resolved | Active spec uses current checkout and phone typing 16px/24px. Older artifact source links still need reconciliation. |

Typography and icon work was explicitly cleared by the reviewer. The open findings above do not authorize skipping the corresponding design or verification.

## Discovery (consult log)

- **Appsmith reference (Nov 11, 2026)** — At Indy's request, cloned `https://github.com/appsmithorg/appsmith` to `/workspace/appsmith-reference` and inspected `app/client/src` at `65921d7294476c2f7af25c3c7d31c43da2eee215`. Useful sources: `WidgetProvider/factory/index.tsx`, `utils/formControl/FormControlFactory.tsx`, `pages/Editor/CustomWidgetBuilder/Editor/AIAssistant/index.tsx`, `sagas/AIAssistantRequestContext.ts` and `ce/reducers/aiAssistantReducer.ts`. Apply typed component registration, separated presentation/state, scoped context and explicit outcome handling through the existing assistant-ui runtime. The public AI-chat integrations control is a placeholder; do not claim it supplies an embeddable chat implementation. Do not copy their custom widget editor, Redux runtime or markdown-driven code application.
- **Shared font correction (Nov 11, 2026)** — Indy extended Manrope to the website. Shared `tokens.css` now imports Manrope once for both interface and display; Commit Mono remains technical. Instrument Sans and Bricolage packages and Usage's system-font overrides were removed. Dashboard token overrides contain no font-family declarations or Tailwind mappings.
- **Focused visual evidence (Nov 11, 2026)** — Eight rendered app/website token-profile combinations passed (dark/light, desktop/phone). The chat typography/alignment browser selection passed 22 checks. Website selected checks passed after correcting the keyboard journey to enter from the home page rather than assuming a direct fragment starts tab order at the document top: the first run passed 25 of 29; the affected keyboard, font ownership and zoom selection then passed all 10. These are focused checks, not the repository `dry` or final acceptance lanes. Logs and desktop/phone screenshots are under `/workspace/artifacts/fleet-refresh-v2` and `/workspace/scratch/refresh-*`.
- **Code-review follow-up (Nov 11, 2026)** — Shared class-merging fixes and radius preservation passed `make test-unit-design-system` (62 files, 489 tests). Collapsed rail tooltips now expose labels on keyboard focus; the focused app navigation/workspace selection passed 39 tests. Final rendered rail review remains part of §5.
- **Consults** — Fresh `origin/main` at `bfcfe5bac`; the merged platform CLI change removed operator pages. `orly doctor` passed both installation and rule checks.
- **Model reviews** — Native EGM implementation-readiness review completed against `bfcfe5bac`: 1 blocker for the separate vocabulary migration, 6 majors, 1 nit. No blocker to shared visual work. Report: `/workspace/artifacts/fleet-refresh-v2/implementation-readiness-current-main.md`. Missing Summary owners and focused test paths are now included. The remaining model, handoff, deletion, vocabulary and identity findings must be resolved in their corresponding sections/specifications before those edits.
- **Open blockers** — None in approved product direction; implementation details and exact callers are checked before each slice.
- **Deferrals and transfers** — Only user-excluded work below. Auth and public vocabulary are separately specified approved work, not dropped scope.

> Owner (Indy, Nov 11, 2026): “there is no primary vault” and “and get to work” — remove the added frontend storage-selection abstraction and follow the existing API ownership.
> Owner (Indy, Nov 11, 2026): “Yes i approve the changes you gave me.”
> Owner (Indy, Nov 11, 2026): “i wanna use the same font across website too.”
> Owner (Indy, Nov 11, 2026): “i set the admin platform default model via cli today, that is fine to me. I donot need a ui for it.”
> Owner (Indy, Nov 11, 2026): “Yes the access scope must be retained.”
> Owner (Indy, Nov 11, 2026): “Permanent stop, retain history, no Restore”.
> Owner (Indy, Nov 11, 2026): “i think you need to allow me to either Archive or Permanently Delete, not on a step level like Archive -> then Permanently Delete”.
> Owner (Indy, Nov 11, 2026): “Yes flow 1”.
> Owner (Indy, Nov 11, 2026): “I dont want to build this, but note it down with the elephant context details somewhere in our usual spot” — Flow 2 deferred to M239's revisit note.

> Owner correction (Nov 11, 2026): “But this is message refactor, you said type chat messages or manifestation like appsmith, where is that?” — complete the typed element adapters and registration; the row layout alone does not satisfy this request.

- **Model checkpoint (Nov 11, 2026)** — `make test-integration-rustd` passes 1,706 regular and 2 exclusive cases. The new same-name/two-workspace resolution, exact deletion guard, cross-account refusal and migration-preservation cases pass. `make lint-app lint-cli` passes; the focused Models app selection passes 154 tests and the provider CLI selection passes 14. Generated OpenAPI includes the explicit workspace fields. Full unit/lint, authenticated UI and final visual checks remain due; this is a review checkpoint.
- **Cloud test setup** — The declared integration lane needs `/workspace/scratch/refresh-rust-env.sh` for user-owned CMake/libclang and the compiler header path. `/workspace/scratch/refresh-compose-trust.yml` supplies the existing certificate bundle to QStash and keeps BusyBox's NATS loopback probe out of `http_proxy`. The gate, health commands, TLS verification and test selection were unchanged. Companion docs branch: `/workspace/docs`, `chore/m249-fleet-refresh-changelog`; no docs edits or publication yet.
