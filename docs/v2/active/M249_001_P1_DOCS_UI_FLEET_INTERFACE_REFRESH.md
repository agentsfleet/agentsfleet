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
| `ui/packages/app/components/domain/chat/AuthoringPrompt.tsx` | CREATE | Keep the native composer hook inside the existing lazy conversation boundary |
| `ui/packages/app/components/domain/chat/AuthoringIntent.tsx` | CREATE | Keep shared navigation intent React-only |
| `ui/packages/app/tests/authoring-intent.test.tsx` | CREATE | Preserve an existing conversation and unsent text while prefilling an explicit starter once |
| `docs/v2/pending/M249_002_P1_API_CLI_DOCS_UI_FLEET_COMMANDS_AND_SCOPED_DISCOVERY.md` | CREATE | Combined approved CLI/API and scope-aware discovery follow-up; starts independently and merges after this UI PR |
| `docs/v2/evidence/M249_VERIFICATION.md` | CREATE | Detailed repair checklist, visual method, measured results and follow-up boundaries |
| `docs/v2/pending/M239_001_P1_API_DOCS_INFRA_UI_FLEET_TO_FLEET_STEER_AND_ACTIVITY.md` | EDIT | Retain the owner-requested deferred Flow 2 customer use case, invocation, ownership and design questions without adding implementation |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/secrets-load.ts` | EDIT | Correct stale upsert guidance to match create-if-absent storage and explicit rotation |
| `/workspace/docs/docs.json` (companion docs branch) | EDIT | Replace retired runner HTTP entries with the admin and stream operations already shipped on main |
| `/workspace/docs/changelog.mdx` (companion docs branch) | EDIT | Describe the visible interface and credential-workspace changes with upgrade requirements |
| `/workspace/docs/fleets/model-providers.mdx` (companion docs branch) | EDIT | Explain inline model creation and explicit credential workspace selection |
| `ui/packages/app/components/domain/useFleetThreadEntries.ts` | EDIT | Preserve event grouping boundaries around temporary upload receipts |
| `ui/packages/app/tests/models-secrets-page.test.ts` | EDIT | Verify workspace display-label reads cannot change model credential ownership or hide registry errors |
| `ui/packages/app/lib/auth/account-config.ts` | CREATE | Read optional provider configuration once without exposing the server key |
| `ui/packages/app/components/assistant-ui/terminal-ansi.test.ts` | CREATE | Check escape-sequence removal, truncation and safe text presentation |
| `ui/packages/app/components/assistant-ui/range.test.ts` | CREATE | Check invalid numeric output bounds without exposing extra lines |
| `ui/packages/app/tests/account-session-recovery.test.tsx` | CREATE | Verify paging, duplicate exclusion, rejected transport promises and stale sign-out responses |
| `ui/packages/app/tests/e2e/fixtures/resource-settings-probe.tsx` | CREATE | Render actual Models and Team components in the visual review shell |
| `ui/packages/app/tests/e2e/fixtures/resource-settings-actions.ts` | CREATE | Isolate sample reads and refuse every write in resource visuals |
| `ui/packages/app/tests/e2e/resource-settings.spec.ts` | CREATE | Check model-key choices, invitations and resource layouts in both appearances |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderModelSelect.tsx` | EDIT | Collapse optional model evidence in Create model so phone actions remain visible |
| `ui/packages/app/tests/provider-model-select.test.tsx` | EDIT | Retain model evidence behind the optional compact disclosure |
| `ui/packages/app/tests/fleet-thread/failure-retry.test.ts` | EDIT | Ensure archived failed history cannot replay a request |
| `ui/packages/app/tests/model-creation-operations.test.tsx` | CREATE | Verify cancelled and uncertain model writes through the public creation operations and React Action owner |
| `ui/packages/app/tests/fleet-native-attachments.test.tsx` | CREATE | Exercise native file selection, preparation, removal, failed upload and receipt rendering through the actual composer |
| `ui/packages/design-system/src/design-system/Input.test.tsx` | EDIT | Check semantic control shape |
| `ui/packages/design-system/src/design-system/Checkbox.test.tsx` | CREATE | Preserve accessible, disabled and caller-owned selection through the native primitive |
| `ui/packages/design-system/src/design-system/RadioGroup.test.tsx` | CREATE | Preserve exclusive selection, disabled choices and controlled ownership |
| `ui/packages/design-system/src/design-system/SectionHeader.test.tsx` | EDIT | Check approved alignment and optional description semantics |
| `ui/packages/design-system/src/design-system/Badge.test.tsx` | EDIT | Check approved unfilled label chips |
| `ui/packages/design-system/src/design-system/Card.test.tsx` | EDIT | Check approved panel radius while retaining composition behavior |
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
| `ui/packages/website/src/pages/DesignSystemGallery.tsx` | EDIT | Keep shared Manrope roles and modal footer/cancel styling consistent |
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

