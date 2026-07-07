<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins - delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M121_001: Tenant Models page becomes a many-model registry — one row per configured model, entries share stored keys, DataTable UI

**Prototype:** v2.0.0
**Milestone:** M121
**Workstream:** 001
**Date:** Jul 07, 2026
**Status:** PENDING
**Priority:** P1 — the shipped 4-slot page cannot represent a real tenant's model set (3 Anthropic models, the same model on two hosts, a keyless local endpoint); Indy hit this on the live page.
**Categories:** API, UI
**Batch:** B1 — folds into the M120_001 branch per Indy's fold-in call; no parallel workstream.
**Branch:** feat/m121-models-registry — added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** M120_001 (DONE — supplies `platform_default_available`, the delete-guard shape, and the model-autocomplete tiers this spec reuses)
**Provenance:** human-directed, LLM-drafted (Fable 5, Jul 07, 2026) — pivot decided after Indy exercised the live 4-row page with a 9-model list; visual/interaction shape settled by a 3-round design shotgun (round 3, Variant C: operations-table).
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §8 (credential model, api_key visibility boundary) — this spec ADDS the tenant model registry noun; the doc gains a matching section at CHORE(close).

---

## Overview

**Goal (testable):** `GET /v1/tenants/me/models` returns one entry per configured model where two entries can reference the same stored key; the Models page renders those entries in a sortable DataTable with the platform Default pinned first; Switch activates any entry without re-entering a key; deleting the active entry — or a vault secret still referenced by entries — is refused with a named error.
**Problem:** the 4 fixed slots hide every Anthropic key after the first, pile all non-Anthropic providers into one "Other provider" bucket, and cannot distinguish the same model on two hosts (GLM 5.2 on fireworks.ai vs wafers.ai). A tenant running many models has no registry of what is configured, and adding a second model for an already-keyed provider forces re-pasting the key.
**Solution summary:** a new `core.tenant_models` table stores one row per configured model entry `(model_id, secret_ref)`; the vault secret keeps the credential (and `base_url`/provider label in its body) while entries carry the model — so N models share one key. Four tenant-scoped endpoints expose list/create/edit/delete with guards. The page becomes a DataTable (Runners/API-Keys conventions): pinned Default row, one row per entry, inline Switch, one ⋯ menu per row (View details / Edit / Remove as dialogs), and an Add-model dialog with a Known provider / Custom endpoint toggle plus a reuse-existing-key path.

## PR Intent & comprehension handshake

