# App interface and CSS audit

The app baseline is main at `e96444ac9`. This audit maps all 21 routes to their components and styles. [App design reference](../ux.html)

## App-wide page loading

The approved Single Relay sends a pulse between three nodes around the brand mark. The dashboard keeps its sidebar and header visible while the full-screen variant centers the status in the window. [Dashboard](../ux.html#app-loading) · [Full screen](../ux.html#app-loading-full)

Reduced motion keeps the 44-pixel mark and “Loading…” label visible with the pulse at rest. `RouteLoading` uses the shared Spinner's page variant for navigation, initial data and connector return, alongside a compact spinner for buttons and forms. [Page loading design](../DESIGN_SYSTEM.md#page-loading) · [RouteLoading](../../ui/packages/app/components/layout/RouteLoading.tsx) · [Connector loading](../../ui/packages/app/app/connectors/loading.tsx)

## Create model proposal

The HTML preview checks saved keys and then shows the form with Fable 5.1 selected. An info button beside **Model** opens context size, rates, sources and the expandable price schedule. [Interactive flow](../ux.html#model-proposal) · [Loading](../ux.html#model-loading) · [Details](../ux.html#model-details) · [Catalogue evidence](model-catalogue/catalogue.md)

### Loading and readiness

- The current dialog opens immediately and requests saved keys and the model catalogue. The form appears after the first saved-key read, even if the catalogue is still loading. [Dialog](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/models/components/AddModelEntryDialog.tsx#L37) · [Saved-key read](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/models/components/use-stored-secrets.ts#L42)
- The proposed loading row uses the existing compact Spinner and a Skeleton that reserves form space. Show one “Checking saved keys…” status, with Cancel and Retry in the relevant states. [Shared status pattern](../../ui/packages/app/components/domain/shared/dialogs/IntentDialogStatus.tsx)
- Show “Loading models…” beside the picker until choices are available. Keep ready options usable during a refresh, along with the exact-ID path and current “Saving…” button status. [Catalogue provider](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/models/components/ModelCatalogueProvider.tsx#L46) · [Form](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/models/components/CreateModelForm.tsx)

### Model details

The current disclosures expand beneath the model picker and move the credential fields down. Open the card on click, tap or keyboard activation and name the action with a tooltip on hover or focus. [Current picker](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/models/components/ProviderModelSelect.tsx) · [IconAction](../../ui/packages/design-system/src/design-system/IconAction.tsx) · [Popover](../../ui/packages/design-system/src/design-system/Popover.tsx)

Reuse the catalogue details, price schedule and source components to show each rate, tier, date and condition. Escape dismisses the card first and returns focus to its button. [Catalogue details](../../ui/packages/app/components/domain/models/CatalogueModelDetails.tsx) · [Price schedule](../../ui/packages/app/components/domain/models/ModelPriceSchedule.tsx)

Changing models should refresh the card's details. The registry details dialog has separate rules for rates and context caps saved on an entry. [Registry details](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/models/components/ModelDetailsDialog.tsx)

## Fleet Setup and Conversation performance

The comparison ran 40 round trips through the app's chat components in local Chromium. Each build had five runs per flow at 1440 × 900 and 390 × 844. Setup resumed with 20 settled messages and Conversation opened with 40 settled turns.

The simulated server sent an HTTP receipt after 40 ms. It began 12 reply updates at 150 ms and sent the completion event at 380 ms. All three delays start at the POST.

The browser measures elapsed time from Send. Reply timings include two animation frames after the text appears. The fixture uses simulated model replies and saved history.

| Flow and viewport | Acceptance, ms | First reply, ms | Completed reply, ms | Frame interval p95, ms |
| --- | ---: | ---: | ---: | ---: |
| Fleet Setup, desktop | 59.0 → 59.3 | 210.7 → 193.8 | 444.2 → 444.3 | 33.2 → 16.8 |
| Fleet Conversation, desktop | 81.3 → 80.6 | 224.7 → 212.1 | 457.9 → 461.4 | 16.7 → 16.8 |
| Fleet Setup, 390 px | 57.8 → 58.2 | 193.0 → 193.0 | 437.1 → 435.6 | 16.7 → 16.8 |
| Fleet Conversation, 390 px | 82.8 → 84.1 | 216.9 → 225.6 | 461.6 → 468.9 | 16.8 → 16.8 |

Values are before → after medians. The frame column is the median of each run's 95th-percentile animation frame gaps. Completed replies stayed within 8 ms of baseline in all four groups.

Desktop Setup showed shorter frame gaps in this five-run sample. Real runner, model and database delays fall outside the fixture. These samples precede the final markup and icon cleanup.

| Fixture asset | Uncompressed bytes | Gzip bytes |
| --- | ---: | ---: |
| Compiled app CSS, including embedded test fonts | 812,940 → 799,238 | 523,760 → 521,509 |
| Minified chat fixture JavaScript | 2,268,828 → 2,275,187 | 697,606 → 699,588 |

Compressed CSS decreased by 2,251 bytes and compressed JS grew by 1,982 bytes. The combined size fell by 269 bytes for the fixture build. Production route sizes vary with their build output.

Both chat flows use `useMemo` for derived messages and runtime adapters and `useCallback` where stable functions are needed. Fleet Conversation also memoizes the viewport and limits which subscribers see text updates. [Conversation](../../ui/packages/app/components/domain/conversation/thread/FleetThread.tsx) · [Setup](../../ui/packages/app/components/domain/setup/conversation/DraftConversation.tsx)

The tests require zero composer or viewport renders across 100 text flushes. Setup reuses parsed Markdown for settled messages across ten reply updates. [Render isolation](../../ui/packages/app/components/domain/conversation/thread/FleetThread.flush.test.tsx#L209) · [Setup history](../../ui/packages/app/tests/draft-stream-rendering.test.tsx#L27)

The chat browser test requires at most 1 px of movement during 25 streamed chunks and viewport resizes. Desktop and phone fixtures also cover the sticky composer, Summary, typography, touch, keyboard and **Jump to latest**. [Scroll and typography](../../ui/packages/app/tests/e2e/chat-typography.spec.ts#L111)

### Chat optimization priorities

1. **Conversation Send at phone width** took 26 ms longer than Setup to reach acceptance with the same simulated delay. Use the existing send and receipt marks alongside timestamps for the optimistic message and POST dispatch. [Send marks](../../ui/packages/app/lib/streaming/send-marks.ts) · [Fleet delivery](../../ui/packages/app/components/domain/conversation/delivery/useFleetMessageDelivery.ts)
2. **Setup first send and Save** need separate measurements, beginning with model readiness and first reply in a fresh draft. Measure Save with the separate request that can add dashboard tools. [Setup send](../../ui/packages/app/lib/api/drafts.ts#L125)
3. **Long transcripts** need a phone profile with active tool output and an open Summary. Keep the render and scroll bounds as regression checks when changing message rendering. [Thread viewport](../../ui/packages/app/components/domain/conversation/thread/FleetThreadViewport.tsx) · [Browser scroll checks](../../ui/packages/app/tests/e2e/chat-typography.spec.ts)

## How app CSS is organized

The app uses the shadcn New York setup, Tailwind tokens, Phosphor icons and React Server Components. At baseline, 181 of 264 production TypeScript component files imported the shared design system directly. That includes Button in 98 files, Alert in 29, Dialog in 22 and Spinner in 15. [shadcn config](../../ui/packages/app/components.json) · [Shared audit](design-system-css-audit.md)

| Style owner | Responsibility |
| --- | --- |
| [`app/globals.css`](../../ui/packages/app/app/globals.css) | Tailwind, shared and app tokens, type, scrollbars, reduced motion, mobile form sizing, Clerk overrides and app shell rules. |
| [ChatThread](../../ui/packages/app/components/domain/shared/chat/ChatThread.tsx), [ChatComposerFrame](../../ui/packages/app/components/domain/shared/chat/ChatComposerFrame.tsx), [FleetWorkSidebar](../../ui/packages/app/components/domain/shared/summary/FleetWorkSidebar.tsx) | Viewport and sticky footer, composer surface and input sizing, and responsive Summary layout. Activity, failures, and resource disclosures own their spacing and typography. |
| [Billing](../../ui/packages/app/app/%28dashboard%29/settings/billing/components/BillingReportView.tsx), [Team](../../ui/packages/app/app/%28dashboard%29/settings/members/components/TeamTable.tsx), [Security](../../ui/packages/app/components/account/SecurityResourceRow.tsx), [Models](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/models/components/ModelsRegistryView.tsx) | Billing filters and totals, Team roster geometry, Security title wrapping, and Models registry and form styles live with their components. |
| [`repository-name.css`](../../ui/packages/app/components/domain/conversation/summary/repository-name.css) | The existing repository-name overflow animation, including touch and reduced-motion rules. |
| [Shared tokens](../../ui/packages/design-system/src/tokens.css), [Tailwind theme](../../ui/packages/design-system/src/theme.css), [app tokens](../../ui/packages/design-system/src/app-tokens.css) | Brand and semantic values, typography including Manrope, Tailwind utilities and app-specific density. |

The global stylesheet used to import `settings/settings.css` and `conversation/conversation.css`. Tailwind utilities now live on their elements, such as Billing’s time-zone note with its font, color and wide-screen alignment. [App style graph](../../ui/packages/app/app/globals.css) · [Billing filters](../../ui/packages/app/app/%28dashboard%29/settings/billing/components/BillingFilters.tsx#L39)

Tailwind emits each distinct utility once and shared components group repeated roles and layouts. The repository-name animation keeps its own stylesheet for hover, touch and reduced motion. [Repository-name CSS](../../ui/packages/app/components/domain/conversation/summary/repository-name.css) · [App conventions](../../ui/packages/app/AGENTS.md)

### Inline CSS and component overrides

The source audit found two explicit `style` props in app routes and components. Both receive layout values at runtime.

| Site | Reason | Recommendation |
| --- | --- | --- |
| [`BillingDailySpend.tsx`](../../ui/packages/app/app/%28dashboard%29/settings/billing/components/BillingDailySpend.tsx#L37) | Each bar's height is a percentage calculated from spend, with a two-pixel minimum for a nonzero bar. | Keep the height as a dynamic style value. A conditional utility can express the two-pixel floor if the chart is already being edited. Check that zero days still have zero bars. |
| [`FleetVirtualOutput.tsx`](../../ui/packages/app/components/domain/activity/messages/FleetVirtualOutput.tsx#L54) | The virtualizer supplies each row's position and dimensions at render time. | Pass the library style through to the row. Test scrolling and the existing output window if that boundary moves. |

Clerk's appearance adapter maps vendor control states to app tokens. Check the adapter and the phone input override together when changing them. [Clerk appearance](../../ui/packages/app/lib/clerkAppearance.ts) · [Auth styles](../../ui/packages/app/lib/auth-control-appearance.ts)

The Create form moved its type and caret styles onto shared Input and SelectTrigger controls. The model picker and saved-key control share the Model form trigger style. [Create form](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/models/components/CreateModelForm.tsx) · [Select](../../ui/packages/design-system/src/design-system/Select.tsx)

## Route-by-route inventory

Each route uses the root style graph. Workspace routes begin with `/w/[workspaceId]`.

| Page | Current component and style boundary | Refactor decision |
| --- | --- | --- |
| [Sign in](../../ui/packages/app/app/%28auth%29/sign-in/%5B%5B...sign-in%5D%5D/page.tsx) | Auth layout, Clerk and shared appearance preset | Keep the wrapper and verify form sizing and error states on phones when moving the auth override. |
| [Sign up](../../ui/packages/app/app/%28auth%29/sign-up/%5B%5B...sign-up%5D%5D/page.tsx) | Same auth layout and appearance preset | Keep the shared auth styling. Check the verification input at the same time. |
| [Invites](../../ui/packages/app/app/%28dashboard%29/invites/page.tsx) | Shared `InvitesView` with route data loader, component utilities | Retain the arrangement and test the shared view once for list and empty states. |
| [Invite detail](../../ui/packages/app/app/%28dashboard%29/invites/%5BinviteId%5D/page.tsx) | Reuses `InvitesView` with the selected invite, component utilities | Keep the accept and dismiss behaviors in the feature, using the shared controls. |
| [Dashboard entry](../../ui/packages/app/app/%28dashboard%29/page.tsx) | Server redirect or `AccountSetup` and `NoWorkspaceEmptyState` | Keep state and redirects at the route and visual composition in the existing components. |
| [Billing](../../ui/packages/app/app/%28dashboard%29/settings/billing/page.tsx) | `UsageLayout` wraps the report. Filters and totals own their responsive layout. | Removed the unused root container. The existing report fixtures cover filters, chart values and history. |
| [Members](../../ui/packages/app/app/%28dashboard%29/settings/members/page.tsx) | `MembersView` and invite flow. Members and invitations share a roster grid local to `TeamTable`. | Browser fixtures check mobile rows and desktop role, date and action alignment. |
| [Security](../../ui/packages/app/app/%28dashboard%29/settings/security/page.tsx) | `SecurityLayout`, sessions and API key panels. `SecurityResourceRow` owns title wrapping. | Keep revocation, confirmation and copy flows independently testable. |
| [Workspace events](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/events/page.tsx) | Shared layout and Suspense, `EventsList` and `EventDetailsDialog` with utilities | Separate column presentation and detail rendering from list pagination and server reads. |
| [Fleets index](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/fleets/page.tsx) | Server route selects a destination | Keep the redirect and its loading boundary simple. |
| [Fleet workspace](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/fleets/%5Bid%5D/page.tsx) | Server page composes conversation, header, Summary and recovery. `ChatThread` owns the viewport and footer. | Transcript scroll, sticky composer and responsive Summary order have browser checks. |
| [Legacy fleet setup URL](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/fleets/%5Bid%5D/setup/page.tsx) | Server redirect to the appropriate fleet view | Keep this redirect covered as part of route behavior. |
| [New fleet](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/fleets/new/page.tsx) | `WorkspaceChat` and `AuthoringIntent` use the shared chat layout. | Browser fixtures cover empty and in-progress setup, including the setup flow's distinct composer position. |
| [Integrations](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/integrations/page.tsx) | Shared page header, resource layout and ConnectorStatus | Reuse the resource layout. Keep connector state presentation local. |
| [Plugins](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/plugins/page.tsx) | Server wrapper and `PluginContent` with inspector components | Keep the plugin lifecycle in the feature. Reuse resource chrome and detail primitives where the semantics match. |
| [Secrets](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/secrets/page.tsx) | Shared resource header, `SecretsList` and `AddSecretDialog`; root selectors control resource scrolling | Move chrome to the resource layout without coupling it to add, edit, or reveal actions. |
| [Workspace defaults](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/defaults/page.tsx) | Shared PageLayout, EmptyState, and Button | Keep this small server page and existing shared primitives. |
| [Workspace models](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/models/page.tsx) | Server shell and catalogue provider, registry, form and details components. Rows, inputs and triggers own their styles. | Implement the proposed dialog after approval. Registry read and mutation coordination has a separate refactor seam. |
| [Workspace security URL](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/security/page.tsx) | Authenticated redirect to account security | Keep the canonical Security UI and styling in one place. |
| [CLI authorization](../../ui/packages/app/app/cli-auth/%5Bsession_id%5D/page.tsx) | The client page owns manual and loopback approval flows. Presentation is already split into adjacent components. | Keep the authorization state and requests at their existing boundary. Review the visual components independently with loading and error fixtures. |
| [Connector completion](../../ui/packages/app/app/connectors/complete/page.tsx) | Server wrapper for a shared EmptyState, PageLayout and Button | Keep shared primitives and test both completion and error results. |

## Modularity, reuse and tests

### Route and feature boundaries

Twenty of 21 route entries are server components. The baseline has four layouts, fourteen route loading files and four production components with at least 250 lines. Extract code when it gives state an owner, shares presentation or makes an interaction testable.

The Create form separates model selection from credentials, while the 257-line registry table mixes reads, mutations, pagination and dialog state. Move the read and mutation coordination to a route-local hook and pass typed data and actions to the existing view. [Create form](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/models/components/CreateModelForm.tsx) · [Registry controller](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/models/components/ModelsRegistryTable.tsx) · [Registry view](../../ui/packages/app/app/%28dashboard%29/w/%5BworkspaceId%5D/settings/models/components/ModelsRegistryView.tsx)

Verify pagination, stale responses, retry and recovery when restructuring the registry. Its dialogs can own their own open state.

The 330-line Events list can split column and cell presentation from pagination and reads. Give request context and runner diagnostics typed inputs while the 299-line dialog owns request and close state. [Events list](../../ui/packages/app/components/domain/events/EventsList.tsx#L69) · [Event details](../../ui/packages/app/components/domain/events/EventDetailsDialog.tsx#L99)

### Reuse in chat

The 281-line `SteerComposer` has inline pending and send-error notices. The current state owners handle cancellation, recovery, retry identity and focus. [Composer](../../ui/packages/app/components/domain/conversation/composer/SteerComposer.tsx) · [Chat ownership](../../ui/packages/app/AGENTS.md)

Tool output and tool rows compose `SignalSpinner` with local status text. Reuse the labeled Spinner when its announcement and visible role match. [Tool output](../../ui/packages/app/components/domain/activity/tools/FleetToolOutputDialog.tsx#L87) · [Tool row](../../ui/packages/app/components/domain/activity/tools/FleetToolCallRow.tsx) · [Connection indicator](../../ui/packages/app/components/domain/conversation/chrome/FleetConnectionIndicator.tsx#L97)

## Verification

App lint, 4,697 app tests and the configured coverage gate passed. The CSS pass also passed 82 Chromium fixtures and four sign-in and protected-route smoke checks. [Coverage scope](../../ui/packages/app/vitest.config.ts#L42)

The browser suite checks Models, Team, Billing, Security and chat with local fixtures. It covers responsive layout, focus, touch, typography and reading position. [Models and Team](../../ui/packages/app/tests/e2e/models-team-parity.spec.ts) · [Resource settings](../../ui/packages/app/tests/e2e/resource-settings.spec.ts) · [Billing](../../ui/packages/app/tests/e2e/usage-report.spec.ts)

Chromium passed the Create model prototype checks at 320, 390, 696, 900 and 1480 px. The checks covered both themes, keyboard and touch, focus return and price schedule expansion. [HTML prototype](../ux.html)

The Create proposal uses fixed sample data for review. When implementing it, measure click to visible dialog and click to usable form as separate times. Verify cold and ready data, retry and the exact-ID path before checking save and activation.
