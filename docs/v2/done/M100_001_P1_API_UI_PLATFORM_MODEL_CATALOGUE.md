<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M100_001: Platform admin builds the model catalogue and sets the priced default — all from the UI

**Prototype:** v2.0.0
**Milestone:** M100
**Workstream:** 001
**Date:** Jun 24, 2026
**Status:** DONE
**Priority:** P1 — unblocks self-serve platform onboarding; removes the manual seed + playbook bootstrap that gate every new environment.
**Categories:** API, UI
**Batch:** B1
**Branch:** feat/m100-platform-model-catalogue
**Test Baseline:** unit=2090 integration=202
**Depends on:** M98_002 (custom OpenAI-compatible endpoints; the provider resolve + catalogue-gate path this extends)
**Provenance:** human-directed, LLM-drafted (Opus 4.8, Jun 24, 2026) — reconciled design from the M100 handoff + Indy decisions A–F; supersedes the discarded `is_default`-on-`model_caps` draft.

> **Provenance is load-bearing.** LLM-drafted: cross-check every named symbol against the codebase before EXECUTE.

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` — the priced-catalogue + platform-key resolve model is the source of truth (Architecture Consult & Update Gate). This spec moves catalogue population and default selection from migration-seed + manual playbook onto admin-managed surfaces; update that doc if the resolve path changes.

## Implementing agent — read these first

1. `src/agentsfleetd/state/tenant_provider_resolver.zig` — `resolvePlatformDefault` reads `PLATFORM_DEFAULT_MODEL`/`_CAP_TOKENS` constants today; source model + cap + base_url from the active `platform_llm_keys` row instead.
2. `src/agentsfleetd/http/handlers/admin/platform_keys.zig` — the `PUT`/`DELETE /v1/admin/platform-keys` upsert (`ON CONFLICT (provider) DO UPDATE SET active=true`) to extend with model/base_url/cap + single-active.
3. `ui/packages/app/app/(dashboard)/admin/runners/` — the admin page pattern to mirror exactly: PageHeader + primary action, `divide-y rounded-md border` list, `Badge` variants, `Dialog` + `Form`, Server Action data flow.
4. `schema/{006_platform_llm_keys,019_model_caps}.sql` + `schema/embed.zig` — the two tables + migration-array discipline (single-concern, ≤100 lines, no shipped-migration edits without an override reason).
5. `src/agentsfleetd/state/model_rate_cache.zig` — process-global; `populate()` is boot-only — add a post-mutation repopulate path.

## PR Intent & comprehension handshake

- **PR title (eventual):** Admin-managed model catalogue + priced platform default (UI + API)
- **Intent (one sentence):** A platform admin builds the priced model catalogue and sets the one active default model+key entirely in the dashboard, so no environment needs a SQL seed or a manual onboarding playbook to serve teammates.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`. A mismatch against the Intent above → STOP and reconcile.

## Product Clarity