| `schema/20261108121500_model_tenant_models.sql` | EDIT | Define explicit model credential ownership directly in the fresh development schema; no backfill or upgrade path |
| `rustd/crates/daemon/afd_db/tests/integration_migrate/mod.rs` | EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
| `rustd/crates/daemon/afd_db/tests/integration_migrate/model_workspace.rs` | CREATE | Verify fresh-schema workspace ownership and retained model identity after source deletion |
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
| `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/model_entry/render/mod.rs` | MOVE + EDIT | Bind model credentials to their explicit workspace and verify ownership, recovery and reference safety |
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
| `ui/packages/app/components/domain/fleet-reply-content.ts` | CREATE | Keep reply context and safe display-field parsing without a parallel part registry |
| `ui/packages/app/components/assistant-ui/range.ts` | CREATE | Reuse bounded range helpers required by the official terminal element |
| `ui/packages/app/components/domain/FleetCommandResult.tsx` | CREATE | Adapt recorded command outcomes to the official terminal element without inferred exit codes |
| `ui/packages/app/components/assistant-ui/UPSTREAM.md` | CREATE | Record official component provenance, upstream license and local adaptations |
| `ui/packages/app/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/uploads/route.ts` | CREATE | Authenticated acknowledgement stub; no body parsing, storage or analysis |
| `ui/packages/app/lib/api/upload-preview.ts` | CREATE | Shared acknowledgement schema, composer file limits and route addressing |
| `ui/packages/app/lib/api/preview-attachment-adapter.ts` | CREATE | Native assistant-ui attachment adapter with scoped cancellation |
| `ui/packages/app/components/domain/useFleetUploadPreviews.ts` | CREATE | Display temporary upload receipts within the existing external-store runtime |
| `ui/packages/app/components/domain/chat/UploadAttachments.tsx` | CREATE | Native composer and message attachment display and removal |
| `ui/packages/app/components/domain/SteerComposer.tsx` | EDIT | Expose native attachment selection and preparation status |
| `ui/packages/app/components/domain/FleetReplyBody.tsx` | EDIT | Show preview receipts as plain text with an explicit temporary label |
| `ui/packages/app/lib/api/setup-proxy/body.ts` | EDIT | Parameterize the existing bounded-body reader for a preview upload |
| `ui/packages/app/tests/upload-preview-route.test.ts` | CREATE | Verify authentication, scope and acknowledgement without reading request bytes |
| `ui/packages/app/tests/preview-attachment-adapter.test.ts` | CREATE | Verify native attachment preparation, cancellation and refusal |
| `ui/packages/app/tests/fleet-upload-previews.test.tsx` | CREATE | Verify user attachments and text receipts without a model call or cross-owner state |
| `ui/packages/app/components/assistant-ui/TerminalBlock.test.tsx` | CREATE | Verify recorded terminal outcomes and plain output rendering |
| `ui/packages/app/components/domain/SteerComposer.test.tsx` | EDIT | Include native attachment capabilities in the composer unit fixture |
| `ui/packages/app/tests/fleet-tool-calls.test.tsx` | EDIT | Verify command output and diff text through the adopted official elements |
| `ui/packages/app/lib/auth/account-types.ts` | CREATE | Provider-neutral profile, browser-session and paging shapes |
| `ui/packages/app/lib/auth/account-provider.ts` | CREATE | Bounded, cancellable provider HTTP reads/writes with runtime validation and no automatic mutation replay |
| `ui/packages/app/lib/auth/account-actions.ts` | CREATE | App-owned server action boundary for current-person browser sessions |
| `ui/packages/app/lib/auth/account-client.ts` | CREATE | Isolate provider hooks, profile editing and verification from settings components |
| `ui/packages/app/lib/auth/credential.ts` | EDIT | Expose a minimal verified current-person/session identity without changing bearer or actor identity |
| `ui/packages/app/lib/auth/client.ts` | EDIT | Remove the unused embedded profile export after replacing its caller |
| `ui/packages/app/components/account/ProfileSettings.tsx` | CREATE | Custom profile form with React Actions and retained secure account controls |
| `ui/packages/app/components/account/SessionSettings.tsx` | CREATE | Custom browser-session list, paging, confirmed sign-out and durable errors |
| `ui/packages/app/components/account/use-account-sessions.ts` | CREATE | Own session reads and mutations per signed-in identity; discard stale completions |
| `ui/packages/app/components/account/AccountAccess.tsx` | CREATE | Loading and signed-out settings presentation shared by account screens |
| `ui/packages/app/app/(dashboard)/settings/account/[[...account]]/page.tsx` | DELETE | Merge native controls into API Keys; no separate profile route or compatibility alias |
| `ui/packages/app/app/(dashboard)/settings/security/page.tsx` | CREATE | Account-scoped browser-session Security route |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/security/page.tsx` | EDIT | Redirect the old placeholder to account Security |
| `ui/packages/app/components/layout/account-menu-data.ts` | EDIT | Make account Security discoverable while keeping API keys separate |
| `ui/packages/app/tests/account-session-actions.test.ts` | CREATE | Verify fresh reads, paging, own-session checks, recent verification and provider refusals |
| `ui/packages/app/tests/account-settings.test.tsx` | CREATE | Verify profile submission, session recovery, confirmation and account isolation |
| `ui/packages/app/tests/account-client.test.tsx` | CREATE | Verify the provider adapter returns neutral data and rejects stale identity operations |
| `ui/packages/app/tests/dashboard-placeholder.test.ts` | EDIT | Replace obsolete workspace Security placeholder assertions with authenticated account-Security routing |
| `ui/packages/app/tests/e2e/fixtures/account-settings-probe.tsx` | CREATE | Render production account views with explicit sample identity for visual review |
| `ui/packages/app/tests/e2e/fixtures/chat-browser-build.ts` | EDIT | Bind the account auth boundary to explicit sample data without bundling server credentials |
| `ui/packages/app/tests/e2e/fixtures/visual-auth-fixture.tsx` | EDIT | Keep provider-owned account access out of component visual fixtures |
| `ui/packages/app/tests/e2e/fixtures/chat-browser-probe.tsx` | EDIT | Mount account settings component fixtures alongside existing production visual fixtures |
| `ui/packages/app/tests/e2e/account-settings.spec.ts` | CREATE | Check settings typography, scope, errors, layout and focus at desktop/phone widths |
| `docs/AUTH.md` | EDIT | Document current-person profile/session adapters and retained verification behavior |
| `ui/packages/app/components/domain/chat/use-task-read.ts` | EDIT | Retain confirmed access loss through retry/loading until an authorized read succeeds |
| `ui/packages/app/tests/app-pages.test.ts` | EDIT | Replace obsolete embedded-profile expectations with the preserved account Security deep link |

The Files Changed table expands with each grounded implementation slice before editing its additional callers or tests.
The owner's approval covers the described surfaces; it does not authorize unrelated changes.

### Acceptance journey repairs

| File | Change | Purpose |
| --- | --- | --- |
| `ui/packages/app/tests/e2e/acceptance/account-accessibility.spec.ts` | EDIT | Verify inline profile, live browser sessions and native verified password controls |
| `ui/packages/app/tests/e2e/acceptance/auth-routing.spec.ts` | EDIT | Protect the new Security route while retaining legacy redirect coverage |
| `ui/packages/app/tests/e2e/acceptance/dashboard-performance.spec.ts` | EDIT | Measure persistent rail navigation without the retired drawer |
| `ui/packages/app/tests/e2e/acceptance/design-coherence.spec.ts` | EDIT | Retain accessible account-menu bounds beside populated tables |
| `ui/packages/app/tests/e2e/acceptance/fixtures/install-ui.ts` | EDIT | Follow the confirmed automatic save handoff into runtime |
| `ui/packages/app/tests/e2e/acceptance/fixtures/private-chat.ts` | EDIT | Prove exact workspace, saved revision and header editing through current controls |
| `ui/packages/app/tests/e2e/acceptance/fixtures/soak-journey.ts` | EDIT | Keep long-run setup revision and identity assertions through automatic handoff |
| `ui/packages/app/tests/e2e/acceptance/fleet-console.spec.ts` | EDIT | Preserve composer focus and draft while opening current header actions |
| `ui/packages/app/tests/e2e/acceptance/fleet-reply-parts.spec.ts` | EDIT | Check steady tool icons, working-label motion and recorded outcomes |
| `ui/packages/app/tests/e2e/acceptance/fleet-thread.spec.ts` | EDIT | Keep authenticated transcript and layout checks through the current header action |
| `ui/packages/app/tests/e2e/acceptance/navigation-accessibility.spec.ts` | EDIT | Verify persistent rail and account-menu keyboard paths at narrow widths |
| `ui/packages/app/tests/e2e/acceptance/settings-billing.spec.ts` | EDIT | Exercise explicit expanded and collapsed states with current controls |
| `ui/packages/app/tests/e2e/acceptance/template-onboarding.spec.ts` | EDIT | Reach Plugins and prove the pinned setup revision source |
| `ui/packages/app/tests/e2e/acceptance/workspace-fetch-dedupe.spec.ts` | EDIT | Pin the switched workspace and retain API fetch ceilings |
| `ui/packages/app/tests/e2e/acceptance/workspace-library.spec.ts` | EDIT | Remove an owned reusable setup through the visible Plugins entry |
| `ui/packages/app/tests/e2e/acceptance/workspace-private-chat.spec.ts` | EDIT | Verify save, edit and saved-draft reopening with the original identity |
| `ui/packages/app/tests/e2e/acceptance/workspace-url-flow.spec.ts` | EDIT | Pin the active workspace tooltip and URL in the compact rail |
| `ui/packages/app/tests/e2e/chat-pr-badge.spec.ts` | EDIT | Measure reading geometry atomically and retain the one-pixel bound through responsive Summary resizing |

### Shared website verification

| File | Change | Purpose |
| --- | --- | --- |
| `ui/packages/website/tests/e2e/design-system-motion.spec.ts` | EDIT | Assert shared Manrope faces and independently overridable display, interface and technical roles |

### Authenticated Team visuals

| File | Change | Purpose |
| --- | --- | --- |
| `ui/packages/app/tests/e2e/acceptance/team-members.spec.ts` | EDIT | Capture the real invite form and accepted-member table during the existing authorized journey |

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
- Define workspace ownership in the original table schema for a freshly initialized development database. Do not backfill, infer a workspace, preserve an old writer shape, or add a compatibility deployment. The owner will reset the development database before merging. A composite workspace/account foreign key prevents cross-account storage; removing a source workspace clears only its reference and retains model/setup history.
- Replace the old unique key with `(tenant_id, workspace_id, model_name, secret_ref)`. Two workspaces may register the same model and same key name independently. Registry metadata batches explicit workspace/name pairs in one read, without per-row queries or decryption.
- Runtime resolution, activation, registration and secret-deletion guards use the exact stored workspace/name pair. Preserve the credential-first lock order. Deleting an unused same-named key in another workspace must remain possible; deleting the referenced key must remain refused.
- The coordinated model fix updates the dashboard, CLI, generated OpenAPI and companion documentation together. `workspace_id` becomes required for model registration and self-managed activation; older callers must supply the workspace that holds the key. This requirement is part of Indy's instruction to fix the incorrect storage boundary, and must be explicit in release notes.

Do not add a frontend storage resolver, workspace enumeration, key copying or implicit rotation. Add live backend cases for two workspaces with same-named keys, activation and resolution after workspace switching, cross-account refusal, exact deletion guards, fresh-schema source deletion and compatible keyless endpoints. Unit form fixtures alone cannot prove this repair.

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

The row-layout split and recorded-event adapter are implemented. Commit `1c5755884` registers typed operator, reply, activity and grouped-event views. The assistant-ui integration checkpoint below covers the subsequent native part, tool, action and preview-attachment work; this does not complete the remaining settings surfaces.

Use assistant-ui's native part pipeline and render-only backend tool registration. Keep a thin typed adapter for recorded fleet events; remove the overlapping reply registry. Reuse official Elements for terminal output, code diffs and code headers, restyled through the shared design tokens. GroupedParts owns adjacent reasoning/tool grouping, ActionBar owns message-copy state, and the existing ExternalStoreRuntime remains the runtime owner.

Owner approval (Nov 11, 2026): "Can you build that then" / "as opposed to just hand rolling the gala" — implement the reviewed assistant-ui reuse proposal. Do not introduce client-side tool execution. The current API inventory has no general chat attachment upload endpoint; runner checkpoint uploads are unrelated. Attachment sending requires an actual supported transport and persistent representation. Structured form submission must use the current scoped backend behavior rather than invent an in-memory success.

Owner clarification: "you can create an upload noop endpoint and accept it" / "in the chat and reply the text" — a bounded authenticated preview-upload endpoint and an explicitly temporary chat receipt are authorized. Return a text acknowledgement; do not claim file storage, extraction or model analysis. Keep preview uploads distinct from durable fleet events and clear them with their conversation/account owner.

Owner simplification (Nov 11, 2026): "can you make this smaller, since this will be a pass thru to a rust or i havent figured out yet" / "other than that you are good to go" — keep the temporary route to origin, session and fleet-access checks plus a fixed acknowledgement. Do not read or parse its request body, inspect file metadata, mint receipt identifiers or invent a future upload API. The composer retains its file limits; no server upload-processing limit is claimed while the stub does not consume the body. Once the backend API is selected, replace this acknowledgement with forwarding and let that backend own upload validation/storage. Keep the current chat presentation and attachment adapter boundary.

Simplification checkpoint: the route is 34 lines, down from 55, without adding another handler layer. `make lint-app test-unit-app` passes lint, TypeScript and 454 files / 4,298 tests. The focused `chat-elements.spec.ts` Playwright run passes all 4 desktop/phone checks. The route tests prove the stub never consumes its request stream and preserves authentication, fleet access, archive and unavailable-backend refusals. Previous file-parsing/receipt-metadata tests were replaced with these stub-boundary checks. Logs: `/workspace/scratch/refresh-upload-stub-checks.log` and `/workspace/scratch/refresh-upload-stub-browser.log`.

Recorded elements cover operator turns, replies, integration activity, scheduled wake-ups, approval events, activity groups and a generic activity default. Keep the existing event order, stable IDs, private actor naming, source links and durable outcomes. Group metadata must be validated rather than cast to an event array. Invalid groups retain the parent activity text and outcome with a quiet details-unavailable note; never expose the malformed metadata.

The native reply-part pipeline keeps assistant-ui's reasoning/tool grouping, clocks, streaming Markdown, safe unknown-part fallback and existing request owners. Registered backend tools use the native tool UI; unregistered tools retain a generic presentation. Command output adapts recorded results to the official terminal element. Diff, plan and clipped input views remain separate from returned output. Recorded success, failure, interruption and unavailable output remain distinct; parsing input must never prove a side effect succeeded.

A renderer presents data and existing scoped actions. It cannot run model-provided code, invent an approval ID, fetch credentials, or send a notification. Schedule details retain the current fleet scope; approval links point to the current fleet's inline Summary, where Approve and Deny retain the existing permission checks. A part without a specialized view uses validated text or a safe resource label/link. If no readable fields exist, show a quiet details-unavailable note. Never serialize arbitrary part objects or execute embedded markup. The separately authorized upload preview above adds no durable file storage, analysis or tool capability.

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
API Keys combines account keys, browser sessions and the identity provider’s native profile/sign-in controls through the auth facade.

### Account controls and browser-session adapter

The approved proposal has no separate Profile page. The combined API Keys page hosts the provider's native controls through the app-owned auth facade; remove the custom profile form. Its header action scrolls and focuses the always-mounted Profile and sign-in settings section without changing Clerk's hash. Preserve the existing subject strings used by chat, pending sends and caches. No person/tenant mapping or token format changes. Keep identity edits, email verification, passwords, passkeys and multi-factor setup in the provider's secure controls, under its existing instance policies. Do not replace those flows with privileged unchecked backend writes.

Read browser sessions fresh through an app-owned server action, since the provider browser SDK caches `user.getSessions()` for the lifetime of its resource. Resolve the current subject/session only inside `credential.ts`; accept an expected subject as a stale-tab guard, never as authority. List only that subject's active sessions, in bounded pages, mapping device/browser/location/last-active/current-device into app-owned data. Unknown times remain unknown. API keys and CLI device credentials remain separate resources.

Session revocation first requires a signed-in matching subject, refuses the current session, requires strict recent verification using the provider's documented reverification hint, then reads the target session and proves its user matches before revoking. Preserve provider-native verification through the client adapter. Never log session metadata, credentials, challenges or personal data. Use the existing Clerk keys; introduce no new credentials. Failed or lost mutation responses stay unconfirmed and offer a fresh read; a successful revoke is retained even if the following refresh fails. Scope state by subject and discard late results after navigation/account changes. Session settings are account-scoped; the old workspace placeholder redirects to the account page.

Owner (Nov 11, 2026): "keep going" and "ensure the PR is sent, CI jobs are green, greptile is resolved. produce a report of the chagnes and final visual to eyeball." — finish the approved implementation, publish the feature branch/PR, and follow checks and review feedback. This explicit publishing instruction supersedes the cloud workflow's default stop-before-push guidance; it does not authorize merging.

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
| R8 | Full Linux acceptance, requested by Indy | `make acceptance-codex` | exit 0; unchanged suites and admission checks | P0 | |
| R9 | App and website bundle sizes | Existing package `build` then `size` commands | Existing limits pass; any excess requires owner sign-off | P0 | |
| S4 | PR gate (CI/ship; cloud stops before this) | `orly gate pr` | recorded before merge | P0 | |
| S3 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Review requirements before later sections

- Summary mounts exactly one task reader and one responsive tree; no hidden duplicate mobile reader. Existing task provider owns the stream.
- Models follows existing API scopes and explicit workspace identity. Do not introduce client-side vault selection. Keep explicit secret-write uncertainty, saved-key reuse, exact registration reconciliation and activation-only retry.
- Save handoff uses confirmed fleet identity independently of readiness failure. Setup history rehydrates through creator-filtered context; keep setup/runtime buffers separate and clear them on identity/access loss. Never copy a setup prompt into runtime messages.
- Deletion retains confirmed-stop state, reads current authorized state after uncertain writes, and handles missing responses without claiming success. A denied or unavailable read cannot prove deletion. Preserve billed attribution and settled runner retention.
- The separate vocabulary spec must map public Plugins names to immutable historical representations without rewriting frozen revisions, and translate only existing library write authority without widening narrowed identities. The separate auth spec must preserve current subject-based actor/cache identity during adapter changes.

## Approved parallel CLI/API follow-up

The combined pending specification is [M249_002](../pending/M249_002_P1_API_CLI_DOCS_UI_FLEET_COMMANDS_AND_SCOPED_DISCOVERY.md). It owns public command/API vocabulary, command composition and scope-aware CLI discovery. Its implementation is separate from this UI refresh. The downloadable package includes the spec and a Claude start prompt; the spec remains PENDING until its fresh design/readiness review.

Indy explicitly permits independent CLI work to begin from current `origin/main` in a separate worktree. This UI PR merges first; the CLI branch then rebases onto that merge before changing refreshed UI callers and before final verification or merge. Do not restore older UI layouts. Preserve workspace-backed credentials, complete-setup Plugins (Flow 1), current authority and the development-only decision: no compatibility aliases, backfill or guarded cutover.

> Owner: "well if the cli is discrete, then it could start before your merged UI PR. But i will merge the cli after PR merges. so they go in parallel."

> Owner: "with 1 being broken down two parts - claude to works from your PR merged on the UI refresh you did with gaps delta on api needed on the backend and wire it into the ui you created (post your PR merge)."

The separately approved post-merge backend data work must use the completed UI as its starting point. Missing daily usage buckets, key metadata and recorded runtime facts are data gaps, not permission to invent values, new secret access policy, real upload storage, mention invocation or Flow 2 capabilities.

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

The fresh review reported eight findings. The dispositions below reflect the implemented source and completed local evidence, while retaining the separate deployment and signed-runner limitations. Recording a requirement alone is not resolution.

| Finding | Status | Resolution / remaining proof |
|---|---|---|
| 1: Summary owner | Implemented; local proofs complete | One responsive Summary consumes existing approval/event and schedule readers; the drawer/toggle is removed. Component/unit ownership checks and the completed 84-case app dry lane cover retained data, responsive placement and reading position. No new poller was introduced. |
| 2: Verification | Local evidence recorded; release acceptance unavailable | Exact-base counts and completed repository/package/browser commands are recorded below. The signed runner-toolbox download is denied by the cloud proxy before release acceptance can execute. The focused real-backend selection is identified separately and does not substitute for fleet execution. |
| 3: Model credential scope/recovery | Implemented; fresh development schema verified | Exact workspace references govern registration, activation, resolution and deletion guards. The owner explicitly chose an empty development database, so the initial schema holds workspace ownership directly and no legacy upgrade/backfill exists. Current main merge passes 5,525 Rust unit tests and 1,722 regular plus 2 exclusive integration tests. The previously recorded real form journey saves, registers and activates against its exact workspace; later presentation changes require current visual evidence. |
| 4: Delete reconciliation | Implemented; component/local proofs complete | Typed stop/delete checkpoints, uncertain-response reads, access-loss handling and paused notices pass focused unit checks and the app dry lifecycle browser cases. No full signed-runner lifecycle acceptance is claimed. |
| 5: Setup handoff | Implemented; local proofs complete | Existing setup tests retain save-key reuse, exactly one confirmed handoff, transient-failure recovery and authorization loss. Authoring history remains private. The app dry lane verifies setup surfaces; real runner execution remains unverified here. |
| 6: Stored vocabulary/authority | Open in coordinated CLI/API batch | This UI batch preserves immutable review records, platform pins and narrowed capabilities. The broader public Plugins/CLI resource vocabulary and scope-filtered discovery remain specified separately, with no completion claim. |
| 7: Identity mapping | Implemented and locally verified | App-owned adapters preserve the existing subject/session identity, exact ownership, recent verification and stale-result exclusion. Real Profile/Security journeys pass at 1440/390/320px, including the supported native password editor and cancellation. No identity-record migration or additional Clerk key was introduced. |
| 8: Reference/type consistency | Resolved in current implementation/reference | The active spec, shared design-system documentation and current final report use the approved Manrope roles and 16px/24px phone typing. The final report links the current checkout and distinguishes implementation captures from the earlier historical proposal artifacts. |

Typography and icon work was explicitly cleared. Code/account re-reviews clear the final eight-file UI delta; all four visual P3 findings are fixed in the current actual captures. The owner’s explicit development-only reset decision closes the earlier migration blocker. The coordinated vocabulary batch and unavailable full signed-runner acceptance remain explicit. The current visual comparison remains open until the refreshed route captures are inspected.

## Discovery (consult log)

- **Appsmith reference (Nov 11, 2026)** — At Indy's request, cloned `https://github.com/appsmithorg/appsmith` to `/workspace/appsmith-reference` and inspected `app/client/src` at `65921d7294476c2f7af25c3c7d31c43da2eee215`. Useful sources: `WidgetProvider/factory/index.tsx`, `utils/formControl/FormControlFactory.tsx`, `pages/Editor/CustomWidgetBuilder/Editor/AIAssistant/index.tsx`, `sagas/AIAssistantRequestContext.ts` and `ce/reducers/aiAssistantReducer.ts`. Apply typed component registration, separated presentation/state, scoped context and explicit outcome handling through the existing assistant-ui runtime. The public AI-chat integrations control is a placeholder; do not claim it supplies an embeddable chat implementation. Do not copy their custom widget editor, Redux runtime or markdown-driven code application.
- **Shared font correction (Nov 11, 2026)** — Indy extended Manrope to the website. Shared `tokens.css` now imports Manrope once for both interface and display; Commit Mono remains technical. Instrument Sans and Bricolage packages and Usage's system-font overrides were removed. Dashboard token overrides contain no font-family declarations or Tailwind mappings.
- **Focused visual evidence (Nov 11, 2026)** — Eight rendered app/website token-profile combinations passed (dark/light, desktop/phone). The chat typography/alignment browser selection passed 22 checks. Website selected checks passed after correcting the keyboard journey to enter from the home page rather than assuming a direct fragment starts tab order at the document top: the first run passed 25 of 29; the affected keyboard, font ownership and zoom selection then passed all 10. These are focused checks, not the repository `dry` or final acceptance lanes. Logs and desktop/phone screenshots are under `/workspace/artifacts/fleet-refresh-v2` and `/workspace/scratch/refresh-*`.
- **Code-review follow-up (Nov 11, 2026)** — Shared class-merging fixes and radius preservation passed `make test-unit-design-system` (62 files, 489 tests). Collapsed rail tooltips now expose labels on keyboard focus; the focused app navigation/workspace selection passed 39 tests. Final rendered rail review remains part of §5.
- **Consults** — Fresh `origin/main` at `bfcfe5bac`; the merged platform CLI change removed operator pages. `orly doctor` passed both installation and rule checks.
- **Model reviews** — Native EGM implementation-readiness review completed against `bfcfe5bac`: 1 blocker for the separate vocabulary migration, 6 majors, 1 nit. No blocker to shared visual work. Report: `/workspace/artifacts/fleet-refresh-v2/implementation-readiness-current-main.md`. Missing Summary owners and focused test paths are now included. The remaining model, handoff, deletion, vocabulary and identity findings must be resolved in their corresponding sections/specifications before those edits.
- **Open blockers** — None in approved product direction; implementation details and exact callers are checked before each slice.
- **Rollout review blocker (Nov 11, 2026)** — The forward model migration removes uniqueness still required by old writers, while old readers can resolve a newly registered model's credential from the wrong workspace during a rolling replacement. Both chat P2 findings are fixed. The model P1 is open: Indy has been asked to choose guarded maintenance cutover or two-release compatibility. No unanswered choice is treated as approval, and no deployment configuration was changed.
- **Final recovery checks (Nov 11, 2026)** — `make lint-app test-coverage-all` passes. App: 462 files / 4,457 tests; website: 20 / 133; CLI: 2,217 passed / 17 skipped / zero failures; design system: 62 / 491. All four package coverage gates pass their declared 100 percent floors; the CLI measures functions and lines. The callback-replacement regression failed before the React Effect Event cleanup fix, then passed. A notice-owner review caught dashboard remounts during auth loading; the revised owner filters only notices and preserves dashboard children. Draft retention and A→B→A stale-callback cases pass, and the account reviewer cleared the repair. Evidence: `/workspace/scratch/refresh-final-coverage-repaired.log`, `refresh-model-callback-red.log`, `refresh-notice-auth-red.log` and `refresh-notice-auth-green.log`.
- **Repository checks (Nov 11, 2026)** — `make test-unit-rustd` passes all three groups: runner 1,737, daemon 167, libraries 3,573. `make lint-all check-version` passes after splitting a migration test and changing an empty-registry assertion to report its count. All versions match 0.76.0. The migration rollout design remains unchanged and open. Final integration, browser and delivery gates remain due.
- **Companion documentation (Nov 11, 2026)** — The dedicated docs branch records the inline model form and explicit workspace selection in commit `78d4a30`. Content checks pass. Full lint exposed seventeen retired runner-operation references and eight missing current operations in the unchanged API navigation. Align that inventory to the already-shipped main OpenAPI: seven admin-runner operations and the runner stream. This does not add API pages ahead of this branch's merge.
- **Settings checkpoint (Nov 11, 2026)** — Custom Profile and Security now use the existing Clerk configuration through app-owned adapters. Names retain failed edits; session lists use current-person reads, recent verification and exact target ownership. A committed sign-out with a lost response is reconciled without a second POST. The focused account/terminal run passes 70 tests with 100 percent statements, branches, functions and lines across its twelve included production files. This is a focused measurement, not the global coverage gate. Six desktop/phone browser cases pass for Profile, Security, Models, Team and invitations in both appearances. Model evidence is collapsed in Create model so phone save controls remain visible; the model selection/recovery selection passes 33 tests. These visuals use production components with isolated sample data and no live resource writes. `make lint-app` passes source rules, Oxlint and TypeScript. Global coverage and final repository checks remain due.
- **Measured merge-base baseline (Nov 11, 2026)** — The exact base `bfcfe5bac18c873d3275a125e742b06fb3c33c4d` has no executed unit/coverage lane in its CI runs; warm-cache only compiles Rust tests. An isolated detached checkout at that revision ran `make test-coverage-all`: app 440 files / 4,232 tests, website 20 / 133, CLI 2,217 passed / 17 skipped / 0 failed, design system 62 / 488. Every package meets its declared 100 percent coverage floor. Evidence: `/workspace/scratch/refresh-baseline-coverage.log`. Base CI metadata: `https://github.com/agentsfleet/agentsfleet/actions/runs/42953803333` (compile-only warm-cache), plus deployed verification `https://github.com/agentsfleet/agentsfleet/actions/runs/42953802879`; neither is claimed as the measured unit run.
- **Rust baseline and container setup (Nov 11, 2026)** — The same detached base passed `make test-unit-rustd`: runner 1,737, daemon 167, daemon libraries 3,572; total 5,476. The cloud container's PID 1 is `tail`, and its shell umask is 0077. Unsupervised tests left orphaned zombies, and the restrictive umask changed an expected published-image mode from 0444 to 0400. The unchanged lane passed under a checksum-verified user-owned Tini 0.19.0 supervisor and command-local `umask 022`. No test assertion or gate was edited. Evidence: `/workspace/scratch/refresh-baseline-rust-unit-supervised-normal-mask.log`. Later Rust gate commands use the same process supervision and mask.
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
- **Native chat review checkpoint (Nov 11, 2026)** — `make lint-app test-unit-app` passes lint, TypeScript and 454 files / 4,298 unit tests. The focused Playwright selection (`chat-elements.spec.ts`, `chat-alignment.spec.ts`, `chat-typography.spec.ts`) passes 26 desktop/phone checks, covering both appearances, native tool registration, command/diff/code/link rendering, preview upload acknowledgement, oversize rejection, typography, alignment and motion fallbacks. Browser integration exposed the SDK's all-file accept spelling (`*`); the adapter now uses it, and native attachment-add errors remain visible beside the composer. Visual review moved terminal status controls below commands on phones. Logs: `/workspace/scratch/refresh-native-ui-checks.log` and `/workspace/scratch/refresh-native-browser-checks.log`; screenshots: `/workspace/artifacts/fleet-refresh-v2/production-browser-results`. These checks use production components with sample identity/backend responses, not a live authenticated acceptance journey or the full `make dry-app` lane. Profile/Security, Team and other remaining refresh work are not complete. Preview files are discarded and their plain-text receipts are temporary; structured form submission remains dependent on supported backend behavior.

