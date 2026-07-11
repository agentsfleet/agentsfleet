<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M120_005: Public `cap.json` retires; the model library serves through an authenticated read

**Prototype:** v2.0.0
**Milestone:** M120
**Workstream:** 005
**Date:** Jul 11, 2026
**Status:** PENDING
**Priority:** P2 — surface-reduction + naming-debt completion; no new capability, no known consumer breaks (pre-flip consult in §3 proves or Indy-acks that).
**Categories:** API, DOCS, UI
**Batch:** B3 — sequenced after M120_003 (B2) lands: that spec renames the exact files this one edits (`model_caps.zig`→`model_library.zig`, `model_caps.ts`→`model_library.ts`). If M125_001 runs concurrently, coordinate on `ModelsRegistryTable.tsx` — shared surface, not a dependency.
**Branch:** {added at CHORE(open)}
**Test Baseline:** {set at CHORE(open) via `make _lint_zig_test_depth`}
**Depends on:** M120_003 (renames the two primary files this spec then reworks; landing first avoids same-line churn)
**Provenance:** LLM-drafted (Claude Opus 4.8, Jul 11, 2026) — Indy-directed: Indy proposed the sunset mid-session; the consumer sweep below was verified in that session (see Discovery).
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §10 — describes the endpoint this spec retires; §5 of this spec rewrites it.

---

## Overview

**Goal (testable):** `GET /v1/models` returns `200 { version, models[] }` for any authenticated tenant and `401` without a token; `GET /_um/<key>/cap.json` returns `404`; zero references to the retired path remain in `src/`, `ui/`, `public/`, or `scripts/`; the dashboard Models-page pickers populate through the authenticated read with the degrade-to-free-text path intact.
**Problem:** the entire model catalogue plus global billing constants sit on a public, unauthenticated, cryptic-prefix URL that nothing needs anymore — the dashboard is its only live reader (and it is authenticated everywhere else), the Command-Line Interface (CLI) resolves caps server-side via `PUT /v1/tenants/me/provider`, the install-skill never calls it (M49_001 §202), and the `rates`/`billing` block has zero consumers at any layer. Meanwhile the URL permanently carries the "cap" name that the caps→library rename (M120_003) cannot touch while the path stays public.
**Solution summary:** add an authenticated tenant-scoped `GET /v1/models` that serves the same `models[]` rows from `core.model_library`; repoint the dashboard catalogue fetch at it through a token-minting Server Action; delete the public route, its cryptic path-key constants, and the consumer-less `rates`/`billing` wire blocks; update OpenAPI, the two coverage/shape checks, the architecture docs, and the docs site (nav, changelog, stale CLI prose). Hard cutover — the old path returns `404` with no alias, matching the M86_002 precedent and the no-compatibility-aliases rule, pre-2.0.0.

## PR Intent & comprehension handshake

