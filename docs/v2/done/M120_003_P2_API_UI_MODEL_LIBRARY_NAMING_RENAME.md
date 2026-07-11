<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M120_003: Internal "model_caps" naming renames to "model_library", matching the table and page title

**Prototype:** v2.0.0
**Milestone:** M120
**Workstream:** 003
**Date:** Jul 07, 2026
**Status:** DEFERRED — superseded by M120_005 (Indy-directed merge, Jul 11, 2026): the rename scope is absorbed there and lands in the same pass as the public-endpoint retirement, so symbols the retirement deletes are never renamed first. Reactivation condition: only if M120_005 is abandoned. See Discovery → Deferrals.
**Priority:** P2 — pure internal-consistency rename; no user-facing behavior change, no bug.
**Categories:** API, UI
**Batch:** B2 — sequenced after M120_001/M120_002 land, since both edit files this spec renames/moves; running concurrently would create avoidable rebase churn on the same import lines.
**Branch:** {added at CHORE(open)}
**Test Baseline:** {set at CHORE(open) via `make _lint_zig_test_depth`}
**Depends on:** M120_001 (edits `ProviderModelSelect.tsx`/creates `known-models.ts`, both touching the public catalogue reader this spec renames), M120_002 (edits `CatalogueList.tsx`/`PlatformDefaultCard.tsx`/creates `EditModelDialog.tsx`, all importing the admin client this spec renames), M120_004 (reworks `AddModelEntryDialog.tsx`/`ModelsRegistryTable.tsx` and edits both client files this spec renames — same-surface sequencing)
**Provenance:** human-directed — Indy noticed mid-session that the schema table (`core.model_library`, since M100) and the admin page title ("Model library") had already moved off "caps" naming, while the Zig module/file names and TypeScript client/type names had not; confirmed by grep during this session (see Discovery).

**Canonical architecture:** none — pure rename, zero data-model or route-contract change. `docs/architecture/billing_and_provider_keys.md` is unaffected.

## Overview

**Goal (testable):** every Zig module/file and TypeScript client file/type still named `model_caps`/`Cap`/`Caps` renames to its `model_library`/`Library` equivalent, with zero remaining references to the old names anywhere in `src/` or `ui/`, and zero change to the public `cap.json` route string or the `core.model_library` table.
**Problem:** the schema table (`core.model_library`) and the admin page title ("Model library") already moved off "caps" naming; five Zig files/modules and roughly a dozen TypeScript files/types did not, so the codebase names the same concept two different ways depending on which layer you're reading.
**Solution summary:** rename the Zig module/file surface and the TypeScript client/type surface to `library`-consistent names, updating every import call site; leave the public wire path and the schema untouched (both are already correct) and prove they stayed byte-identical.

## PR Intent & comprehension handshake

- **PR title (eventual):** Rename internal model_caps naming to model_library, matching the table and page title
- **Intent (one sentence):** the codebase names the model catalogue/library concept consistently at every layer, not just at the table and page-title level.
- **Handshake:** implementing agent restates the intent + lists `ASSUMPTIONS I'M MAKING: …` at PLAN, before EXECUTE; a mismatch against the Intent above STOPs for reconciliation.

## Implementing agent — read these first

1. `docs/TEMPLATE.md` → "Teardown/rename/flip specs open with a blast-radius grep first" — run `git grep -rn -w '<token>'` from repo root, no path filter, for every renamed token below, BEFORE touching any file; record the full call-site set in Discovery.
2. `src/agentsfleetd/http/handlers/model_caps.zig` + `src/agentsfleetd/state/model_caps_store.zig` — the two Zig modules to rename; `model_caps.zig` also serves the PUBLIC `/_um/<key>/cap.json` route (route STRING is explicitly unchanged — see Out of Scope).
3. `ui/packages/app/lib/api/model_caps.ts` + `ui/packages/app/lib/api/admin_models.ts` — the two TypeScript client files to rename, including their `Cap`-named exported types.
4. `schema/003_model_library.sql` — confirms the target naming this spec converges the surrounding code onto; already renamed, not touched by this spec.

## Files Changed (blast radius)