- **Final repository checkpoint (Nov 12, 2026, Asia/Calcutta)** — `make test-unit-all` passed with 5,477 Rust tests and every package coverage floor at 100 percent. App: 462 files / 4,457 tests; website: 20 / 133; CLI: 2,217 passed and 17 skipped; design system: 62 / 491. The clean `make test-integration-rustd` run passed 1,706 regular and two exclusive tests. Logs: `/workspace/scratch/refresh-final-unit-all.log` and `/workspace/scratch/refresh-final-integration.log`.

- **Acceptance and visual repair checkpoint** — The final component browser selection passed 36 cases. Visual review requested intrinsic phone notice width; the prepared acceptance repair preserves namespace cleanup, exact workspace redirects, saved revision readback, keyboard navigation, native password cancellation and direct deletion from an active fleet. The 24-file acceptance repair was prepared outside the checkout during integration, then reviewed before application. The retired drawer, old creation labels and post-save explicit-open assumptions no longer describe the approved flow. Authenticated execution and dry lanes remain due.

- **Final review scope** — The manual gate-integrity pass completed all 211 eligible hunks, including the 161 beyond the helper cap, plus 33 current test paths. It found no unjustified weakening. The migration rollout P1 remains open pending the owner's choice; no approval or deferral is inferred.

- **Narrow conversation regression** — The new 320px disclosure test failed both project cases against the prior flex minimum. Adding `w-full min-w-0` to the shared tool trigger keeps the summary arrow and expanded tool controls inside the column. Both project cases then passed. Logs: `/workspace/scratch/refresh-narrow-disclosure-red.log` and `/workspace/scratch/refresh-narrow-disclosure-green.log`.

- **Reading-position measurement correction** — Releasing task details grows mobile Summary from 227px to 236.3125px. The viewport moves 9.3125px and native scrolling compensates by 9px; visible text shifts only 0.3125px. Twenty control frames and twenty released frames confirm stable geometry and loaded fonts. The browser test now measures geometry atomically and keeps the one-pixel bound on page reading position; a fixed viewport retains the original raw-scroll bound. All rectangle, focus, draft, badge and read-count assertions remain. Both project cases pass, and original/repaired scratch probes each reject a forced 12px badge layout regression. Review evidence is in `/workspace/artifacts/fleet-refresh-v2/chat-pr-badge-scroll-investigation.md`.