- **PR title (eventual):** Retire public cap.json; serve the model library via authenticated read
- **Intent (one sentence):** the model library stops being served on an unauthenticated public URL nothing needs, completing the caps→library rename at the wire that M120_003 deliberately could not touch.
- **Handshake:** implementing agent restates the intent + lists `ASSUMPTIONS I'M MAKING: …` at PLAN, before EXECUTE; a mismatch against the Intent above STOPs for reconciliation.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/tenant_model_entries.zig` — the nearest authenticated tenant-scoped GET; mirror its auth resolution and response envelope for `GET /v1/models`.
2. `src/agentsfleetd/http/handlers/model_library.zig` (post-M120_003 name of `model_caps.zig`) — the handler being retired; its store read is what the new route reuses; its docstring carries the stale claim that the CLI and install-skill call the endpoint, which dies with the file's public role.
3. `ui/packages/app/app/(dashboard)/settings/billing/components/BillingUsageTab.tsx` and the `listTenantBillingChargesAction` it calls — the Server-Action token-mint pattern (session token via `auth().getToken()`) the catalogue fetch adopts.
4. `docs/REST_API_DESIGN_GUIDELINES.md` — this spec adds a `/v1` route; the `write_http` dispatch fires.
5. `docs/architecture/billing_and_provider_keys.md` §10 — the flow description §5 rewrites.

## Files Changed (blast radius)

File names below are the post-M120_003 names; every call site the blast-radius grep (Dimension 1.1) surfaces also updates in the same commit.

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/model_library.zig` | EDIT | public path-key constants, `rates`/`billing` response blocks, and stale docstring go; becomes the authenticated `GET /v1/models` handler reusing the existing store read |
| `src/agentsfleetd/http/handlers/model_library_integration_test.zig` | EDIT | tests flip to: `200` authed, `401` unauthenticated, `404` on the retired path |
| `src/agentsfleetd/http/router.zig` | EDIT | `_um` exact-match removed; `/v1/models` added |
| `src/agentsfleetd/state/tenant_billing.zig` | EDIT | `publicConfig()`/`PublicConfig` deleted — their single consumer was the retired public response (RULE NDC) |
| `ui/packages/app/lib/api/model_library.ts` | EDIT | fetch moves to `/v1/models` via the existing `lib/api/client.ts` `request()` (Bearer); `CAP_JSON_PATH`/`CAP_JSON_PATH_KEY` and the rates/billing types deleted |
| `ui/packages/app/lib/api/model_library.test.ts` | EDIT | follows the client change |
| `ui/.../settings/models/components/ModelCatalogueProvider.tsx` | EDIT | calls a token-minting Server Action instead of the unauthenticated fetch; once-per-session + degrade-to-free-text behavior unchanged |
| new Server Action for the library fetch (file placed per the local actions pattern) | CREATE | mints the session token via `auth().getToken()` and returns the library payload, mirroring `listTenantBillingChargesAction` |
| `public/openapi/paths/model-caps.yaml` | DELETE | retired path leaves the spec |
| `public/openapi/paths/models.yaml` | CREATE | documents `GET /v1/models` |
| `public/openapi/root.yaml` + `public/openapi.json` | EDIT | path swap; the json is re-bundled by `make check-openapi` |
| `scripts/check_openapi_route_coverage.py` | EDIT | `model_caps` route mapping becomes the new authenticated route |
| `scripts/check_openapi_url_shape.py` | EDIT | the `_um` static-path allowlist entry is removed |
| `docs/architecture/billing_and_provider_keys.md`, `docs/architecture/scenarios/README.md`, `docs/architecture/user_flow.md` | EDIT | §10 and every flow mention rewritten to the authenticated read |
| `~/Projects/docs`: `docs.json`, `changelog.mdx`, `cli/agentsfleet.mdx`, `api-reference/error-codes.mdx` | EDIT (own branch, per operating model) | nav page swap, retirement `<Update>`, stale "resolves from the cap.json endpoint" CLI prose fixed, `UZ-PROVIDER-004` guidance reworded to "the model library" |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (`publicConfig`, the path-key constants, and the rates/billing wire types die with their only consumer), NLR (touch-it-fix-it on every edited file), ORP (cross-layer orphan sweep — the Dead Code Sweep below is the point of this spec), UFS (the `/v1/models` route literal is a named constant shared verbatim across router, handler, and client), EMS (the `401` reuses the existing problem+json auth envelope — no new error structure), TST-NAM (milestone-free test identifiers), XCC (cross-compile both linux targets).
- **`dispatch/write_zig.md`** — handler rework + router edit across `.zig`.
- **`dispatch/write_ts_adhere_bun.md`** — client + Server Action edits across `.ts`/`.tsx`.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — new `/v1` route; read before EXECUTE (`write_http` dispatch).
- **`docs/CHANGELOG_VOICE.md`** — the retirement `<Update>` (`write_changelog` dispatch); history entries mentioning `cap.json` are append-only and stay.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile both linux targets after every Zig edit |
| PUB / Struct-Shape | yes | new pub surface is one route constant + one handler entry; shape verdict at PLAN |
| File & Function Length | no | the handler shrinks (blocks deleted); no file approaches the cap |
| UFS | yes | `/v1/models` as a named constant; no repeated literals |
| UI Substitution / DESIGN TOKEN | no | no markup or token changes — provider logic only |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no new log lines, error codes, or schema change — `core.model_library` untouched |

## Prior-Art / Reference Implementations

- **Reference:** `tenant_model_entries.zig` (authenticated tenant GET envelope) + `BillingUsageTab.tsx`/`listTenantBillingChargesAction` (client component fed by a token-minting Server Action). The retirement itself mirrors **M86_002**: hard cutover, old path `404`, no alias — that spec renamed `model-caps.json`→`cap.json` the same way.

## Sections (implementation slices)

### §1 — Authenticated model-library read (`GET /v1/models`)