Primary rename targets below; every import/call site the blast-radius grep (read-first #1) surfaces also updates in the same commit — this table names the sources of truth, not an exhaustive site list.

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/state/model_caps_store.zig` | RENAME → `model_library_store.zig` | module + its `Cap`/`Caps`-named exported symbols rename to `Library`-consistent names |
| `src/agentsfleetd/http/handlers/model_caps.zig` | RENAME → `model_library.zig` | handler module rename; the route STRING (`/_um/<key>/cap.json`) is unchanged |
| `src/agentsfleetd/http/handlers/model_caps_integration_test.zig` | RENAME → `model_library_integration_test.zig` | follows its subject module |
| `src/agentsfleetd/http/handlers/admin/model_caps_admin.zig` | RENAME → `model_library_admin.zig` | admin handler module rename |
| `src/agentsfleetd/http/handlers/admin/model_caps_admin_integration_test.zig` | RENAME → `model_library_admin_integration_test.zig` | follows its subject module |
| every `@import("model_caps...")` call site (`model_rate_cache.zig`, `tenant_provider_resolver.zig`, `admin/platform_keys.zig`, route registration) | EDIT | import paths follow the renamed files |
| `ui/packages/app/lib/api/model_caps.ts` | RENAME → `model_library.ts` | public catalogue reader; `ModelCap`→`LibraryModel`, `CapJson`→`ModelLibraryJson`, `CapRates`→`LibraryRates`, `CapBilling`→`LibraryBilling`, `getModelCaps`→`getModelLibrary` |
| `ui/packages/app/lib/api/admin_models.ts` | RENAME → `admin_model_library.ts` | admin CRUD client; `ModelCapInput`→`LibraryModelInput` |
| every import site under `admin/models/**` and `w/[workspaceId]/settings/models/**` | EDIT | import paths + type names follow the renamed files |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC/ORP (a rename must leave zero orphaned references to the old names — this is the entire point of the Dead Code Sweep below).
- **`dispatch/write_zig.md`** — file moves + import updates across `.zig`; cross-compile both linux targets after.
- **`dispatch/write_ts_adhere_bun.md`** — file moves + import/type updates across `.ts`/`.tsx`.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile both linux targets after every Zig rename + import fix-up |
| PUB / Struct-Shape | no | no struct *shape* changes — names only |
| File & Function Length | no | pure rename, no new logic |
| UFS | no | no new literals introduced |
| UI Substitution / DESIGN TOKEN | no | no markup/token changes |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no schema, log line, or error-code change — `core.model_library` is already the table name |

## Prior-Art / Reference Implementations

- `schema/003_model_library.sql` and `ui/.../admin/models/components/ModelsView.tsx`'s `<PageTitle>Model library</PageTitle>` — the already-renamed table and page title this spec's code naming converges onto. No new naming invented; this spec makes the code agree with what already shipped.

## Sections (implementation slices)

### §1 — Rename the Zig module/file surface

**Implementation default:** `git mv` each file, then rename every exported symbol carrying `Cap`/`Caps` to the `Library`-equivalent, fixing every `@import` call site the blast-radius grep surfaced. The route STRING `/_um/<key>/cap.json` and the `core.model_library` table/column names are untouched — this section renames code identifiers, not the wire contract or schema (both already correct).

- **Dimension 1.1** — the blast-radius grep for every `Cap`/`Caps`-named Zig file, function, and struct identifier is run and its full result set recorded in Discovery before any file is touched → Acceptance (Discovery record, not a unit test)
- **Dimension 1.2** — after the rename, zero references to the old file paths or old `Cap`/`Caps` identifiers remain anywhere in `src/` → Test `test_zig_rename_zero_old_references`
- **Dimension 1.3** — the full existing Zig test suite (unit + integration) passes unchanged, and both linux targets cross-compile clean → Test `test_zig_suite_green_post_rename`

### §2 — Rename the TypeScript client surface

**Implementation default:** `git mv` each file, rename the `Cap`-named exported types/functions to `Library`-equivalents, fix every import site across `admin/models/**` and `w/[workspaceId]/settings/models/**`. `CAP_JSON_PATH`/`CAP_JSON_PATH_KEY` (the constants naming the literal, unchanged wire path) are untouched — only the aggregate catalogue-shape types rename, not the constant naming the actual endpoint string.

- **Dimension 2.1** — the blast-radius grep for every `Cap`-named TypeScript file, type, and function identifier is run and its full result set recorded in Discovery before any file is touched → Acceptance (Discovery record, not a unit test)
- **Dimension 2.2** — after the rename, zero references to the old file paths or old `Cap`-named types/functions remain anywhere in `ui/` (excluding the untouched `CAP_JSON_PATH` constants) → Test `test_ts_rename_zero_old_references`
- **Dimension 2.3** — the full existing UI test suite passes unchanged after the rename → Test `test_ui_suite_green_post_rename`

### §3 — Prove the public contract and schema are untouched

- **Dimension 3.1** — the public wire path (`/_um/<key>/cap.json`) and the `core.model_library` table/column names are byte-for-byte unchanged by this spec's diff → Test `test_public_contract_and_schema_unchanged`

## Interfaces

Not applicable — zero request/response shape or route-path change. This spec renames code identifiers and file paths only.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|----------------------------------------------------------|
| Stale import after a file move | an `@import`/`import` site missed during the rename | build/typecheck fails immediately — caught by `make test`/`make lint` before merge, not a runtime failure mode |
| Accidental route-string or table-name edit | rename touches more than identifiers | Dimension 3.1's byte-diff check catches it |

## Invariants

1. The public `/_um/<key>/cap.json` route string is never edited by this spec — enforced by Dimension 3.1.
2. `core.model_library`'s table/column names are never edited by this spec — enforced by Dimension 3.1.

## Metrics & Observability

Not applicable — no product/operator signal changes; pure internal rename.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.2 | unit (grep-based) | `test_zig_rename_zero_old_references` | `grep -rn "model_caps\|ModelCaps" src/` → 0 matches outside `CAP_JSON`-style wire-path constants |
| 1.3 | integration (regression) | `test_zig_suite_green_post_rename` | `make test && make test-integration` → same pass count as the CHORE(open) baseline |
| 2.2 | unit (grep-based) | `test_ts_rename_zero_old_references` | `grep -rn "model_caps\|ModelCap\b" ui/` → 0 matches outside `CAP_JSON_PATH`/`CAP_JSON_PATH_KEY` |
| 2.3 | unit (regression) | `test_ui_suite_green_post_rename` | `make test-unit-app` → same pass count as the CHORE(open) baseline |
| 3.1 | unit (regression) | `test_public_contract_and_schema_unchanged` | diff of the route string + `schema/003_model_library.sql` vs. `origin/main` → empty |

Regression: 1.3/2.3/3.1 ARE the regression proof for this spec — a pure rename has no new behavior to test, only old behavior to prove unmoved. Idempotency/replay: N/A.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Zero old Zig identifiers remain (§1) | `grep -rn "model_caps\|ModelCaps" src/` | no output outside wire-path constants | P0 | |
| R2 | Zero old TS identifiers remain (§2) | `grep -rn "model_caps\|ModelCap\b" ui/` | no output outside `CAP_JSON_PATH*` | P0 | |
| R3 | Public route + schema untouched (§3) | `git diff origin/main -- schema/003_model_library.sql` | empty diff | P0 | |
| S1 | Unit tests pass | `make test` | exit 0, same count as baseline | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0, same count as baseline | P0 | |
| S6 | Cross-compile (Zig renamed) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**1. Orphaned files — old paths, pre-rename.**

| File to delete | Verify |
|----------------|--------|
| `src/agentsfleetd/state/model_caps_store.zig` | `test ! -f src/agentsfleetd/state/model_caps_store.zig` |
| `src/agentsfleetd/http/handlers/model_caps.zig` | `test ! -f src/agentsfleetd/http/handlers/model_caps.zig` |
| `src/agentsfleetd/http/handlers/model_caps_integration_test.zig` | `test ! -f src/agentsfleetd/http/handlers/model_caps_integration_test.zig` |
| `src/agentsfleetd/http/handlers/admin/model_caps_admin.zig` | `test ! -f src/agentsfleetd/http/handlers/admin/model_caps_admin.zig` |
| `src/agentsfleetd/http/handlers/admin/model_caps_admin_integration_test.zig` | `test ! -f src/agentsfleetd/http/handlers/admin/model_caps_admin_integration_test.zig` |
| `ui/packages/app/lib/api/model_caps.ts` | `test ! -f ui/packages/app/lib/api/model_caps.ts` |
| `ui/packages/app/lib/api/admin_models.ts` | `test ! -f ui/packages/app/lib/api/admin_models.ts` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `model_caps_store`/`model_caps.zig`/`model_caps_admin` (Zig import paths) | `grep -rn "model_caps" src/` | 0 matches |
| `ModelCap`, `CapJson`, `CapRates`, `CapBilling`, `ModelCapInput`, `getModelCaps` | `grep -rn "ModelCap\b\|CapJson\|CapRates\|CapBilling\|getModelCaps" ui/` | 0 matches |

## Out of Scope

- Renaming the public wire path `/_um/<key>/cap.json` — real cross-repo footprint (CLI tests, `docs/architecture/`); a breaking-change-shaped surface, not a pure rename. Left untouched (Indy-confirmed this session).
- Renaming `ModelCatalogueProvider.tsx`/"catalogue" prose — a third, distinct naming axis ("catalogue" vs. "library") not part of the "caps→library" complaint this spec addresses; left untouched to keep this rename precisely scoped.
- Any data-model, route-contract, or validation change — none; see Interfaces.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an engineer grepping the codebase for "model library" now finds it consistently named at every layer (table, page title, module files, types) instead of the table saying "library" while the code underneath still says "caps."
2. **Preserved user behaviour** — zero behavior change; every request/response shape, route path, and table name is byte-identical before and after.
3. **Optimal-way check** — a pure rename is the most direct fix for a naming-consistency complaint; no logic changes, no new abstraction.
4. **Rebuild-vs-iterate** — iterate (trivially) — this is a rename, not a redesign.
5. **What we build** — the Zig module/file rename (§1), the TS client/type rename (§2), and the byte-diff proof that the public contract + schema stayed put (§3).
6. **What we do NOT build** — no endpoint rename, no `ModelCatalogueProvider`/"catalogue" rename, no schema change.
7. **Fit with existing features** — must not regress any Models-page or admin-catalogue behavior from M100/M113/M120_001/M120_002; §1.3/§2.3 are exactly that regression proof.
8. **Surface order** — internal (Zig + TS client) only; no user-facing surface changes.
9. **Dashboard restraint** — not applicable; no UI-visible change at all.
10. **Confused-user next step** — not applicable; no user-facing surface.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections (Zig rename, TS rename, contract-untouched proof) — each independently gradeable via grep + existing test suite, no new behavior to design.
- **Alternatives considered:** renaming the public `cap.json` path too, for full consistency — rejected (Indy-confirmed); real external reference points (CLI tests, docs) make it a breaking-change-shaped change, not a pure rename, and doesn't belong in the same low-risk sweep.
- **Patch-vs-refactor verdict:** **patch** — mechanical rename, zero behavior or shape change.

## Discovery (consult log)

- **Consults** — empty at creation; §1.1/§2.1's blast-radius grep results land here once EXECUTE begins.
- **Metrics review** — not applicable — no product/operator signal changes.
- **Skill-chain outcomes** — empty at creation.
- **Deferrals** —
  - Whole-spec supersession — `> Indy (2026-07-11 08:59): "Do you think M120_005 supercedes M120_003? Since fixing M120_003 is pointless? That means we only fix the delta between M120_005 and M120_003 (base our spec on M120_005 and find delta with M120_003) and move it to M12_005 and continue as one spec M120_005 and M120_003 is closed as deferred?" — context: this spec's full rename scope (store/admin Zig renames, admin TypeScript client rename, import fix-ups, `ModelCap`→`LibraryModel`) is absorbed into M120_005 §1–§4; the symbols M120_005's retirement deletes (`CapJson`/`CapRates`/`CapBilling`, the wire-path constants) are deleted there instead of renamed here. Reactivation: only if M120_005 is abandoned.`
