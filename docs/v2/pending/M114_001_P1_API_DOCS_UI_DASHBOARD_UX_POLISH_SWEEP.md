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
**Status:** PENDING
**Priority:** P1 — concretely-reported dashboard issues from a live product walkthrough this session (redundant nav surface, stale copy, a visibly-broken animation, an operator-jargon error toast); none blocks another, none needs new architecture.
**Categories:** API, DOCS, UI
**Batch:** B1 — single workstream; §1-§3 (nav/API Keys/WorkspaceSwitcher/accent bar) share `Shell.tsx` and `WorkspaceSwitcher.tsx`, §4-§9 each touch disjoint files.
**Branch:** set at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none
**Provenance:** LLM-drafted (Claude, Jul 05, 2026) from a live plan-mode product review this session — three parallel Explore agents traced exact file:line locations across `ui/packages/app/`, `cli/`, and `src/agentsfleetd/`; Kishore made explicit calls on naming/removal/structure recorded in Discovery.
**Canonical architecture:** `docs/AUTH.md` §Scope catalogue (§9 only) — §1-§8 are presentation-layer fixes with no dedicated architecture doc, same citation pattern as M113_001/M113_003 ("layout/presentation only, data model unchanged").

---

## Overview

**Goal (testable):** the Organization nav item that duplicated "Manage workspace" is gone and its API Keys destination stands alone with workspace identity on it, the active nav item shows a left accent bar; Billing and dashboard/library copy match the agreed wording, including a full "Add library entry" → "Create fleet library" rename; the Models page's active-model row never animates when it isn't live and has no redundant reset control; the "platform defaults" error toast reads as a sentence, not an operator log line; `docs/AUTH.md` and the public docs site both list every scope a tenant can be granted.

**Problem:** a live UI walkthrough found a two-tab settings page where one tab (Workspace) duplicates the top-right "Manage workspace" menu; a nav-selected state with no directional indicator beyond a background tint; four stale/inconsistent copy strings on Billing and the Fleet dashboard plus an inconsistently-named onboarding action; a CSS selector bug that makes the Models-page hero row glow permanently regardless of live state, plus a redundant "Switch to platform defaults" button already covered by the provider list below it; a raw backend string ("…operator action required") leaking into a customer-facing toast; and a scope (`platform-library:write`) that exists in code and gates real functionality but has no row in the scope-catalogue doc, with no public reference for scopes at all.