- **Website verification repair** — Indy requested, “i wanna use the same font across website too.” The website font test now requires Manrope for display and interface, Commit Mono for technical text, and unchanged independent role overrides. Five focused keyboard/font cases pass. A four-times CPU-throttled probe reproduced five first-Tab failures with no mounted skip link and an empty React root. Waiting for the actual skip link preserves native first-Tab activation and passes all five equivalent probes. Original and repaired scratch guards each reject a deliberately wrong first focus target and collapsed font roles. The full 133-case website lane is being rerun.

- **Final browser lanes** — `make dry-app` passed 462 files / 4,457 app tests and all 84 browser cases. `make _dry_website` passed all 133 website browser cases. Logs: `/workspace/scratch/refresh-final-dry-app-complete.log` and `/workspace/scratch/refresh-final-dry-website-complete.log`. Both visual P3 findings are cleared in the fresh app captures; final hero recapture follows. The isolated merge-base integration lane passed 1,702 regular and two exclusive tests, compared with 1,706 and two on this branch. Earlier baseline attempts failed from build storage and a shared-target cache mismatch; neither failed build was counted as a result.

- **Exact-base app browser measurement** — The detached merge base `bfcfe5bac18c873d3275a125e742b06fb3c33c4d` passed the actual `make dry-app` lane: 4,232 unit tests and 68 browser cases. The refresh passes 4,457 unit tests and 84 browser cases. Evidence: `/workspace/scratch/refresh-baseline-dry-app.log`. Next generated only its development route-type imports; those were restored after the run, leaving baseline source unchanged.
- **Authenticated setup attempt** — The unchanged `make acceptance-codex SUITE=e2e ACCEPTANCE_SCOPE=post-deploy` refused the cached signed toolbox before building or executing tests. That manifest admits runner 0.73.0; this branch is 0.76.0. The signed toolbox from the current base’s successful deployment run is being acquired. Signature and version admission remain enabled; no authenticated success is claimed from this attempt. Evidence: `/workspace/scratch/refresh-authenticated-acceptance.log`.
- **Authenticated prerequisite limitation** — The current base’s successful deployment run `42953802879` retains signed runner-toolbox artifact `13825368657`. Both GitHub CLI and a redirect-isolated client reached the artifact URL, then the cloud proxy refused its Azure Blob CONNECT with HTTP 403. This is a network prerequisite failure, not an executed product test. Do not edit the signed manifest, bypass signature/version checks, or claim the post-deploy lane passed. Focused account/resource browser checks may use the real local backend without starting a runner, explicitly excluding release preflight and execution.
- **Final non-test repository gate** — `orly gate verify` passed: repository configuration, documentation language, the complete `make lint-all` invocation and `make check-version` are green. All versions match 0.76.0. Evidence: `/workspace/scratch/refresh-final-verify-gate-current.log`. This is the local verification gate, not `orly gate pr`, remote CI or Greptile clearance.
- **Final orphan and secret audit** — A whole-repository case-insensitive sweep of the seven removed paths found only dated review inventories and the separate Rust `FleetConfig` type; no current import points at a removed component or SQL file. A stale test-helper comment naming the removed UI component was deleted; its existing callers still use the helper. `gitleaks detect` scanned all 16 branch commits, approximately 722 KB, and reported no leaks. Evidence: `/workspace/scratch/refresh-final-orphan-sweep.log` and `refresh-final-branch-secret-scan.log`.
- **Local TLS setup repair** — The focused authenticated setup built both Rust binaries, CLI and production Next successfully, then Caddy health failed because BusyBox sent its loopback GET through the inherited HTTP proxy. The exact GET succeeds with only that container process’s `http_proxy` empty. A task-only Compose runtime overlay applies that environment setting. Docker configuration, registry credentials, committed harness, health URL, TLS validation and readiness checks are unchanged. This failed setup executed no authenticated browser test.

### Authenticated follow-up repair scope

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/lib/auth/account-controls.tsx` | CREATE | Embed the supported native account widget behind the auth facade |
| `ui/packages/app/tests/account-controls.test.tsx` | CREATE | Verify the accessible owner and secure native handoff without exposing provider state |
| `ui/packages/app/lib/auth/account-types.ts` | EDIT | Keep any native-control ownership shape behind the existing account adapter |
| `ui/packages/app/lib/auth/client.ts` | EDIT | Keep the native account component behind the existing provider facade if needed |
| `ui/packages/app/lib/profile-appearance.ts` | EDIT | Fit native controls within the accessible app surface |

- **Authenticated findings (Nov 12, 2026, Asia/Calcutta)** — The real local backend plus existing development Clerk fixtures ran the 15 selected account, navigation, model and billing checks: nine passed and six failed. Password editing removes the native modal’s accessible name at all three widths; expanded account navigation fails the visible-label/name match in both themes; the new-model dialog closes before provider selection. The original assertions remain enabled. Repair these actual findings and rerun the affected journeys; sample-component visuals alone did not expose them. The focused run explicitly excludes runner-dependent release preflight; it is not full acceptance or CI.

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/components/account/AccountSettingsSection.tsx` | CREATE | Keep a named native-control section mounted on API Keys; clear its widget when account/session changes |

- **Supported native-control repair** — The installed `UserProfileModalProps` exposes profile options and portal container, with no accessible-name override. Use the supported embedded `UserProfile` with hash routing on `/settings/security`, inside its always-mounted named Profile and sign-in settings section. Keep a persistent app page title/region and key the live native widget by the existing subject/session context. This preserves native verification portals without adding a second modal focus trap or DOM patch. The account reviewer cleared this direction read-only; fresh tests remain due.

| `ui/packages/app/tests/e2e/acceptance/integrations-nav.spec.ts` | EDIT | Retain purpose-based menu selection while the native accessible name includes its visible account/workspace labels |

| `ui/packages/app/tests/e2e/acceptance/settings-api-keys.spec.ts` | EDIT | Retain purpose-based menu selection while the native accessible name includes its visible account/workspace labels |

| `ui/packages/app/tests/e2e/acceptance/fixtures/nav.ts` | EDIT | Retain purpose-based menu selection while the native accessible name includes its visible account/workspace labels |
- **Authenticated repair checkpoint** — The supported embedded account route, visible-name switcher fix and loaded-owner model trigger passed the focused seven-file selection: 55 tests. Account/security static re-review found no new concrete issue. `make lint-app test-unit-app` passed lint and TypeScript, then 4,459 tests; its sole failure expected Create model to reopen while signed out. That recovery test now explicitly proves the disabled signed-out state, restores sign-in, and retains its empty-key assertion after reopening. The original browser accessibility assertions remain enabled; package coverage and the real authenticated rerun remain due.
- **Authenticated-repair package gate** — The actual `make test-coverage-all` passed: app 463 files / 4,460 tests, website 20 / 133, CLI 2,217 passed / 17 skipped / zero failures, and design system 62 / 491. Each package passes its declared 100 percent floors; the CLI reports functions and lines. App totals are 12,652 statements, 9,249 branches, 3,315 functions and 10,249 lines, all covered. Evidence: `/workspace/scratch/refresh-auth-repairs-coverage-all.log`. The supported native-page repair and original browser assertions now require a real production rebuild and authenticated rerun.
- **Final authenticated journeys** — The real local 0.76 backend and existing development Clerk instance passed the 15 selected account/navigation/model/Billing cases and three Team invitation/membership cases. The switcher repair includes visible initials and literal word boundaries. The named native account page retains verified password controls at 1440, 390 and 320px. A visual sweep additionally caught native compounded padding and a fixed 264px section header. Supported appearance values constrain the header and remove duplicate padding; a doubled-class selector outranks the vendor mobile rule without hiding overflow. The new scrollport and field bounds pass. Four repeated account/model cases passed after the last spacing refinement to refresh the captures; they are not counted as four additional unique journeys. Twelve real screenshots are copied and hashed under `/workspace/artifacts/fleet-refresh-v2/authenticated-visuals`. Logs: `/workspace/scratch/refresh-auth-final-live.log` and `refresh-auth-final-visuals-refined.log`. All recorded browser and fixture sessions were revoked during teardown. This focused selection excludes release preflight and fleet execution; it does not claim full acceptance or CI.
- **Final model display finding** — The authenticated model screenshot exposed a long selected identifier overflowing its control. Installed Radix SelectValue discards className/style, so its previous truncation classes never reached the DOM; the unit stub forwarded them and masked the defect. Move truncation onto a real flex child, bound the picker grid/trigger and popup, and retain the full identifier in options, title and details. Add real browser field/chevron bounds. The repeated full coverage run also exposed an immediate Saved key query after a React Action reset; await the actual field without changing the storage/replay assertions or time limits. This red run is retained, not counted as a pass.

- **Final selected-model repair verification (Nov 12, 2026, Asia/Calcutta)** — The real truncation wrapper and unchanged recovery/no-replay assertions pass 27 focused tests. `make lint-app` passed. A fresh production rebuild passed the four repeated real account/model cases at 1440/390/320px, including actual selected-value/chevron and native editor/scrollport bounds. The twelve authenticated captures were copied immediately and hashed; all match the regenerated 61-image gallery. Final read-only code review clears the exact eight-file delta; final visual review closes every reported P3. Evidence: `/workspace/scratch/refresh-auth-model-final.log`, `refresh-model-display-recovery-focused.log`, `refresh-model-display-lint.log`, and `/workspace/artifacts/fleet-refresh-v2/final-visual-review.md`. These repeated cases do not add to the 18 unique authenticated checks already recorded.

- **Final current package gate (Nov 12, 2026, Asia/Calcutta)** — The fresh actual `make test-coverage-all` passed: app 463 files / 4,460 tests; website 20 / 133; CLI 2,217 passed / 17 skipped / zero failures; design system 62 / 491. All package floors remain 100 percent. App statements 12,653, branches 9,257, functions 3,315 and lines 10,250 are all covered. Evidence: `/workspace/scratch/refresh-release-final-coverage.log`. The following verification invocation passed the complete `make lint-all` and version 0.76.0 checks but found two four-sentence paragraphs in the updated authentication guide. Those paragraphs were split without dropping content or changing the rule; repeat the actual verification gate. Its previous red log is retained at `/workspace/scratch/refresh-release-final-verify.log`.

- **Final verified source (Nov 12, 2026, Asia/Calcutta)** — The actual `orly gate verify` repeat passed configuration, documentation language, the complete `make lint-all` invocation and version 0.76.0. Evidence: `/workspace/scratch/refresh-release-final-verify-green.log`. The prior red documentation result remains recorded; no suppression or rule change was used. Package coverage and the current authenticated captures are complete as recorded above. The CLI coverage floor measures functions and lines; the other three packages report statements, branches, functions and lines. Final visual review is clear, with all four layout findings fixed. The mixed-version migration P1 remains open pending the owner’s rollout choice, so this spec remains active and no PR, remote CI or Greptile completion is claimed.

### Owner correction: fresh development database (Nov 12, 2026, Asia/Calcutta)

> Owner: “i am only in development”; “i will tear the db”; “and merge your pr”; “i dont need a guarded cutover”.

Implement the current schema directly for an empty development database. Remove the forward backfill and all legacy-upgrade test setup; retain normal workspace ownership, history retention and current API authorization. No guarded cutover, production deployment change, compatibility release or database reset is performed by this branch. The earlier rollout question is superseded by this explicit environment decision. Sync current main, verify the new schema and merge resolution, then complete the authorized Pull Request and checks.

### Approved mock comparison repairs (Nov 12, 2026, Asia/Calcutta)

