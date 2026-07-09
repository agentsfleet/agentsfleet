<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M120_004: Model library feedback batch — default-row identity+rates, iconified actions, unified add-model dialog

**Prototype:** v2.0.0
**Milestone:** M120
**Workstream:** 004
**Date:** Jul 09, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing feedback batch: the workspace Models table shows a blank Default row today, and two action surfaces still hide behind text buttons / a dropdown.
**Categories:** API, UI
**Batch:** B1 — runs alone; M120_003 (pending internal rename) re-sequences after this spec because both edit the same import surface.
**Branch:** feat/m120-004-model-library-feedback
**Test Baseline:** unit=2393 integration=265
**Depends on:** none outstanding — M120_001/M120_002 are merged; M120_003 stays pending and its `Depends on:` line gains this workstream (amended in the same authoring commit)
**Provenance:** human-directed — Indy's screenshot feedback batch (Jul 09, 2026 session): blank Default-row context, dialog wording, label renames, iconified actions, and the unified add-model dialog (unified-form option Indy-confirmed in-session); agent-drafted against the code as merged at PR #495
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` — platform default resolution + priced model library; this spec adds one read-only field to an existing tenant list response and changes no resolution or billing behavior

---

## Overview

**Goal (testable):** the workspace Models table renders the platform default's model, context, and per-token rates on its Default row (and rates on every library-known entry row); every row action on the Models registry and API-keys tables is an inline icon button; and the add-model dialog is a single Name → Provider → Model → API-key form where "Custom — OpenAI-compatible" is a provider choice that reveals Base URL — with zero remaining "Model id"/"Key name" labels.
**Problem:** the Default row of the workspace Models table is blank — no model, no context, no rates — because the list response only says a default *exists*; entry rows show context but never price; registry actions hide in a `⋯` dropdown and API-key actions are text buttons while every other table got icon actions in M120_002; and adding a model forces a Known-provider vs Custom-endpoint tab choice with a "Key name" that silently rewrites itself while you type.
**Solution summary:** extend `GET /v1/tenants/me/models` with the platform default's identity (provider, model, context cap) — the backend already resolves it and throws it away after boolean-izing; join per-token rates client-side from the already-fetched public model library; replace the registry dropdown and the API-keys text buttons with inline icon actions; collapse the add-model dialog tabs into one form whose Provider dropdown carries the OpenAI-compatible option; and finish the wording/label sweep ("Model id"→"Model", "Key name"→"Name", trimmed create-dialog description).

## PR Intent & comprehension handshake

- **PR title (eventual):** Model library feedback: default-row identity+rates, icon actions, unified add-model dialog
- **Intent (one sentence):** an operator looking at the Models page sees what the platform default actually is and what every model costs, and adds a model — hosted or custom endpoint — through one straightforward form.
- **Handshake** (filled at PLAN, Jul 09, 2026) — restated: the workspace Models page stops hiding what the platform default is and what models cost — the Default row carries the default's real identity/context/rates, every row prices itself from the library, actions are one-click icons instead of a dropdown, and adding a model (hosted or OpenAI-compatible endpoint) is one form with no tabs. ASSUMPTIONS: (1) rotate-in-place submit semantics are preserved verbatim; (2) the Default row's "Use default" action is iconified like entry-row actions; (3) rates shown on own-key rows are the library's informational rate, "—" when unpriced; (4) `platform_default_available` stays on the wire alongside the new object (both derived from one view read); (5) paste-detect provider inference is removed with the tabs (key field moves last, so detection would fire after the provider is already picked). No mismatch with the Intent.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/tenant_model_entries_view.zig` — `buildList` already calls `tenant_provider.platformDefaultView` and discards everything but a boolean; §1 keeps the identity instead. Mirror its ownership/`freeView` discipline for the new heap-owned fields.
2. `ui/packages/app/app/(dashboard)/admin/models/components/CatalogueList.tsx` — the M120_002 icon-action pattern (ghost icon `Button` + per-row `aria-label`, spinner while busy) that §2 and §5 replicate, and the rates formatting (`nanosToUsdPerMtok`, "in / cached / out") §2 reuses.
3. `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider.tsx` + `lib/api/model_caps.ts` — the once-per-session public library fetch (`ModelCap` carries context + all three rates) that §2's client-side rates join reads; no new fetch is added.
4. `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog.tsx` — the tabs + duplicated custom-state machinery §3 collapses; keep its rotate-in-place submit semantics (`submitKnown`) verbatim.
5. `docs/REST_API_DESIGN_GUIDELINES.md` — the list-response extension in §1 follows the existing `emit_null_optional_fields=false` convention (optional object omitted, never null).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/tenant_model_entries_view.zig` | EDIT | `ListResult` gains the platform default identity (provider/model/context cap) instead of boolean-izing it |
| `src/agentsfleetd/http/handlers/tenant_model_entries.zig` | EDIT | serialize the optional `platform_default` object on the list response |
| `src/agentsfleetd/http/tenant_model_entries_integration_test.zig` | EDIT | assert identity present with an active default, omitted without |
| `src/agentsfleetd/db/test_fixtures_provider.zig` | EDIT | seeded platform-default identity constants become `pub` so the new asserts share them (RULE TFX) |
| `src/agentsfleetd/db/test_fixtures.zig` | EDIT | re-export the now-pub identity constants beside the existing provider re-exports |
| `ui/packages/app/lib/types.ts` | EDIT | `TenantModelEntryList` gains optional `platform_default` |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable.tsx` | EDIT | Default-row identity/context/rates; rates join for entry rows; inline icon actions replace the dropdown; already over the 350-line cap → cells extracted (next row) |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCells.tsx` | CREATE | pure presentational cell/action components + format helpers split out of the table (LENGTH GATE) |
| `public/openapi/paths/tenant-models.yaml` | EDIT | `TenantModelEntryList` schema gains the optional `platform_default` object |
| `public/openapi.json` | EDIT | regenerated bundle (`make check-openapi`) — never hand-edited |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog.tsx` | EDIT | unified form: Name/Provider/(Base URL)/Model/API key; tabs and auto-fill machinery removed |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog.tsx` | EDIT | "Model id"→"Model", "Key name"→"Name" |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/lib/detect-provider.ts` | DELETE | paste-detect loses its purpose once the key field moves last; sole consumer is the reworked dialog |
| `ui/packages/app/app/(dashboard)/admin/models/components/AddModelDialog.tsx` | EDIT | description trimmed to Indy's exact wording; "Model id" label → "Model" |
| `ui/packages/app/app/(dashboard)/admin/models/components/EditModelDialog.tsx` | EDIT | "Model id" label + description prose → "Model" |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/ApiKeyList.tsx` | EDIT | Revoke/Delete become icon buttons per the M120_002 pattern |
| `ui/packages/app/tests/detect-provider.test.ts` | DELETE | follows its deleted subject (RULE NDC/ORP) |
| `ui/packages/app/tests/models-registry-table.test.tsx` | EDIT | Default-row identity/rates + icon-action asserts |
| `ui/packages/app/tests/models-registry-add.test.tsx` | EDIT | unified-form flow (field order, free-form Name, rotate-in-place) |
| `ui/packages/app/tests/models-registry-add-custom.test.tsx` | EDIT | custom-endpoint flow rewritten as the OpenAI-compatible provider path |
| `ui/packages/app/tests/models-registry-edit-remove.test.tsx` | EDIT | dropdown selectors → icon-button selectors |
| `ui/packages/app/tests/admin-models-ui.test.ts` | EDIT | wording/label asserts |
| `ui/packages/app/tests/api-keys-components.test.ts` | EDIT | icon-action asserts |
| `ui/packages/app/tests/helpers/dashboard-app-mocks.tsx` | EDIT | registry list mock gains `platform_default` |
| `ui/packages/app/tests/e2e/acceptance/operator-journey.spec.ts` | EDIT | selector updates if the journey walks the reshaped dialog/actions |
| `docs/v2/pending/M120_003_P2_API_UI_MODEL_LIBRARY_NAMING_RENAME.md` | EDIT | `Depends on:` gains M120_004 (same-surface sequencing) |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NDC/ORP** (deleted detect-provider module + test leave zero references; removed tab/auto-fill symbols swept), **UFS** (new label/description strings and the `platform_default` wire key live as named constants where repeated), **TST-NAM** (new test identifiers milestone-free), **DFS** (no dead struct fields left on `ListResult` after the boolean/identity rework).
- **`dispatch/write_zig.md`** — §1 touches Zig handlers: ownership/`errdefer` on the new heap-owned fields, tagged shapes, cross-compile both linux targets.
- **`dispatch/write_ts_adhere_bun.md`** — every `.tsx` edit: design-system primitives only, no raw-HTML substitutes, token utilities.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — optional-object emission convention for the extended list response (§1).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — two Zig files edited | cross-compile `x86_64-linux` + `aarch64-linux` after §1 |
| PUB / Struct-Shape | yes — `ListResult` shape changes | keep it a plain result struct; shape verdict recorded at EXECUTE |
| File & Function Length (≤350/≤50/≤70) | yes — `ModelsRegistryTable.tsx` (363 lines) is already over; `AddModelEntryDialog.tsx` (302) is close | the registry table sheds the dropdown import block and splits cells/actions into a sibling component file if still >350; the dialog shrinks by design (tab + duplicate state removal) |
| UFS (repeated/semantic literals) | yes | wire key `platform_default`, reworded descriptions, and aria-label templates as named constants where >1 use |
| UI Substitution / DESIGN TOKEN | yes — icon buttons, form rework | design-system `Button`/`Select`/`Input`/`Label` primitives; lucide icons sized per the M120_002 sweep (14) |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no log lines, error codes, or schema changes; `core.model_library` untouched |

## Prior-Art / Reference Implementations

- **Reference:** `CatalogueList.tsx` `RowActions` (M120_002) — the exact inline icon-action shape (ghost `Button`, `aria-label` naming the row, spinner-on-busy) §2/§5 replicate; divergence: none.
- **Reference:** `MakeDefaultDialog.tsx` — the conditional Base URL field for the OpenAI-compatible provider (label + https-only helper text) §3 mirrors on the tenant side.

## Sections (implementation slices)

### §1 — Registry list carries the platform default identity

The backend already resolves the active platform default (provider, model, context cap) inside `buildList` and reduces it to `platform_default_available: bool`. Keep the identity: the list response gains an optional `platform_default` object, present exactly when an active default exists. **Implementation default:** reuse the existing single `platformDefaultView` call — no second query; the boolean stays derived from the same view so the two can never disagree.

- **Dimension 1.1** — DONE — with an active platform default, `GET /v1/tenants/me/models` returns `platform_default: {provider, model, context_cap_tokens}` and `platform_default_available: true` → Test `test_entries_list_default_identity`
- **Dimension 1.2** — DONE — with no active default, `platform_default` is omitted (not null) and the boolean is false → Test `test_entries_list_no_default_omits_identity`
- **Dimension 1.3** — DONE — entry-row projection (`has_key`, `base_url`, `context_cap_tokens`, `active`) is byte-identical to before → existing integration asserts pass unchanged → Test `test_entries_list_rows_unchanged` (existing suite)

### §2 — Registry table: Default row shows the default; Context shows rates

The Default row renders the platform default's model (mono, alongside the existing "Default" + lock treatment), its provider label, its context, and its per-token rates. Every entry row's Context cell gains a rates line joined client-side from the once-per-session public model library by `(provider, model_id)` — models not in the library (custom endpoints) show "—" for rates. **Implementation default:** one pure join helper in the table module reading `useModelCatalogue()`; rates format reuses the admin catalogue's "in / cached / out" $-per-1M presentation.

- **Dimension 2.1** — Default row shows model id, provider label, context, and rates when `platform_default` is present → Test `test_registry_default_row_identity`
- **Dimension 2.2** — Default row degrades to "—" with the existing "No default is configured." note when absent → Test `test_registry_default_row_absent`
- **Dimension 2.3** — entry rows show rates when the library knows `(provider, model_id)`, "—" otherwise (including when the library fetch failed) → Test `test_registry_rates_join`
- **Dimension 2.4** — actions render as inline ghost icon buttons — view, switch (hidden while active), edit, delete (disabled while active) — each `aria-label`ed with the model id; no dropdown menu remains in the module → Test `test_registry_actions_iconified`
- **Dimension 2.5** — switch/remove flows (stale-race refresh-on-failure, confirm dialog) behave exactly as before behind the new buttons → Test existing `models-registry-edit-remove` suite green after selector updates

### §3 — Unified add-model dialog

One form, ordered **Name → Provider → Base URL (OpenAI-compatible only) → Model → API key**; the Known-provider/Custom-endpoint tabs and the duplicated custom-state fields die. The Provider dropdown lists the library's providers plus "Custom — OpenAI-compatible" (existing `providerLabel` entry). Name is free-form: nothing auto-fills or rewrites it, so the auto-fill/dirty-tracking and paste-detect machinery is removed with its module. Rotate-in-place submit semantics are kept verbatim.

- **Dimension 3.1** — the dialog renders a single tab-free form in the specified field order; Base URL appears only while the OpenAI-compatible provider is selected → Test `test_add_dialog_unified_order`
- **Dimension 3.2** — OpenAI-compatible selected: API key becomes optional, Base URL required + https-validated inline; named provider selected: API key required, no Base URL field → Test `test_add_dialog_openai_compatible_gating`
- **Dimension 3.3** — typing an API key or picking a provider never mutates Name → Test `test_add_dialog_name_free_form`
- **Dimension 3.4** — same Name + same provider rotates the stored key in place; a Name owned by a different provider's key errors without overwriting → Test existing rotate/mismatch asserts green after rework
- **Dimension 3.5** — the OpenAI-compatible path stores the secret with the same fields as the old custom tab (provider sentinel, base_url, optional api_key) → Test `test_add_dialog_openai_compatible_submit`

### §4 — Admin library dialogs: wording + label sweep

- **Dimension 4.1** — the create dialog description reads exactly "A model library entry prices a model per token. Rates are per 1M tokens." → Test `test_admin_create_dialog_wording`
- **Dimension 4.2** — zero "Model id" / "Key name" strings remain anywhere under `ui/packages/app/app`; the replacement labels are "Model" and "Name" (admin add/edit dialogs, workspace details dialog, add-entry dialog description) → Test `test_no_stale_labels` (grep-based) + rendered-label asserts in the existing dialog suites

### §5 — API-keys actions iconified

- **Dimension 5.1** — Revoke and Delete render as icon buttons (ban-style icon for revoke, trash for delete) with the existing `aria-label`s and disabled/pending behavior preserved → Test `test_api_keys_actions_iconified`

## Interfaces

```
GET /v1/tenants/me/models  (response — extended, backward compatible)
{
  "models": [ { ...unchanged entry rows... } ],
  "platform_default_available": true,
  "platform_default": {                    // present iff an active default exists
    "provider": "anthropic",
    "model": "claude-sonnet-4-6",
    "context_cap_tokens": 200000
  }
}
```

No other endpoint, request shape, or route path changes. The public library document (`cap.json`) and its path are untouched.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| No platform default configured | admin never set one / deactivated | `platform_default` omitted, boolean false; Default row shows "—" + existing "No default is configured." note |
| Library fetch fails client-side | `cap.json` unreachable | rates render "—" on every row; table otherwise fully functional (existing degrade path of `ModelCatalogueProvider`) |
| Non-https Base URL | operator pastes http/loopback endpoint | inline client error before submit; server re-check unchanged (existing `BASE_URL_NOT_HTTPS` + backend SSRF gate) |
| Name owned by another provider's key | reuse typo | error, no overwrite (existing mismatch guard, kept verbatim) |
| Stale activation race | switch/remove races a concurrent mutation | existing refresh-on-failure behavior preserved behind the new icon buttons |
| Vault load failure on a row | secret deleted out-of-band | row degrades to opaque no-key entry (existing view resilience, unchanged) |

## Invariants

1. No key material ever appears in a list response or dialog — `has_key` remains the only signal; enforced by the existing view projection, which §1 extends without touching secret fields.
2. `platform_default` is present iff `platform_default_available` is true — both derive from the single `platformDefaultView` call; enforced by construction and `test_entries_list_no_default_omits_identity`.
3. The public `cap.json` path and `core.model_library` schema are byte-identical before and after — enforced by the diff staying inside Files Changed (rubric R5).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no new or renamed events | — | existing `secret_added`, model-activated, and platform-default events fire unchanged from the reworked surfaces | unchanged | unchanged — no key material in any event | existing analytics asserts in the dialog suites stay green |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_entries_list_default_identity` | active default seeded → list JSON carries provider/model/context identity + true boolean |
| 1.2 | integration | `test_entries_list_no_default_omits_identity` | no default row → `platform_default` key absent from the raw body, boolean false |
| 1.3 | integration (regression) | existing entries-list asserts | entry-row fields byte-identical to the pre-change wire shape |
| 2.1 | unit | `test_registry_default_row_identity` | mock list with identity → Default row renders model id, provider label, context, rates |
| 2.2 | unit | `test_registry_default_row_absent` | mock without identity → "—" + "No default is configured." |
| 2.3 | unit | `test_registry_rates_join` | library hit → "in / cached / out" rates; miss or fetch-error → "—" |
| 2.4 | unit | `test_registry_actions_iconified` | four aria-labeled icon buttons; zero dropdown roles; switch absent on the active row; delete disabled while active |
| 2.5 | unit (regression) | `models-registry-edit-remove` suite | edit/remove flows unchanged behind icon buttons |
| 3.1 | unit | `test_add_dialog_unified_order` | rendered field order Name→Provider→Model→API key; no tablist; Base URL only after OpenAI-compatible pick |
| 3.2 | unit | `test_add_dialog_openai_compatible_gating` | key optional + Base URL required (https) for OpenAI-compatible; key required for named provider |
| 3.3 | unit | `test_add_dialog_name_free_form` | typing `sk-ant-…` into API key and picking providers leaves Name untouched |
| 3.4 | unit (regression) | existing rotate/mismatch asserts | same-name/same-provider rotates; cross-provider name errors |
| 3.5 | unit | `test_add_dialog_openai_compatible_submit` | submit stores provider sentinel + base_url + optional key, then registers the entry |
| 4.1 | unit | `test_admin_create_dialog_wording` | description string equals Indy's exact wording |
| 4.2 | unit (grep-based) | `test_no_stale_labels` | `grep -rn "Model id\|Key name" ui/packages/app/app` → 0 matches |
| 5.1 | unit | `test_api_keys_actions_iconified` | active row: revoke icon button; revoked row: delete icon button; aria-labels + disabled-while-pending preserved |
| all UI | e2e (regression) | `make acceptance-e2e` | operator journey walks the reshaped dialog/actions green (environment permitting) |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | List response carries the default identity (§1) | `make test-integration` | exit 0 incl. both new identity tests | P0 | |
| R2 | Zero stale labels (§4) | `grep -rn "Model id\|Key name" ui/packages/app/app` | no output | P0 | |
| R3 | Tabs dead in the add dialog (§3) | `grep -n "TabsTrigger\|TabsContent" "ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog.tsx"` | no output | P0 | |
| R4 | Dropdown dead in the registry (§2) | `grep -n "DropdownMenu" "ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable.tsx"` | no output | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test && make test-unit-app` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | e2e walks the operator journey | `make acceptance-e2e` | exit 0 (or environment-constraint note per VERIFY tiers) | P1 | |
| S5 | No leaks (Zig view allocation touched) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/lib/detect-provider.ts` | `test ! -f "ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/lib/detect-provider.ts"` |
| `ui/packages/app/tests/detect-provider.test.ts` | `test ! -f ui/packages/app/tests/detect-provider.test.ts` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `detectProviderFromKey` | `grep -rn "detectProviderFromKey" ui/ \| grep -v node_modules` | 0 matches |
| `keyNameDirty` / `autoFillKeyName` | `grep -rn "keyNameDirty\|autoFillKeyName" ui/ \| grep -v node_modules` | 0 matches |
| tab shape constant + custom-state fields (`SHAPE`, `customBaseUrl`, `customApiKey`, `customModel`, `customName`) | `grep -rn "customBaseUrl\|customApiKey\|customModel" ui/ \| grep -v node_modules` | 0 matches |