- **PR title (eventual):** Models page: many-model registry — entries share stored keys, DataTable UI
- **Intent (one sentence):** a tenant can register every model they use (any provider, any host, key optional), see them all in one table, and switch between them in one click without ever re-pasting a key.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `schema/024_core_tenant_fleet_library.sql` + `docs/SCHEMA_CONVENTIONS.md` — the nearest per-tenant registry table (tenant rows referencing a library); mirror its key/constraint/teardown conventions for `core.tenant_models`.
2. `src/agentsfleetd/http/handlers/tenant_provider.zig` — the tenant-scoped handler to mirror (auth, tenant resolution, `readProviderView`, `platform_default_available`); the new handler sits beside it and reuses its resolver plumbing.
3. `src/agentsfleetd/http/handlers/fleets/secret_metadata.zig` — the decrypt-at-read metadata projection; the models list joins entries to this projection for provider/base_url/kind display without ever exposing `api_key`.
4. `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/api-keys/components/ApiKeyList.tsx` and `.../admin/runners/components/RunnerList.tsx` — the DataTable-with-sortable-headers + Badge + row-actions conventions this page must match verbatim.
5. `docs/architecture/billing_and_provider_keys.md` §8 — the api_key visibility boundary and the `secret_ref` indirection this spec extends; the resolve/activate path (`PUT /v1/tenants/me/provider`) is reused unchanged.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/027_core_tenant_models.sql` | CREATE | registry table: id, tenant id, `model_id`, `secret_ref`, created-at; unique (tenant, model, secret) |
| `schema/embed.zig` | EDIT | embed + migration-array entry for 027 |
| `src/agentsfleetd/state/tenant_models.zig` | CREATE | list/create/update/delete queries + referenced-secret lookup |
| `src/agentsfleetd/state/tenant_models_test.zig` | CREATE | state-layer unit tests |
| `src/agentsfleetd/http/handlers/tenant_models.zig` | CREATE | GET/POST/PATCH/DELETE handlers; active-entry synthesis; guards |
| `src/agentsfleetd/http/router.zig` + `route_matchers.zig` + a `route_table_invoke_*` file + `route_scopes.zig` | EDIT | route registration per existing table pattern |
| `src/agentsfleetd/http/tenant_models_integration_test.zig` | CREATE | endpoint + guard integration tests |
| `src/agentsfleetd/http/handlers/fleets/` secrets delete path | EDIT | extend the delete guard: refuse when entries reference the secret |
| error registry source (where `UZ-PROVIDER-*` rows live) | EDIT | new `UZ-MODELS-001..003` rows |
| `public/openapi/paths/` new `tenant-models.yaml` + `root.yaml` | CREATE/EDIT | document the four endpoints |
| `ui/packages/app/lib/api/tenant_models.ts` | CREATE | typed client for the four endpoints |
| `ui/packages/app/lib/types.ts` | EDIT | entry type + named constants |
| `ui/.../settings/models/page.tsx` | EDIT | compose the registry table instead of the 4-row list |
| `ui/.../settings/models/components/ModelsRegistryTable.tsx` | CREATE | DataTable: pinned Default, entry rows, Switch, ⋯ menu |
| `ui/.../settings/models/components/AddModelEntryDialog.tsx` | CREATE | Known provider / Custom endpoint toggle; reuse-existing-key path |
| `ui/.../settings/models/components/EditModelEntryDialog.tsx` | CREATE | model change + key rotate (blank keeps) |
| `ui/.../settings/models/components/ModelDetailsDialog.tsx` | CREATE | read-only View details |
| `ui/.../settings/models/components/{ProviderSwitchList,ProviderRows,ProviderRowHelpers,ProviderEditPanel,ProviderKeyForm,CustomEndpointForm}.tsx` | DELETE | superseded by the registry table + dialogs |
| `ui/packages/app/tests/{provider-switch-list,provider-edit-panel,provider-key-form,custom-endpoint-form}.test.tsx` | DELETE | superseded suites |
| `ui/packages/app/tests/models-registry-*.test.tsx` (table, add, edit/remove) | CREATE | new suites per Test Specification |
| `ui/packages/app/tests/helpers/models-component-mocks.tsx` | EDIT | stubs follow the new component set |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | new registry section (the `core.tenant_models` noun + entry/key indirection) |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC/NLR (the six superseded components leave no dead code; their helpers `detect-provider.ts`, `known-models.ts`, `ProviderModelSelect.tsx`, `use-provider-action.ts`, `track.ts` are retained and re-wired); ORP (orphan sweep for every deleted symbol); UFS (endpoint paths, error codes, and dialog copy as named constants — `model_id`/`secret_ref` field names shared verbatim across Zig/TypeScript); FLL (every new file ≤350 lines; the table and dialogs are separate files by construction).
- **`dispatch/write_zig.md`** — new state + handler files: `conn.query()`→`.drain()` discipline, tagged-union results, errdefer placement, cross-compile both linux targets.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — four new routes on a plural noun (`/v1/tenants/me/models`), id in path for PATCH/DELETE, RFC 7807 error bodies with registry codes.
- **`docs/SCHEMA_CONVENTIONS.md`** — 027 file shape, pre-v2.0 teardown convention, embed + migration array in the same diff.
- **`dispatch/write_ts_adhere_bun.md`** — all UI files; design-system primitives only (DataTable, Badge, DropdownMenu, Dialog, ConfirmDialog); no raw-HTML substitutes.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile x86_64-linux + aarch64-linux after handler/state work |
| PUB / Struct-Shape | yes | new pub surfaces (state fns, handler, entry struct, TS types) get a FILE SHAPE DECISION at PLAN |
| File & Function Length (≤350/≤50/≤70) | yes | table/dialogs/client are separate files; state vs handler split in Zig |
| UFS | yes | route paths, `UZ-MODELS-*` codes, dialog labels as named constants; `model_id`/`secret_ref` verbatim cross-runtime |
| UI Substitution / DESIGN TOKEN | yes | DataTable/Badge/DropdownMenu/Dialog/ConfirmDialog primitives; token utilities only |
| SCHEMA GUARD | yes | new 027 file + embed.zig + migration array in one diff; no DROP of existing tables |
| ERROR REGISTRY | yes | `UZ-MODELS-001` (delete active), `UZ-MODELS-002` (unknown secret_ref), `UZ-MODELS-003` (duplicate entry) + referenced-secret refusal reuses or extends the secrets-delete code per registry conventions |
| LOGGING / LIFECYCLE | yes | handler logging per LOGGING_STANDARD; no new long-lived resources beyond pg conns (drain-audited) |

## Prior-Art / Reference Implementations

- **Schema/state** → `schema/024_core_tenant_fleet_library.sql` + its state module — per-tenant registry referencing a shared library; same row-ownership and teardown shape.
- **API** → `src/agentsfleetd/http/handlers/tenant_provider.zig` (tenant scoping, view building) and the api-keys handler family for list+mutate CRUD (Create/Read/Update/Delete) on a tenant-owned collection.
- **UI** → `ApiKeyList.tsx` (sortable DataTable, just shipped) + `RunnerList.tsx` (status Badge, row actions) + `admin/models/AddModelDialog.tsx` (dialog form conventions). The activate path reuses `setProviderSelfManagedAction` / `resetProviderAction` unchanged.

## Sections (implementation slices)

### §1 — Registry table + state layer

`core.tenant_models` stores one row per configured model entry: tenant id, `model_id` (text, the provider-namespaced model string), `secret_ref` (text, NOT NULL — always names a vault secret; keyless endpoints store a secret whose body carries an empty `api_key`, keeping the activate/resolve chain uniform), created-at, and a unique constraint on (tenant, `model_id`, `secret_ref`) so the same model on two hosts is two rows while an exact duplicate is refused. Provider label, kind, and `base_url` are NOT columns — they live in the referenced secret's body and are joined at read via the metadata projection. **Implementation default:** key/id conventions mirror `024_core_tenant_fleet_library.sql`; no static strings in SQL (app-side constants only).

- **Dimension 1.1** — create + list round-trips entries for the owning tenant only → Test `test_tenant_models_create_list_tenant_scoped`
- **Dimension 1.2** — inserting an exact duplicate (tenant, model, secret) fails with the constraint surfaced as a typed state error → Test `test_tenant_models_duplicate_rejected`
- **Dimension 1.3** — update (model change) and delete round-trip; deleting a row leaves the referenced secret untouched → Test `test_tenant_models_update_delete_leaves_secret`

### §2 — Tenant models endpoints + guards

Four endpoints (Interfaces below). GET joins each entry to the secret metadata projection (provider, kind, `base_url`, created-at) and computes `active` per entry by comparing (`secret_ref`, `model_id`) to the tenant's current provider row; the response also carries `platform_default_available` (reused) so the pinned Default row self-gates. **Active-entry synthesis:** when the tenant's active self-managed selection has no matching entry (pre-registry configuration), GET upserts one idempotently — the registry self-heals with no separate backfill. Guards: POST/PATCH validate the referenced secret exists and is a provider-key/custom-endpoint kind; DELETE refuses the active entry (`UZ-MODELS-001`, 409); the secrets delete path refuses a secret still referenced by entries, naming the count. `api_key` is structurally absent from every response.

- **Dimension 2.1** — GET lists entries with joined metadata + per-entry `active` flag → Test `test_models_list_joins_metadata_and_active`
- **Dimension 2.2** — GET with an active selection but empty registry synthesizes the matching entry (idempotent on repeat) → Test `test_models_list_synthesizes_active_entry`
- **Dimension 2.3** — POST with unknown `secret_ref` → 404 `UZ-MODELS-002`; duplicate entry → 409 `UZ-MODELS-003` → Test `test_models_create_guards`
- **Dimension 2.4** — DELETE on the active entry → 409 `UZ-MODELS-001`; on a non-active entry → 204 → Test `test_models_delete_active_guard`
- **Dimension 2.5** — deleting a vault secret referenced by ≥1 entry is refused with the reference count; unreferenced secrets delete as today → Test `test_secret_delete_blocked_when_referenced`

### §3 — Registry DataTable

`ModelsRegistryTable` renders: a pinned Default row (lock glyph, "Use default" action wired to `resetProviderAction`, disabled with inline copy when `platform_default_available` is false — M120_001 behavior carried), then one sortable row per entry: model, provider/host (from joined metadata), context (right-aligned, tabular numerals, from the model-caps catalogue when known), status Badge (`Active` green outline on the active entry; `no key · local` on empty-key entries), and an actions cell: inline Switch button (inactive rows) + one ⋯ DropdownMenu. **Implementation default:** column alignment, sortable headers, and Badge variants copied from `ApiKeyList.tsx`/`RunnerList.tsx` — this page must be visually indistinguishable in conventions from those two.

- **Dimension 3.1** — N entries render N rows plus the pinned Default first; sorting by model/provider reorders entries but never unpins Default → Test `test_registry_renders_rows_with_pinned_default`
- **Dimension 3.2** — Switch on an inactive row calls the existing activate action with that entry's (`secret_ref`, `model_id`); the Active badge moves; no key is requested → Test `test_switch_activates_entry_without_key`
- **Dimension 3.3** — Default row's Use-default disabled with explanatory copy when no platform default exists (regression from M120_001) → Test `test_use_default_disabled_when_unavailable`

### §4 — Add model dialog (two shapes, reuse-existing-key)

One dialog, segmented **Known provider | Custom endpoint**. Known: paste a key (provider detected via `detect-provider.ts`) **or** pick "use existing key" from the tenant's stored provider keys — the reuse path never shows a key field; model comes from the three-tier autocomplete (admin catalogue → `known-models.ts` → free text, M120_001 §6 carried). Custom: endpoint, key **optional** (blank stores an empty `api_key` in the secret body), model, provider name. Submitting creates the secret first when a new key/endpoint was entered (body carries credential + `base_url` + provider label; the body no longer carries `model` for new writes — entries own the model; the legacy body field remains readable), then POSTs the entry; primary action "Save & make active" also activates, secondary "Save" only registers.

- **Dimension 4.1** — reuse-existing-key: with an Anthropic key stored, adding a second Anthropic model shows no key field and creates an entry sharing the same `secret_ref` → Test `test_add_second_model_reuses_key`
- **Dimension 4.2** — custom shape with blank key registers a keyless entry (secret body `api_key` empty) that can be activated → Test `test_keyless_custom_endpoint_registers_and_activates`
- **Dimension 4.3** — known shape with a pasted key stores a secret without `model` in the body and the new entry appears in the table → Test `test_known_provider_add_stores_secret_and_entry`

### §5 — Row actions: View / Edit / Remove

The ⋯ menu carries View details (read-only dialog: provider, kind, endpoint, model id, key name + created-at from metadata — never the key material), Edit (dialog: model field pre-filled + key field blank-keeps, wired to PATCH + the existing rotate action), and Remove (ConfirmDialog; disabled with the reason on the active entry). Dialog footer buttons are text-only per the app's dialog conventions.

- **Dimension 5.1** — Edit saves a model change via PATCH; entering a key also rotates the shared secret (both models on that key now use it) → Test `test_edit_changes_model_and_optionally_rotates`
- **Dimension 5.2** — Remove on a non-active entry deletes it after confirm; the shared secret and sibling entries survive → Test `test_remove_entry_keeps_secret_and_siblings`
- **Dimension 5.3** — Remove is disabled with reason on the active entry → Test `test_remove_disabled_on_active_entry`

### §6 — Supersede sweep + docs

The six 4-row-era components and their four test suites are deleted; `page.tsx` composes the registry; the orphan sweep proves zero references; `docs/architecture/billing_and_provider_keys.md` gains the registry section (entries/secret indirection, guard semantics); the changelog entry describes the registry as the Models page (never "replacing the 4-row page" — that never shipped to users).

- **Dimension 6.1** — every deleted symbol greps to zero non-historical references → Acceptance (Dead Code Sweep table)
- **Dimension 6.2** — the architecture doc carries the registry section in the same PR → Acceptance (doc diff present)

## Interfaces

```
GET    /v1/tenants/me/models
  → { models: [ { id, model_id, secret_ref, provider, kind, base_url?, has_key,
                  context_cap_tokens?, active, created_at } ],
      platform_default_available }