The library catalogue becomes readable only with a Bearer token. **Implementation default:** rework the existing public handler in place (RULE NLR — same file, new route) reusing its existing store read; auth resolution mirrors `tenant_model_entries.zig`. The response carries `version` + `models[]` only — the `rates`/`billing` blocks are not ported (zero consumers, see §3). The retired document's `?model=` filter is not ported either — no remaining consumer.

- **Dimension 1.1** — blast-radius grep for `_um/`, `cap.json`, `CAP_JSON`, `publicConfig` run from repo root, no path filter, full result set recorded in Discovery before any edit → Acceptance (Discovery record, not a unit test)
- **Dimension 1.2** — `GET /v1/models` with a valid token → `200 { version, models[] }`; each row carries exactly the retired document's `models[]` fields → Test `test_model_library_read_serves_catalogue`
- **Dimension 1.3** — missing or invalid token → `401` problem+json via the existing auth envelope → Test `test_model_library_read_requires_auth`
- **Dimension 1.4** — empty catalogue → `200` with `models: []` (the not-yet-seeded state stays valid, unchanged semantics) → Test `test_model_library_read_empty_catalogue_ok`

### §2 — Dashboard repoint (token-minting Server Action)

The Models page keeps its exact behavior; only the transport changes. **Implementation default:** a Server Action mints the session token via `auth().getToken()` and fetches `/v1/models` (mirroring `listTenantBillingChargesAction`); `ModelCatalogueProvider` calls the action on mount, preserving once-per-session semantics and the degrade-to-free-text path.

- **Dimension 2.1** — the provider populates pickers (`ProviderModelSelect`, `AddModelEntryDialog`, `ModelsRegistryTable`) through the authenticated read → Test `test_catalogue_provider_uses_authed_read`
- **Dimension 2.2** — a failed fetch (network error, `401`, `5xx`) degrades pickers to free-text entry exactly as today → Test `test_catalogue_fetch_failure_degrades_free_text`
- **Dimension 2.3** — zero unauthenticated catalogue fetches remain in `ui/` → Test `test_ui_zero_public_path_references` (grep-based)

### §3 — Public endpoint retirement

**Pre-flip go/no-go (blocking):** before the route flips, Discovery must carry either access-log evidence that no third party reads `cap.json`, or Indy's acked acceptance of breaking anonymous readers. Agent-unilateral flipping without that record violates the Deferral/consult discipline.

- **Dimension 3.1** — the go/no-go consult above is recorded in Discovery → Acceptance (Discovery record, not a unit test)
- **Dimension 3.2** — `GET /_um/<key>/cap.json` → `404`; no alias, no redirect → Test `test_cap_json_path_returns_404`
- **Dimension 3.3** — zero `_um/` or `cap.json` references remain in `src/`, `ui/`, `public/`, `scripts/` → Test `test_zero_cap_json_references` (grep-based)
- **Dimension 3.4** — `publicConfig`/`PublicConfig` and the rates/billing wire types are deleted; zero references remain → Test `test_public_config_orphan_removed` (grep-based)

### §4 — OpenAPI + coverage checks follow

- **Dimension 4.1** — `make check-openapi` passes: `GET /v1/models` documented, the retired path absent, Redocly lint + URL-shape + route-coverage checks green → Test `test_openapi_gate_green` (build gate)
- **Dimension 4.2** — the coverage/shape scripts' own unit tests pass with the updated mappings → Test `test_openapi_script_units_green`

### §5 — Docs alignment (architecture + docs site)

- **Dimension 5.1** — `docs/architecture/` carries zero stale `cap.json` flow references; §10 and the user-flow/scenario mentions describe the authenticated read → Test `test_architecture_docs_updated` (grep-based)
- **Dimension 5.2** — docs-repo branch (own branch per operating model): nav page swapped to the new route, retirement `<Update>` in `changelog.mdx` naming the replacement, `cli/agentsfleet.mdx` stale resolution prose fixed, `error-codes.mdx` `UZ-PROVIDER-004` guidance reworded; historical changelog entries untouched → Acceptance (docs-repo diff, graded by rubric R5)

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