The earlier visual sweep checked built quality without a complete mock comparison. The direct comparison found Models/Team composition drift, a capped mobile Summary and missing active fleet navigation. Repair these against the approved captures, retain actual data and request owners, and inspect every other redesigned route before claiming visual completion.

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/app/(dashboard)/settings/members/components/MembersView.tsx` | EDIT | Center the approved Team layout, retain account scope and search both people and pending invitations |
| `ui/packages/app/app/(dashboard)/settings/members/components/MembersView.test.tsx` | EDIT | Retain role, delivery, action and focus proofs while verifying grouped searchable team rows |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryView.tsx` | EDIT | Render approved grouped model cards while preserving search, sort and bounded paging |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCards.tsx` | CREATE | Present existing model cells and authorized actions in responsive cards |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryView.test.tsx` | EDIT | Preserve paging, filtering and read-recovery assertions through the approved card composition |

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/tests/helpers/members-fixtures.tsx` | EDIT | Preserve user-visible data, action, sorting and recovery assertions through shared Card composition |
| `ui/packages/app/tests/models-registry-prices.test.tsx` | EDIT | Preserve user-visible data, action, sorting and recovery assertions through shared Card composition |

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/components/domain/chat/LibraryBrowser.tsx` | EDIT | Compose existing owned/shared setup choices from shared Card and List primitives without changing Flow1 authority |

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/tests/chat-failure.test.tsx` | EDIT | Preserve exact retry failure/clearing outcomes with the native Alert region |

| File | Action | Purpose |
|---|---|---|

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/components/layout/ResourceHeader.tsx` | CREATE | Compose the approved compact resource title bar from shared PageHeader and existing navigation metadata |
| `ui/packages/app/components/layout/ResourceHeader.test.tsx` | CREATE | Verify resource labels and exclusion of fleet conversations and unknown routes |
| `ui/packages/design-system/src/design-system/PageHeader.tsx` | EDIT | Expose the stable page-header slot for app theme composition |

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/tests/e2e/fixtures/resource-pages-probe.tsx` | CREATE | Capture actual resource and empty-chat components with explicit sample data and denied writes |
| `ui/packages/app/tests/e2e/fixtures/chat-browser-build.ts` | EDIT | Route sample resource actions through the existing browser fixture boundary |
| `ui/packages/app/tests/e2e/fixtures/chat-browser-probe.tsx` | EDIT | Expose the additional real-component review states |
| `ui/packages/app/tests/e2e/fixtures/resource-settings-actions.ts` | EDIT | Supply nonsecret sample metadata and refuse every mutation in visual fixtures |

Empty setup chat retains the native assistant-ui thread and composer. Match the approved centered welcome and suggestions below the composer; preserve editable suggestions, unfinished setup links and all request ownership. Resource captures use actual components with visible sample-data labels; they do not prove authenticated backend journeys.

Native Summary scroll verification (Nov 12, 2026): the existing badge test initially reports 103px raw scroll movement on phones after Summary grows from 227px to 330px inside the same native viewport. Diagnostic captures show unchanged reply/header/composer rectangles and focus, with native scrollTop moving from 26px to 129px to compensate. The test now independently measures the preceding Summary height and requires exact compensation within the original one-pixel bound. Desktop still requires zero offset drift. Every visible rectangle, badge position, draft, focus and read-count assertion remains. Diagnostic log: `/workspace/scratch/refresh-pr-badge-native-diagnostic.log`. Re-run and forced-layout negative checks remain due before completion.

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.tsx` | EDIT | Keep both existing secret actions reachable within the phone table |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/ApiKeyList.tsx` | EDIT | Keep key status and complete revoke/delete targets visible on phones |
| `ui/packages/design-system/src/design-system/Pagination.tsx` | EDIT | Keep Previous and Next together when native pagination wraps |
| `docs/DESIGN_SYSTEM.md` | EDIT | Document native Summary flow and shared responsive resource composition |

| `ui/packages/design-system/src/design-system/Button.tsx` | EDIT | Keep shared small icon-action targets at the documented 32px desktop minimum without enlarging glyphs |

| `ui/packages/design-system/src/design-system/IconAction.test.tsx` | EDIT | Retain the fixed shared action size proof at the approved 32px target |

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/tests/models-registry-paging.test.tsx` | EDIT | Preserve loaded rows, cursor termination and end-of-feed proofs with the hidden single-page pager |
| `ui/packages/app/tests/fleet-thread/role-activity.test.ts` | EDIT | Preserve exact startup failure content in the shared named Alert |
| `ui/packages/app/tests/fleet-thread/surface.test.ts` | EDIT | Verify the native footer contains the bounded composer wrapper |

| `rustd/crates/daemon/afd_db/src/migration.rs` | EDIT | Remove the superseded forward migration from the fresh-schema registry |
| `schema/20261111230000_model_tenant_models_workspace.sql` | DELETE | Remove the owner-rejected legacy forward migration and backfill |

Visual sample clarity: the browser conversation probe overrides the deterministic unit helper’s 1970 timestamp with its own current sample instant and explicitly labels the page Sample data. Do not change shared unit timestamps or present sampled captures as live fleets.

### Independent Astra comparison and whole-composition polish (Nov 12, 2026)

Owner: “I need a polished aligned UX that you portrayed to be that matches pixel to your design. Do what you need to do to get it visually, ask indy on eyeball review.” His header, bubble, identity, composer and Summary notes are review feedback, not an exhaustive acceptance checklist. Use design judgment across the entire composition and return concrete matching captures for his review.

Astra independently confirmed that the current Summary is too verbose and uses the wrong hierarchy; its 1024px stacking boundary differs from the approved 680px boundary. Matching fonts and 248px width did not establish visual fidelity. The default tool label also enumerates internal tool names. Report: `/workspace/artifacts/fleet-refresh-v2/astra-visual-validation.md`. Paired actual viewport captures: `/workspace/artifacts/fleet-refresh-v2/astra-current-comparison` (1480×1000, 696×846 and 390×846). The owner’s supplied image may be cropped; its image width is not viewport evidence.

Revise the whole composition through the current design-system and assistant-ui primitives. Keep stable Changes, Outputs and Sources headings with quiet empty states after a confirmed empty read; insert approval/attention first when present. Put task objectives, controls and repository-policy explanations behind closed disclosures, retain urgent failures, and add supported activity groups as real data arrives. Environment, child-agent lifecycle, process lifecycle, diff totals and approval action details must come from authoritative data; never invent them from tool names or a successful spawn/command. The earlier blanket instruction to omit every empty group is superseded by this review. Keep the existing approval decision owner and model/setup/navigation state owners.

Current pre-polish verification checkpoint: actual `make test-coverage-all` passes app464 files/4468 tests, website20/133, CLI2217 passed/17 skipped, design system62/491, with all declared coverage floors100%. Actual `make dry-app` passes4468 app units and90 browser cases. These commands prove behavior and coverage, not pixel fidelity. Rust remains5525 unit tests and1722 regular plus2 exclusive integration tests on the merged fresh-schema source. No PR/push/remote feature CI/Greptile success is claimed yet.

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/components/assistant-ui/CodeBlock.tsx` | CREATE | Compose the official Markdown syntax-highlighter slot and native Pre/Code inside a coherent shared-token code surface |
| `ui/packages/app/components/domain/chat/TaskPanel.tsx` | EDIT | Present a compact Summary digest and disclose secondary task context |
| `ui/packages/app/components/domain/chat/WorkContextSections.tsx` | EDIT | Stable compact Summary sections, truthful output labels and approval emphasis |
| `ui/packages/app/components/domain/chat/WorkWakeups.tsx` | EDIT | Keep compact next-wake-up context and explicit schedule failures |
| `ui/packages/app/components/domain/tool-call-activity.ts` | EDIT | Replace unbounded collapsed tool enumeration with bounded count/outcome labels |

The next visual pass must include coherent empty, working and populated Fleet states in both themes, with recorded Changes and a real-shaped pull-request target actually passed into the sample fixture. SetupSummary does not substitute for Fleet Summary. Compare the full header, transcript, user/assistant turns, composer and Summary together; preserve unmodified screenshots and the data/state distinction. Astra rechecks independently, then Indy receives an accessible eyeball review.

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/components/domain/FleetApprovalStatus.tsx` | CREATE | Project the existing shared fleet facts into header attention without another backend reader |
| `ui/packages/app/components/domain/FleetStreamProvider.tsx` | EDIT | Expose an optional presentation subscriber while retaining strict stream ownership |
| `ui/packages/app/components/domain/FleetApprovalStatus.test.tsx` | CREATE | Verify initial/live approval attention and unchanged shared stream behavior |

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/tests/e2e/fixtures/refresh-visual-data.ts` | CREATE | Supply coherent typed empty, working and populated fixture events, task metadata and pull-request outputs |
| `ui/packages/app/tests/e2e/fixtures/refresh-visual-probe.tsx` | EDIT | Render those same recorded facts in native conversation and Summary, with explicit sample labels |
| `ui/packages/app/tests/e2e/fixtures/chat-probe.ts` | EDIT | Serve matching visual task reads without replacing production readers |

| File | Action | Purpose |
|---|---|---|
| `ui/packages/design-system/src/design-system/time-utils.ts` | EDIT | Own the shared minute-precision UTC clock formatting role |
| `ui/packages/design-system/src/design-system/index.ts` | EDIT | Export the shared clock role |
| `ui/packages/design-system/src/index.ts` | EDIT | Export the shared clock role |
| `ui/packages/design-system/src/design-system/time-utils.test.ts` | EDIT | Verify UTC clock precision, locale and invalid-date handling |

### Whole-composition follow-up scope, Nov 12, 2026

Indy's notes are review feedback, not an exhaustive acceptance checklist. Compare the entire composition and all supported outcomes against the approved reference.

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/components/domain/FleetToolCalls.tsx` | EDIT | Restore shared disclosure hairline rhythm without a parallel tool renderer |
| `ui/packages/app/components/domain/FleetChatElements.tsx` | EDIT | Place native attachment chips before user prose |
| `ui/packages/app/components/domain/chat/UploadAttachments.tsx` | EDIT | Reuse native attachment identity for compact sent chips and bounded composer rows |
| `ui/packages/app/components/domain/chat/NativeTaskPrBadge.tsx` | EDIT | Present an explicitly associated saved pull request as a result card |
| `ui/packages/app/components/domain/FleetReplyBody.tsx` | EDIT | Place the native result association after the answer, retaining shared readers |
| `ui/packages/app/components/domain/fleetMessageRenderers.tsx` | EDIT | Add calendar boundaries through the native message index and shared Time |
| `ui/packages/app/components/domain/FleetTimestamp.tsx` | EDIT | Shared clock and calendar boundary presentation |
| `ui/packages/app/components/domain/chat/TaskPanel.test.tsx` | EDIT | Exercise closed work details, attention expansion, ownership and recovery |
| `ui/packages/app/components/domain/tool-call-activity.test.ts` | EDIT | Verify bounded counts while preserving mixed outcomes |

The duplicate unlayered screen-reader-only rule is removed so Tailwind responsive visibility can work. Conversation input minimum stays 60px; the reference's 100px rule applies only to its empty setup. Summary uses 248/228/216px widths above 680px. Provider errors use actual supported native failure tags in comparison records, with retry bound to the sample operator request.

### Composition verification and attention follow-up