POST   /v1/tenants/me/models        { model_id, secret_ref }            → 201 entry
PATCH  /v1/tenants/me/models/{id}   { model_id }                        → 200 entry
DELETE /v1/tenants/me/models/{id}                                       → 204
  409 UZ-MODELS-001 when {id} is the active entry
  404 UZ-MODELS-002 when secret_ref names no vault secret (POST)
  409 UZ-MODELS-003 on duplicate (tenant, model_id, secret_ref)
Activation is NOT new surface: PUT /v1/tenants/me/provider (existing) with the
entry's { secret_ref, model }. Secrets create/rotate/delete endpoints unchanged
except the delete guard (refusal names the referencing entry count).
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Delete active entry | user removes the entry currently powering inference | 409 `UZ-MODELS-001`; UI pre-disables Remove with the reason |
| Unknown secret_ref | POST references a deleted/renamed secret | 404 `UZ-MODELS-002`; dialog surfaces the friendly registry copy |
| Duplicate entry | same (model, key) added twice | 409 `UZ-MODELS-003`; dialog says the entry already exists |
| Secret still referenced | vault secret delete while entries point at it | 409 naming the count; Secrets page copy tells the user to remove entries first |
| Platform default absent | no active platform key | pinned Default row disabled with inline copy; no round-trip error (M120_001 carry) |
| Stale activation | Switch races a concurrent entry delete | activate validates `secret_ref` at PUT (existing path); UI surfaces the existing friendly error and refreshes the list |