`models[]` rows are byte-shaped like the retired document's rows. The global `rates`/`billing` blocks do not move — they are deleted with the endpoint (zero consumers; the constants stay pinned in `tenant_billing.zig` · `ui/packages/website/src/lib/rates.ts` · `~/Projects/docs/snippets/rates.mdx`).

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|----------------------------------------------------------|
| Unauthenticated read | missing/invalid Bearer on `/v1/models` | `401` problem+json; the dashboard action surfaces it as a failed fetch → pickers degrade to free-text |
| Client fetch fails | network error, token-mint failure, `5xx` | existing `ModelCatalogueProvider` degrade path — pickers fall back to free-text entry, page fully functional |
| Request to retired path | stale third-party reader or bookmark | `404`; the changelog `<Update>` names the replacement route — the accepted outcome of the §3 go/no-go consult |
| Empty catalogue | table not yet seeded via `/v1/admin/models` | `200` with `models: []`; pickers degrade as today — unchanged semantics |
| Stale import after handler rework | missed call site | build/typecheck fails at `make test`/`make lint` — not a runtime failure mode |

## Invariants

1. The model library is never reachable unauthenticated after this spec — the router carries no `_um` route; enforced by `test_cap_json_path_returns_404` + `test_zero_cap_json_references`.
2. `models[]` row shape is identical to the retired document's rows — enforced by field assertions in `test_model_library_read_serves_catalogue`.
3. Rate/billing constants remain pinned in their three files and are never fetched by any client — enforced by deleting the only wire projection and `test_public_config_orphan_removed`.
4. `core.model_library` table/column names and the admin Create/Read/Update/Delete (CRUD) surface are untouched — enforced by the diff staying inside Files Changed (rubric R6).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes; the retirement is proven by the `404` integration test, not an event | — | — | — | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.2 | integration | `test_model_library_read_serves_catalogue` | seeded row + valid token → `200`; body has `version` + `models[]` with exactly the six per-row fields |
| 1.3 | integration | `test_model_library_read_requires_auth` | no token → `401` problem+json; garbage token → `401` |
| 1.4 | integration | `test_model_library_read_empty_catalogue_ok` | empty table + valid token → `200`, `models: []` |
| 2.1 | unit | `test_catalogue_provider_uses_authed_read` | provider mount → Server Action called once; pickers receive its rows |
| 2.2 | unit | `test_catalogue_fetch_failure_degrades_free_text` | action rejects → provider state `error=true`; pickers render free-text (existing behavior re-asserted) |
| 2.3 | unit (grep) | `test_ui_zero_public_path_references` | `grep -rn "_um/" ui/` → 0 matches |
| 3.2 | integration | `test_cap_json_path_returns_404` | `GET /_um/<key>/cap.json` → `404` |
| 3.3 | unit (grep) | `test_zero_cap_json_references` | `grep -rn "_um/\|cap\.json" src/ ui/ public/ scripts/` → 0 matches |
| 3.4 | unit (grep) | `test_public_config_orphan_removed` | `grep -rn "publicConfig\|PublicConfig" src/` → 0 matches |
| 4.1 | build gate | `test_openapi_gate_green` | `make check-openapi` → exit 0 |
| 4.2 | unit | `test_openapi_script_units_green` | `python3 -m unittest discover -s scripts` → exit 0 |
| 5.1 | unit (grep) | `test_architecture_docs_updated` | `grep -rn "cap\.json" docs/architecture/` → 0 matches |
| regression | integration | full existing suites pass; Models-page behavior (M120_001/M120_004) unchanged | `make test && make test-integration && make test-unit-app` → exit 0, counts vs baseline |

Idempotency/replay: N/A — read-only GET surfaces.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Zero retired-path references (§2, §3) | `grep -rn "_um/\|cap\.json" src/ ui/ public/ scripts/` | no output | P0 | |
| R2 | Global-block orphans removed (§3) | `grep -rn "publicConfig\|PublicConfig" src/` | no output | P0 | |
| R3 | OpenAPI gate green (§4) | `make check-openapi` | exit 0 | P0 | |
| R4 | Architecture docs clean (§5) | `grep -rn "cap\.json" docs/architecture/` | no output | P0 | |
| R5 | Docs site aligned (§5) | `cd ~/Projects/docs && git grep -n "cap\.json" -- docs.json cli/agentsfleet.mdx api-reference/error-codes.mdx` | no output (changelog history exempt) | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0; includes the new `/v1/models` + `404` tests | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `public/openapi/paths/model-caps.yaml` | `test ! -f public/openapi/paths/model-caps.yaml` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| the wire-path constants (post-M120_003 names of `MODEL_CAPS_PATH`/`MODEL_CAPS_PATH_KEY`) and every `_um`/`cap.json` literal | `grep -rn "_um/\|cap\.json" src/ ui/ public/ scripts/` | 0 matches |
| `CAP_JSON_PATH`, `CAP_JSON_PATH_KEY` (client constants kept by M120_003, retired here) | `grep -rn "CAP_JSON" ui/` | 0 matches |
| `publicConfig`, `PublicConfig`, the Zig rates/billing response structs, the client rates/billing types | `grep -rn "publicConfig\|PublicConfig" src/ && grep -rn "LibraryRates\|LibraryBilling" ui/` | 0 matches |