**Solution summary:** collapse the Workspace/API-Keys tab pair into a single API Keys page carrying workspace name+ID (§1); rename "New workspace" and remove the redundant "Manage workspace" item (§2); add a left accent bar to the active nav item (§3); fix Billing copy (§4) and dashboard/fleet-library copy including the "Create fleet library" rename (§5); fix the Models page's `[data-live]` CSS selector (§6) and delete its redundant hero reset control (§7); give the platform-key-missing case its own curated registry entry instead of a raw passthrough, after producing a reviewable inventory of the error registry (§8); add the missing scope-catalogue row, fix a stale comment, add a doc/code parity test, and ship a public scopes reference page (§9).

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
| `ui/packages/app/app/(dashboard)/settings/api-keys/page.tsx` | EDIT | additionally resolves active workspace + full workspace list; renders header directly, no `SettingsTabs` |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/ApiKeysView.tsx` | EDIT | title "API Keys"; workspace name/ID block with `CopyButton`s; inline operator-only `Alert` on 403 instead of a redirect |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog.tsx` | EDIT | trigger "New API key" → "Create key"; dialog title → "Create API key" |
| `ui/packages/app/components/layout/WorkspaceSwitcher.tsx` | EDIT | "New workspace" → "Create workspace" (§2); delete the "Manage workspace" item and its `showManageItem` prop (§2) |
| `ui/packages/app/app/(dashboard)/settings/billing/page.tsx` | EDIT | subtitle → "Manage credits and usage."; "Payment Method" → "Payment method" |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx` | EDIT | "Purchase credits" → "Buy credits" with a leading icon, becomes an active `mailto:agentsfleet@agentsmail.to` link (drops `disabled`/`aria-disabled`/`pointer-events-none`) |
| `ui/packages/app/app/(dashboard)/page.tsx` | EDIT | dashboard description string → "Start a fleet from the prebuilt fleet library." |
| `ui/packages/app/app/(dashboard)/fleets/new/library-docs.tsx` | EDIT | empty-state title/description strings updated; comments referencing "Add library entry" updated |
| `ui/packages/app/app/(dashboard)/fleets/new/AddLibraryDialog.tsx` | EDIT | trigger/DialogTitle/action-verb/spinner-label/submit renamed "Add library entry" → "Create fleet library"; GitHub-repo description lowercases "fleet library entry" |
| `ui/packages/app/app/(dashboard)/fleets/new/InstallEntry.tsx` | EDIT | CTA renamed to match |
| `cli/src/commands/fleet_library.ts` | EDIT | empty-state message aligned to "No prebuilt fleet library found." |
| `ui/packages/app/app/(dashboard)/settings/models/components/ActiveModelRow.tsx` | EDIT | drop the unconditional `data-live` attribute (§6); delete the "Switch to platform defaults" button, its handler, and `RESET_ACTION` (§7) |
| `ui/packages/design-system/src/tokens.css` | EDIT | `[data-live]` → `[data-live="true"]` so the wake-pulse animation only fires when actually live |
| `src/agentsfleetd/http/handlers/tenant_provider.zig` | EDIT | platform-key-missing path returns a dedicated error code instead of a raw `internalOperationError` literal |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | new `eu()` entry for the platform-key-missing code with a curated `user_message` |
| `docs/AUTH.md` | EDIT | add the missing `platform-library:write` row to the Discrete-verbs table; add a short "development provisioning" note under Provisioning grants |
| `src/agentsfleetd/auth/middleware/bearer_or_api_key.zig` | EDIT | fix the stale line-6 comment (still describes `publicMetadata.role` gating; gating is scope-based) |
| `src/agentsfleetd/auth/scopes_test.zig` (or a new `auth_md_parity_test.zig`) | EDIT/CREATE | parity test: every `scopes.zig` `WIRE` string appears in `docs/AUTH.md` |
| `~/Projects/docs/scopes.mdx` (separate repo, own branch `chore/m114-scopes-docs-changelog`) | CREATE | public scopes reference page linked from the existing `api-reference/error-codes` page |
| test files: `app-components.test.ts`, `settings-tabs.test.ts` (deleted with the component), `api-keys-page.test.ts`, `api-keys-components.test.ts`, `dashboard-workspace.test.ts`, `billing-card.test.ts`, `billing-tabs.test.ts`, `dashboard-placeholder.test.ts`, `fleets-install-entry-gate.test.ts`, `fleets-routes.test.ts`, `add-template-dialog.test.tsx`, `add-template-dialog-deep-link.test.tsx`, `fleets-install-flow.test.ts`, `active-model-row.test.tsx`, `provider-switch-list.test.tsx`, `cli/test/fleet-library.unit.test.ts` | EDIT | assertions updated to the new copy/structure named above |
| `ui/packages/app/tests/e2e/acceptance/settings-api-keys.spec.ts` | EDIT | heading assertion `/^settings$/i` → `/^api keys$/i`; the `"settings sections"` nav-landmark lookup (`SettingsTabs`'s nav, deleted in §1) reworked to look up the sidebar link directly; `"new api key"` button name → `"create key"` |
| `ui/packages/app/tests/e2e/acceptance/workspace-create.spec.ts` | EDIT | `getByTestId("workspace-new")`'s accessible name / the "New workspace" dialog name updated to "Create workspace" (§2) |
| `ui/packages/app/tests/e2e/acceptance/settings-billing.spec.ts` | EDIT | line ~35's `toBeDisabled()` assertion on `{name: "Purchase Credits"}` replaced with an enabled-link assertion on `{name: "Buy credits"}` with `href="mailto:agentsfleet@agentsmail.to"` |
| `ui/packages/app/tests/e2e/acceptance/template-onboarding.spec.ts` | EDIT | **pre-existing bug found during spec authoring, unrelated to but touched by §5**: `test_github_source_error_stays_in_dialog` asserts button/dialog name `"Create a template"` and error text `"Couldn't add the template"` — neither matches the current `AddLibraryDialog.tsx` copy ("Add library entry"/"Couldn't add the library entry"), so this test is already stale pre-M112. Fixed to assert the new "Create fleet library"/"Couldn't create the fleet library" copy (RULE NLR, touch-it-fix-it — this spec edits the same dialog) |
| `ui/packages/app/tests/e2e/acceptance/settings-models.spec.ts` | EDIT | new assertions added: no wake-pulse animation state when not live, no "Switch to platform defaults" button on the hero row (this file does not test either today) |

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
- **Dimension 1.2** — the API Keys page shows title "API Keys", the existing subtitle "Authenticate with the agentsfleet API. Each key is shown once.", the active workspace's name + ID with copy buttons, and a "Create key" action (not "New API key") → Test `test_api_keys_page_shows_workspace_identity_and_create_key`
- **Dimension 1.3** — a 403 on the API-keys list renders the existing "API keys need admin access" alert inline on this page, replacing the old `?notice=api-keys-operator-only` redirect (which targeted a page that no longer exists) → Test `test_api_keys_operator_gate_renders_inline`

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
- **Dimension 8.2** — a repo-wide sweep for other handler call sites passing a raw literal into the same shared internal-error path; each finding gets its own dedicated entry (disposition recorded in Discovery) → Test `test_internal_error_bypass_sweep_closed`

### §9 — Clerk scope documentation parity

`platform-library:write` gates real functionality but has no row in `docs/AUTH.md`'s catalogue; a stale comment still describes role-based gating; and no public page lets a tenant look up what a scope means when they hit `UZ-AUTH-022`.

- **Dimension 9.1** — `docs/AUTH.md`'s Discrete-verbs table includes `platform-library:write` with its grant description; a test asserts every `scopes.zig` `WIRE` string appears somewhere in the doc → Test `test_auth_md_scope_parity`
- **Dimension 9.2** — `bearer_or_api_key.zig:6`'s comment describes scope-based gating, not `publicMetadata.role` → Test `test_bearer_or_api_key_comment_current`
- **Dimension 9.3** — the public docs site (`~/Projects/docs`, own branch) gains a scopes reference page listing the tenant-facing catalogue, linked from the existing `api-reference/error-codes` page → Test `test_public_docs_scopes_page_exists`

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

---

## Invariants

1. Every existing server action/endpoint this spec's UI touches keeps its exact signature — §1-§7 are presentation-only; enforced by unchanged existing action test suites passing unmodified.
2. `[data-live]` never animates unless the attribute's value is literally `"true"` — enforced by Dimension 6.1's test and the CSS selector itself.
3. Every `scopes.zig` `WIRE` string has a corresponding row in `docs/AUTH.md` — enforced by Dimension 9.1's parity test (Zig, reads the doc as text at test time).
4. `ProviderSwitchList`'s "Platform defaults" row keeps working after the hero button is deleted — enforced by Dimension 7.1's regression assertion.

---

## Metrics & Observability

Not applicable — no product/operator signal changes; this spec is copy, a CSS selector fix, a control removal, an error-message curation, and a docs parity fix.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_nav_api_keys_replaces_workspace` | sidebar renders "API Keys" not "Workspace"; visiting `/settings` redirects to `/settings/api-keys` |
| 1.1 | e2e | `settings-api-keys.spec.ts` (existing) | the sidebar link `/api keys/i` navigates to `/settings/api-keys` in a real rendered session — no `"settings sections"` nav landmark remains (that landmark belonged to the deleted `SettingsTabs`) |
| 1.2 | unit | `test_api_keys_page_shows_workspace_identity_and_create_key` | render → title "API Keys", workspace name/ID with copy buttons present, "Create key" button, no "New API key" string |
| 1.2 | e2e | `settings-api-keys.spec.ts` (existing) | heading assertion updated `/^api keys$/i`; `getByRole("button", {name: /new api key/i})` → `/create key/i` in the real mint/reveal/revoke/delete round-trip |
| 1.3 | unit | `test_api_keys_operator_gate_renders_inline` | mock a 403 list response → alert renders on the same page, no navigation occurs |
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
| 8.2 | unit | `test_internal_error_bypass_sweep_closed` | grep-style check: no handler outside `error_entries.zig`'s registry constructs a raw literal for `internalOperationError` beyond the codes this sweep dispositioned |
| 9.1 | unit (zig) | `test_auth_md_scope_parity` | every `scopes.zig` `WIRE` string is a substring of `docs/AUTH.md`'s contents |
| 9.2 | unit | `test_bearer_or_api_key_comment_current` | file's header comment no longer contains the string `publicMetadata.role` |
| 9.3 | manual/cross-repo | `test_public_docs_scopes_page_exists` | `~/Projects/docs/scopes.mdx` exists, non-empty, and is linked from the error-codes page |

