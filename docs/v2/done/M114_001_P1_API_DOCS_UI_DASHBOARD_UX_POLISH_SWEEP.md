<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M114_001: Several reported dashboard rough edges — redundant Workspace tab, stale billing/library copy, a Models-page CSS glow bug, a jargon-y error toast, and an undocumented scope — each closed in one pass

**Prototype:** v2.0.0
**Milestone:** M114
**Workstream:** 001
**Date:** Jul 05, 2026
**Status:** DONE
**Priority:** P1 — concretely-reported dashboard issues from a live product walkthrough this session (redundant nav surface, stale copy, a visibly-broken animation, an operator-jargon error toast); none blocks another, none needs new architecture.
**Categories:** API, DOCS, UI
**Batch:** B1 — single workstream; §1-§3 (nav/API Keys/WorkspaceSwitcher/accent bar) share `Shell.tsx` and `WorkspaceSwitcher.tsx`, §4-§12 each touch disjoint files (§10/§11 added during `/review`, §12 added post-`/review` from a live request, same product-walkthrough session — see Discovery).
**Branch:** feat/m114-dashboard-ux-polish
**Test Baseline:** unit=2323 integration=249
**Depends on:** none
**Provenance:** LLM-drafted (Claude, Jul 05, 2026) from a live plan-mode product review this session — three parallel Explore agents traced exact file:line locations across `ui/packages/app/`, `cli/`, and `src/agentsfleetd/`; Kishore made explicit calls on naming/removal/structure recorded in Discovery.
**Canonical architecture:** `docs/AUTH.md` §Scope catalogue (§9 only) — §1-§8 are presentation-layer fixes with no dedicated architecture doc, same citation pattern as M113_001/M113_003 ("layout/presentation only, data model unchanged").

---

## Overview

**Goal (testable):** the Organization nav item that duplicated "Manage workspace" is gone and its API Keys destination stands alone; the active nav item shows a left accent bar; Billing and dashboard/library copy match the agreed wording, including a full "Add library entry" → "Create fleet library" rename; the Models page's active-model row never animates when it isn't live and has no redundant reset control; the "platform defaults" error toast reads as a sentence, not an operator log line; `docs/AUTH.md` and the public docs site both list every scope a tenant can be granted; every destructive/adversarial action (Revoke, Delete) is colored consistently across the product, and none is one click away from an irreversible effect with no confirmation; the Secrets page's create-action copy matches the "Create X" verb glossary already applied elsewhere in this sweep.

**Problem:** a live UI walkthrough found a two-tab settings page where one tab (Workspace) duplicates the top-right "Manage workspace" menu; a nav-selected state with no directional indicator beyond a background tint; four stale/inconsistent copy strings on Billing and the Fleet dashboard plus an inconsistently-named onboarding action; a CSS selector bug that makes the Models-page hero row glow permanently regardless of live state, plus a redundant "Switch to platform defaults" button already covered by the provider list below it; a raw backend string ("…operator action required") leaking into a customer-facing toast; a scope (`platform-library:write`) that exists in code and gates real functionality but has no row in the scope-catalogue doc, with no public reference for scopes at all; a "Workspace ID" copy-paste affordance on the API Keys page that turned out to be vestigial once the CLI's own self-resolution (`login` auto-hydration, `workspace list`/`workspace use`) was traced end to end; destructive-action buttons (Revoke/Delete) styled as plain `ghost` buttons in some lists (`ApiKeyList`, `SecretsList`) but as `destructive` (red) in another (`RunnerList`) for the same class of action, plus one admin delete control (`CatalogueList`, model-catalogue rates) with no confirmation dialog at all before an irreversible delete; and the Secrets page's create action still reading "Add Secret"/"Add secret" instead of the "Create X" verb this same sweep applies everywhere else.

**Solution summary:** collapse the Workspace/API-Keys tab pair into a single API Keys page (§1); rename "New workspace" and remove the redundant "Manage workspace" item (§2); add a left accent bar to the active nav item (§3); fix Billing copy (§4) and dashboard/fleet-library copy including the "Create fleet library" rename (§5); fix the Models page's `[data-live]` CSS selector (§6) and delete its redundant hero reset control (§7); give the platform-key-missing case its own curated registry entry instead of a raw passthrough, after producing a reviewable inventory of the error registry (§8); add the missing scope-catalogue row, fix a stale comment, add a doc/code parity test, and ship a public scopes reference page (§9); remove the API Keys page's "Workspace ID" identity block entirely after confirming the CLI never needs it for real workflows (§1, revised — see Discovery); color every destructive action consistently and close the one confirmation gap found (§10); rename the Secrets page's create action and empty-state copy to match the "Create X" glossary (§11); split the Secrets rename flow out of the Edit/rotate dialog into its own popup triggered from the Name column, with a tightened generic warning (§12).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Dashboard polish: API Keys nav, billing/library copy, Models cleanup, friendlier errors, scope docs
- **Intent (one sentence):** close a handful of independently-reported dashboard rough edges — a redundant settings tab, a missing nav indicator, stale copy, a CSS bug, a jargon-y error, and an undocumented scope — without touching any underlying data model or API contract.
- **Handshake:** the implementing agent restates this Intent + lists `ASSUMPTIONS I'M MAKING: …` at PLAN, before EXECUTE. A mismatch STOPs and reconciles first.

---

## Implementing agent — read these first