Keep pending submission attachments on the SDK selector, suppress revoked header attention, and retain shared reader ownership. Routine settled assessment opens from existing run details; failure or unconfirmed assessment remains visible. Saved Fleet footers stay docked to the native viewport at every width, with Jump to latest above the composer. Empty setup footers remain in normal flow so a phone can scroll through the hero and starter prompts. Summary stacks in the phone conversation flow.

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/components/domain/FleetTimestamp.test.tsx` | CREATE | Verify and finish the approved composition and native state behavior |
| `ui/packages/app/components/domain/FleetReplyFigures.tsx` | EDIT | Verify and finish the approved composition and native state behavior |
| `ui/packages/app/components/domain/TaskActivityTimeline.tsx` | EDIT | Verify and finish the approved composition and native state behavior |
| `ui/packages/app/tests/fleet-markdown.test.tsx` | EDIT | Verify and finish the approved composition and native state behavior |

### Final composition verification scope

| File | Action | Purpose |
|---|---|---|
| `ui/packages/app/components/domain/FleetReplyFigures.test.tsx` | EDIT | Complete the approved composition and preserve native outcomes under asynchronous updates |
| `ui/packages/app/components/domain/TaskActivityTimeline.test.tsx` | EDIT | Complete the approved composition and preserve native outcomes under asynchronous updates |

The late native pull-request result reserves the same 78px card surface while its authorized detail loads. Browser verification retains the original 1px bound for the reply, sender and composer; no wider tolerance is allowed. Loading cannot reappear after an access-denied read. Summary rhythm is 8px around dividers with 4px label gaps; supported disclosure targets remain 44px on phones.

### Indy and Astra visual corrections — Nov 12, 2026

Indy's review reopens visual completion. Restore the approved neutral rail hover/selection, 20px collapse glyph, tiled mint identity, lavender avatar, 90% desktop user turn, 19px reply heading, 12px attachment filename, squared-pencil edit icon and composer insets. Result cards and their loading surfaces share 78px height and 16px radius. Preserve a visible focused composer and distinguish an empty disabled Send from an enabled draft. Compare those states separately, with the real model-selection owner mounted in the visual fixture.

Approval metadata and denial already exist in the backend. The earlier claim that they were unavailable was incorrect. Summary reads the pending gate for its own workspace and fleet, shows its recorded action and blast radius, offers Approve and Deny through the existing authenticated decision actions, and exposes action details inline. Reads and decisions must reject stale scope/results and show failures durably. A terminal result must not remain an actionable pending request. Recorded run duration belongs in the worked row; costs must identify the run they describe. Do not invent token-budget percentages, delivery receipts, repository diff totals or mention targets.

Additional files in this repair scope: `components/domain/chat/FleetApprovalSummary.tsx`, `components/domain/chat/use-fleet-approval.ts`, `components/domain/approvals/use-approval-decision.ts`, the existing approval detail `ResolveButtons.tsx`, `components/domain/chat/ChatQuote.tsx`, `lib/chat-message-text.ts`, `components/layout/MobileNavigation.tsx`, `components/domain/FleetRunCost.tsx`, the fleet page/header cost props, and focused tests beside their existing owners. Shell, transcript, shared tokens, approved visual fixtures and design-system documentation remain in scope.

### assistant-ui coverage and AskUser

The current official recommendation is `MessagePrimitive.GroupedParts`; retain it for reasoning and tools. Existing native thread/composer/message/attachment/Markdown/tool registration and copy controls remain the foundation. Use the Elements catalogue as the reference for Question Flow, Option List, Elicitation Form, Approval Card, terminal output, diffs, file trees, citations, artifacts and activity displays. Those components are UI compositions, not evidence that a backend capability is connected.

AskUser is not implemented end to end in the current setup protocol. A single choice should use Option List; related questions should use Question Flow. The required path is a typed persisted question with stable option identifiers, one authenticated answer, backend continuation, and a settled answer receipt that survives reload. A duplicate, stale or cross-workspace answer must not resume another request. Use native tool UI answer/resume hooks when the transport supplies that capability; never make a disabled backend path appear interactive. This remains explicitly unverified until that protocol and the reload/duplicate-answer journeys exist.

### Final async-result review repair

The shared task owner publishes its selected references alongside resources. Result placeholders now follow that actual pending work, not an absent map entry. A settled 503/invalid list, denied identity or completed task outside the 16-reference history window cannot remain labelled as loading. The 78px space remains for a real pending reader; existing 1px browser bounds are retained. Header insets follow 12/16/24px at the same narrow/intermediate/wide breakpoints as the approved shell.

Checkpoint evidence before integrating main 0619545: `make test-coverage-all` on the isolated UI candidate passed 4484 app, 133 website, 2217 CLI (17 declared skips), and 493 design-system tests at every declared 100 percent floor. Root targeted 103 tests and final 36 browser checks passed. Source/coverage manifests and logs are under `/workspace/scratch/refresh-astra-*`; captures are labelled sample data. These counts describe the pre-integration checkpoint, not the eventual merge revision.


### Approval review repair scope

Add `lib/api/approval-resolution.ts` and its boundary tests; update `lib/api/approvals.ts`, the shared decision hook, and existing approval response tests. Confirm the requested gate and a terminal outcome before recording success; a competing decision must carry the existing already-resolved error code. Invalid responses remain unconfirmed.

Update `lib/streaming/fleet-stream-entry.ts`, `fleet-stream-snapshot.ts`, `fleet-stream-dispatch.ts`, their existing tests, `components/domain/useFleetEventStream.ts`, `FleetThread.tsx`, and the shared task/Summary owner. A dedicated approval read revision advances for each gate hint and after reconnect; the task revision and unchanged-count optimization retain their separate meanings. Replacing one pending gate with another must refresh the action even when the total stays at one. Focused quote, run-cost and mobile navigation tests are included with their components.

The approval reader stays mounted for a scoped fleet with zero remembered approvals, so hello and reconnect can discover a first missed request. A confirmed empty read hides the approval section. `WorkSection.tsx` supplies the shared section presentation. Conversation headings use a dedicated 19px token in `FleetMarkdown.tsx`; dialog headings retain their approved 18px token.


### Current review repair files

The final source review found that native quote metadata was excluded from pre-submit byte-limit checks. Runtime tests also showed that the queue adapter restored failed quoted messages as plain text. The review's earlier duplicate-operation claim is withdrawn: it assumed an SDK restoration path that this queue bypasses. The shared serializer now supplies delivery, recovery observation, notice equality and the size limit.

A matching notice resend clears native quote and text together. Queue recovery restores native quote state and preserves a newer quote and draft. The focused run passes 46 tests across six files, including exact retry identity, native restoration, resend cleanup, complete-payload limits and newer-edit preservation. Evidence: `/workspace/scratch/refresh-quote-queue-focused-v2.log`; full merged-branch verification remains due.

Indy's latest eyeball response is "I have visual corrections". The final visual sign-off remains open while those corrections are collected; independent backend verification continues.

Nov 12 verification checkpoint: exact main `0619545b6d718fc629473a1ec6ad3e87ff1d639c` passed actual `make test-coverage-all` (app 441 files / 4261 tests; website 20 / 133; CLI 2217 passed / 17 declared skips; design system 62 / 488, every declared floor 100 percent). Its `make dry-app` passed 68 browser cases. Actual `make test-unit-rustd` under the normal `022` umask passed 1731 runner, 172 daemon and 3649 daemon-library tests, totaling 5552. Evidence: `/workspace/scratch/refresh-main061-baseline-coverage-v2.log`, `refresh-main061-baseline-dry.log`, and `refresh-main061-baseline-rust-unit-v2.log`.

Exact-base remote runs are [warm-cache](https://github.com/agentsfleet/agentsfleet/actions/runs/43156340957) and [deploy development](https://github.com/agentsfleet/agentsfleet/actions/runs/43156341221). Neither supplies executed unit counts for this baseline; the isolated checkout measurements above do. The development deployment run failed; no success is inferred from its build artifacts.

The current branch's first complete app run passed 4554 tests but missed eleven branches and two statements at the coverage floor. Added checks cover recorded duration, duplicate approval submissions, late decisions after unmount, failed zero-count inbox reads, and pending-count invalidation. Removed obsolete optional-key write branches after the existing mandatory-key validation. The full package gate is rerunning; this checkpoint is not a green branch claim.

Astra inspected all 57 distinct V8 captures and all ten V9 interaction images. The remaining recent-fleet selection issue is corrected, hover colors match in both themes, and reduced-motion labels are static and readable. V9 records loader motion; Astra inspected sampled video frames without claiming continuous playback. The review Page contains the fresh comparisons, navigation images and recording. These component captures use labelled sample data; authenticated journeys remain due.

| File | Action | Purpose |
|---|---|---|
| `rustd/crates/daemon/afd_authoring/src/catalogue/sql/page.sql` | EDIT | Preserve workspace model scope while integrating current main |
| `rustd/crates/daemon/afd_authoring/tests/draft/model_metadata.rs` | EDIT | Preserve workspace model scope while integrating current main |
| `rustd/crates/daemon/afd_credential/tests/integration_platform_key_cache.rs` | EDIT | Preserve workspace model scope while integrating current main |
| `ui/packages/app/app/(dashboard)/AccountSetup.tsx` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/approvals/[gateId]/ResolveButtons.tsx` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/approvals/[gateId]/page.tsx` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/domain/FleetRunCost.test.tsx` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/domain/FleetRunCost.tsx` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/domain/FleetToolGroup.tsx` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/domain/approvals/use-approval-decision.ts` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/domain/approvals/use-approval-decision.test.ts` | CREATE | Prove duplicate-submit exclusion and ignore late approval responses after scope unmount |
| `ui/packages/app/tests/fleet-thread/steer-quote-recovery.test.ts` | CREATE | Prove native quoted-message retry identity, resend cleanup and complete-payload byte limits |
| `ui/packages/app/components/domain/useFleetSteerQueue.ts` | EDIT | Restore native quote and text after uncertain delivery while retaining a newer quote and draft |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | Replace the removed primary-workspace bridge with exact credential workspace ownership |
| `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/model_entry/render/tests.rs` | CREATE | Move unchanged rendering tests beside their production module to keep file size bounded |
| `rustd/crates/daemon/afd_credential/tests/platform_key_cache/cases.rs` | CREATE | Keep cache integration cases separate from their fixture owner without changing assertions |
| `ui/packages/app/tests/e2e/chat-schedules.spec.ts` | CREATE | Keep schedule activity and reload browser checks separate from typography |
| `ui/packages/app/components/domain/chat/ChatQuote.test.tsx` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/domain/chat/ChatQuote.tsx` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/domain/chat/FleetApprovalSummary.test.tsx` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/domain/chat/FleetApprovalSummary.tsx` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/domain/chat/WorkSection.tsx` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/domain/chat/use-fleet-approval.ts` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/domain/useFleetEventStream.ts` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/domain/useFleetMessageDelivery.ts` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/layout/MobileNavigation.test.tsx` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/components/layout/MobileNavigation.tsx` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/lib/api/approval-resolution.test.ts` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/lib/api/approval-resolution.ts` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/lib/api/approvals.resolve.test.ts` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/lib/api/approvals.ts` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/lib/chat-message-text.test.ts` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/lib/chat-message-text.ts` | CREATE | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/lib/streaming/fleet-stream-dispatch.ts` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/lib/streaming/fleet-stream-entry.ts` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/lib/streaming/fleet-stream-registry.facts.test.ts` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/lib/streaming/fleet-stream-snapshot.ts` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/tests/account-setup.test.tsx` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |
| `ui/packages/app/tests/fleet-thread/steer-submission.test.ts` | EDIT | Complete the approved visual corrections and native component behavior with scoped recovery checks |


### Fleet-owned approval decisions (Indy correction, 2026-11-12)

“I dont need review, we agreed to nuke the approvals, grants page they will go away.”

Remove the standalone Approvals collection/detail routes and their navigation, loading screens and page-only components/tests. There is no standalone Grants route in this checkout. Retain approval APIs, standing grants, scope checks, audit history and CLI operations. Fleet Summary owns Approve and Deny; no Review navigation or replacement inbox. Show the requested action and blast radius before the buttons, with evidence available inline. Keep refusal, uncertain response, competing decision and expired outcomes explicit. Other pending requests remain reachable after a decision. Approval activity and readiness links target the fleet itself.

Additional files: the existing `app/(dashboard)/w/[workspaceId]/approvals/**` tree (delete); `lib/actions/fleet-approvals.ts` (move authenticated actions); `components/domain/chat/FleetApprovalSummary.tsx`, `ApprovalActions.tsx`, `use-fleet-approval.ts`, `WorkContextSections.tsx`, `WorkSection.tsx`; `components/domain/FleetActivityElements.tsx`; `components/layout/account-menu-data.ts`; `lib/workspace-routes.ts`; fleet page/status and existing tests; removed approvals page/list tests, retained action tests; acceptance `operator-journey.spec.ts`, `multi-fleet-grant-journey.spec.ts` and their approval fixture; browser action aliases and visual fixtures; `rustd/crates/daemon/afd_authoring/src/review/credentials/mod.rs` and existing grant readiness tests. Refresh the visual gallery and companion docs to describe fleet-local decisions. This supersedes earlier inbox-retention wording.

Setup approval placement also changes `DraftConversation.tsx` and adds `SetupApprovalSummary.tsx` with a scoped identity test. Backend readiness returns a fleet destination only after Save has created it. Before Save there is no decision to make. `dashboard-performance.spec.ts` uses Models as its surviving intermediate destination.


The final inline-decision review found a stream-before-response race: the backend announces a resolved gate before returning its HTTP receipt. Preserve the selected request across same-scope count/revision updates until its confirmed response is shown. Access loss still removes it and ignores late responses. Setup Summary receives the existing authenticated subject from `WorkspaceChat.tsx`; it does not add another identity-provider hook. Add regression tests for both orderings.

After a confirmed setup approval, the existing DraftClient rereads readiness. Finish setup becomes available when the authoritative checks permit it; approval itself does not activate the fleet. Failed readiness reads retain the client's existing durable error. The shared RefreshButton lifecycle test replaces incidental coverage lost with the removed page tests (`tests/identity-and-controls.test.tsx`).

The final readiness review adds `tests/workspace-chat/fleet-setup.test.tsx` to the repair scope. A confirmed decision queues a scope-tagged refresh while the existing DraftClient is busy; a client, user, fleet or access change discards that intent. The pending label and badge become Approval decision after a confirmed terminal receipt. Clippy requires the extracted rendering module to move from `render.rs` to `render/mod.rs`; the tests remain in `render/tests.rs`.

Indy requested final visuals for every other page on Nov 12. Capture Models and its dialogs, Profile, Security, Team/invitations, Usage, API keys, Secrets, Connected apps, Plugins and New fleet at desktop and phone widths in both appearances; inspect and repair the actual components before publishing the gallery. Finish the visible Plugins heading, scope label and read-error copy in the existing library copy/browser files and their existing page/browser tests. Public route/CLI renaming remains separately tracked.