Regression: every existing test file named in Files Changed keeps its underlying scenario coverage — assertions move to the new copy/structure, none are deleted outright except the ones testing a deleted component (`settings-tabs.test.ts`). `template-onboarding.spec.ts`'s `test_github_source_error_stays_in_dialog` was already asserting stale pre-M112 copy (see Discovery) — this spec fixes it rather than leaving it silently broken.

Idempotency/replay: N/A — no retry semantics touched.

---

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | §1-§3 nav/API Keys/WorkspaceSwitcher/accent bar render correctly | `make test-unit-app` (Dimensions 1.1-3.1) | exit 0 | P0 | |
| R2 | §4-§5 Billing/dashboard/library copy matches agreed strings | `make test-unit-app` (Dimensions 4.1-5.2) | exit 0 | P0 | |
| R3 | §6-§7 Models hero no longer glows / has no redundant button | `make test-unit-app` (Dimensions 6.1-7.1) | exit 0 | P0 | |
| R4 | §8 platform-key-missing toast shows curated copy | `make test-integration` (Dimension 8.1) | exit 0 | P0 | |
| R5 | §9 AUTH.md/public docs scope parity | `zig build test` (Dimension 9.1) + `test -f ~/Projects/docs/scopes.mdx` | exit 0 / file exists | P1 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes (§8 backend touched) | `make test-integration` | exit 0 | P0 | |
| S4 | e2e walks the real path (UI category, five existing specs touched) | `make acceptance-e2e` | exit 0 | P0 | |
| S6 | Cross-compile (§8/§9 Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

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

---

## Out of Scope

- Allowing plain-`http`/loopback base URLs for the Custom OpenAI-compatible provider — explicitly parked (Indy, Jul 05, 2026); the control-plane SSRF host guard (`base_url_guard.zig`'s `ip_literal.isBlockedHostLiteral`) needs a dial-path trace (who actually connects: control plane vs. tenant's own runner) before any relaxation is safe. Follow-up spec if picked back up.
- Friendly copy for the ~85 backend error codes outside the two concretely-reachable cases this spec's §8 sweep names — tracked as a further follow-up if they surface as real complaints (same boundary M113_002 already drew).
- Any change to the vault/credential/provider data model, CRUD server actions, or API routes — every section here is presentation/copy/error-text/docs only.
- A generic RBAC/permissions system — §9 documents the existing scope model, it does not change it.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a user clicks "API Keys" in the sidebar and lands directly on their keys with workspace identity visible; the active nav item is unambiguous at a glance; Billing/dashboard copy reads consistently; the Models page's active row is calm unless genuinely live; a failed "switch to platform defaults" action (when reached via the provider list) reads as a plain sentence; a tenant hitting `UZ-AUTH-022` can look up what the missing scope means.
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
- **Verification-pass finding (pre-existing bug, unrelated to this milestone's intent but touched by §5):** `template-onboarding.spec.ts`'s `test_github_source_error_stays_in_dialog` asserts button/dialog name `"Create a template"` and error text `"Couldn't add the template"` — neither matches `AddLibraryDialog.tsx`'s actual current copy ("Add library entry"/"Couldn't add the library entry"), meaning this e2e test has been silently stale since at least M112's fleet-library rename. `settings-api-keys.spec.ts:24`'s heading assertion (`/^settings$/i`) is similarly inconsistent with the page's actual current title ("Workspace" via `SettingsTabs`). Both are fixed in this spec's Files Changed under RULE NLR (touch-it-fix-it) since both dialogs/pages are directly edited here — not scope creep.
- **Metrics review:** not applicable — no product/operator signal changes (stated in Metrics & Observability).
- **Skill-chain outcomes:** populated after `/write-unit-test` and `/review` run at VERIFY/CHORE(close).
- **Deferrals:** none yet — the http/loopback item above is a Kishore-directed park, not an agent-unilateral deferral, and is fully out of Dimension scope (Out of Scope), not a deferred Dimension within scope.