1. **Successful user moment** — an admin opens `/admin/models`, adds `fireworks / glm-5.2` with rates, activates it as the default (key + base_url), and the next platform-mode teammate runs GLM 5.2 — no SQL, no playbook step, no redeploy.
2. **Preserved user behaviour** — self-managed and "Custom — OpenAI-compatible" own-key flows stay run-fee-only and catalogue-bypassed (M98); the public caps endpoint shape and `GET /v1/tenants/me/provider` (mode+model, never a key) are unchanged.
3. **Optimal-way check** — most direct shape is "catalogue is a table you edit; default is one row you activate." We build exactly that — no rules engine, no per-tenant override.
4. **Rebuild-vs-iterate** — iterate; tables, resolver, and admin-page pattern exist. Refactoring the resolve chain would trade determinism for no user-visible gain.
5. **What we build** — catalogue write API + `/admin/models` page; `platform_llm_keys` gains model/base_url/context_cap; resolver + lease/runner thread base_url; rate-cache repopulates on mutation; seed + playbook bootstrap removed.
6. **What we do NOT build** — activation-time "Test Connection" (D); per-tenant override; audit trail (agent-firewall proxy, Indy-acked); catalogue bulk import.
7. **Fit** — compounds with M98 provider modes + the billing spine (`tenant_billing.zig`); must not destabilize the run-fee-only degrade (`fleet/renewal.zig`) — the silent-leak path this closes.
8. **Surface order** — API + UI together; the UI is the only intended caller (no CLI for platform-key management).
9. **Dashboard restraint** — the card shows the active model name only; the key is write-only (masked, never re-shown); no "Test Connection" control until that signal exists (D).
10. **Confused-user next step** — uncatalogued default → one-line error naming the fix; bad key → failed `fleet_event` in Events/steer, not a silent stall.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline.
- **`dispatch/write_zig.md`** — handlers + resolver + cache touch `*.zig` (pg-drain lifecycle, tagged-union results, `errdefer`, cross-compile both linux targets).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — new `src/agentsfleetd/http/handlers/admin/**` routes (URL design, route registration, handler signature).
- **`docs/SCHEMA_CONVENTIONS.md`** + **`dispatch/write_sql.md`** — `platform_llm_keys` column add + `019` seed removal (single-concern migration, `embed.zig` + array update, Schema Removal Guard).
- **`dispatch/write_ts_adhere_bun.md`** — the `/admin/models` page (design-system primitives, design tokens, no raw HTML/arbitrary utilities).
- **`docs/AUTH.md`** — `platform_keys.zig` is a credential-minting handler; the role-gate change is an auth-boundary edit.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile both linux targets; pg-drain in every query fn; tagged-union results |
| PUB / Struct-Shape | yes | shape verdict for each new pub handler + cache repopulate fn |
| File & Function Length (≤350/≤50/≤70) | yes | new `model_caps_admin.zig` handler stays single-concern; split if it nears the cap |
| UFS (repeated/semantic literals) | yes | route paths, error codes, provider-mode strings as named constants shared cross-runtime verbatim |
| UI Substitution / DESIGN TOKEN | yes | mirror `admin/runners` primitives; tokens only, no `*-[...]` arbitrary values |
| LOGGING / ERROR REGISTRY / SCHEMA / MILESTONE-ID | yes | new `UZ-PROVIDER-0NN` registry entry; no `M100`/`§` in source; SCHEMA GUARD for the seed removal |

## Overview

**Goal (testable):** An admin can CRUD priced `model_caps` rows and set exactly one active `platform_llm_keys` default (model + key + base_url) via `/admin/models`; `PUT /v1/admin/platform-keys` rejects an uncatalogued model, a platform-mode teammate resolves that default per-lease, and catalogue edits hit the rate cache with no restart.

**Problem:** The catalogue ships as a 13-row migration seed and the first platform key is inserted by a manual `admin_bootstrap` step, per environment. No admin surface manages either, and `resolvePlatformDefault` reads compile-time constants — so the default can drift from the priced catalogue and silently bill run-fee-only.

**Solution summary:** Add a platform-admin write API + a single `/admin/models` page (catalogue table + Platform Default card). The catalogue stays the priced billing spine; the default lives on `platform_llm_keys` (gaining model/base_url/context_cap), validated against the catalogue, enforced single-active. Resolver + lease/runner read the default from that row (base_url included). Seed and playbook step removed.

## Prior-Art / Reference Implementations