## Invariants

1. `api_key` never appears in any `/models` response or the entries table — structural: no key column exists; responses build from the metadata projection (§8.2 boundary) — enforced by schema shape + `test_models_list_joins_metadata_and_active` asserting field absence.
2. Every entry's `secret_ref` names an existing vault secret at write time, and a referenced secret cannot be deleted — enforced by POST/PATCH validation + the extended delete guard (Dimensions 2.3/2.5).
3. The active selection is always representable: GET synthesis guarantees the active (`secret_ref`, `model`) pair has an entry row — enforced by the idempotent upsert (Dimension 2.2).
4. At most one entry per (tenant, `model_id`, `secret_ref`) — enforced by the database unique constraint (Dimension 1.2).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no new events | — | — | — | — | — |

Existing events are reused unchanged: `model_activated` on Switch, `secret_added` on key store, `key_rotated` on rotate, `provider_reset` on Use-default. Entry create/delete emits no analytics event (parity with the Secrets page). No funnel changes; no playbook update required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit (Zig, DB) | `test_tenant_models_create_list_tenant_scoped` | two tenants insert entries → each lists only its own |
| 1.2 | unit (Zig, DB) | `test_tenant_models_duplicate_rejected` | same (tenant, model, secret) twice → typed constraint error |
| 1.3 | unit (Zig, DB) | `test_tenant_models_update_delete_leaves_secret` | delete entry → vault row still present |
| 2.1 | integration | `test_models_list_joins_metadata_and_active` | seeded key + 2 entries → provider/kind/base_url joined, exactly one `active:true`, no `api_key` key anywhere in body |
| 2.2 | integration | `test_models_list_synthesizes_active_entry` | active selection, empty registry → first GET creates the entry; second GET creates nothing |
| 2.3 | integration (negative) | `test_models_create_guards` | unknown ref → 404 UZ-MODELS-002; duplicate → 409 UZ-MODELS-003 |
| 2.4 | integration (negative) | `test_models_delete_active_guard` | active entry → 409 UZ-MODELS-001; other → 204 |
| 2.5 | integration (negative) | `test_secret_delete_blocked_when_referenced` | referenced secret delete → 409 with count; unreferenced → deletes |
| 3.1 | unit (vitest) | `test_registry_renders_rows_with_pinned_default` | 9 entries → 10 rows, Default first, sort keeps pin |
| 3.2 | unit (vitest) | `test_switch_activates_entry_without_key` | click Switch → activate action called with entry (secret_ref, model); no key input rendered |
| 3.3 | unit (vitest, regression) | `test_use_default_disabled_when_unavailable` | `platform_default_available:false` → disabled + copy |
| 4.1 | unit (vitest) | `test_add_second_model_reuses_key` | stored anthropic key → add dialog reuse path shows no key field; POST body carries the shared secret_ref |
| 4.2 | unit (vitest) | `test_keyless_custom_endpoint_registers_and_activates` | blank key + base_url → secret created with empty api_key; entry activate enabled |
| 4.3 | unit (vitest) | `test_known_provider_add_stores_secret_and_entry` | pasted key → secret body has no model field; table gains the row |
| 5.1 | unit (vitest) | `test_edit_changes_model_and_optionally_rotates` | edit model → PATCH; key entered → rotate also called |
| 5.2 | unit (vitest) | `test_remove_entry_keeps_secret_and_siblings` | confirm remove → DELETE entry only; sibling row remains |
| 5.3 | unit (vitest, negative) | `test_remove_disabled_on_active_entry` | active row menu → Remove disabled with reason |