The all-page visual review also covers `settings/api-keys/components/ApiKeysView.tsx` and `tests/api-keys-page.test.ts` for consistent API keys casing. Use the shared title and canvas spacing for Usage in `settings/billing/page.tsx`, `app/globals.css` and the matching `resource-pages-probe.tsx`. In the existing model credential/edit/view components, shorten placeholders and remove unavailable Next guidance. In the shared secret form and name cell, give phone values adequate input width and show complete names. These are copy/layout corrections; resource ownership and writes stay unchanged.

All-page visual checkpoint, Nov 12: `other-pages-v14/results.json` records 128 captures (32 states, 1480px/390px, dark/light), no browser errors and no page horizontal overflow. Astra inspected all 104 initial captures and 68 repaired/additional captures; all six numbered findings and the copy fixes are closed in `astra-visual-validation.md`. The complete archive includes an HTML gallery, unmodified PNGs, measurement results and image digests; its 128 image links, selectors, actual-size control and phone fit passed the artifact check. The review Page contains 25 representative images plus that archive. These are production-component captures using sample records, not authenticated acceptance. Indy’s eyeball feedback, final repository checks and pull requests remain pending.

The companion docs check named two stale Runner API navigation entries in `docs.json`: GET/PUT context-checkpoints. Neither exists in the current main OpenAPI or the daemon route source at `0619545`. Remove those two obsolete navigation references without changing the checker or adding API pages ahead of the product merge.

