<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M120_005: `model_caps` naming retires with its public endpoint — `model_library` at every layer, served by an authenticated read

**Prototype:** v2.0.0
**Milestone:** M120
**Workstream:** 005
**Date:** Jul 11, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — naming-debt completion + surface reduction; no new capability, no known consumer breaks (pre-flip consult in §4 proves or Indy-acks that).
**Categories:** API, DOCS, UI
**Batch:** B1 — no open dependency; M120_001/M120_002/M120_004 (the Models-page surface this must not regress) are in `done/`. If M125_001 runs concurrently, coordinate on `ModelsRegistryTable.tsx` — shared surface, not a dependency.
**Branch:** feat/m120-library-sunset
**Test Baseline:** unit=2486 integration=290
**Depends on:** none — M120_003 closed DEFERRED; its rename scope is absorbed here (Indy-directed merge, verbatim quote in Discovery)
**Provenance:** LLM-drafted (Claude Opus 4.8, Jul 11, 2026) — Indy-directed: the sunset and the M120_003 merge were both proposed by Indy in-session; the consumer sweep below was verified in that session (see Discovery).
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §10 — describes the endpoint this spec retires; §6 of this spec rewrites it.

---

## Overview

**Goal (testable):** `grep -rn "model_caps\|ModelCaps" src/` and `grep -rn "model_caps\|ModelCap\b\|CAP_JSON" ui/` return zero matches with no carve-outs; `GET /v1/models` returns `200 { version, models[] }` for any authenticated tenant and `401` without a token; `GET /_um/<key>/cap.json` returns `404`; the dashboard Models-page pickers populate through the authenticated read with the degrade-to-free-text path intact.
**Problem:** two halves of one inconsistency. (a) The schema table (`core.model_library`, since M100) and the admin page title moved off "caps" naming, but five Zig modules and the TypeScript client surface still say `model_caps`/`Cap`. (b) The catalogue plus global billing constants sit on a public, unauthenticated, cryptic-prefix URL that nothing needs — the dashboard is its only live reader, the Command-Line Interface (CLI) resolves caps server-side via `PUT /v1/tenants/me/provider`, the install-skill never calls it (M49_001 §202), and the `rates`/`billing` block has zero consumers at any layer — and that URL permanently carries the "cap" name a pure rename could not touch. Renaming first and retiring second (the original M120_003→M120_005 sequencing) would rename symbols only to delete them — so both land in one pass.
**Solution summary:** rename the Zig and TypeScript `model_caps` surface straight to its final `model_library` state; rework the public handler into an authenticated tenant-scoped `GET /v1/models` serving the same `models[]` rows; repoint the dashboard catalogue fetch through a token-minting Server Action; delete the public route, its path-key constants, and the consumer-less `rates`/`billing` wire blocks (never renamed — deleted); update OpenAPI, the coverage/shape checks, the architecture docs, and the docs site. Hard cutover — the old path returns `404` with no alias (M86_002 precedent, no-compatibility-aliases rule, pre-2.0.0). Commit discipline: pure-rename commits land first, rework/retirement commits after, so the Pull Request (PR) stays reviewable.

## PR Intent & comprehension handshake

- **PR title (eventual):** Rename model_caps to model_library and retire the public cap.json
- **Intent (one sentence):** the model library is named one way at every layer and stops being served on an unauthenticated public URL nothing needs — one pass, no intermediate naming state.
- **Handshake:** implementing agent restates the intent + lists `ASSUMPTIONS I'M MAKING: …` at PLAN, before EXECUTE; a mismatch against the Intent above STOPs for reconciliation.

## Implementing agent — read these first