Regression: the M120_001 suites being deleted are replaced 1:1 by rows above (default-row gating 3.3, delete guards 2.4/5.3, autocomplete tiers exercised inside 4.1/4.3). End-to-end: the settings page is auth-gated; per repo precedent (M119/M120) the dry page-render lane plus integration on the API stands in for a Clerk-authenticated e2e — no new e2e lane in this spec.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Registry API with guards (§1/§2) | `make test-integration` | exit 0; the six §2 tests pass | P0 | |
| R2 | Registry table + dialogs (§3–§5) | `make test-unit-app` | exit 0; the nine §3–§5 tests pass | P0 | |
| R3 | Shared-key add never re-asks for a key (§4) | `make test-unit-app` | Dimension 4.1 passes | P0 | |
| R4 | Supersede sweep (§6) | Dead Code Sweep greps | 0 matches on every deleted symbol | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `ui/.../models/components/ProviderSwitchList.tsx` | `test ! -f "ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderSwitchList.tsx"` |
| `ProviderRows.tsx`, `ProviderRowHelpers.tsx`, `ProviderEditPanel.tsx`, `ProviderKeyForm.tsx`, `CustomEndpointForm.tsx` (same dir) | `test ! -f` each |
| `ui/packages/app/tests/{provider-switch-list,provider-edit-panel,provider-key-form,custom-endpoint-form}.test.tsx` | `test ! -f` each |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `ProviderSwitchList` | `grep -rn -w "ProviderSwitchList" ui/ src/ --include="*.ts*"` | 0 matches |
| `ProviderRows` / `ProviderRowHelpers` / `ProviderEditPanel` | same pattern per symbol | 0 matches |
| `ProviderKeyForm` / `CustomEndpointForm` | same pattern per symbol | 0 matches |