- **UI** → `ui/packages/app/app/(dashboard)/admin/runners/**` — admin page + dialog + Server Actions; `/admin/models` mirrors it. Visual layout approved as a live HTML mock (Jun 24, 2026; see §6 wireframe).
- **API** → `src/agentsfleetd/http/handlers/admin/platform_keys.zig` (the upsert to extend) + `docs/REST_API_DESIGN_GUIDELINES.md`; `register_runner` at `route_table.zig` for the `registry.platformAdmin()` gate.
- **Schema** → `schema/006_platform_llm_keys.sql` (table to widen) + the latest migration slot for the column-add and seed removal.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/0NN_platform_llm_keys_default.sql` | CREATE | add `model`, `base_url` (nullable), `context_cap_tokens` to `core.platform_llm_keys` |
| `schema/019_model_caps.sql` | EDIT | remove the 13-row seed INSERT (keep table DDL); catalogue is admin-populated |
| `schema/embed.zig` | EDIT | register the new migration in the array |
| `src/agentsfleetd/http/handlers/admin/model_caps_admin.zig` | CREATE | platform-admin CRUD for `model_caps` (list/create/update/delete), repopulate cache on mutation |
| `src/agentsfleetd/http/handlers/admin/platform_keys.zig` | EDIT | accept model/base_url/context_cap; validate model ∈ catalogue; enforce single-active |
| `src/agentsfleetd/http/route_table.zig` | EDIT | register `/v1/admin/models` routes; tighten `admin_platform_keys` gate `admin()` → `platformAdmin()` |
| `src/agentsfleetd/state/tenant_provider_resolver.zig` | EDIT | source default model/cap/base_url from the active row; drop the constants from the resolve path |
| `src/agentsfleetd/state/tenant_provider.zig` | EDIT | thread base_url through `readProviderView`; retire `PLATFORM_DEFAULT_MODEL`/`_CAP_TOKENS` |
| `src/agentsfleetd/state/model_rate_cache.zig` | EDIT | expose a repopulate path callable from the mutation handlers |
| `src/agentsfleetd/errors/error_registry.zig` + `error_entries.zig` | EDIT | add the uncatalogued-platform-default error (UZ-PROVIDER-0NN) |
| `src/agentsfleetd/fleet/*` (lease/runner dial) | EDIT | carry base_url into the platform-default dial path |
| `playbooks/operations/admin_bootstrap/001_playbook.md` | EDIT | remove steps 7.0 + 8.0 (store Fireworks key / register platform default), the header platform-key sentence, and Rollback step 4; KEEP 0.0–6.0 (Clerk admin user + role promotion + API key — the admin who logs into `/admin/models`); point at `/admin/models` for default setup |
| `ui/packages/app/app/(dashboard)/admin/models/page.tsx` + `components/*` + `actions.ts` | CREATE | the `/admin/models` page (catalogue table + Platform Default card + Add-model dialog) |
| `ui/packages/app/lib/api/model_caps.ts` | CREATE | typed client for the admin catalogue + platform-default endpoints |

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six Sections — schema, catalogue API, cache refresh, default API, resolver, UI — each independently testable; UI last so it builds on a proven API.
- **Alternatives considered:** (a) keep the default on a `model_caps.is_default` flag (the discarded draft) — rejected: conflates "priced catalogue" with "which key is live," and the key/base_url have no home on `model_caps`; (b) keep the migration seed and only add the key UI — rejected: leaves every environment dependent on a SQL seed Indy must hand-maintain.
- **Patch-vs-refactor verdict:** **patch** — wires admin write paths onto existing tables + resolver. The resolve chain is not refactored; only the default's source changes.

## Sections (implementation slices)

> **All Sections §1–§6 DONE** (Jun 25, 2026). Every Dimension shipped + tested; deviations recorded in Discovery (uid-keyed PATCH/DELETE, schema fold into `006`/`019`, `model_caps_store` consolidation, the rate-cache UAF fix + regression guard, the install-worker drain, the four integration test-semantic fixes). §1's columns land in `006_platform_llm_keys.sql` (not a `029` ALTER — pre-v2.0 SCHEMA GUARD) and the seed is removed from `019`.

### §1 — Schema: widen `platform_llm_keys`, remove the catalogue seed

`platform_llm_keys` gains `model` (TEXT), `base_url` (TEXT, nullable — named providers dial without it), `context_cap_tokens` (INTEGER). The `019` seed INSERT is removed so the catalogue is admin-populated. **Implementation default:** drop the seed from `019` (keep the table DDL) — no shipped production DB pre-launch; emit `SCHEMA GUARD: SKIPPED per user override (reason: pre-launch seed removal, no production DB)` and surface to Indy. Print the 13 current rows for Indy's records before removal.

- **Dimension 1.1** — new columns exist with correct types/nullability → Test `test_platform_llm_keys_has_default_model_columns`
- **Dimension 1.2** — fresh-DB migration run yields an empty `model_caps` (no seed) → Test `test_model_caps_unseeded_after_migration`

### §2 — Catalogue write API (`/v1/admin/models`)

Platform-admin CRUD over `model_caps`: list, create, update (caps/rates), delete. Gated `registry.platformAdmin()` (tenant/`agt_t` → 403). Delete is blocked when the row is the active platform default's model. **Implementation default:** keys on `(provider, model_id)` matching the table's unique domain key.

- **Dimension 2.1** — create persists a priced row; list returns it → Test `test_admin_model_create_then_list`
- **Dimension 2.2** — update mutates rates; delete removes the row → Test `test_admin_model_update_and_delete`
- **Dimension 2.3** — deleting the active default's model is rejected → Test `test_admin_model_delete_blocked_when_active_default`
- **Dimension 2.4** — operator/`agt_t` principal → 403 → Test `test_admin_models_requires_platform_admin`

### §3 — Rate cache repopulates on mutation

`model_rate_cache` gains a repopulate path the §2 handlers call after every successful create/update/delete, so catalogue edits are live without a restart.

- **Dimension 3.1** — after a create, `lookup_model_rate` returns the new rate → Test `test_rate_cache_reflects_create`
- **Dimension 3.2** — after a delete, the lookup misses → Test `test_rate_cache_reflects_delete`

### §4 — Platform default write API + single-active

`PUT /v1/admin/platform-keys` accepts `model`, `base_url?`, and `context_cap_tokens` alongside the existing provider/workspace fields, writes the key into the acting admin's workspace vault (Decision F: `source_workspace_id` = admin's current workspace), and **validates `model` is a catalogued `(provider, model_id)` row** — else `UZ-PROVIDER-0NN`. The upsert deactivates every other provider's row so exactly one active row remains. The gate tightens to `registry.platformAdmin()`.

- **Dimension 4.1** — PUT with a catalogued model activates exactly one row → Test `test_platform_default_single_active`
- **Dimension 4.2** — PUT with an uncatalogued model → 400 `UZ-PROVIDER-0NN`, no row activated → Test `test_platform_default_rejects_uncatalogued_model`
- **Dimension 4.3** — operator principal → 403 → Test `test_platform_keys_requires_platform_admin`
- **Dimension 4.4** — base_url + model + cap round-trip and persist → Test `test_platform_default_persists_base_url_model_cap`

### §5 — Resolver + lease/runner read the default from the row

`resolvePlatformDefault` sources model, context cap, and base_url from the active `platform_llm_keys` row; the `PLATFORM_DEFAULT_MODEL`/`_CAP_TOKENS` constants leave the resolve path. base_url is threaded through `readProviderView` and the lease/runner dial so a non-named (OpenAI-compatible) default actually dials (Decision A). Default changes propagate per-lease (Decision C).

- **Dimension 5.1** — resolve returns the active row's model/cap/base_url, not the constants → Test `test_resolve_platform_default_from_row`
- **Dimension 5.2** — no active row → `PlatformKeyMissing` (unchanged) → Test `test_resolve_platform_default_missing`
- **Dimension 5.3** — a base_url default reaches the dial path → Test `test_platform_default_base_url_threaded`

### §6 — `/admin/models` page + tenant Models card + runners alignment

One page, two sections, mirroring `admin/runners`. Approved layout:

```
/admin/models
┌───────────────────────────────────────────────┐  Models            [+ Add model]
│ Model catalogue · N models                     │  (eyebrow + divide-y border list)
│ Provider  Model      Context  Rates($/1M i/c/o) │  badge · mono id · mono rates · 🗑
├───────────────────────────────────────────────┤
│ Platform default        ● active               │  (card; --pulse only on LED + CTA)
│ Provider [▾]   Model [▾ from catalogue]         │
│ API key [••••• masked]                          │  write-only, never re-shown
│ Base URL [...]   Context cap [...]              │
│ ⓘ Default model must be in the catalogue.       │  ← simplified guard copy
│ Active: provider · model        [Save default] │
└───────────────────────────────────────────────┘
```

Rates display as `$/1M tokens` (in / cached / out). The guard copy is short and human ("Default model must be in the catalogue") — not the long revenue-leak explanation. The tenant Models page names the active default model from `GET /v1/tenants/me/provider` (`currentModel`); end users never see the key. **Runners alignment review:** audit `admin/runners` modal/badge/table against this design; record drift in Discovery; fold only token-level consistency fixes here — non-trivial drift becomes a follow-up spec (scope discipline).

- **Dimension 6.1** — catalogue table renders rows + Add-model dialog creates one → Test `test_admin_models_page_lists_and_creates`
- **Dimension 6.2** — Platform Default card saves with a catalogue-picked model; uncatalogued attempt shows the error → Test `test_platform_default_card_validation`
- **Dimension 6.3** — tenant Models page shows the active default model name, no key → Test `test_tenant_models_shows_default_name`
- **Dimension 6.4** — runners-alignment review note recorded in Discovery → Acceptance (review artifact, not a unit test)

## Interfaces

```
# Catalogue (platform-admin; registry.platformAdmin(); tenant/agt_t → 403)
GET    /v1/admin/models                              → { models: [{provider, model_id, context_cap_tokens,
                                                          input_nanos_per_mtok, cached_input_nanos_per_mtok,
                                                          output_nanos_per_mtok}] }
POST   /v1/admin/models      {provider, model_id, context_cap_tokens, input_*, cached_input_*, output_*}
PATCH  /v1/admin/models/{provider}/{model_id}        rates/caps; → updated row
DELETE /v1/admin/models/{provider}/{model_id}        → 204; 409 if it is the active default's model

# Platform default (platform-admin)
PUT    /v1/admin/platform-keys
        {provider, api_key, model, context_cap_tokens, base_url?, source_workspace_id}
        → {provider, model, active:true}      ; 400 UZ-PROVIDER-0NN if model ∉ catalogue
DELETE /v1/admin/platform-keys/{provider}            → active=false (row retained)

# Tenant read (unchanged) — bearer, no key in body
GET    /v1/tenants/me/provider                       → {mode, provider, model, context_cap_tokens}

# Unchanged: the public unauthenticated caps endpoint + its wire shape.
```

Rates are integers, nanos per million tokens (existing `model_caps` convention). The UI presents `$/1M` for entry/display and converts.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Uncatalogued default | `PUT` model not a `(provider, model_id)` row | 400 `UZ-PROVIDER-0NN`; no row activated; UI shows "Default model must be in the catalogue" |
| Delete active default's model | `DELETE /v1/admin/models/...` on the live default | 409; row kept; UI explains the default must change first |
| Non-platform-admin | operator / `agt_t` principal | 403 (`platformAdmin` gate); UI never renders the page for them |
| No active platform key | resolve with zero active rows | `PlatformKeyMissing` (unchanged); install surfaces the existing error |
| Bad platform key (runtime) | key invalid at dial time | failed `fleet_event` in Events/steer (Decision D); no activation-time check |
| base_url unset for non-named default | OpenAI-compatible default missing base_url | `PUT` validation requires base_url when provider is custom; 400 with the missing-field message |
| Catalogue edit mid-lease | rate changed during a live lease | cache repopulates immediately; in-flight lease keeps its resolved rate, next lease re-resolves (Decision C) |

## Invariants

1. The active platform default's model is always a catalogued `(provider, model_id)` row — enforced by the `PUT` validation against `model_caps`; an uncatalogued model cannot be activated.
2. At most one `platform_llm_keys` row is `active=true` at any time — enforced by the upsert deactivating all other rows in the same statement/transaction.
3. `model_caps` and `platform_llm_keys` admin write routes require `registry.platformAdmin()` — enforced by the route table, not handler-internal checks.
4. The platform key never appears in any tenant-facing response — `GET /v1/tenants/me/provider` returns model+mode only; the key is resolved server-side from the admin workspace vault.
5. A `model_caps` mutation leaves the rate cache consistent — the repopulate path runs in the same handler before it returns success.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_platform_llm_keys_has_default_model_columns` | migration adds model/base_url/context_cap with expected types |
| 1.2 | integration | `test_model_caps_unseeded_after_migration` | fresh-DB run → `SELECT count(*) FROM model_caps` = 0 |
| 2.1 | integration | `test_admin_model_create_then_list` | POST row → GET list contains it |
| 2.2 | integration | `test_admin_model_update_and_delete` | PATCH changes rate; DELETE removes row |
| 2.3 | integration | `test_admin_model_delete_blocked_when_active_default` | DELETE of live default's model → 409, row remains |
| 2.4 | integration | `test_admin_models_requires_platform_admin` | operator principal → 403 |
| 3.1 | unit | `test_rate_cache_reflects_create` | post-create lookup returns new rate |
| 3.2 | unit | `test_rate_cache_reflects_delete` | post-delete lookup misses |
| 4.1 | integration | `test_platform_default_single_active` | PUT B after PUT A → exactly one active row (B) |
| 4.2 | integration | `test_platform_default_rejects_uncatalogued_model` | PUT uncatalogued model → 400 UZ-PROVIDER-0NN, none active |
| 4.3 | integration | `test_platform_keys_requires_platform_admin` | operator → 403 |
| 4.4 | integration | `test_platform_default_persists_base_url_model_cap` | PUT round-trips base_url/model/cap |
| 5.1 | integration | `test_resolve_platform_default_from_row` | resolve returns row values, not constants |
| 5.2 | unit | `test_resolve_platform_default_missing` | no active row → `PlatformKeyMissing` |
| 5.3 | integration | `test_platform_default_base_url_threaded` | base_url default reaches the dial path |
| 6.1 | e2e | `test_admin_models_page_lists_and_creates` | render page, add model via dialog, row appears |
| 6.2 | e2e | `test_platform_default_card_validation` | save catalogued model ok; uncatalogued shows error |
| 6.3 | e2e | `test_tenant_models_shows_default_name` | tenant Models page shows model name, no key |

**Regression:** self-managed + custom OpenAI-compatible own-key flows stay run-fee-only and catalogue-bypassed (M98); public caps endpoint shape unchanged. **Idempotency:** repeated `PUT` of the same default is a no-op beyond `updated_at`.

## Acceptance Criteria

- [ ] Catalogue CRUD works platform-admin-only — verify: `make test-integration` (§2 tests green)
- [ ] Default rejects uncatalogued model; single active row enforced — verify: `make test-integration` (§4 tests green)
- [ ] Resolver reads model/cap/base_url from the row; constants gone from resolve path — verify: `grep -n PLATFORM_DEFAULT_ src/agentsfleetd/state/tenant_provider*.zig` shows no resolve-path use
- [ ] Fresh DB has empty catalogue (seed removed) — verify: `make test-integration` (§1.2)
- [ ] `/admin/models` lists/creates and the Platform Default card validates — verify: UI e2e (§6)
- [ ] Runners-alignment review recorded — verify: Discovery note present
- [ ] `make lint` clean · `make test` passes · `make test-integration` passes
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no non-`.md` file over 350 lines added

## Eval Commands (post-implementation)

```bash
# E1: constants left the resolve path (expect none); seed removed from 019 (expect 0)
grep -c "PLATFORM_DEFAULT" src/agentsfleetd/state/tenant_provider_resolver.zig; grep -c "INSERT INTO core.model_caps" schema/019_model_caps.sql
# E2: backend tiers + UI lint + cross-compile + gitleaks
make test && make test-integration && make memleak && make lint-app && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && gitleaks detect
# E3: 350-line gate (exempts .md) + migration size
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'; wc -l schema/0NN_platform_llm_keys_default.sql
```

## Dead Code Sweep

**1. Orphaned references — the retired default constants.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `PLATFORM_DEFAULT_MODEL` (resolve path) | `grep -rn "PLATFORM_DEFAULT_MODEL" src/` | 0 in the resolve path (remove the const if no other caller) |
| `PLATFORM_DEFAULT_CAP_TOKENS` | `grep -rn "PLATFORM_DEFAULT_CAP" src/` | 0 in the resolve path |
| `019` seed rows | `grep -c "INSERT INTO core.model_caps" schema/019_model_caps.sql` | 0 |
| `admin_bootstrap` steps 7.0/8.0 | `grep -niE "Store platform Fireworks key\|Register Fireworks as platform default" playbooks/operations/admin_bootstrap/001_playbook.md` | 0 (steps removed; role/surface mentions in 0.0–6.0 stay) |

No files deleted (handler is new; tables widened, not dropped).

## Discovery (consult log)

> Empty at creation. Append consults, skill outcomes, and Indy-acked deferrals as work surfaces them.

- **Decisions captured (pre-spec, Indy):** A (base_url threaded — default may be OpenAI-compatible), B (named own-key needs catalogue; else custom), C (per-lease propagation), D (no activation-time key check; Test Connection is follow-up), E (one admin page, two sections), F (key stored in the acting admin's current workspace vault).
- **Deferrals (Indy-acked):** "Test Connection" key verification → follow-up (Decision D). Audit trail → agent-firewall proxy, out of scope. Capture verbatim quotes here before CHORE(close) if challenged.
- **Runners-alignment review** — record the modal/badge/table drift findings here at §6.

### Pickup-session decisions + deviations (Orly, Jun 25, 2026)

- **uid-keyed PATCH/DELETE** — `/v1/admin/models/{uid}` keys on the uuidv7 `uid`, not `{provider}/{model_id}` (model_id contains `/`, can't be a path segment). Interface change vs the spec's original §2 path shape.
- **`model_caps_store.zig` consolidation (Indy override, in this PR — NOT deferred to M101)** — a single owner of all `core.model_caps` SQL (`listForAdmin`/`listForPublic`/`capFor`/`isReferencedByActiveDefault`/`create`/`updateRates`/`remove`). The public-GET handler, admin CRUD, and the platform-default cap-snapshot now call it; `model_rate_cache` shares its `TABLE` constant (its hot-path SELECT stays local). Removed ~3 duplicated column-lists, 2 row structs, 2 append loops, inline `catalogueCap`/`isActiveDefaultModel`.
- **`credential_probe.zig` split (RULE FLL)** — `tenant_provider_resolver.zig` hit 379 > 350; the self-managed credential probe + SSRF endpoint gate moved to `credential_probe.zig` (resolver → 230 lines). `tenant_provider.zig` façade re-exports re-pointed; behavior-preserving (secureZero/errdefer on api_key intact).
- **Rate-cache use-after-free FIX (production correctness)** — M100 made the admin handler call `model_rate_cache.populate()` per-mutation. The pre-M100 boot-only contract passed the caller's allocator; with a request-scoped allocator the process-global cache held request-lifetime memory → UAF on reset + cross-allocator free on the next build-then-swap. Fixed: `populate(conn)` now owns its memory off `std.heap.page_allocator`, dropping the misuse-prone allocator param. Regression guard added (`catalogue mutations repopulate the rate cache`).
- **Schema fold (SCHEMA GUARD, pre-v2.0)** — migration `029`'s `ALTER TABLE ADD COLUMN` violated the pre-v2.0 teardown convention (no agent override). Folded the 3 columns into `006_platform_llm_keys.sql`'s `CREATE TABLE` and the `model_caps` write grant into `019_model_caps.sql`; removed `029` entirely (DBs torn down fresh — Indy-confirmed). Net vs `origin/main`: `006` + `019` edits only (embed.zig + migration array unchanged).
- **Test-fixture + test-semantic fixes (integration was red, not green)** — the handoff's "confirm green" was wrong: 26 failures. Root causes + fixes: (1) `seedPlatformProviderWithKey` never set the new `model`/`context_cap_tokens` columns → `tenant_resolve_failed` across ~24 lease-path tests; (2) `control_plane` overlay test pinned cap on `tenant_providers` (M100 sources it from `platform_llm_keys` now); (3) `rbac` test asserted `UZ-AUTH-009` but M100's `platformAdmin()` gate returns `UZ-AUTH-021` (admin-role-alone now insufficient — security tightening); (4) `credentials_json` needed its own `model_caps` seed (seedless catalogue).
- **Install-worker drain (flaky segfault FIX)** — fleet-create spawns a detached install worker that called `pool.acquire()` after harness teardown → timing-flaky segfault under the parallel runner. Added `common.WaitGroup` (this Zig lacks `std.Thread.WaitGroup`), threaded via `Context.install_wg` (null in prod), drained in `TestHarness.deinit()` before pool/queue free.
- **Blast-radius adds beyond §1–§6:** `model_caps.zig` empty-catalogue → 200 (was 503); `model_caps_integration_test.zig` self-seeds; §6.4 runners alignment — no token-level drift requiring a fix (admin/models mirrors admin/runners: PageHeader + `divide-y` list + Badge + Dialog).
- **`/review` (adversarial, this session):** 0 P0/P1; 2 P2 comment-only fixes applied (stale `formatVersion` 503-comment; `seedPlatformProvider` run-fee-only caveat). Per-file large-refactor assessment (Indy directive): NO for all touched files — focused post-dedup.
- **Skill chain:** `/write-unit-test` — coverage added (3 admin edge/negative tests incl. the cache regression guard). `/review` — clean (above). `/review-pr` + `kishore-babysit-prs` — run after `gh pr create`.

## Skill-Driven Review Chain (mandatory)

Standard chain, outcomes logged in Discovery: `/write-unit-test` (coverage audit) → `/review` (adversarial, before CHORE(close)) → `/review-pr` (after `gh pr create`) → `kishore-babysit-prs`. Skipping any violates CHORE(close).

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Integration tests | `make test-integration` | 3 consecutive clean runs; 0 error/leak/panic blocks; no Build Summary (zig emits it only on failure) | ✅ |
| Memleak | `make memleak` | 1352 passed, 0 failed, 0 leaks (allocator gate; macOS `leaks` SIP-advisory) | ✅ |
| Lint (Zig) | `make lint-zig` | ZLint + pg-drain + test-depth + **schema-gate** all pass | ✅ |
| UI typecheck + lint + vitest | `bun run typecheck && bun run lint && vitest` | 0 type errors, lint clean, 14/14 vitest | ✅ |
| Cross-compile (Zig) | `zig build -Dtarget={x86_64,aarch64}-linux` | both exit 0 | ✅ |
| Gitleaks | `gitleaks detect` | no leaks (2902 commits scanned) | ✅ |
| Dead code sweep | `grep -rn PLATFORM_DEFAULT_ src/` | constants deleted; no production refs | ✅ |
| Test delta | `make _lint_zig_test_depth` | unit 2090 → 2103 (+13), integration 202 (baseline) | ✅ |

## Out of Scope

- Key "Test Connection" / activation-time verification — follow-up (Decision D).
- Per-tenant default override; catalogue bulk import/edit.
- Audit trail for catalogue/default changes — deferred to the agent-firewall proxy (Indy-acked).
- CLI for platform-key/catalogue management — the UI is the only intended caller.