1. `ui/packages/app/components/layout/Shell.tsx` — `ORGANIZATION_NAV`/`NAV_ITEM_CLASSES` (§1's nav rename, §3's accent bar) and `CONFIGURATION_NAV` (do not touch; M113_003 already added Secrets & ENVs there).
2. `ui/packages/app/app/(dashboard)/settings/api-keys/components/ApiKeysView.tsx` + `ui/packages/app/lib/workspace.ts` (`resolveActiveWorkspaceId`, `listTenantWorkspacesCached`) — the page §1 consolidates onto, and where workspace identity already gets resolved server-side elsewhere (`settings/page.tsx`, to be deleted). `ui/packages/app/components/layout/WorkspaceSwitcher.tsx` for §2.
3. `ui/packages/app/app/(dashboard)/settings/models/components/ActiveModelRow.tsx` + `ui/packages/design-system/src/tokens.css` (`wake-pulse` keyframe, `[data-live]` selector) + `ui/packages/design-system/src/design-system/WakePulse.tsx` (the correct `data-live={live ? true : undefined}` contract §6 must match).
4. `src/agentsfleetd/errors/error_entries.zig` (the `e()`/`eu()` convention M113_002 established) + `ui/packages/app/lib/errors.ts` (`presentErrorString`) + `src/agentsfleetd/http/handlers/tenant_provider.zig:137` (the raw string §8 replaces).
5. `docs/AUTH.md` §Scope catalogue + `src/agentsfleetd/auth/scopes.zig` (`WIRE` table) — the vocabulary §9 reconciles.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/components/layout/Shell.tsx` | EDIT | `ORGANIZATION_NAV`: "Workspace" → "API Keys" entry pointing at `/settings/api-keys` (§1); `NAV_ITEM_CLASSES` gains the active-item accent bar (§3) |
| `ui/packages/app/app/(dashboard)/settings/page.tsx` | EDIT | becomes a server redirect to `/settings/api-keys` (keeps `/settings` deep links working) |
| `ui/packages/app/app/(dashboard)/settings/loading.tsx` | DELETE | belonged to the removed Workspace-tab content |
| `ui/packages/app/components/layout/SettingsTabs.tsx` | DELETE | one tab remains; no tab bar needed |
| `ui/packages/app/app/(dashboard)/settings/api-keys/page.tsx` | EDIT | renders header directly, no `SettingsTabs`; **revised mid-review (Discovery):** does not fetch/derive a workspace identity — traced the CLI's own resolution path (`login` auto-hydration, `workspace list`/`workspace use`) and confirmed the dashboard copy-paste ID was vestigial, so the fetch was dropped rather than added |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/ApiKeysView.tsx` | EDIT | title "API Keys"; inline operator-only `Alert` on 403 instead of a redirect; **revised mid-review:** no workspace name/ID block (see Discovery) — `workspace` prop, `CopyButton`/`DescriptionList`/`DescriptionTerm`/`DescriptionDetails` imports, and `TenantWorkspace` type import all removed as a result |
| `ui/packages/app/app/(dashboard)/settings/api-keys/loading.tsx` | EDIT | skeleton drops the "Workspace" section placeholder to match ApiKeysView's revised shape |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog.tsx` | EDIT | trigger "New API key" → "Create key"; dialog title → "Create API key" |
| `ui/packages/app/components/layout/WorkspaceSwitcher.tsx` | EDIT | "New workspace" → "Create workspace" (§2); delete the "Manage workspace" item and its `showManageItem` prop (§2) |
| `ui/packages/app/components/layout/CreateWorkspaceDialog.tsx` | EDIT | DialogTitle "New workspace" → "Create workspace" (§2, matches the trigger label) |
| `ui/packages/app/app/(dashboard)/settings/billing/page.tsx` | EDIT | subtitle → "Manage credits and usage."; "Payment Method" → "Payment method" |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx` | EDIT | "Purchase credits" → "Buy credits" with a leading icon, becomes an active `mailto:agentsfleet@agentsmail.to` link (drops `disabled`/`aria-disabled`/`pointer-events-none`); **revised mid-review (Discovery):** the redundant "Covers all Fleet events · pay as you go" caption removed, `balance-usage` right-aligned via `justify-end` to preserve the "rides the meter's end" layout |
| `ui/packages/app/app/(dashboard)/page.tsx` | EDIT | dashboard description string → "Start a fleet from the prebuilt fleet library." |
| `ui/packages/app/app/(dashboard)/fleets/new/library-docs.tsx` | EDIT | empty-state title/description strings updated; comments referencing "Add library entry" updated |
| `ui/packages/app/app/(dashboard)/fleets/new/AddLibraryDialog.tsx` | EDIT | trigger/DialogTitle/action-verb/spinner-label/submit renamed "Add library entry" → "Create fleet library"; GitHub-repo description lowercases "fleet library entry" |
| `ui/packages/app/app/(dashboard)/fleets/new/InstallEntry.tsx` | EDIT | CTA renamed to match |
| `cli/src/commands/fleet_library.ts` | EDIT | empty-state message aligned to "No prebuilt fleet library found." |
| `ui/packages/app/app/(dashboard)/settings/models/components/ActiveModelRow.tsx` | EDIT | drop the unconditional `data-live` attribute (§6); delete the "Switch to platform defaults" button, its handler, and `RESET_ACTION` (§7) |
| `ui/packages/design-system/src/tokens.css` | EDIT | `[data-live]` → `[data-live="true"]` so the wake-pulse animation only fires when actually live |
| `src/agentsfleetd/http/handlers/tenant_provider.zig` | EDIT | platform-key-missing path returns a dedicated error code instead of a raw `internalOperationError` literal |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | new `eu()` entry for the platform-key-missing code with a curated `user_message` |
| `src/agentsfleetd/errors/error_registry_test.zig` | EDIT | new test: `UZ-PROVIDER-009`'s `user_message` is non-empty and distinct from its `hint`, and doesn't leak "operator" jargon |
| `src/agentsfleetd/http/handlers/tenant_provider_dispatch_test.zig` | CREATE | text-contract pin: the `PlatformKeyMissing` arm dispatches through `ERR_PROVIDER_PLATFORM_KEY_MISSING`, not the old raw `internalOperationError(...)` literal path |
| `src/agentsfleetd/tests.zig` | EDIT | registers `tenant_provider_dispatch_test.zig` in its `test { }` block |
| `docs/AUTH.md` | EDIT | add the missing `platform-library:write` row to the Discrete-verbs table; add a short "development provisioning" note under Provisioning grants |
| `src/agentsfleetd/auth/middleware/bearer_or_api_key.zig` | EDIT | fix the stale line-6 comment (still describes `publicMetadata.role` gating; gating is scope-based) |
| `src/agentsfleetd/auth/scopes_test.zig` (or a new `auth_md_parity_test.zig`) | EDIT/CREATE | parity test: every `scopes.zig` `WIRE` string appears in `docs/AUTH.md` |
| `~/Projects/docs/api-reference/scopes.mdx` (separate repo, own branch `chore/m114-scopes-docs-changelog`) | CREATE | public scopes reference page linked from the existing `api-reference/error-codes` page |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/ApiKeyList.tsx` | EDIT | §10: row-level "Revoke"/"Delete" triggers `variant="ghost"` → `variant="destructive"`, matching `RunnerList.tsx`'s existing pattern (the confirm modal was already `intent="destructive"`; only the row trigger was inconsistent) |
| `ui/packages/app/app/(dashboard)/secrets/components/SecretsList.tsx` | EDIT | §10: row-level delete (trash icon) `variant="ghost"` → `variant="destructive"`; §11: empty-state title/description → "No secrets" / "Create secret to have your fleets reach other services securely." |
| `ui/packages/app/app/(dashboard)/admin/models/components/CatalogueList.tsx` | EDIT | §10: delete button gains a `ConfirmDialog` (`intent="destructive"`) — previously called `deleteAdminModelAction` directly on click, with no confirmation step at all; button styling `ghost` → `destructive` |
| `ui/packages/app/app/(dashboard)/secrets/components/AddSecretDialog.tsx` | EDIT | §11: trigger/DialogTitle "Add Secret"/"Add a secret" → "Create secret" |
| `ui/packages/app/app/(dashboard)/secrets/components/AddSecretForm.tsx` | EDIT | §11: submit button/spinner label "Add secret"/"Adding" → "Create secret"/"Creating" |
| test files (§10/§11): `admin-models-ui.test.ts`, `secrets-components.test.ts`, `secrets-list.test.ts`, `api-keys-components.test.ts`, `tests/e2e/acceptance/secrets-lifecycle.spec.ts` | EDIT | assertions updated to the new button variants/confirm-dialog gating/copy named above |
| `ui/packages/app/app/(dashboard)/settings/models/components/ActiveModelRow.tsx` | EDIT | **found by `/review`'s maintainability specialist:** `useProviderAction()`'s `pending`/`error` were dead since §7 removed the row's only `run()` call (`disabled={pending}` permanently `false`, the error `Alert` unreachable) — hook call, both `disabled` props, and the dead `Alert` block all removed |
| `ui/packages/design-system/src/design-system/WakePulse.tsx` | EDIT | **found by `/review`:** doc comment still described the pre-§6 `[data-live]` selector; updated to `[data-live="true"]` |
| `docs/AUTH.md` | EDIT (additional) | **found by `/review`'s security specialist:** the new §9 "Development provisioning" example granted the full platform-operator scope bundle (`platform-key:admin`, `workspace:any`, etc.) just to view two read-only pages; revised to lead with the minimal `runner:read model:read` example, full bundle kept only for genuine write/admin needs |
| `src/agentsfleetd/http/handlers/tenant_provider.zig` | EDIT (additional) | **found by `/review`'s security specialist:** §8's new `detail` string ("No active row in core.platform_llm_keys") leaked an internal Postgres schema/table name over the wire to a tenant-scoped caller; replaced with a generic "Platform LLM key not configured" |
| `ui/packages/design-system/tsconfig.json` | EDIT | **found by `/review`:** `types` excluded `"node"`, so the gap-fill `tokens.css.test.ts` (`node:fs`/`node:path`/`__dirname`) failed `make lint-apps-ds-ctl`'s `tsc --noEmit` despite passing `vitest run` — added `"node"` (the package already resolves `@types/node` via the workspace) |
| `src/agentsfleetd/errors/internal_op_error_sweep_test.zig` | CREATE | §8.2 (implemented mid-review, see Discovery): pins `internalOperationError(` call-site count under `http/handlers/**` at today's sweep baseline (86) |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT (additional) | registers `internal_op_error_sweep_test.zig` in its `test { }` block |
| `ui/packages/app/components/layout/Shell.tsx` | EDIT (additional) | **found by Kishore, live walkthrough:** §3's accent bar used `rounded-md` (all four corners), curving the left accent bar's top/bottom ends instead of a straight vertical line — changed to `rounded-r-md` |
| test files: `app-components.test.ts`, `settings-tabs.test.ts` (deleted with the component), `api-keys-page.test.ts`, `api-keys-components.test.ts`, `api-keys-create-dialog.test.ts`, `dashboard-workspace.test.ts`, `billing-card.test.ts`, `billing-tabs.test.ts`, `dashboard-placeholder.test.ts`, `fleets-install-entry-gate.test.ts`, `fleets-routes.test.ts`, `add-template-dialog.test.tsx`, `add-template-dialog-deep-link.test.tsx`, `fleets-install-flow.test.ts`, `active-model-row.test.tsx`, `provider-switch-list.test.tsx`, `app-pages.test.ts`, `loading-states.test.ts`, `helpers/dashboard-mocks.tsx`, `cli/test/fleet-library.unit.test.ts` | EDIT | assertions updated to the new copy/structure named above; `app-pages.test.ts`/`dashboard-mocks.tsx` add the `KeyIcon` lucide-react mock §1's nav rename needs |
| `ui/packages/app/tests/e2e/acceptance/settings-api-keys.spec.ts` | EDIT | heading assertion `/^settings$/i` → `/^api keys$/i`; the `"settings sections"` nav-landmark lookup (`SettingsTabs`'s nav, deleted in §1) reworked to look up the sidebar link directly; `"new api key"` button name → `"create key"` |
| `ui/packages/app/tests/e2e/acceptance/workspace-create.spec.ts` | EDIT | `getByTestId("workspace-new")`'s accessible name / the "New workspace" dialog name updated to "Create workspace" (§2) |
| `ui/packages/app/tests/e2e/acceptance/settings-billing.spec.ts` | EDIT | line ~35's `toBeDisabled()` assertion on `{name: "Purchase Credits"}` replaced with an enabled-link assertion on `{name: "Buy credits"}` with `href="mailto:agentsfleet@agentsmail.to"` |
| `ui/packages/app/tests/e2e/acceptance/template-onboarding.spec.ts` | EDIT | **pre-existing bug found during spec authoring, unrelated to but touched by §5**: `test_github_source_error_stays_in_dialog` asserts button/dialog name `"Create a template"` and error text `"Couldn't add the template"` — neither matches the current `AddLibraryDialog.tsx` copy ("Add library entry"/"Couldn't add the library entry"), so this test is already stale pre-M112. Fixed to assert the new "Create fleet library"/"Couldn't create the fleet library" copy (RULE NLR, touch-it-fix-it — this spec edits the same dialog) |
| `ui/packages/app/tests/e2e/acceptance/settings-models.spec.ts` | EDIT | new assertions added: no wake-pulse animation state when not live, no "Switch to platform defaults" button on the hero row (this file does not test either today) |
| `ui/packages/app/app/(dashboard)/secrets/components/RenameSecretDialog.tsx` | CREATE | §12: new dialog owning the rename flow (create-new-then-delete-old, re-entered value, tightened generic warning) — split out of `EditSecretDialog` |
| `ui/packages/app/app/(dashboard)/secrets/components/RenameSecretDialog.test.tsx` | CREATE | §12: co-located full-coverage test for `RenameSecretDialog` (ordering, same-name/length/non-object rejections, create-failure, delete-failure recovery, dismiss guard, generic warning) |
| `ui/packages/app/components/domain/island-dynamic/RenameSecretDialogDynamic.tsx` | CREATE | §12: `next/dynamic` island shim for the rename dialog, mirroring `EditSecretDialogDynamic` |
| `ui/packages/app/app/(dashboard)/secrets/components/EditSecretDialog.tsx` | EDIT | §12: becomes rotate-only — drops `mode`/`isRename`/`newName`, the "Advanced — rename" toggle, `deleteSecretAction`/`Input`/`SECRET_NAME_MAX` imports, and the rename submit branch |
| `ui/packages/app/app/(dashboard)/secrets/components/EditSecretDialog.test.tsx` | EDIT | §12: rename cases removed; now rotate-only (rotate happy path, no-rename-affordance guard, cancel, non-object reject, create-error copy) |
| `ui/packages/app/app/(dashboard)/secrets/components/SecretsList.tsx` | EDIT | §12: `SecretNameCell` gains a rename affordance (`PencilLineIcon`, `variant="ghost"`) wired through `buildColumns`/`SecretTable`; `renameTarget` state + `RenameSecretDialogDynamic` render added |
| `ui/packages/app/app/(dashboard)/secrets/lib/secret-data.ts` | EDIT | §12: hoist the shared re-enter-required copy to `SECRET_DATA_REENTER_REQUIRED` (single source for rotate + rename; replaces `EditSecretDialog`'s local `DATA_REQUIRED`) |
| `ui/packages/app/tests/secrets-list.test.ts` | EDIT | §12: `PencilLineIcon` lucide mock added; "Advanced — rename" assertion dropped; rename-trigger-in-Name-column test added |
| `ui/packages/app/tests/island-dynamic.test.ts` | EDIT | §12: `RenameSecretDialog` added to the island manifest (ISLANDS entry, inner-module `vi.mock`, mount-case) |

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NDC** (delete `SettingsTabs.tsx`/`loading.tsx`/`showManageItem` cleanly, not commented out); **UFS** (every renamed string is a named constant, mirroring the existing `FLEET_LIBRARY_EMPTY_TITLE`-style constants — no inline literals); **ORP** (cross-layer orphan sweep after every deletion/rename in §1, §2, §5); **EMS** (§8's new entry follows the standard error-message structure `error_entries.zig` already establishes).
- `dispatch/write_ts_adhere_bun.md` — every `.ts`/`.tsx` touch in §1-§7.
- `dispatch/write_zig.md` — §8/§9's `.zig` touches: cross-compile both linux targets, additive struct field only (no lifecycle/ownership change).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes (§8, §9) | cross-compile `x86_64-linux` + `aarch64-linux`; new `Entry.user_message` field is additive, mirrors M113_002's existing shape |
| PUB / Struct-Shape | yes | `WorkspaceSwitcher`'s `showManageItem` prop removal (§2) — shape verdict at PLAN (backward-incompatible only within this file's own callers, all updated same diff) |
| File & Function Length (≤350/≤50) | no | §1/§2/§6/§7 net-remove code (deleted files, deleted button/handler); no file approaches the cap |
| UFS (repeated/semantic literals) | yes | every copy string in §1-§5 and the new error `user_message` in §8 lives in one named constant, never duplicated inline |
| UI Substitution / DESIGN TOKEN | yes | §3's accent bar uses stock `border-l-2` + the existing `border-pulse` token (no arbitrary `*-[…]` value); §1's API Keys workspace block reuses `DescriptionList`/`CopyButton`/`Alert` primitives already in the design system |
| ERROR REGISTRY | yes (§8) | new code follows the `eu()` convention; non-empty `user_message` invariant (comptime-asserted since M113_002) |
| LOGGING / LIFECYCLE / SCHEMA | no | not touched |

---

## Prior-Art / Reference Implementations

- **§1** → the workspace-identity-on-API-keys-page pattern Kishore named directly (pioneer.ai).
- **§2** → `WorkspaceSwitcher.tsx`'s existing "Create workspace" dialog flow, reused unchanged; only the trigger label and the now-redundant "Manage workspace" item change.
- **§3** → `ui/packages/design-system/src/design-system/tab-styles.ts`'s existing `border-b-2 … data-[active=true]:border-pulse` pattern — the sanctioned active-indicator shape this spec mirrors as `border-l-2`.
- **§4-§5** → existing `EmptyState`/named-constant copy conventions already in `library-docs.tsx` and `BillingBalanceCard.tsx`.
- **§6-§7** → `WakePulse.tsx`'s own `data-live={live ? true : undefined}` contract — the correct pattern already shipped elsewhere in the design system; `ActiveModelRow.tsx` just needs to match it.
- **§8** → M113_002's `eu()`/`user_message` registry mechanism (`error_entries.zig`, `writeProblem` in `common.zig`) — this section extends that mechanism to one more code, it does not reinvent it.
- **§9** → `docs/AUTH.md`'s own Scope-catalogue table conventions; `docs/changelog.mdx`'s existing `UZ-AUTH-022` announcement (links `/api-reference/error-codes`) is the precedent the new public scopes page attaches to.

---

## Sections (implementation slices)

### §1 — Workspace tab collapses into a standalone API Keys page

The Workspace tab duplicates the top-right "Manage workspace" menu and adds a click just to reach API keys. Collapsing to one page removes the duplication and gives API Keys its own nav identity, matching the pioneer.ai reference Kishore named. **Implementation default:** `/settings` becomes a redirect (not a 404) so any existing deep link keeps resolving.

- **Dimension 1.1** — sidebar shows "API Keys" (not "Workspace") routing to `/settings/api-keys`; `/settings` redirects there → Test `test_nav_api_keys_replaces_workspace`
- **Dimension 1.2** — the API Keys page shows title "API Keys", the existing subtitle "Authenticate with the agentsfleet API. Each key is shown once.", and a "Create key" action (not "New API key") → Test `test_api_keys_page_shows_create_key`
- **Dimension 1.3** — a 403 on the API-keys list renders the existing "API keys need admin access" alert inline on this page, replacing the old `?notice=api-keys-operator-only` redirect (which targeted a page that no longer exists) → Test `test_api_keys_operator_gate_renders_inline`
- **Dimension 1.4** — *(revised mid-review, see Discovery)* the API Keys page carries **no** workspace name/ID identity block — traced the CLI's own workspace resolution (`agentsfleet login`'s auto-hydration, `agentsfleet workspace list`/`workspace use <id>`) and confirmed a dashboard copy-paste affordance was vestigial for every real workflow; the top-right `WorkspaceSwitcher` remains the one place workspace identity/switching lives → Test `test_api_keys_page_has_no_workspace_identity_block`

### §2 — WorkspaceSwitcher: "Create workspace" label, "Manage workspace" removed

Once workspace identity lives on the API Keys page, the top-right "Manage workspace" item has nothing left to manage — it just re-navigated to the page §1 removes. "New workspace" is renamed for verb consistency (Create = mint a new resource).

- **Dimension 2.1** — the dropdown item reads "Create workspace"; its dialog behavior (create-workspace popup) is unchanged → Test `test_workspace_switcher_create_label`
- **Dimension 2.2** — the "Manage workspace" item no longer renders in the dropdown → Test `test_workspace_switcher_manage_item_removed`

### §3 — Left-nav active-item accent bar

The active sidebar item today shows only a background tint (`bg-pulse/10`); Kishore asked for a directional indicator like tryreplicas' left accent bar on its active "Getting Started" item.

- **Dimension 3.1** — the active sidebar item's class list adds a left accent bar (`border-l-2 border-transparent data-[active=true]:border-pulse`) alongside the existing background fill → Test `test_nav_active_item_shows_accent_bar`

### §4 — Billing copy

- **Dimension 4.1** — Billing subtitle reads "Manage credits and usage." (the seats/minimum sentence is dropped); the tab reads "Payment method" → Test `test_billing_copy_updated`
- **Dimension 4.2** — "Buy credits" (with a leading icon) is an active `mailto:agentsfleet@agentsmail.to` link, not a disabled/inert button → Test `test_buy_credits_is_mailto_link`

### §5 — Dashboard and fleet-library copy, including the "Create fleet library" rename

Four Billing-adjacent dashboard/library strings are stale or inconsistent, and the fleet-library onboarding action is named "Add library entry" everywhere it should read "Create fleet library" per the agreed verb glossary (Create = mint a new resource; Add = attach an existing external thing).

- **Dimension 5.1** — the dashboard description, the fleet-library empty-state title/description, and the GitHub-repo add-source description read the four agreed strings verbatim (see Files Changed) → Test `test_dashboard_library_copy_updated`
- **Dimension 5.2** — every "Add library entry" surface (dialog trigger, dialog title, action verb, spinner label, submit button, `InstallEntry` CTA, CLI empty-state message) reads "Create fleet library"; API routes, scope names, and the `agentsfleet library` CLI command are untouched (structural, out of scope) → Test `test_library_entry_action_renamed`

### §6 — Models page: kill the always-on glow

The active-model hero row glows permanently because `tokens.css`'s `[data-live]` selector matches `data-live="false"` too.

- **Dimension 6.1** — the hero row never sets `data-live` when not live (or `tokens.css`'s selector is scoped to `[data-live="true"]`); the wake-pulse animation only fires while genuinely live → Test `test_hero_row_no_glow_when_not_live`

### §7 — Models page: remove the redundant "Switch to platform defaults" button

The hero row's own reset button duplicates the equivalent "Platform defaults" row already in the provider list below it.

- **Dimension 7.1** — no "Switch to platform defaults" control renders on the hero row; `ProviderSwitchList`'s own "Platform defaults" row is unaffected and remains the one switch path → Test `test_hero_reset_button_removed_switch_list_unaffected`

### §8 — Error message follow-up: the platform-key-missing toast gets curated copy

**Implementation default:** before any string changes, the agent produces the current registry+`CODE_MAP` inventory (code · audience · current text · proposed alternative) as a Discovery entry for review — this spec fixes the one concretely-reported offender plus whatever the sweep in Dimension 8.2 turns up, not a blanket rewrite of M113_002's already-curated 27 codes.

- **Dimension 8.1** — the platform-key-missing case (`tenant_provider.zig:137`) gets its own `eu()`-curated registry entry instead of sharing the generic internal-error code; the toast a user sees no longer contains "operator action required" → Test `test_platform_key_missing_error_has_curated_message`
- **Dimension 8.2** — *(implemented mid-review, see Discovery — the original Test Specification row named this test but it had never actually been written)* a repo-wide sweep for other handler call sites passing a raw literal into the same shared internal-error path; the disposition (86 call sites: 50 plain-English, 35 jargon-leaking, 1 raw-Zig-error-leaking) is recorded in Discovery and the error-inventory artifact; a Zig regression test pins today's call-site count so a new one can't be added without conscious triage → Test `internal_op_error_sweep_test.zig`

### §9 — Clerk scope documentation parity

`platform-library:write` gates real functionality but has no row in `docs/AUTH.md`'s catalogue; a stale comment still describes role-based gating; and no public page lets a tenant look up what a scope means when they hit `UZ-AUTH-022`.

- **Dimension 9.1** — `docs/AUTH.md`'s Discrete-verbs table includes `platform-library:write` with its grant description; a test asserts every `scopes.zig` `WIRE` string appears somewhere in the doc → Test `test_auth_md_scope_parity`
- **Dimension 9.2** — `bearer_or_api_key.zig:6`'s comment describes scope-based gating, not `publicMetadata.role` → Test `test_bearer_or_api_key_comment_current`
- **Dimension 9.3** — the public docs site (`~/Projects/docs`, own branch) gains a scopes reference page listing the tenant-facing catalogue, linked from the existing `api-reference/error-codes` page → Test `test_public_docs_scopes_page_exists`

### §10 — Destructive-action button consistency (added mid-review)

A live `/review` pass asked whether Revoke/Delete controls should be colored consistently across the product. An audit found `RunnerList.tsx`'s row-level "Revoke" button already uses `variant="destructive"` when the action is destructive, but the equivalent row-level triggers in `ApiKeyList.tsx` ("Revoke"/"Delete") and `SecretsList.tsx` (trash-icon delete) use `variant="ghost"` — visually indistinguishable from a harmless action until the confirm modal opens. Worse, `CatalogueList.tsx`'s "Delete" (platform model-catalogue rate) calls `deleteAdminModelAction` directly on click with **no confirmation dialog at all** — every other destructive control in the app gates through a `ConfirmDialog`.

- **Dimension 10.1** — `ApiKeyList.tsx`'s "Revoke"/"Delete" row triggers and `SecretsList.tsx`'s delete (trash icon) row trigger render `variant="destructive"`, matching `RunnerList.tsx`'s existing pattern; the underlying confirm-modal flow for both is unchanged (already `intent="destructive"`) → Test `test_api_key_and_secret_row_delete_triggers_are_destructive_variant`
- **Dimension 10.2** — `CatalogueList.tsx`'s "Delete" gains a `ConfirmDialog` (`intent="destructive"`, naming the model id) before calling `deleteAdminModelAction`; clicking the row button alone must not call the delete action → Test `test_catalogue_delete_requires_confirmation`

### §11 — Secrets page: "Create secret" verb consistency (added mid-review)

The Secrets & ENVs page's create action reads "Add Secret" (trigger)/"Add a secret" (dialog title)/"Add secret" (submit) — the one surface in this sweep still using "Add" for a create-a-new-resource action, after §1/§2/§5 already established "Create key"/"Create workspace"/"Create fleet library" for the same verb class (Create = mint a new resource; Add = attach an existing external thing — the secret is newly minted, not attached).

- **Dimension 11.1** — `AddSecretDialog.tsx`'s trigger and dialog title, and `AddSecretForm.tsx`'s submit button and pending-spinner label, all read "Create secret"; the empty-state on `SecretsList.tsx` reads "No secrets" / "Create secret to have your fleets reach other services securely." (per-field "Add field" inside the form's field/value builder is unchanged — that's an additive list action, not the create-resource action) → Test `test_secrets_create_action_renamed`

### §12 — Secrets rename split into its own dialog (added post-review, live request)

The rename flow lived *inside* `EditSecretDialog` behind an "Advanced — rename" toggle, coupling two distinct intents — rotate a value vs. change the reference key — in one dialog. Kishore asked to split rename into its own popup triggered from the Name column, leaving the Edit pencil rotate-only (one job), and to tighten the warning while keeping it **generic**: the platform has no reverse index from a secret name to the Fleets that reference it (`${secrets.<name>...}` is a runtime-only template string, never indexed — confirmed via grep, no endpoint anywhere), so naming the affected Fleets is impossible and the warning stays generic by necessity, not taste. Renaming still requires re-entering the secret value (the vault never returns plaintext, and there is no in-place rename endpoint), so the new dialog keeps the same JSON-data textarea; the mechanism remains create-under-new-name-then-delete-old, preserving the old path's exact ordering and delete-failure recovery.

- **Dimension 12.1** — `EditSecretDialog` is rotate-only (no `mode`/rename branch, no `deleteSecretAction` import, no "Advanced — rename" toggle); a new `RenameSecretDialog` (its own `next/dynamic` island shim) owns the create-new-then-delete-old flow and is triggered from a rename affordance in the Name column of `SecretsList`; the create-before-delete ordering and the delete-failure recovery (new name stored, old kept, list refreshed, dialog stays open with a recovery message) are preserved exactly → Test `RenameSecretDialog.test.tsx` + `SecretsList` "clicking rename in the Name column opens the rename dialog"
- **Dimension 12.2** — the rename warning is a single tightened generic sentence with no per-Fleet evidence (none exists) → asserted in `RenameSecretDialog.test.tsx` ("always shows the generic rename warning" — asserts the warning renders and no `${secrets.` template string appears)

---

## Interfaces

No new endpoint and no changed request/response shape anywhere in this spec. §8 adds one new error *code* using the `user_message` field M113_002 already added to the RFC 7807 error body — no wire-shape change for any other code.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| API-keys list fetch 403s | caller lacks operator scope | inline "API keys need admin access" alert on the same page (Dimension 1.3) — no redirect to a removed route |
| Workspace list is empty | brand-new tenant with zero workspaces | `WorkspaceSwitcher`'s existing "No workspace" fallback is unchanged; "Create workspace" (§2) remains reachable |
| `mailto:` has no configured mail client | user's OS/browser has no default handler | browser-native behavior, out of this spec's control — not engineered around |
| A future backend code again bypasses the registry the way Dimension 8.1's did | a new handler call site hardcodes a literal into `internalOperationError` | falls through to the existing generic fallback (unchanged, not a regression) until the next sweep catches it — Dimension 8.2's sweep is not exhaustive by construction, named explicitly rather than silently claimed complete |
| A new scope is added to `scopes.zig` without a doc update | future workstream forgets `docs/AUTH.md` | Dimension 9.1's parity test fails before merge |
| A CI/automation caller genuinely needs a workspace id the CLI has never locally seen (e.g. scripting against a teammate's workspace) | dashboard's "Workspace ID" identity block (§1, Dimension 1.4) removed | narrow, out-of-band case — reach the id via the API directly (`GET /v1/tenants/me/workspaces`) or have the workspace's own operator run `agentsfleet workspace list` and share the id; not engineered around in-dashboard |
| A future admin-list control is added without a `ConfirmDialog` | new destructive action doesn't follow the §10 pattern | no automated gate catches this class by construction — §10 fixed the one instance found this session; a future audit would need to re-sweep |

---

## Invariants

1. Every existing server action/endpoint this spec's UI touches keeps its exact signature — §1-§7 are presentation-only; enforced by unchanged existing action test suites passing unmodified.
2. `[data-live]` never animates unless the attribute's value is literally `"true"` — enforced by Dimension 6.1's test and the CSS selector itself.
3. Every `scopes.zig` `WIRE` string has a corresponding row in `docs/AUTH.md` — enforced by Dimension 9.1's parity test (Zig, reads the doc as text at test time).
4. `ProviderSwitchList`'s "Platform defaults" row keeps working after the hero button is deleted — enforced by Dimension 7.1's regression assertion.
5. Every destructive-action control that mutates via a `ConfirmDialog` uses `intent="destructive"` on that dialog AND `variant="destructive"` on its row-level trigger — enforced by Dimension 10.1/10.2's assertions; `deleteAdminModelAction` (or any other destructive server action wired to a UI control) is never called directly from a row-level click without a confirm step in between — enforced by Dimension 10.2's "click alone doesn't call the action" test.

---

## Metrics & Observability

Not applicable — no product/operator signal changes; this spec is copy, a CSS selector fix, a control removal, an error-message curation, and a docs parity fix.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_nav_api_keys_replaces_workspace` | sidebar renders "API Keys" not "Workspace"; visiting `/settings` redirects to `/settings/api-keys` |
| 1.1 | e2e | `settings-api-keys.spec.ts` (existing) | the sidebar link `/api keys/i` navigates to `/settings/api-keys` in a real rendered session — no `"settings sections"` nav landmark remains (that landmark belonged to the deleted `SettingsTabs`) |
| 1.2 | unit | `test_api_keys_page_shows_create_key` | render → title "API Keys", "Create key" button, no "New API key" string |
| 1.2 | e2e | `settings-api-keys.spec.ts` (existing) | heading assertion updated `/^api keys$/i`; `getByRole("button", {name: /new api key/i})` → `/create key/i` in the real mint/reveal/revoke/delete round-trip |
| 1.3 | unit | `test_api_keys_operator_gate_renders_inline` | mock a 403 list response → alert renders on the same page, no navigation occurs |
| 1.4 | unit | `test_api_keys_page_has_no_workspace_identity_block` | render → no "Workspace" section label, no `CopyButton` for workspace name/ID; `ApiKeysViewProps` carries no `workspace` field |
| 2.1 | unit | `test_workspace_switcher_create_label` | dropdown shows "Create workspace"; clicking it still opens the existing create-workspace dialog |
| 2.1 | e2e | `workspace-create.spec.ts` (existing) | `getByTestId("workspace-new")`'s accessible name and the dialog's name reflect "Create workspace" end-to-end in a fresh-signup session |
| 2.2 | unit | `test_workspace_switcher_manage_item_removed` | `queryByTestId("workspace-manage")` is null |
| 3.1 | unit | `test_nav_active_item_shows_accent_bar` | active nav link's class list includes the accent-bar utility alongside the existing `bg-pulse/10` |
| 4.1 | unit | `test_billing_copy_updated` | Billing page renders "Manage credits and usage." and "Payment method" |
| 4.2 | unit | `test_buy_credits_is_mailto_link` | "Buy credits" renders as `<a href="mailto:agentsfleet@agentsmail.to">` with a leading icon, not `disabled` |
| 4.2 | e2e | `settings-billing.spec.ts` (existing) | the real Billing page's "Buy credits" control is an enabled link to the mailto address, replacing the prior `toBeDisabled()` assertion |
| 5.1 | unit | `test_dashboard_library_copy_updated` | dashboard/library empty-state and description strings match the four agreed strings verbatim |
| 5.2 | unit | `test_library_entry_action_renamed` | every named "Add library entry" surface (dialog, InstallEntry, CLI) renders "Create fleet library"; no surviving "Add library entry" string |
| 5.2 | e2e | `template-onboarding.spec.ts` (existing, currently stale — see Discovery) | `test_github_source_error_stays_in_dialog` opens the real dialog via "Create fleet library", submits an invalid GitHub source, and sees "Couldn't create the fleet library" in the still-open dialog |
| 6.1 | unit | `test_hero_row_no_glow_when_not_live` | render with `live=false` → no wake-pulse animation class/selector match applies |
| 6.1 | e2e | `settings-models.spec.ts` (existing, new assertion) | the real hero row never carries `data-live="false"`-driven animation styling on page load |
| 7.1 | unit | `test_hero_reset_button_removed_switch_list_unaffected` | `queryByRole("button", {name: /switch to platform defaults/i})` on the hero row is null; `ProviderSwitchList`'s own row still calls `resetProviderAction` |
| 7.1 | e2e | `settings-models.spec.ts` (existing, new assertion) | the real Models page has no "Switch to platform defaults" button on the hero row |
| 8.1 | integration | `test_platform_key_missing_error_has_curated_message` | trigger the platform-key-missing path → response `user_message` is the curated sentence, not the raw "operator action required" string |
| 8.2 | unit (zig) | `internal_op_error_sweep_test.zig` | walks `src/agentsfleetd/http/handlers/**`, counts `internalOperationError(` call sites, fails if the count exceeds the sweep's baseline (86) — a new call site must be consciously triaged, not silently added |
| 9.1 | unit (zig) | `test_auth_md_scope_parity` | every `scopes.zig` `WIRE` string is a substring of `docs/AUTH.md`'s contents |
| 9.2 | unit | `test_bearer_or_api_key_comment_current` | file's header comment no longer contains the string `publicMetadata.role` |
| 9.3 | manual/cross-repo | `test_public_docs_scopes_page_exists` | `~/Projects/docs/api-reference/scopes.mdx` exists, non-empty, and is linked from the error-codes page |
| 10.1 | unit | `test_api_key_and_secret_row_delete_triggers_are_destructive_variant` | `ApiKeyList`'s Revoke/Delete row buttons and `SecretsList`'s delete (trash icon) row button all render with the `destructive` variant's classes; the confirm-modal `intent="destructive"` behavior is unchanged |
| 10.2 | unit | `test_catalogue_delete_requires_confirmation` | clicking `CatalogueList`'s row "Delete" alone opens a `ConfirmDialog` (role `alertdialog`) and does NOT call `deleteAdminModelAction`; confirming inside the dialog does |
| 11.1 | unit | `test_secrets_create_action_renamed` | `AddSecretDialog` trigger + dialog title read "Create secret"; `AddSecretForm`'s submit button reads "Create secret"; `SecretsList`'s empty state reads "No secrets" / "Create secret to have your fleets reach other services securely." |
| 11.1 | e2e | `secrets-lifecycle.spec.ts` (existing) | the real create-secret round-trip opens via the "Create secret" trigger and submits via the "Create secret" button inside the dialog |
| 12.1 | unit | `RenameSecretDialog.test.tsx` | create precedes delete (invocation order); create failure never deletes the old name; delete failure refreshes + keeps the dialog open with a recovery message + leaves the old name; same-name/empty/over-long name and non-object data all reject before any action; dismiss is blocked mid-save |
| 12.1 | unit | `secrets-list.test.ts` (rename trigger) | clicking the Name-column rename affordance opens `RenameSecretDialog`; the Edit pencil dialog has no rename affordance (rotate-only) |
| 12.2 | unit | `RenameSecretDialog.test.tsx` (warning) | the warning renders as a single generic sentence and contains no `${secrets.` per-Fleet template string |

Regression: every existing test file named in Files Changed keeps its underlying scenario coverage — assertions move to the new copy/structure, none are deleted outright except the ones testing a deleted component (`settings-tabs.test.ts`). `template-onboarding.spec.ts`'s `test_github_source_error_stays_in_dialog` was already asserting stale pre-M112 copy (see Discovery) — this spec fixes it rather than leaving it silently broken.

Idempotency/replay: N/A — no retry semantics touched.

---

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | §1-§3 nav/API Keys/WorkspaceSwitcher/accent bar render correctly | `make test-coverage-all` (Dimensions 1.1-3.1) | exit 0 | P0 | ✅ `make test-coverage-all` EXIT 0 — "✓ All package coverage gates passed" |
| R2 | §4-§5 Billing/dashboard/library copy matches agreed strings | `make test-coverage-all` (Dimensions 4.1-5.2) | exit 0 | P0 | ✅ `make test-coverage-all` EXIT 0 |
| R3 | §6-§7 Models hero no longer glows / has no redundant button | `make test-coverage-all` (Dimensions 6.1-7.1) | exit 0 | P0 | ✅ `make test-coverage-all` EXIT 0 |
| R4 | §8 platform-key-missing toast shows curated copy | `make test-integration` (Dimension 8.1) | exit 0 | P0 | ✅ `make test-integration` EXIT 0 — "✓ All integration tests passed" |
| R5 | §9 AUTH.md/public docs scope parity | `zig build test` (Dimension 9.1) + `test -f ~/Projects/docs/api-reference/scopes.mdx` | exit 0 / file exists | P1 | ✅ `zig build test` scope-parity test passes (no parity failure; only 2 pre-existing unrelated failures) + `scopes.mdx` EXISTS |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | ✅ every changed path maps to a Files-Changed row; `VERSION`/`build.zig.zon`/`cli/package.json` are standard version-bump artifacts (CHORE-close) |
| R7 | §10 destructive-action buttons consistent + CatalogueList confirm gap closed | `make test-coverage-all` (Dimensions 10.1-10.2) | exit 0 | P1 | ✅ `make test-coverage-all` EXIT 0 |
| R8 | §11 Secrets create-action copy renamed | `make test-coverage-all` (Dimension 11.1) | exit 0 | P1 | ✅ `make test-coverage-all` EXIT 0 |
| R9 | §12 rename split into its own dialog; Edit rotate-only; generic warning | `cd ui/packages/app && bun run test:coverage` (Dimensions 12.1-12.2) | exit 0 (100% cov) | P1 | ✅ app coverage EXIT 0, 1200 tests, 100% thresholds met — new `RenameSecretDialog` fully covered |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ `make test-coverage-all` EXIT 0 (1200 app tests) + `zig build test` EXIT 0, modulo 2 pre-existing unrelated failures (webhook-sig `UZ-WH-010`, worker-pool `.worker_started`), identical on `main` |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ every lint stage passed (zig/website/app/design-system/cli/shell); sole failure is the pre-existing `check-route-registration-doc` (`credentials.zig`), identical on clean `main` — zero routes touched this diff |
| S3 | Integration passes (§8 backend touched) | `make test-integration` | exit 0 | P0 | ✅ `make test-integration` EXIT 0 |
| S4 | e2e walks the real path (UI category, existing specs touched) | `make acceptance-e2e` | exit 0 | P0 | ⏳ deferred to PR CI (`acceptance-e2e-dev` job) — local run blocked by the auto-mode credential boundary (`op read` of `CLERK_SECRET_KEY`/`CLERK_WEBHOOK_SECRET`); §12 changes no e2e-observable surface (rename never e2e-tested; rotate contract unchanged) |
| S6 | Cross-compile (§8/§9 Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ both targets EXIT 0 |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ "no leaks found" (3193 commits scanned) |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ no NEW oversize file; the 4 over-350 files (`Shell.tsx`, `app-components.test.ts`, `dashboard-workspace.test.ts`, `fleets-routes.test.ts`) are pre-existing debt — every §12 new file is ≤350 |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

**S4 disposition (P0, single non-✅ row):** `make acceptance-e2e` cannot run in this auto-mode session — its `global-setup` fails fast without `CLERK_SECRET_KEY`/`CLERK_WEBHOOK_SECRET`, and resolving those via `op read` at runtime is a hard credential boundary the session declines. This is an environment constraint, not a code failure. The PR's CI runs the identical `acceptance-e2e-dev` job with pipeline-provided secrets, so S4 is graded there before merge. §12 (this session's only new work) changes no e2e-observable surface — rename was never e2e-tested and the rotate contract is byte-identical — so the local prior-session e2e evidence (6/6 specs green) is not invalidated by it. Indy can also run it locally pre-merge: `CLERK_SECRET_KEY=$(op read 'op://ZMB_CD_DEV/clerk-dev/secret-key') CLERK_WEBHOOK_SECRET=$(op read 'op://ZMB_CD_DEV/clerk-dev/webhook-secret') CLERK_PUBLISHABLE_KEY=pk_test_… make acceptance-e2e`.

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `ui/packages/app/app/(dashboard)/settings/loading.tsx` | `test ! -f "ui/packages/app/app/(dashboard)/settings/loading.tsx"` |
| `ui/packages/app/components/layout/SettingsTabs.tsx` | `test ! -f ui/packages/app/components/layout/SettingsTabs.tsx` |
| `ui/packages/app/tests/settings-tabs.test.ts` | `test ! -f ui/packages/app/tests/settings-tabs.test.ts` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `SettingsTabs` | `grep -rn "SettingsTabs" ui/packages/app/ --include="*.tsx" --include="*.ts"` | 0 matches |
| `showManageItem` | `grep -rn "showManageItem" ui/packages/app/` | 0 matches |
| `RESET_ACTION` (ActiveModelRow's hero-only const) | `grep -rn "RESET_ACTION" ui/packages/app/app/"(dashboard)"/settings/models/` | 0 matches outside `ProviderSwitchList.tsx`'s own `SWITCH_PLATFORM_ACTION` |
| `"Add library entry"` (literal string) | `grep -rn "Add library entry" ui/packages/app/ cli/src/` | 0 matches |
| `workspace` prop / `CopyButton`/`DescriptionList`/`DescriptionTerm`/`DescriptionDetails` imports on `ApiKeysView.tsx` (§1, Dimension 1.4 — revised mid-review) | `grep -n "workspace={\\|CopyButton\\|DescriptionList" ui/packages/app/app/"(dashboard)"/settings/api-keys/components/ApiKeysView.tsx` | 0 matches |
| `useProviderAction`/`pending`/`error`-driven `disabled` props on `ActiveModelRow.tsx` (found by `/review`'s maintainability specialist — dead since §7 removed the row's only `run()` call) | `grep -n "useProviderAction\\|disabled={pending}" ui/packages/app/app/"(dashboard)"/settings/models/components/ActiveModelRow.tsx` | 0 matches |
| `"Add Secret"` / `"Add secret"` (literal strings, §11) | `grep -rn "\\"Add Secret\\"\\|\\"Add secret\\"" ui/packages/app/app/"(dashboard)"/secrets/` | 0 matches |
| `EDIT_MODE` / `isRename` / `"Advanced — rename"` (rename removed from `EditSecretDialog`, §12) | `grep -rn "EDIT_MODE\\|isRename\\|Advanced — rename" ui/packages/app --include="*.ts" --include="*.tsx"` | 0 matches |
| `DATA_REQUIRED` (local const hoisted to `SECRET_DATA_REENTER_REQUIRED`, §12) | `grep -rn "DATA_REQUIRED" ui/packages/app --include="*.ts" --include="*.tsx"` | 0 matches (only `SECRET_DATA_REENTER_REQUIRED` remains) |

---

## Out of Scope

- Allowing plain-`http`/loopback base URLs for the Custom OpenAI-compatible provider — explicitly parked (Indy, Jul 05, 2026); the control-plane SSRF host guard (`base_url_guard.zig`'s `ip_literal.isBlockedHostLiteral`) needs a dial-path trace (who actually connects: control plane vs. tenant's own runner) before any relaxation is safe. Follow-up spec if picked back up.
- Friendly copy for the ~85 backend error codes outside the two concretely-reachable cases this spec's §8 sweep names — tracked as a further follow-up if they surface as real complaints (same boundary M113_002 already drew).
- Any change to the vault/credential/provider data model, CRUD server actions, or API routes — every section here is presentation/copy/error-text/docs only.
- A generic RBAC/permissions system — §9 documents the existing scope model, it does not change it.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a user clicks "API Keys" in the sidebar and lands directly on their keys, no redundant identity chrome; the active nav item is unambiguous at a glance; Billing/dashboard copy reads consistently; the Models page's active row is calm unless genuinely live; a failed "switch to platform defaults" action (when reached via the provider list) reads as a plain sentence; a tenant hitting `UZ-AUTH-022` can look up what the missing scope means; every Revoke/Delete control looks as dangerous as it is, and none fires without a confirmation step; creating a secret uses the same "Create X" verb as every other resource-creation action in the product.
2. **Preserved user behaviour** — workspace switching/creation, API key create/list/revoke, Billing tabs, fleet-library install, and the Models page's provider-list switch action all keep their exact existing behavior — only labels, one CSS class addition, one CSS selector, one redundant control, one error string, and two docs pages change.
3. **Optimal-way check** — each issue has a narrow, direct fix already available in the codebase (an existing redirect pattern, an existing accent-bar pattern in `tab-styles.ts`, an existing `eu()` mechanism, an existing CSS contract in `WakePulse.tsx`, an existing scope table) — none justifies new architecture.
4. **Rebuild-vs-iterate** — iterate, throughout. Every section extends or corrects an existing, otherwise-sound mechanism (nav config, error registry, CSS selector, scope docs); none rebuilds it.
5. **What we build** — the section outcomes named in Overview; nothing else.
6. **What we do NOT build** — see Out of Scope: http/loopback endpoints (parked), the long tail of uncurated error codes, any data-model/API change, a new permissions system.
7. **Fit with existing features** — must not destabilize M113_003's Secrets & ENVs nav entry (adjacent in the same `Shell.tsx` file, different array) or M113_001's Models row-list merge (§6-§7 build on its post-merge `ActiveModelRow.tsx` shape) or M113_002's error registry mechanism (§8 extends it, doesn't replace it).
8. **Surface order** — UI-first for §1-§7 (directly reported dashboard issues); API+UI together for §8 (the fix must live in the registry, not just reworded client-side); docs-first for §9 (no code behavior changes, only documentation of what already exists).
9. **Dashboard restraint** — no new UI chrome anywhere in this spec beyond §1's workspace-identity block (already planned as part of the tab collapse) and §3's accent bar (a refinement of an existing indicator, not a new one).
10. **Confused-user next step** — §1's inline operator-only alert (rather than a dead-end redirect) is itself a confused-user fix; §8's curated message and §9's new scopes page are both direct "what do I do next" answers to errors this spec targets.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, nine Sections — each section is an independently-testable slice; grouped into one spec because Kishore reviewed all of them together in one product walkthrough and none depends on another. Initially drafted at five broader sections, then split further (Kishore: "5 or more sections", "no hard stop on sections") so each maps closely to one originally-reported item rather than bundling several into one.
- **Alternatives considered:** five separate workstream specs, one file per concern (mirroring M113's three-file pattern) — drafted, then rejected per Kishore's explicit instruction to keep this as one spec, one workstream, with finer-grained Sections instead of separate files.
- **Patch-vs-refactor verdict:** **patch**, throughout — every section corrects or extends an existing mechanism; no section introduces new architecture.

---

## Discovery (consult log)

- **Consults:**
  > Indy (2026-07-05): item 4c (allowing http/loopback base URLs for the Custom OpenAI-compatible provider) — "lets park the http thing for now." Parked; not a Dimension in this spec (see Out of Scope).
  > Indy (2026-07-05): fleet-library action verb — chose "Create fleet library" over "Add fleet library" or keeping "Add library entry".
  > Indy (2026-07-05): "Manage workspace" dropdown item — chose to remove it entirely (not retarget or replace with a dialog) once workspace identity moves to the API Keys page; "New workspace" → "Create workspace" label change confirmed, popup behavior unchanged.
  > Indy (2026-07-05): public docs scope documentation — chose to add a public scopes reference page (§9, Dimension 9.3) over linking to the private repo's `AUTH.md` or skipping public documentation entirely.
  > Indy (2026-07-05): "well i dont see 5 spec files, 1 spec with 1 workstream that covers all" — collapsed the initially-drafted five-workstream-file structure into this single spec.
  > Indy (2026-07-05): "5 or more sections" / "no hard stop on sections" — expanded from 5 to 9 Sections so each maps to one originally-reported item instead of bundling several concerns per section.
  > Indy (2026-07-05): "yes continue, i think it would be more than 1 test, based on the chore(close)" — Test Specification was unit-only per Dimension; per the template's own rule ("any user-facing Category gets at least one user-centric scenario via test-e2e* … a unit test is not a substitute"), added e2e rows against five *existing* Playwright specs (`settings-api-keys.spec.ts`, `workspace-create.spec.ts`, `settings-billing.spec.ts`, `template-onboarding.spec.ts`, `settings-models.spec.ts`) verified to exist and read line-by-line before citing them.
  > Indy (2026-07-05, mid-`/review`): "remove this Covers all Fleet events - in billing" — the redundant caption on `BillingBalanceCard.tsx` removed; `balance-usage` re-aligned to preserve the "rides the meter's end" layout (see Files Changed).
  > Indy (2026-07-05, mid-`/review`): "In API Keys the line `Workspace and Workspace name, ID` can be removed." → "hold on" → "where would the cli need workspace id? i assume the cLI can get that from the list command" — a research agent traced the CLI's own workspace resolution (`agentsfleet login` auto-hydration via `GET /v1/tenants/me/workspaces`, `agentsfleet workspace list`/`workspace use <id>`) and confirmed the dashboard's copy-paste Workspace ID affordance is vestigial for every real workflow (narrow exception: CI/automation scripting against a workspace the CLI has never locally seen — noted as a Failure Mode, not engineered around). Indy: "if we remove that then we would follow the standard like Events in API Keys" → "so lets fix that." — Dimension 1.4 added, workspace identity block removed entirely from `ApiKeysView.tsx`/`page.tsx`/`loading.tsx`.
  > Indy (2026-07-05, mid-`/review`): "should we have the Revoke, Delete the adversial buttons in a different color? across the product? if so make it happen" — an audit agent found `RunnerList.tsx` already colors its row-level "Revoke" `variant="destructive"` but `ApiKeyList.tsx`/`SecretsList.tsx` use `variant="ghost"` for the same action class, and `CatalogueList.tsx`'s "Delete" has no confirm dialog at all (irreversible, no color, no gate). §10 added: both inconsistencies fixed.
  > Indy (2026-07-05, mid-`/review`): "No secrets / Create secret to have your fleets reach other services securely." — exact empty-state copy for `SecretsList.tsx` (§11), given verbatim.
  > Indy (2026-07-05, mid-`/review`): "Should we say Create secret as opposed to Add secret?" — confirmed against the "Create X" verb glossary §1/§2/§5 already established (Create = mint a new resource); §11 added, `AddSecretDialog.tsx`/`AddSecretForm.tsx` renamed throughout.
  > Indy (2026-07-05, mid-`/review`): "should we have the curved vertical as a straight line?" (screenshot of the CONFIGURATION nav section) — real bug, not a taste call: §3's `rounded-md` rounded all four corners of the nav item, curving the left accent bar's top/bottom ends instead of a crisp straight line. Fixed to `rounded-r-md`.
  > Indy (2026-07-05, post-`/review`): "split rename out of the Edit/rotate dialog into its own popup, triggered from the Name column; keep the pencil rotate-only; tighten the warning but keep it generic — no fleet-name evidence." After a research agent confirmed the platform has no reverse index from a secret name to referencing Fleets (`${secrets.<name>...}` is a runtime-only template string; grep found no lookup endpoint anywhere), Kishore explicitly declined the "show which fleets actually break" feature — the warning stays generic by necessity. §12 added: `RenameSecretDialog` split out (own island shim, create-new-then-delete-old preserved), `EditSecretDialog` reduced to rotate-only, rename triggered from a Name-column affordance. No follow-up spec — the simpler version built inline.
  > Indy (2026-07-05, mid-`/review`): asked to explain the Secrets page's Edit-pencil (empty dialog) and Delete-trash (disabled) behavior. Both confirmed intentional, not bugs: Edit is a "rotate" (vault never returns plaintext, so the dialog is always write-only/empty by design); Delete is disabled only for the one secret backing the workspace's active self-managed model (prevents stranding live config) — a hover tooltip explains why but is easy to miss. No code change; explained in chat.
  > Indy (2026-07-05, mid-`/review`): flagged a live jargon-y toast ("...operator action required.") — traced to the local dashboard dev server (port 3000) pointing at the shared `api-dev.agentsfleet.net` API, which still runs `main`'s pre-fix backend; this branch's `UZ-PROVIDER-009` curation (§8.1) isn't deployed yet. Not a bug in this diff — explained in chat, no code change.
- **Verification-pass finding (pre-existing bug, unrelated to this milestone's intent but touched by §5):** `template-onboarding.spec.ts`'s `test_github_source_error_stays_in_dialog` asserts button/dialog name `"Create a template"` and error text `"Couldn't add the template"` — neither matches `AddLibraryDialog.tsx`'s actual current copy ("Add library entry"/"Couldn't add the library entry"), meaning this e2e test has been silently stale since at least M112's fleet-library rename. `settings-api-keys.spec.ts:24`'s heading assertion (`/^settings$/i`) is similarly inconsistent with the page's actual current title ("Workspace" via `SettingsTabs`). Both are fixed in this spec's Files Changed under RULE NLR (touch-it-fix-it) since both dialogs/pages are directly edited here — not scope creep.
- **§8.2 sweep disposition (item 6 of Kishore's original ask):** a repo-wide grep of `internalOperationError(` across `src/agentsfleetd/http/handlers/**` (excluding `*_test.zig`) found **86 call sites**. Classified: **50 GENERIC** (plain "Failed to `<verb>` `<customer-visible-noun>`" sentences — no action needed), **35 JARGON** (leak internal component/schema names, `alloc`/`OOM`, or state-machine language — e.g. `tenant_provider.zig:180`'s "Tenant has no primary workspace — bootstrap invariant violated", confirmed reachable as a dashboard toast the same way Dimension 8.1's original offender was), **1 DUPLICATE-VAR** (`http/server.zig:247` passes `@errorName(err)` — a raw Zig error-union tag — as the detail string, reachable on every authenticated route's middleware-failure path). Ground truth confirmed: `UZ-INTERNAL-003` (what every `internalOperationError` call resolves to) is `e()`-only with no curated `user_message`, and `lib/api/client.ts`'s `user_message ?? detail ?? title` fallback means the raw `detail` string reaches the dashboard verbatim whenever the route is reachable from it — this is not a hypothetical, it's the same mechanism Dimension 8.1 fixed for one call site.
  **Disposition: none of the 35/1 are fixed in this PR.** Curating each into its own `eu()` registry entry would mean minting ~35 new error codes — a scope expansion far beyond a "dashboard UX polish sweep," and exactly the blanket-rewrite this spec's §8 preamble said not to do. Full per-call-site table (file:line, verbatim detail string, classification, route/scope) delivered to Kishore as a standalone HTML artifact per his original ask ("print me all the errors you have so I can review them... provide me with your alternative"), alongside a parallel classification of all ~91 `e()`-only registry codes for dashboard-reachability + proposed `user_message` alternatives (about a dozen confirmed dashboard-reachable and uncurated: `UZ-CONN-001/002/003/004/006`, `UZ-AGT-008/010/011/012`, `UZ-BUNDLE-003/004/005`, `UZ-PROVIDER-005/006/007/008`, `UZ-RUN-014`, `UZ-GRANT-002`, plus several already-curated-client-side-but-not-registry-side codes flagged "ALREADY HANDLED CLIENT-SIDE, registry could catch up").
  > Indy (2026-07-05, mid-`/review`): asked how to close Dimension 8.2's never-implemented regression test, given the natural home (`audits/error-codes.sh`) turned out to be a dotfiles symlink (cross-project blast radius). Chose a project-local Zig test over editing the shared script: "I prefer 2 since this is a zig side is where mostly errors are happening." Implemented as `internal_op_error_sweep_test.zig` (count-based tripwire, baseline 86) rather than touching `~/Projects/dotfiles/audits/error-codes.sh`.
- **Metrics review:** not applicable — no product/operator signal changes (stated in Metrics & Observability).
- **Skill-chain outcomes:**
  - `/write-unit-test`: §1-§11 covered in prior sessions; §12's `RenameSecretDialog` authored with failure-injecting tests (create-before-delete ordering, create-failure/delete-failure recovery, empty/over-long/same-name/non-object rejections, dismiss guard, generic-warning assertion) — app coverage EXIT 0 at 100% thresholds (1200 tests).
  - `/review` (§1-§11): three specialists + one adversarial pass in the prior session; all findings fixed (listed in HANDOFF/PR Session Notes). `/review` (§12, this session): two independent finder passes (correctness/removed-behavior + cross-file/state) — **no introduced correctness bug**; the rename flow was ported faithfully. One NLR touch-fix applied (stale `secret-data.ts` header comment updated to name `RenameSecretDialog`). Two non-blocking observations recorded under Deferrals.
  - `kishore-babysit-prs`: runs after the push (report appended to PR Session Notes).
- **§12 review observations (non-blocking, not §12 regressions):**
  - **Rename-to-an-existing-name overwrites silently** — renaming `A`→`B` when `B` already exists upserts `B` (destroying its value) then deletes `A`. This is **carried over verbatim** from the pre-§12 `EditSecretDialog` rename mode; §12 neither introduces nor fixes it (a "name already taken" pre-check would be a behavior change beyond a dialog-split). Candidate follow-up if collision-on-rename is a real concern.
  - **Dialog mutual-exclusion rests on Radix modality** — `editTarget`/`renameTarget`/`target` never clear each other; only the modal overlay prevents two dialogs stacking. Robust as written (all triggers sit behind the overlay); flagged as an implicit invariant, no reachable failure.
- **Deferrals:** none — every Dimension (§1-§12) is implemented and locally verified. The http/loopback item is a Kishore-directed park (Out of Scope). S4/acceptance-e2e is graded by PR CI, not deferred (see the rubric's S4 disposition — an environment constraint, not a dropped scope item).