1. `docs/TEMPLATE.md` → "Teardown/rename/flip specs open with a blast-radius grep first" — run `git grep -rn -w '<token>'` from repo root, no path filter, for every renamed/deleted token below, BEFORE touching any file; record the full call-site set in Discovery (Dimension 1.1).
2. `src/agentsfleetd/http/handlers/model_caps.zig` + `src/agentsfleetd/state/model_caps_store.zig` — the rename+rework subject and the rename-only store; the handler's docstring carries the stale claim that the CLI and install-skill call the endpoint, which dies with its public role.
3. `src/agentsfleetd/http/handlers/tenant_model_entries.zig` — the nearest authenticated tenant-scoped GET; mirror its auth resolution and response envelope for `GET /v1/models`.
4. `ui/packages/app/app/(dashboard)/settings/billing/components/BillingUsageTab.tsx` and the `listTenantBillingChargesAction` it calls — the Server-Action token-mint pattern (session token via `auth().getToken()`) the catalogue fetch adopts.
5. `docs/REST_API_DESIGN_GUIDELINES.md` — this spec adds a `/v1` route; the `write_http` dispatch fires. `docs/architecture/billing_and_provider_keys.md` §10 is the flow §6 rewrites.

## Files Changed (blast radius)

Primary targets below; every import/call site the blast-radius grep (Dimension 1.1) surfaces also updates in the same commit — this table names the sources of truth, not an exhaustive site list.

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/state/model_caps_store.zig` | RENAME → `model_library_store.zig` | module + `Cap`/`Caps` exported symbols → `Library` names; behavior untouched |
| `src/agentsfleetd/http/handlers/admin/model_caps_admin.zig` | RENAME → `model_library_admin.zig` | admin handler module rename; Create/Read/Update/Delete (CRUD) behavior untouched |
| `src/agentsfleetd/http/handlers/admin/model_caps_admin_integration_test.zig` | RENAME → `model_library_admin_integration_test.zig` | follows its subject module |
| `src/agentsfleetd/http/handlers/model_caps.zig` | RENAME+REWORK → `model_library.zig` | path-key constants, `rates`/`billing` blocks, stale docstring deleted; becomes the authenticated `GET /v1/models` handler reusing the store read |
| `src/agentsfleetd/http/handlers/model_caps_integration_test.zig` | RENAME+REWORK → `model_library_integration_test.zig` | tests flip to: `200` authed, `401` unauthenticated, `404` on the retired path |
| every `@import("model_caps...")` call site (`model_rate_cache.zig`, `tenant_provider_resolver.zig`, `admin/platform_keys.zig`) | EDIT | import paths + symbols follow the renames |
| `src/agentsfleetd/http/router.zig` | EDIT | `_um` exact-match removed; `/v1/models` added; imports follow |
| `src/agentsfleetd/state/tenant_billing.zig` | EDIT | `publicConfig()`/`PublicConfig` deleted — their single consumer was the retired public response (RULE NDC) |
| `ui/packages/app/lib/api/model_caps.ts` | RENAME+REWORK → `model_library.ts` | `ModelCap`→`LibraryModel`, `getModelCaps`→`getModelLibrary` over authed `/v1/models` via `lib/api/client.ts` `request()`; `CAP_JSON_PATH`/`CAP_JSON_PATH_KEY` and `CapJson`/`CapRates`/`CapBilling` deleted, never renamed |
| `ui/packages/app/lib/api/model_caps.test.ts` | RENAME+EDIT → `model_library.test.ts` | follows the client |
| `ui/packages/app/lib/api/admin_models.ts` | RENAME → `admin_model_library.ts` | admin CRUD client; `ModelCapInput`→`LibraryModelInput`; rename only |
| every import site under `admin/models/**` and `w/[workspaceId]/settings/models/**` | EDIT | import paths + type names follow |
| `ui/.../settings/models/components/ModelCatalogueProvider.tsx` | EDIT | calls a token-minting Server Action instead of the unauthenticated fetch; once-per-session + degrade behavior unchanged |
| new Server Action for the library fetch (file placed per the local actions pattern) | CREATE | mints the session token via `auth().getToken()`, returns the library payload, mirroring `listTenantBillingChargesAction` |
| `public/openapi/paths/model-caps.yaml` | DELETE | retired path leaves the spec |
| `public/openapi/paths/models.yaml` | CREATE | documents `GET /v1/models` |
| `public/openapi/root.yaml` + `public/openapi.json` | EDIT | path swap; the json is re-bundled by `make check-openapi` |
| `scripts/check_openapi_route_coverage.py` + `scripts/check_openapi_url_shape.py` | EDIT | `model_caps` route mapping becomes the new authenticated route; `_um` static-path allowlist entry removed |
| `src/agentsfleetd/http/handlers/admin/model_library_admin_delete_guard_test.zig` | RENAME (was `model_caps_admin_delete_guard_test.zig`) | third admin test the blast-radius grep surfaced; follows its subject |
| `src/agentsfleetd/state/model_library/sql.zig` | CREATE | SQL Statement Modules touch-extraction (Indy-flagged in-session): the store's and rate cache's statements + the TABLE constant move here, mirroring `state/tenant_model_entries/sql.zig` |
| `src/agentsfleetd/state/model_rate_cache.zig` | EDIT | its catalogue SELECT moves to the domain sql.zig; stale "model-caps" prose fixed |
| route wiring (`routes.zig`, `route_scopes.zig`, `route_table.zig`, `route_table_invoke.zig`, `tests.zig`) | EDIT | `.model_caps`→`.model_library` variant; scope bucket no-auth→authenticated-only; bearer middleware; invoke wrapper owns the GET method check |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | `UZ-PROVIDER-004` hint repoints from the retired endpoint to the model library (text-only; no code change) |
| stale-prose sites (`state/tenant_provider.zig`, `fleet/context_resolve.zig`, `fleet/service_token_splits_wire_test.zig`, `http/rbac_http_integration_test.zig`, `http/handlers/fleets/backpressure_integration_test.zig`) | EDIT | comments/fixture names off the old naming; the backpressure probe repoints at `/v1/models` (admission sheds pre-auth, so its unauthenticated shed legs still 429; the ok-leg attaches a fixture bearer) |
| `docs/architecture/billing_and_provider_keys.md`, `docs/architecture/scenarios/README.md`, `docs/architecture/user_flow.md` | EDIT | §10 and every flow mention rewritten to the authenticated read |
| `~/Projects/docs`: `docs.json`, `changelog.mdx`, `cli/agentsfleet.mdx`, `api-reference/error-codes.mdx` | EDIT (own branch, per operating model) | nav page swap, retirement `<Update>`, stale "resolves from the cap.json endpoint" CLI prose fixed, `UZ-PROVIDER-004` guidance reworded to "the model library" |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (`publicConfig`, the path-key constants, and the rates/billing wire types die with their only consumer — deleted at authoring, never renamed), NLR (touch-it-fix-it on every edited file), ORP (cross-layer orphan sweep — the Dead Code Sweep below is the point of a rename+retirement), UFS (the `/v1/models` route literal is a named constant shared verbatim across router, handler, and client), EMS (the `401` reuses the existing problem+json auth envelope), TST-NAM (milestone-free test identifiers), XCC (cross-compile both linux targets).
- **`dispatch/write_zig.md`** — file moves + import fix-ups + handler rework across `.zig`.
- **`dispatch/write_ts_adhere_bun.md`** — file moves + import/type updates + Server Action across `.ts`/`.tsx`.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — new `/v1` route; read before EXECUTE (`write_http` dispatch).
- **`dispatch/write_zig.md` §SQL Statement Modules (SQLMOD)** — the touched store carries inline SQL, so extraction to a domain `sql.zig` is owed in the same diff (touch-arm; Indy-flagged in-session).
- **`docs/CHANGELOG_VOICE.md`** — the retirement `<Update>` (`write_changelog` dispatch); historical `cap.json` changelog entries are append-only and stay.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile both linux targets after every Zig rename/rework |
| PUB / Struct-Shape | yes | renamed pub symbols keep their shapes; the only new pub surface is one route constant + one handler entry — shape verdict at PLAN |
| File & Function Length | no | pure renames + a shrinking handler; no file approaches the cap |
| UFS | yes | `/v1/models` as a named constant; no repeated literals introduced |
| UI Substitution / DESIGN TOKEN | no | no markup or token changes — provider transport only |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no new log lines, error codes, or schema change — `core.model_library` untouched |

## Prior-Art / Reference Implementations

- **Reference:** `tenant_model_entries.zig` (authenticated tenant GET envelope) + `BillingUsageTab.tsx`/`listTenantBillingChargesAction` (client component fed by a token-minting Server Action). The retirement mirrors **M86_002**: hard cutover, old path `404`, no alias. The target naming is what already shipped — `schema/003_model_library.sql` and the "Model library" page title; no new naming is invented.

## Sections (implementation slices)

### §1 — Rename the Zig library surface (store + admin)

The rename-only slice: files whose behavior is untouched go straight to their final names. **Implementation default:** `git mv` each file, rename every exported `Cap`/`Caps` symbol to its `Library` equivalent, fix every `@import` call site the blast-radius grep surfaced. These land as pure-rename commits, before any rework commit.

- **Dimension 1.1** — the blast-radius grep for every renamed/deleted token (`model_caps`, `ModelCaps`, `Cap`-named exports, `_um/`, `cap.json`, `CAP_JSON`, `publicConfig`) is run from repo root, no path filter, and its full result set recorded in Discovery before any file is touched → Acceptance (Discovery record, not a unit test) — **DONE** (see Discovery)
- **Dimension 1.2** — the full existing Zig suite (unit + integration) passes at baseline counts after the renames, and both linux targets cross-compile clean → Test `test_zig_suite_green_post_rename`

### §2 — Authenticated model-library read (`GET /v1/models`)

The public handler renames AND reworks: `model_caps.zig` → `model_library.zig` becomes the authenticated read. **Implementation default:** auth resolution mirrors `tenant_model_entries.zig`; the existing store read is reused; the response carries `version` + `models[]` only — the `rates`/`billing` blocks and the retired document's `?model=` filter are not ported (zero consumers).

- **Dimension 2.1** — `GET /v1/models` with a valid token → `200 { version, models[] }`; each row carries exactly the retired document's `models[]` fields → Test `test_model_library_read_serves_catalogue`
- **Dimension 2.2** — missing or invalid token → `401` problem+json via the existing auth envelope → Test `test_model_library_read_requires_auth`
- **Dimension 2.3** — empty catalogue → `200` with `models: []` (the not-yet-seeded state stays valid) → Test `test_model_library_read_empty_catalogue_ok`

### §3 — Rename the TypeScript surface + dashboard repoint

**Implementation default:** `git mv` both client files; `ModelCap`→`LibraryModel` and `ModelCapInput`→`LibraryModelInput` rename (they survive); `CapJson`/`CapRates`/`CapBilling` and the `CAP_JSON_*` constants are deleted, never renamed. The catalogue fetch becomes a Server Action minting the session token via `auth().getToken()` (mirroring `listTenantBillingChargesAction`); `ModelCatalogueProvider` calls it on mount, preserving once-per-session semantics and the degrade path.

- **Dimension 3.1** — the provider populates pickers (`ProviderModelSelect`, `AddModelEntryDialog`, `ModelsRegistryTable`) through the authenticated read → Test `test_catalogue_provider_uses_authed_read`
- **Dimension 3.2** — a failed fetch (network error, `401`, `5xx`, token-mint failure) degrades pickers to free-text entry exactly as today → Test `test_catalogue_fetch_failure_degrades_free_text`
- **Dimension 3.3** — the full existing UI suite passes at baseline counts after the rename+repoint → Test `test_ui_suite_green_post_rename`

### §4 — Public endpoint retirement

**Pre-flip go/no-go (blocking):** before the route flips, Discovery must carry either access-log evidence that no third party reads `cap.json`, or Indy's acked acceptance of breaking anonymous readers. Agent-unilateral flipping without that record violates the consult discipline.

- **Dimension 4.1** — the go/no-go consult above is recorded in Discovery → Acceptance (Discovery record, not a unit test) — **DONE** (Indy ack recorded at CHORE(open), see Discovery)
- **Dimension 4.2** — `GET /_um/<key>/cap.json` → `404`; no alias, no redirect → Test `test_cap_json_path_returns_404`
- **Dimension 4.3** — zero `model_caps`/`ModelCaps` references in `src/`; zero `model_caps`/`ModelCap`/`CAP_JSON` references in `ui/`; zero `_um/`/`cap.json` references in `src/`, `ui/`, `public/`, `scripts/`. Single carve-out: `model_library_integration_test.zig` carries the retired path literal as the 404 pin test (Dimension 4.2's proof is not serving code) → Test `test_zero_old_name_references` (grep-based)
- **Dimension 4.4** — `publicConfig`/`PublicConfig` and the rates/billing wire types are deleted; zero references remain → Test `test_public_config_orphan_removed` (grep-based)

### §5 — OpenAPI + coverage checks follow

- **Dimension 5.1** — `make check-openapi` passes: `GET /v1/models` documented, the retired path absent, Redocly lint + URL-shape + route-coverage checks green → Test `test_openapi_gate_green` (build gate)
- **Dimension 5.2** — the coverage/shape scripts' own unit tests pass with the updated mappings → Test `test_openapi_script_units_green`

### §6 — Docs alignment (architecture + docs site)

- **Dimension 6.1** — `docs/architecture/` carries zero stale `cap.json` flow references — every surviving mention is an explicit retirement note ("the former public cap.json route is retired"); §10 and the user-flow/scenario mentions describe the authenticated read → Test `test_architecture_docs_updated` (grep-based)
- **Dimension 6.2** — docs-repo branch (own branch per operating model): nav page swapped to the new route, retirement `<Update>` in `changelog.mdx` naming the replacement, `cli/agentsfleet.mdx` stale resolution prose fixed, `error-codes.mdx` `UZ-PROVIDER-004` guidance reworded; historical changelog entries untouched → Acceptance (docs-repo diff, graded by rubric R4)

## Interfaces

```
GET /v1/models                          (Bearer — any authenticated tenant)
  200 { "version": "YYYY-MM-DD",
        "models": [ { "id", "provider", "context_cap_tokens",
                      "input_nanos_per_mtok", "cached_input_nanos_per_mtok",
                      "output_nanos_per_mtok" } ] }
  401 problem+json                      (existing auth envelope; no new error codes)

GET /_um/<key>/cap.json                 → 404 (retired; no alias, no redirect)
```

`models[]` rows are byte-shaped like the retired document's rows. The global `rates`/`billing` blocks do not move — they are deleted with the endpoint (zero consumers; the constants stay pinned in `tenant_billing.zig` · `ui/packages/website/src/lib/rates.ts` · `~/Projects/docs/snippets/rates.mdx`). Renamed Zig/TypeScript symbols keep their shapes — names only.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|----------------------------------------------------------|
| Unauthenticated read | missing/invalid Bearer on `/v1/models` | `401` problem+json; the dashboard action surfaces it as a failed fetch → pickers degrade to free-text |
| Client fetch fails | network error, token-mint failure, `5xx` | existing `ModelCatalogueProvider` degrade path — pickers fall back to free-text entry, page fully functional |
| Request to retired path | stale third-party reader or bookmark | `404`; the changelog `<Update>` names the replacement route — the accepted outcome of the §4 go/no-go consult |
| Empty catalogue | table not yet seeded via `/v1/admin/models` | `200` with `models: []`; pickers degrade as today — unchanged semantics |
| Stale import after a rename or the handler rework | missed call site | build/typecheck fails at `make test`/`make lint` — not a runtime failure mode |

## Invariants

1. The model library is never reachable unauthenticated after this spec — the router carries no `_um` route; enforced by `test_cap_json_path_returns_404` + `test_zero_old_name_references`.
2. `models[]` row shape is identical to the retired document's rows — enforced by field assertions in `test_model_library_read_serves_catalogue`.
3. Rate/billing constants remain pinned in their three files and are never fetched by any client — enforced by deleting the only wire projection and `test_public_config_orphan_removed`.
4. `core.model_library` table/column names and the admin CRUD surface are untouched — enforced by the diff staying inside Files Changed (rubric R5).
5. Renamed modules keep behavior — enforced by the Zig and UI suites passing at baseline counts (`test_zig_suite_green_post_rename`, `test_ui_suite_green_post_rename`).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes; the retirement is proven by the `404` integration test, not an event | — | — | — | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.2 | integration (regression) | `test_zig_suite_green_post_rename` | `make test && make test-integration` → exit 0 at baseline counts; both linux targets cross-compile |
| 2.1 | integration | `test_model_library_read_serves_catalogue` | seeded row + valid token → `200`; body has `version` + `models[]` with exactly the six per-row fields |
| 2.2 | integration | `test_model_library_read_requires_auth` | no token → `401` problem+json; garbage token → `401` |
| 2.3 | integration | `test_model_library_read_empty_catalogue_ok` | empty table + valid token → `200`, `models: []` |
| 3.1 | unit | `test_catalogue_provider_uses_authed_read` | provider mount → Server Action called once; pickers receive its rows |
| 3.2 | unit | `test_catalogue_fetch_failure_degrades_free_text` | action rejects → provider state `error=true`; pickers render free-text (existing behavior re-asserted) |
| 3.3 | unit (regression) | `test_ui_suite_green_post_rename` | `make test-unit-app` → exit 0 at baseline counts |
| 4.2 | integration | `test_cap_json_path_returns_404` | `GET /_um/<key>/cap.json` → `404` |
| 4.3 | unit (grep) | `test_zero_old_name_references` | the three R1 greps below → 0 matches each, no carve-outs |
| 4.4 | unit (grep) | `test_public_config_orphan_removed` | `grep -rn "publicConfig\|PublicConfig" src/` → 0 matches |
| 5.1 | build gate | `test_openapi_gate_green` | `make check-openapi` → exit 0 |
| 5.2 | unit | `test_openapi_script_units_green` | `python3 -m unittest discover -s scripts` → exit 0 |
| 6.1 | unit (grep) | `test_architecture_docs_updated` | `grep -rn "cap\.json" docs/architecture/` → 0 matches |

Regression: 1.2/3.3 ARE the rename's regression proof (names change, behavior doesn't); 2.x/4.2 prove the only intended behavior change. Idempotency/replay: N/A — read-only GET surfaces.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Zero old names or retired-path refs (§1, §3, §4) | `grep -rn "model_caps\|ModelCaps" src/; grep -rn "model_caps\|ModelCap\b\|CAP_JSON" ui/; grep -rn "_um/\|cap\.json" src/ ui/ public/ scripts/ \| grep -v model_library_integration_test.zig` | no output from any grep (the 404 pin test is the single allowed retired-path mention) | P0 | |
| R2 | OpenAPI gate green (§5) | `make check-openapi` | exit 0 | P0 | |
| R3 | Architecture docs clean (§6) | `grep -rn "cap\.json" docs/architecture/ \| grep -v "retired"` | no output (explicit retirement notes are the only surviving mentions) | P0 | |
| R4 | Docs site aligned (§6) | `cd ~/Projects/docs && git grep -n "cap\.json" -- docs.json cli/agentsfleet.mdx api-reference/error-codes.mdx` | no output (changelog history exempt) | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0; count ≥ baseline | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0; includes the new `/v1/models` + `404` tests | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**1. Orphaned files — old paths, pre-rename, plus true deletions.**

| File to delete | Verify |
|----------------|--------|
| `src/agentsfleetd/state/model_caps_store.zig` | `test ! -f src/agentsfleetd/state/model_caps_store.zig` |
| `src/agentsfleetd/http/handlers/model_caps.zig` | `test ! -f src/agentsfleetd/http/handlers/model_caps.zig` |
| `src/agentsfleetd/http/handlers/model_caps_integration_test.zig` | `test ! -f src/agentsfleetd/http/handlers/model_caps_integration_test.zig` |
| `src/agentsfleetd/http/handlers/admin/model_caps_admin.zig` | `test ! -f src/agentsfleetd/http/handlers/admin/model_caps_admin.zig` |
| `src/agentsfleetd/http/handlers/admin/model_caps_admin_integration_test.zig` | `test ! -f src/agentsfleetd/http/handlers/admin/model_caps_admin_integration_test.zig` |
| `ui/packages/app/lib/api/model_caps.ts` | `test ! -f ui/packages/app/lib/api/model_caps.ts` |
| `ui/packages/app/lib/api/model_caps.test.ts` | `test ! -f ui/packages/app/lib/api/model_caps.test.ts` |
| `ui/packages/app/lib/api/admin_models.ts` | `test ! -f ui/packages/app/lib/api/admin_models.ts` |
| `public/openapi/paths/model-caps.yaml` | `test ! -f public/openapi/paths/model-caps.yaml` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `model_caps` import paths + `ModelCaps`/`MODEL_CAPS_*` identifiers | `grep -rn "model_caps\|ModelCaps\|MODEL_CAPS" src/` | 0 matches |
| `ModelCap`, `ModelCapInput`, `CapJson`, `CapRates`, `CapBilling`, `getModelCaps`, `CAP_JSON_PATH`, `CAP_JSON_PATH_KEY` | `grep -rn "ModelCap\b\|ModelCapInput\|CapJson\|CapRates\|CapBilling\|getModelCaps\|CAP_JSON" ui/` | 0 matches |
| `_um`/`cap.json` path literals | `grep -rn "_um/\|cap\.json" src/ ui/ public/ scripts/ \| grep -v model_library_integration_test.zig` | 0 matches (the 404 pin test is the single allowed mention) |
| `publicConfig`, `PublicConfig` | `grep -rn "publicConfig\|PublicConfig" src/` | 0 matches |

## Out of Scope

- `/v1/admin/models` (admin CRUD route) — untouched; it already has the correct auth posture. Its handler module renames; its route and behavior do not.
- The "catalogue"-vs-"library" prose axis (`ModelCatalogueProvider` name and friends) — a third naming axis; edited here only where transport requires, never renamed.
- A Content Delivery Network (CDN)-cache replacement — the authenticated read is uncached by design; the payload is small and fetched once per session.
- Porting the `?model=` filter or the `rates`/`billing` blocks — zero consumers for either.
- Any `core.model_library` schema change or CLI change — the CLI already resolves caps server-side.

---

## Product Clarity (authoring record)

1. **Successful user moment** — two at once: an engineer grepping for the model library finds one name at every layer (table, page title, modules, types, route); a platform operator confirms the library is no longer world-readable — the old public URL returns `404` while the dashboard Models page populates exactly as before.
2. **Preserved user behaviour** — the Models page (pickers, add-entry dialog, registry rates column, free-text degrade), the CLI provider flows, and admin catalogue CRUD all work unchanged.
3. **Optimal-way check** — renaming straight to final state in the same pass as the retirement is the direct fix; sequencing them separately renames symbols that immediately die (the rejected M120_003 sequencing).
4. **Rebuild-vs-iterate** — iterate: mechanical renames + one route swap + one transport repoint; no redesign.
5. **What we build** — the Zig rename slice (§1), the authenticated read (§2), the TypeScript rename + repoint (§3), the retirement + orphan deletion (§4), OpenAPI/check updates (§5), docs alignment (§6).
6. **What we do NOT build** — no deprecation window or alias (M86_002 precedent, pre-2.0.0), no rates/billing relocation (zero consumers), no admin-surface change, no "catalogue"-prose rename.
7. **Fit with existing features** — completes the M120 model-library family (M120_001–004); must not destabilize the Models page those specs built — §1–§3's regression tests are exactly that proof.
8. **Surface order** — API + User Interface (UI) transport and naming only; no new user-facing surface.
9. **Dashboard restraint** — no visible UI change at all; same pickers, same degrade.
10. **Confused-user next step** — a third-party reader hitting the `404` finds the changelog `<Update>` naming `GET /v1/models` and the token requirement; the API reference documents the replacement on the same nav spot.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six Sections — Zig rename, authed read, TypeScript rename + repoint, retirement, gate updates, docs — each independently gradeable by grep, integration test, or build gate. Pure-rename commits land before rework commits so review separates noise from behavior.
- **Alternatives considered:** (a) sequence as two specs, rename first (M120_003) then retire — rejected (Indy, Jul 11, 2026): the rename spec renames symbols the retirement deletes (`CapJson`/`CapRates`/`CapBilling`, the path-key constants), forces two PRs over the same lines, and leaves an intermediate naming state with no standalone value; M120_003 is closed DEFERRED with its delta absorbed here. (b) delete the public endpoint without a replacement read — rejected: the dashboard needs a catalogue source. (c) keep an unauthenticated endpoint under a `library.json` name — rejected: the rename is the breaking event anyway, and it preserves a world-readable surface nothing needs. (d) deprecation window with dual-serving — rejected: no-compatibility-aliases rule + M86_002 precedent, pre-2.0.0.
- **Patch-vs-refactor verdict:** **patch** — mechanical renames, a route swap, a transport repoint, and an orphan sweep; no architectural reshape.

## Discovery (consult log)

- **Consults** —
  - Session consult (Jul 11, 2026, Indy-directed): sunset proposed by Indy; verified in-session — the dashboard Models page is the only live consumer (`ModelCatalogueProvider` fetch on mount); the CLI resolves caps server-side via `PUT /v1/tenants/me/provider` (`cli/src/commands/tenant.ts`); the install-skill never calls the endpoint (M49_001 §202); the `rates`/`billing` block has zero consumers (the dashboard discards it, the CLI pins `cli/src/constants/billing.ts`, the website pins `rates.ts`).
  - Merge consult — `> Indy (2026-07-11 08:59): "Do you think M120_005 supercedes M120_003? Since fixing M120_003 is pointless? That means we only fix the delta between M120_005 and M120_003 (base our spec on M120_005 and find delta with M120_003) and move it to M12_005 and continue as one spec M120_005 and M120_003 is closed as deferred?" — context: M120_003's rename scope is absorbed into this spec; M120_003 closed DEFERRED.`
  - Architecture consult: route name `GET /v1/models` checked against `docs/architecture/` — no conflicting stream/route naming; `billing_and_provider_keys.md` §10 is amended by §6.
  - Dimension 1.1 blast-radius record (Jul 11, 2026, from repo root, no path filter): `model_caps` word — 26 files (5 Zig handler/store/test + 5 route-wiring + 12 UI source/test + 2 scripts + schema comment sites); `ModelCaps|ModelCap\b|MODEL_CAPS` — 15 files; `CAP_JSON|CapJson|CapRates|CapBilling|getModelCaps` — 6 UI files; `_um/|cap.json` — 16 files (incl. 6 `cli/test` false positives: a mock variable named `cap` with a `.json` property, outside every acceptance grep); `publicConfig|PublicConfig` — 3 files; `admin_models` — 13 UI files. Per-file hit counts in PR Session Notes. Surfaced beyond the authored table: the delete-guard admin test, `route_table.zig`/`routes.zig`/`route_scopes.zig`/`route_table_invoke.zig`/`tests.zig` wiring, `rbac_http_integration_test.zig` comment, the backpressure probe, and hyphenated `model-caps` prose (7 sites incl. the `UZ-PROVIDER-004` hint) — all folded into Files Changed above. `schema/embed.zig` + `schema/003_model_library.sql` comment mentions stay (append-only migration surface, outside every acceptance grep).
  - SQLMOD consult (Indy-flagged mid-EXECUTE, Jul 11, 2026): the touched store carried inline SQL; the SQL Statement Modules touch-arm was owed and initially missed. Extraction to `state/model_library/sql.zig` landed in the same diff (store + rate-cache statements + TABLE). Root cause: the deterministic checker (`audits/sql-mod.sh --staged`) by design flags only ADDED SQL, and the product `make harness-verify` carries no SQLMOD row at all. Follow-up proposed to Indy (outside this spec's scope): (a) wire `sql-mod.sh --staged` into the product `make harness-verify`; (b) dotfiles `edit_rules` change teaching the checker that a rename is a touch.
  - §4 pre-flip go/no-go — **CLOSED at CHORE(open)**: `> Indy (2026-07-11 09:44): "No one outside our code is reading the public URL so you are free to remove and the cap.json" — context: Dimension 4.1 satisfied; the route flip to 404 is cleared, no access-log scan required.`
- **Metrics review** — not applicable — no product/operator signal changes.
- **Skill-chain outcomes** — empty at creation.
- **Deferrals** — empty at creation.