## Out of Scope

- The M120_003 internal `model_caps`→`model_library` rename — runs after this spec; its blast-radius grep absorbs the import sites this diff adds/moves.
- Reshaping the admin `MakeDefaultDialog` flow — its key+Base-URL collection is correct as merged in M120_002.
- Renaming the admin "Create model library" button/title prose — not part of Indy's feedback; touch only the description string named in §4.
- Any change to the public `cap.json` path, billing/resolution behavior, or `core.model_library` schema.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator opens the workspace Models page and the Default row reads like every other row: which model it is, its context window, what it costs per million tokens — no more blank dashes on the thing every fleet runs by default.
2. **Preserved user behaviour** — switching models, removing entries, rotating a key by reusing its Name, revoking/deleting API keys, and setting the platform default all keep their exact semantics; only their presentation (icons, field order) changes.
3. **Optimal-way check** — the identity ride-along on the existing list response is the most direct fix (the backend already resolves it); the rates join reuses an already-fetched public document; no new endpoint, no second fetch.
4. **Rebuild-vs-iterate** — iterate: five contained slices on merged M120/M121 surfaces; nothing here wants a redesign.
5. **What we build** — one optional response object, one rates join + Default-row rendering, two icon-action sweeps, one unified dialog, one wording/label sweep.
6. **What we do NOT build** — a per-tenant pricing endpoint (public library suffices); a provider-management admin surface; any MakeDefaultDialog rework — see Out of Scope.
7. **Fit with existing features** — compounds M120_002's icon-action language and M121's registry; must not destabilize activation (switch/rotate/remove semantics preserved verbatim).
8. **Surface order** — UI-first by necessity (the feedback is visual), with the one API field the UI needs; CLI unaffected (it reads none of the changed shapes — verified by grep).
9. **Dashboard restraint** — rates render only from the priced library; rows the library can't price show "—" rather than a guessed number.
10. **Confused-user next step** — a missing default shows "No default is configured." inline (existing note, now next to an otherwise-explained row); a rejected Base URL explains the https requirement inline.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five Sections mirroring the five independent feedback clusters — one backend ride-along, one table rendering slice, one dialog rework, two sweeps — each independently testable and DONE-markable.
- **Alternatives considered:** (a) returning rates from the backend list response instead of the client-side join — rejected: duplicates data the client already fetches once per session from the public library, and widens the wire shape for no new information; (b) keeping the tabs and only reordering fields — rejected in-session by Indy in favor of the unified form.
- **Patch-vs-refactor verdict:** this is a **patch** across merged surfaces; the only structural change (tab removal) *shrinks* the dialog rather than restructuring the feature.

## Discovery (consult log)

- **Consults** — Indy confirmed the unified add-model form (provider dropdown carries "Custom — OpenAI-compatible", tabs removed) over keeping tabs, Jul 09, 2026 session.
- **Metrics review** — no analytics/funnel playbook update required: no event added, renamed, or removed; existing events fire unchanged from the reworked surfaces.
- **Skill-chain outcomes** — empty at creation.
- **Deferrals** — empty at creation.