## Out of Scope

- `/v1/admin/models` (admin CRUD) — untouched; it already has the correct auth posture.
- The "catalogue"-vs-"library" prose axis (`ModelCatalogueProvider` name and friends) — the separate naming axis M120_003 also excluded; edited here only where transport logic requires, never renamed.
- A Content Delivery Network (CDN)-cache replacement — the authenticated read is uncached by design; the payload is small and fetched once per session.
- Porting the `?model=` filter or the `rates`/`billing` blocks — zero consumers for either.
- Any `core.model_library` schema change or CLI change — the CLI already resolves caps server-side.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a platform operator confirms the model library is no longer world-readable: the old public URL returns `404`, while the dashboard Models page populates its pickers exactly as before.
2. **Preserved user behaviour** — the Models page (pickers, add-entry dialog, registry rates column, free-text degrade), the CLI provider flows, and admin catalogue CRUD all work unchanged.
3. **Optimal-way check** — an authenticated read replacing an unauthenticated one is the direct fix; the only shortcut rejected is leaving the public endpoint up under a renamed path (still world-readable, still breaking on rename).
4. **Rebuild-vs-iterate** — iterate: one route swap + one transport repoint; no redesign.
5. **What we build** — `GET /v1/models` (§1), the Server-Action repoint (§2), the retirement + orphan deletion (§3), OpenAPI/check updates (§4), docs alignment (§5).
6. **What we do NOT build** — no deprecation window or alias (M86_002 precedent, pre-2.0.0), no rates/billing relocation (zero consumers), no admin-surface change.
7. **Fit with existing features** — completes the M120 model-library family (M120_001–004); must not destabilize the Models page those specs built — §2's regression tests are exactly that proof.
8. **Surface order** — API + User Interface (UI) transport only; no new user-facing surface.
9. **Dashboard restraint** — no visible UI change at all; same pickers, same degrade.
10. **Confused-user next step** — a third-party reader hitting the `404` finds the changelog `<Update>` naming `GET /v1/models` and the token requirement; the API reference documents the replacement on the same nav spot.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five Sections — authed read, dashboard repoint, retirement, gate updates, docs — each independently gradeable by grep, integration test, or build gate.
- **Alternatives considered:** (a) delete the public endpoint without a replacement read — rejected: the dashboard needs a catalogue source; (b) keep an unauthenticated endpoint under a `library.json` name — rejected: the rename is the breaking event anyway, and it would preserve a world-readable surface with no consumer that needs it; (c) deprecation window with dual-serving — rejected: no-compatibility-aliases rule + M86_002 hard-cutover precedent, pre-2.0.0.
- **Patch-vs-refactor verdict:** **patch** — a mechanical route swap, transport repoint, and orphan sweep; no architectural reshape.

## Discovery (consult log)

- **Consults** —
  - Session consult (Jul 11, 2026, Indy-directed): sunset proposed by Indy; verified in-session — the dashboard Models page is the only live consumer (`ModelCatalogueProvider` fetch on mount); the CLI resolves caps server-side via `PUT /v1/tenants/me/provider` (`cli/src/commands/tenant.ts`); the install-skill never calls the endpoint (M49_001 §202); the `rates`/`billing` block has zero consumers (the dashboard discards it, the CLI pins `cli/src/constants/billing.ts`, the website pins `rates.ts`).
  - Architecture consult: route name `GET /v1/models` checked against `docs/architecture/` — no conflicting stream/route naming; `billing_and_provider_keys.md` §10 is amended by §5.
  - **OPEN — §3 pre-flip go/no-go:** access-log evidence or Indy's verbatim ack accepting the break lands here before the route flips.
- **Metrics review** — not applicable — no product/operator signal changes.
- **Skill-chain outcomes** — empty at creation.
- **Deferrals** — empty at creation.