## Out of Scope

- Runner/NullClaw behavior for keyless endpoints (empty `api_key` rides the existing envelope; header behavior unchanged — any provider-side auth failure surfaces as today's inference error).
- Admin Model Library data model and `/admin/models` presentation (M120_002/M120_003).
- CLI surface for the registry (`agentsfleet tenant provider add` continues to work against the activate path; a registry CLI is future work).
- Per-entry spend/metering display; per-workspace vault isolation (post-v2.0 per the architecture doc).

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy opens Models, sees all nine of his configured models (three Anthropic on one key, GLM 5.2 on two hosts, a keyless local Qwen) in one sortable table, clicks Switch on Fable 5, and the Active badge moves — no key pasted, no error.
2. **Preserved user behaviour** — activate/reset/rotate actions and their events unchanged; Secrets page flows unchanged except the new referenced-secret refusal; Default row semantics (lock, gated Switch) carried from M120_001.
3. **Optimal-way check** — a registry table is the direct representation of "many configured models"; the rejected alternative (keeping fixed slots with pickers) hides exactly the states Indy hit. Entries-reference-keys is the minimal schema that kills key re-pasting.
4. **Rebuild-vs-iterate** — iterate on the credential/resolve substrate (M45/M87/M100 — untouched); rebuild the presentation layer only. The 4-row components are deleted, not adapted — adapters would preserve a shape the data model has outgrown.
5. **What we build** — one table (`core.tenant_models`), four endpoints, one guard extension, one DataTable page, three dialogs, the sweep.
6. **What we do NOT build** — no new activation path, no analytics events, no CLI, no admin-library changes, no runner changes.
7. **Fit with existing features** — compounds with M100 platform-default resolve and M102 rotate; must not destabilize the billing posture switch (`core.tenant_providers` untouched structurally).
8. **Surface order** — User Interface (UI)-first with one additive API family; CLI explicitly deferred (Out of Scope) since the dashboard is where the many-model pain was hit.
9. **Dashboard restraint** — no per-model health/latency indicators (no signal behind them yet); status column carries only Active and the keyless note, both facts the backend already knows.
10. **Confused-user next step** — every refusal names its reason inline (active-entry, referenced-secret, duplicate); the empty registry state points at "+ Add model"; free-text model entry remains the escape hatch when autocomplete misses.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six Sections — schema/state, endpoints+guards, table, add dialog, row actions, sweep — one workstream, because every slice serves the single outcome (the registry) and the UI slices share the new client/types.
- **Alternatives considered:** (a) keep 4 slots + add pickers everywhere — rejected: cannot represent same-model-two-hosts or per-model rows, and the M120 live review already showed the bucket row failing; (b) UI-only registry reading secrets-with-model bodies (no new table) — rejected: keeps key-per-model coupling, so shared keys and "add model without key" stay impossible; (c) split backend and UI into two workstreams — rejected: the UI is unshippable without the API and Indy directed one PR.
- **Patch-vs-refactor verdict:** this is a **refactor** of the Models presentation + a small additive data model, deliberately superseding M120_001's presentation in the same branch per Indy's fold-in call; the credential substrate is untouched.

## Discovery (consult log)

- **Consults** — Design shotgun round 2 (Codex, three variants) then round 3 (Fable 5: A accordion / B master-detail / C ops-table) after Indy rejected the 4-slot model against his real 9-model list. **Indy's pick (2026-07-07): Variant C** — "I will go with C … Just follow standard alignment data table and so on so this looks and appears like other pages." Sequencing: fold into this branch — Indy: "drive the registry version to done here, per Indys fold-in call … ensure its fixed in this PR. feel free to rename the branch of the PR and update the description of the PR when you push." Shared-key design: Indy chose "Proper: models reference a key" over paste-per-row. Numbering: Indy wrote "M121_004"; normalized to M121_001 (first workstream of a new milestone) — flagged in-session. Architecture consult: `billing_and_provider_keys.md` §8 read; the registry noun is new — the doc gains it at CHORE(close) (Dimension 6.2).
- **Metrics review** — no analytics/funnel playbook update required: no new events (table above).
- **Skill-chain outcomes** — empty at creation.
- **Deferrals** — empty at creation.