The companion documentation PR is [docs#245](https://github.com/agentsfleet/docs/pull/245), head `0d26c5b`. Local verification and PR gates passed; the isolated docs base and branch each pass 25 documentation tests. The model edit placeholder assertion in `tests/models-registry-edit-remove.test.tsx` now checks the reviewed Enter replacement key wording. Its title describes the actual missing-credential case; the form and mutation assertions remain unchanged.

Nov 12 verification checkpoint: `make lint-all` passes, and the three Rust unit lanes pass 1,731 runner, 172 daemon and 3,650 library tests. After the placeholder assertion repair, `make test-coverage-all` passes: app 4,517, website 133, CLI 2,217 and design system 493 tests. Every package meets its 100 percent coverage floor. Evidence: `refresh-final-full-v12.log` and `refresh-final-remaining-v13.log` in the task scratch directory. Integration, browser and authenticated checks remain in progress.

Docs PR #245 completed two quiet green polls on `0d26c5b`; Greptile reported 5/5 with no findings. Indy requested visual corrections and the original proposal ZIP for comparison. The review Page now provides `approved-proposal-bundle.zip`, containing the unchanged HTML and exact extracted assets. Its offline conversation, Models and Team views passed the browser check without remote requests. Visual sign-off stays open until the corrections are supplied and addressed.

Integration exposed a helper error in `creation/grants.rs`: draft readiness uses typed `CheckKind::Grant`, while saved setup aggregates use the plural grants label. The helper now checks the typed draft observation and its absent pre-save destination; saved-destination and approval/denial assertions are retained. Update the adjacent action documentation in `src/types/review/readiness.rs` to name inline fleet controls. The wire enum spelling and authorization stay unchanged.


### Whole-proposal rebuild after Indy’s comparison, Nov 12

Indy: “Well you will have to check all not just the ones we mentiond”. The original unchanged `proposal.html` is the comparison source for every screen, shared control, dialog and interaction state. The earlier all-page review did not establish structural parity and does not close visual sign-off. Restore the approved resource layouts, shared controls, Security composition, and New fleet composition. API keys and signed-in sessions share Security in the UI; CLI API keys stay independent. Preserve profile editing through the avatar’s account controls without presenting an unapproved separate Profile page. Preserve real authorization, recovery, model selection and billing facts; sample-only fields are never invented in production.

Repair scope covers the existing design-system primitives, source tokens and utility mappings; all current account/resource view components and their existing tests; resource shell and account navigation; native setup composer and Fleet sample fixtures. New small composition components belong beside their first caller in the same directories. Update DESIGN_SYSTEM.md and comparison captures together. Include every proposal view, dialogs, empty/loading/error cases, hover/focus, both themes, and phone checks. Replace duplicate gallery captures with genuinely distinct states. Show Activity and Wake-up with recorded sample events through production components. Compare at 1480×1000, 100 percent zoom, with matching 46px review chrome.


### Proposal alignment implementation and data differences — November 12

The owner selected the approved proposal structure for every resource page. The visual reference remains the original `proposal.html`, SHA-256 `8feabc24f6d6e91d2caa22a4b52060074f2877b291fce4d2f1da5c3fc6d8be5a`. The review covers all pages, dialogs, Fleet Activity and Wake-up states, both themes, phone layouts, hover and focus. Completion requires new rendered evidence; previous captures do not certify the rebuild.

Shared controls now use the approved border, shape, spacing and disabled state. Models restore Platform default and Your models. Team restores avatar, role, date and menu rows. Security combines keys and sessions; personal identity edits remain native provider controls. Usage restores its metric strip, chart section and fleet table. Secrets, Plugins and Connected apps restore their grouped row structures. New fleet uses the approved hero, writing pad and starter pills.

The data differences must accompany the gallery:

- Billing exposes settled period totals and grouped charges, without UTC daily buckets or a complete model set per fleet. The daily section shows an unavailable state. Activity and per-model charge inspection remain available in Charge details.
- Secret storage is workspace scoped and write-only. The API does not provide arbitrary per-secret fleet authorization checkboxes; Manage exposes actual replacement and deletion with a clear access-data limitation.
- API key rows have creation and revocation state but no key suffix or last-used timestamp. Do not synthesize either field.
- Plugin cards have an immutable head identifier. The details read returns the human version and supports pinned use for a new fleet; owned setup editing uses an explicit publication target.
- Conversation sections use recorded events. Slack delivery, exact diff totals, token-budget percentages and mention behavior must not be invented to match sample text. Capability composition remains deferred to M239.

Wake-up controls read the current schedule, carry its ETag for edits, preserve inherited overrides, distinguish pending deregistration from deletion, and cancel late responses when their owner changes. A stale edit offers a reload of saved intent. Security bulk sign-out reads all pages before writing, excludes every session identified as current, and stops on a refusal without claiming complete success.


### Final proposal comparison repairs — November 12

Indy: “Align to your proposal, the delta data can be highlighted by you.” This supersedes the extra resource-page search/sort/table structures from earlier checkpoints. Keep real timestamps, resource ownership, backend refusals, and pagination needed to reach all records. The approved Security composition owns both API keys and sessions. Personal account opens native identity-provider controls through the auth facade.

Astra inspected all 94 V18 captures individually, comparing all proposal pages and both appearances. Its bounded findings were the desktop wake-up Summary inset, the no-tool worked row side borders, the Model label gap, and Usage metric/section rhythm. Repairs use shared spacing and type roles. Separate top/latest long-conversation captures distinguish normal runtime scrolling from component geometry; focused dialog captures verify the label-to-input clearance.

The code review closed a private wake-up-message portal exclusion, restored the available key revocation date, and corrected an obsolete plugin-action assertion. Browser journeys now follow Security, native personal account controls, plugin management dialogs and Models Load more. Tests retain key reveal-once and revocation, hidden-secret behavior, accessible actions, fixture cleanup, exact dates, delayed reads and scope checks. The removed Search control no longer has a search-pagination journey; the load-more journey proves retained rows, pending controls and unique complete results. Unit checks retain superseded-response protection.

Outputs require a confirmed publication receipt with a commit and matching pull-request URL. A related pull request without that evidence appears under Pull requests. Headers derive Scheduled and Needs attention from actual schedule/run data and keep approval attention on the existing shared stream reader. Wake-up edits remain owned by authenticated subject, workspace and fleet even when schedule ordering changes.

Resuming an unpublished plugin edit cannot infer the intended publication destination from its source revision. An explicit owned-plugin picker lets the user select the destination and then publish its next revision through the existing conditional API. Selection alone never writes. The API does not currently remember that unpublished destination across reloads; report this limitation beside the visual comparison.

Full final repository gates, current browser evidence and Indy's eyeball review remain open. Earlier successful gate counts describe their recorded revisions only.

### Current comparison and exact-base evidence, Nov 12

The V20 comparison contains 160 distinct production-component captures, with no browser errors, duplicate image digests or horizontal page overflow in its measurement report. Astra inspected all 160 V19 images and the 20 repaired V20 dialog/error images; all blocking visual findings are closed in `proposal-full-audit.md`. The unchanged proposal hash above remains the reference. The review Page contains desktop/phone comparison ZIPs, inline screenshots, Activity and Wake-up, the loading recording, and API/data differences: https://chatgpt.com/space/page_2523a232bea081919750d090aaba00e3. Sample records remain explicitly labelled. This proves visual comparison, not authenticated runtime acceptance or Indy's final sign-off.

The self-contained gallery check loaded all 160 images and the original proposal, exercised actual-size and phone navigation, and recorded no errors or page overflow (`/workspace/scratch/verify-proposal-gallery-v20.log`). Desktop and phone archives each retain the original proposal, references, captures, measurements and reports and fit the 10 MiB download limit.

The exact current merge base is `e8a5e8022a013403ed47bffa1c599ba9a572e37a`. Its isolated checkout ran the Rust unit lane: runner 1,731, daemon 172 and daemon libraries 3,669 passed, total 5,572 (`refresh-maine8-baseline-v22.log`). The current exact-base package and browser run is `make test-coverage-all dry-app`: app 4,261 tests in 441 files, website 133 in 20, CLI 2,217 passed with 17 skipped, design system 488 in 62, and 68 browser checks passed (`refresh-maine8-baseline-v29.log`). All package coverage floors passed. An earlier package invocation inherited `NEXT_PUBLIC_POSTHOG_ENABLED=false` and failed 29 analytics expectations; the repeat unset that task-level flag and passed without source or test changes. Base [warm-cache](https://github.com/agentsfleet/agentsfleet/actions/runs/43189195448) compiled tests; it is not the executed baseline. The later [development deployment](https://github.com/agentsfleet/agentsfleet/actions/runs/43200064821) passed and is recorded separately from these local counts.

The bounded source rereview of session sign-out, wake-up controls and plugin destinations found no remaining defects (`final-standards-rereview.md`, 14:28 UTC). It checked disposal epochs, duplicate exclusion, all-page discovery before bulk writes, retention of every current-device identifier, partial confirmed removals, conditional schedule writes and explicit owned-plugin destinations. Full current branch checks remain in progress; these review and baseline results do not declare them green.

The current app run found an API-key receipt dismissal race: a confirmed key could render while React still reported a concurrent Action as pending, leaving Done enabled but ignoring it. The creation receipt now ends the dismissal lock; Escape/outside-click still protect the one-time key. A deterministic native React concurrent-Action test fails against the prior guard and passes with the fix. The complete dialog suite passes 13 tests (`refresh-key-close-repro-v43.log`, `refresh-key-close-fixed-v44.log`). Model price evidence retains the exact source timestamp in the native title alongside the shared readable UTC date; its normalized datetime remains owned by `Time`.

Verification repair scope also includes `tests/appearance-menu.test.tsx`, `tests/authoring-intent.test.tsx`, `tests/fleet-markdown-dynamic.test.tsx`, `tests/plugin-publication-selection.test.tsx`, `components/domain/chat/use-wakeup-control.test.tsx`, `components/domain/ModelPriceSchedule.test.tsx`, and the existing Team, API-key, wake-up and work-context tests. These prove native menu interaction, scoped intent/destination, loading handoff, original timestamp availability, one-time key dismissal, cancellation and deduplication. The only additional composition cleanup makes the sole caller supply ChatComposer's empty state and lets ConfirmDialog own pending deletion dismissal. No gate threshold or retry policy changes.

The current application coverage run passes 4,547 tests in 477 files with 100 percent statements, branches, functions and lines; website and CLI coverage also passed (`refresh-proposal-checks-v48.log`). That command then stopped on 16 design-system assertions describing superseded tint, border, radius, control size and heading markup. The five existing Badge, Button, Card, Input and SectionHeader test files now assert the immutable proposal's approved appearance. `make lint-design-system test-unit-design-system` passes 494 tests with source and type checks (`refresh-ds-dry-v49.log`). The bounded reviewer confirmed these assertion repairs preserve behavior checks and weaken no gate. Canonical design-system prose now agrees on outlined label chips, neutral secondary actions and control heights. Full branch verification remains open.

Indy's latest review reply requests a prompt for Claude to inspect the comparison ZIPs. The supplied prompt covers every page and state, measured visual differences, responsive behavior, hover/focus evidence and documented API/data limitations. It is not visual sign-off.

`make dry-app` now passes on the current working tree: 4,547 app tests and 90 desktop/phone browser checks (`refresh-ds-dry-v49.log`, exit 0). The browser lane retains upload/refusal recovery, lifecycle confirmation, shared type and motion, schedule controls, resource dialogs, native scrolling and bounded output rendering. The exact-base browser count is 68, so this branch adds 22 checks. The new Checkbox and RadioGroup package tests exercise accessible selection, disabled refusals and caller-owned controlled state; SectionHeader tests cover its optional description and paragraph semantics.

The final shared-package coverage check passes 502 tests in 64 files with 100 percent of 564 statements, 606 branches, 191 functions and 508 lines (`refresh-ds-coverage-v51.log`); `make lint-design-system` also passes (`refresh-ds-lint-v52.log`). The added UTC creation-date case covers the real Security/Team date format across an offset midnight boundary. These package results do not replace the final repository gate.

V21 recaptures all 160 states with zero browser errors, duplicate images or horizontal overflow. The offline gallery loads every image and the original proposal (`verify-proposal-gallery-v21.log`). Astra confirmed 132 images are byte-identical to V20, 27 have at most 109 changed edge pixels, and the remaining secret-dialog difference is its Name-field focus outline. All 28 model captures are byte-identical. The review Page now contains the V21 desktop/phone archives, current loader recording, matching inline images and verification limits; upload access and Page readback are confirmed. The original proposal remains unchanged. No new material layout finding or owner sign-off is claimed.

The exact-base integration run finishes with 1,759 passed and one failed: `integration_runner_stream::recovery::a_blackholed_native_runner_reconnects_and_settles_its_memory_report_once` times out waiting for the real tool process at `recovery/runner.rs:67` (`refresh-maine8-integration-v46.log`, make exit 2). The exclusive lane did not run. This is measured on isolated base `e8a5e8022`, not attributed to the UI diff. Final branch integration remains required.

The current authenticated build completed both Rust executables, CLI and optimized Next.js build (`refresh-auth-current-v53.log`). Startup then refused an old task database containing removed migration `20261111230000`. The task helper now selects a fresh isolated development database and a private copied environment without the previous database's platform-workspace ID. Existing databases and the repository schema remain untouched; no backfill or compatibility path was introduced. Authenticated checks continue against that fresh schema.

The fresh-schema authenticated run reached all 20 selected journeys: 14 passed and six failed (`refresh-auth-current-v56.log`). Secrets exposed an unnamed Actions column and lost keyboard focus when management handed off to replacement/deletion. The header now has a screen-reader label; the native Dialog close lifecycle restores the lasting Manage trigger before opening the next dialog and cancels that handoff if the row disappears. The cancellation regression failed before the fix, then both Secrets suites passed 26 tests (`refresh-secret-focus-red-v57.log`, `refresh-secret-focus-green-v58.log`). An added removal test covers navigation during the handoff. The remaining failures expected the previous Usage layout, button/switch model selectors, or an empty-state send label despite a loaded plugin preview. Their selectors now follow the approved controls while retaining data, activation, pinned-revision, accessibility, geometry and cleanup assertions. Bounded source rereview found no new defect; current app lint passes. The authenticated rerun and final repository gate remain open.

Claude's first-cut review is retained at `/workspace/artifacts/fleet-refresh-v2/claude-first-cut-review.txt`. Its comparison used V21 desktop and V20 phone captures and leaves light-theme dialogs, loader playback, lifecycle and account-menu coverage open. It is a review input, not owner acceptance. Confirmed first repairs cover dialog title/close alignment, checkbox corners, label weight, neutral successful action glyphs, command monospace, compact wake-up labels and glyphs, phone filter width, composer action alignment, the redundant model tooltip, plugin description/options, and the missing recorded-changes flap. The flap reuses the Summary's recorded-write projection and counts only confirmed local writes, with no invented line totals. Team's production self marker already exists; sample member `owner` did not match sample authenticated `fixture-owner`, so that capture fixture now uses the correct identity. Precise residual spacing, missing lifecycle/account captures and the remaining first-cut findings stay open until remeasured; no blanket visual closure is claimed.


### Required final checks, Nov 13, 2026

> Owner: "Ensure typescript is 100% coverage - test, lints pass, dry-app test, bundle-size (if you pass the limits inform indy and get a signoff) and acceptance-codex passes."

Run the actual acceptance-codex target and both package bundle-size checks. Keep all TypeScript coverage floors at 100 percent. A bundle-budget increase requires Indy's separate approval after exact measured sizes are reported. No increase is approved by this instruction.


### Final performance verification, Nov 13, 2026

The exact main base `4f3159269` passed `make test-unit-all` under verified Tini, `umask 022` and `RUST_TEST_THREADS=1`. Rust counts: runner 1,731, daemon 172, libraries 3,669; total 5,572. Package counts: app 4,271, website 133, CLI 2,223 with 17 existing skips, design system 488. Every package coverage floor passed (`refresh-main4f-unit-serial-v97.log`).

Earlier unsupervised base tests left zombies under the container's `tail` PID 1. Tini closed those two cleanup failures. A subsequent parallel base run met `AddrInUse` in the IPv4 dual-transport test. The final full lane used serial test scheduling; it retained all selections and each test's internal concurrency. No test, timeout, assertion or gate was weakened.

Before the final performance changes, feature package coverage passed: app 4,564; website 133; CLI 2,223 plus 17 existing skips; design system 502. All available coverage metrics are 100 percent (`refresh-final-ui-checks-v89.log`). Its browser portion passed 88 checks and failed two obsolete dark loader-color assertions. Updating only that exact expected color passed both desktop/phone cases, retaining motion, reduced-motion, forced-color and geometry assertions (`refresh-loader-browser-v94.log`). The final full dry rerun remains required.

The authoring context now imports React only. Its native assistant-ui prompt hook lives inside the existing dynamic conversation. Setup presentation loads only after confirmed setup data exists, and the lifecycle dialog loads after an explicit action. DraftClient, validation, action confirmation and identity owners stay in place.

A mobile-dialog experiment reduced shared bytes but moved dialog code into saved-fleet incremental bytes. The final candidate retains the original native mobile navigation and avoids that tradeoff. Current app measurements: shared 292.30 kB against 276.48 kB; New fleet 87.38 kB and saved fleet 98.86 kB against 102.40 kB. Other route limits pass. Indy subsequently approved only the shared cap at 288 KiB: "Approve shared cap of 288 KiB". His follow-up, "but if there is a way to reduce i recommend that", keeps reduction work in scope. The shared limit is now 288 KiB (294.91 kB); other limits remain unchanged. The fresh size check passed at 292.30 kB / 294.91 kB (`refresh-app-bundle-size-v113.log`); `refresh-app-bundle-size-v107.log` records the earlier limit.

Website build and size pass: JavaScript 109.71 kB against 128 kB, CSS 19.17 kB against 20 kB (`refresh-website-bundle-size-v104.log`). Estimo's browser cache selects the existing automated-test Chromium; the managed browser refused the benchmark's local file. The runtime cache change preserves actual measurement and leaves package source, budgets and timing checks unchanged.

The full merged-candidate lint passed. Rust unit shards passed 5,573 tests (runner 1,731, daemon 172, libraries 3,670). The package portion found a synchronous first-open lookup in the lifecycle-dialog cancellation test; waiting for the visible native alert dialog preserves all cancellation/no-write assertions and passes the full 17-test file. Complete package coverage and repository checks are being rerun. Evidence: `refresh-root-unit-v111.log`, `refresh-lifecycle-lazy-wait-v115.log`.


### Local acceptance health and verification, Nov 13, 2026

File inventory adds `deploy/local/codex-compose.yml`. Its Caddy health probe disables the HTTP proxy only for the fixed loopback admin request. Container egress, certificate trust, health URL, retries and timeouts stay unchanged. The existing container passed that exact probe with `wget -Y off`; inherited proxy routing failed.

The complete TypeScript coverage target passes on `b22d51c91`: app 4,564, website 133, CLI 2,223, design system 502. Every enforced coverage metric is 100 percent. CLI retains its 17 existing live-lane skips. Evidence: `refresh-typescript-coverage-v121.log`, exit 0.

The repository gate passed integration with 1,772 tests, lint and version checks. Two runner process-cleanup tests failed under the detached supervisor. A verified Tini recipe shell then passed the unchanged native runner target: 1,731 tests. This runtime setup preserves all commands, assertions, test selections and deadlines. Evidence: `refresh-pr-gate-v122.log` and `refresh-runner-reaper-v125.json`. The full gate still requires a fresh run on the pushed candidate.


### Runner receive failure found during final verification, Nov 13, 2026

The lower-parallelism gate exposed a QUIC failure during sustained-outage shutdown. The runner returned a fatal handshake error instead of preserving the receiver's original network cause. Evidence: `refresh-integration-v129.log`, `recovery/runner.rs:91`. No test assertion or timeout changes are authorized by this repair.

Bounded repair inventory: `rustd/crates/protocol/af_stream/src/h3/client.rs` and `rustd/crates/protocol/af_stream/src/h3/client_tests.rs`. Reproduce the competing ready errors deterministically before changing production code. Preserve certificate, key-exchange and protocol refusals. Preserve the original receiver cause only when the native worker has stopped and the handshake wrapper has erased its type. The existing connection and supervisor remain the sole transport and retry owners.


The regression failed before repair: the competing ready handshake wrapper was classified as TLS instead of the receiver's network failure. A second regression exposed erased post-negotiation refusals. The existing key-exchange watch now excludes identity checks and H3 initialization from reconciliation. Nine focused tests pass, including typed refusals, successful receiver completion, cancellation and the unchanged deadline. Source review closed its security finding; full repository verification remains required. Evidence: `refresh-handshake-race-red-v131.log`, `refresh-handshake-security-red-v133.log`, `refresh-handshake-security-green-v135.log`.

### Claude final visual verdict, Nov 13, 2026

Owner supplied the completed review in `claude-final-verdict-v22.txt`. Restore the approved Changes pill and account-menu hierarchy. Capture actual archived history and the post-delete destination. Correct remaining composer, wake-up, activity, Secrets, gutter, evidence-row, focus and footer spacing differences. Personal account follows the shared resource-header pattern. Keep Personal account access and workspace creation/switching available through the existing scoped owners.

This repair extends the existing visual inventory to the shared menu, dialog focus, composer and table primitives, account header and lifecycle capture fixtures. Preserve recorded file counts and native assistant-ui, Radix, authentication and mutation owners. New evidence must record revision, time and UI source digest, including real authenticated account captures. No old toast-only image establishes archived or deleted behavior.


### Pre-submission gate requested by owner

Validate all eight sections of the owner's November 13 checklist. Package only
when every required check passes; missing evidence remains NOT RUN. Read the
unchanged proposal at matching viewports and record computed geometry, rather
than inferring dimensions from screenshots. Every sample, loader and authenticated
capture must identify the same clean candidate commit, source-tree digest and
UTC capture time. Hash all archive payloads, including reports and index. Any
subsequent source repair invalidates its affected evidence.

Additional bounded inventory: shared `dialog-focus.ts` and its regressions,
`visual-provenance.ts`, `visual-lifecycle-notice.tsx`, lifecycle journey evidence,
workspace-menu grouping/focus and creation regressions, task-history read-only
regression, shared sentence-case labels and `docs/DESIGN_SYSTEM.md`.

Internal development measurements match the requested composer dimensions and
Changes pill height. Both-theme motion frames differ by 288–452 pixels; all
four reduced-motion pairs are byte-identical. These dirty-source development
captures are excluded from final evidence. Source review found and closed a
closing-dialog focus race; its two regressions failed before repair and pass
with the existing dialog tests (23 total). Final clean captures and all required
repository/browser gates remain pending.


### Owner-approved combined account destination

Owner: “The workspace is  fine”; “The Personal account and Security must be
merged, and renamed may be to API Keys?”; “SEcurity renamed to API Keys”.
Retain workspace switching/creation in the identity submenu. Remove the separate
Personal account route/menu item. Label the existing keys/sessions destination
API Keys and retain its existing operations. A Profile and sign-in settings action scrolls to the always-mounted native-control section
on the combined page through the existing auth facade; password
availability follows the existing provider configuration. Additional inventory:
`components/account/AccountSettingsAction.tsx`, account controls and authentication
accessibility journeys; remove the former account catch-all page/loading route.
Record the changed title, menu entry and sign-in launcher as explicit approved
differences from the immutable proposal. No backend API or access-scope change.

Native account settings use the supported embedded `UserProfile` with hash
routing on the combined page. Its heading and region remain mounted during
verification. Do not nest it in another application modal or restore the earlier
native profile modal whose password editor lost its accessible name. The header
action only scrolls/focuses this section, preserving native verification hashes.
