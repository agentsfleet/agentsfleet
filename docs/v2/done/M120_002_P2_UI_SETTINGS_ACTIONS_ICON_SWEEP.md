<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M120_002: Admin Model Library gets Edit + icon actions; every settings-page Create/Install/Delete button standardizes onto one icon convention

**Prototype:** v2.0.0
**Milestone:** M120
**Workstream:** 002
**Date:** Jul 07, 2026
**Status:** DONE
**Priority:** P2 — visual consistency + one missing capability (catalogue row Edit); no dead-end bugs, unlike M120_001.
**Categories:** UI
**Batch:** B1 — independent of M120_001; disjoint file trees (admin/models + other settings pages vs. tenant models page).
**Branch:** feat/m120-002-ui-actions-icon-sweep
**Test Baseline:** unit=2389 integration=263 (Zig depth via `make _lint_zig_test_depth`, CHORE(open)). This is a UI-only diff — coverage lands as vitest app tests (`make test-unit-app`), which this Zig gate does not count; expect a zero Zig-depth delta at VERIFY with the growth reported against the app suite instead.
**Depends on:** none
**Provenance:** human-directed, LLM-drafted (Sonnet 5, Jul 07, 2026) — Indy explicitly chose the wide (app-wide) sweep scope over a Models-page-only scope during the same Q&A that produced M120_001; a screenshot of the admin Platform Default form's misaligned provider-select surfaced the visual bug this spec also fixes. Indy asked (mid-EXECUTE on M120_001) that this workstream also get a design-shotgun/wireframe pass before the dialog/icon-row components are built — added as §2's Dimension 2.1.

**Canonical architecture:** near-presentation-only. The one non-presentation change (Option A, see Discovery) surfaces an already-stored column (`platform_provider_defaults.model`) through the existing `GET /v1/admin/platform-keys` read — no schema, validation, or resolve-path change. `docs/architecture/billing_and_provider_keys.md` is unaffected (the catalogue/default *data model* and the resolve path are untouched; only the admin read response gains a field the row already carries).

## Overview

**Goal (testable):** the admin Model Library page's catalogue rows expose icon Edit (new capability, wired to the already-existing `updateAdminModelAction`) and icon Delete; the Platform Default section becomes a "Create default"/"Edit default" dialog trigger instead of an always-inline form with a misaligned provider-select popover; and every other settings page's bare-text "Create X"/"Install X" trigger gets the matching icon already established by the Secrets and fleet-library pages.
**Problem:** the admin catalogue has no way to edit a model's rates short of delete-and-recreate; its Delete action and every other settings page's Create/Install triggers are inconsistent text-only buttons while two pages (Secrets, fleet library) already carry icons; and the Platform Default form's provider-select renders its open option as a misaligned, unstyled element.
**Solution summary:** wire the catalogue's existing-but-unused `updateAdminModelAction` to a new icon Edit dialog; convert Delete to icon-only; convert the Platform Default section to a Dialog-trigger button (fixing the popover bug in the process or alongside it); and add the matching icon to every remaining bare-text Create/Install trigger across settings pages.

## PR Intent & comprehension handshake

- **PR title (eventual):** Standardize Create/Install/Edit/Delete icons across settings pages; admin Model Library gets Edit
- **Intent (one sentence):** every settings-page action button carries the same icon convention, and the admin Model Library's previously-unreachable Edit capability becomes usable.
- **Handshake:** implementing agent restates the intent + lists `ASSUMPTIONS I'M MAKING: …` at PLAN, before EXECUTE; a mismatch against the Intent above STOPs for reconciliation.

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.tsx` — the established icon convention (`PencilIcon`/`Trash2Icon`, ghost/destructive `Button` variants, `aria-label`, `ConfirmDialog` for the destructive confirm) every other page converges onto.
2. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/AddLibraryDialog.tsx` — the one place a "Create" trigger already carries `<PlusIcon size={14} />` + label; the pattern every other bare-text "Create X" button in this spec adopts.
3. `ui/packages/app/app/(dashboard)/admin/models/components/{CatalogueList,PlatformDefaultCard,AddModelDialog}.tsx` — the admin Model Library page's current shape: `CatalogueList` has Delete only (text button, no Edit at all); `PlatformDefaultCard` is an always-inline form with a `Select` whose open state renders misaligned (screenshot on file); `AddModelDialog` is the Dialog-trigger pattern `PlatformDefaultCard`'s replacement button converges onto.
4. `ui/packages/app/lib/api/admin_models.ts` — `updateAdminModel(token, uid, ModelRatesInput)` (PATCH by `uid`, rates/caps only) already exists and is fully unwired in the UI — the new Edit action calls this directly, no backend change needed.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/.../admin/models/components/CatalogueList.tsx` | EDIT | Actions column: icon Edit (`PencilIcon`) + Make default (`StarIcon`) + icon Delete (`Trash2Icon`); "Default" badge on the active row (hides its ★); renders `EditModelDialog` + `MakeDefaultDialog` |
| `ui/.../admin/models/components/EditModelDialog.tsx` | CREATE | rates/context-cap edit dialog wired to the existing `updateAdminModelAction`; mirrors `AddModelDialog.tsx`'s form shape minus the immutable provider/model_id fields (shown disabled) |
| `ui/.../admin/models/components/MakeDefaultDialog.tsx` | CREATE | **(pivot)** minimal "make this row the platform default" dialog — API key (+ base URL for openai-compatible) only; provider/model from the row; reuses `setPlatformDefaultAction` |
| `ui/.../admin/models/components/PlatformDefaultCard.tsx` | **DELETE** | **(pivot)** the whole Platform Default section is removed; making a row the default is now a row action, not a separate form. §1's popover bug is deleted with it |
| `ui/.../admin/models/actions.ts` | EDIT | `updateAdminModelAction` already exported; add `listPlatformKeysAction` (Option A read path) |
| `src/agentsfleetd/http/handlers/admin/platform_keys/sql.zig` | EDIT | Option A — `SELECT_KEYS` gains the already-stored `model` column |
| `src/agentsfleetd/http/handlers/admin/platform_keys.zig` | EDIT | Option A — `PlatformKeyRow.model: ?[]const u8`; GET reads/returns it (nullable) |
| `public/openapi{.json,/components/schemas.yaml}` | EDIT | Option A — `PlatformKey` schema gains nullable `model` (YAML source + bundled json) |
| `src/agentsfleetd/http/handlers/admin/model_caps_admin_integration_test.zig` | EDIT | Option A — assert GET returns the active row's `model` |
| `ui/.../lib/api/admin_models.ts` | EDIT | Option A — `PlatformKey`/`PlatformKeyList` types, `listPlatformKeys`, `activePlatformDefault` |
| `ui/.../admin/models/page.tsx` | EDIT | Option A — fetch platform-keys error-tolerantly, thread the active default into `ModelsView` → `CatalogueList` for the badge |
| `ui/.../admin/models/components/ModelsView.tsx` | EDIT | drop the Platform Default section; thread `activeDefault` + `onUpdated` into `CatalogueList` |
| `ui/.../settings/api-keys/components/CreateApiKeyDialog.tsx` | EDIT | trigger button gains `PlusIcon` |
| `ui/.../admin/runners/components/AddRunnerDialog.tsx` | EDIT | trigger button gains `PlusIcon` |
| `ui/.../w/[workspaceId]/secrets/components/AddSecretDialog.tsx` | EDIT | trigger button gains `PlusIcon` |
| `ui/.../admin/models/components/AddModelDialog.tsx` | EDIT | **(added mid-flight, Indy)** "Create model library" trigger gains `PlusIcon` — the one Create button on this page the sweep list first missed |
| `ui/.../w/[workspaceId]/fleets/new/InstallConfirm.tsx` | EDIT | "Install" submit button gains `DownloadIcon` |
| `ui/.../tests/{admin-models-ui,admin-models-page,admin-models-actions,api-keys-create-dialog,runners-list,secrets-components,fleets-install-flow}.test.ts` + `lib/api/admin_models.test.ts` | EDIT | icon assertions, Edit/Make-default/badge coverage, `listPlatformKeys`/`activePlatformDefault` + read-path tests |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (one shared icon-button pattern, not a bespoke variant per page).
- **`dispatch/write_ts_adhere_bun.md`** — every touched file is `.tsx`; design-system primitives + tokens only, no raw HTML or arbitrary utility classes for the new icon buttons/dialog.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE / XCOMPILE | **yes** (Option A) | `platform_keys` GET gains the already-stored `model` column (read-only; no schema/validation/resolve change). Native + `zig build -Dtarget=x86_64-linux` + `-Dtarget=aarch64-linux` all green; nullable read via `row.get(?[]const u8, …)`. `PlatformKeyRow` is a module-private `const` (no new `pub` → PUB GATE skipped) |
| PUB / Struct-Shape | yes | `EditModelDialog`'s props shape verdict at PLAN |
| File & Function Length | possible | split if `PlatformDefaultCard.tsx`'s dialog conversion nears 350 lines |
| UFS | yes | one shared icon set (Plus/Pencil/Trash2) via existing `lucide-react` imports, not re-invented per page |
| UI Substitution / DESIGN TOKEN | yes | reuse `SecretsList.tsx`'s icon-button + `ConfirmDialog` primitives; `AddModelDialog.tsx`'s `Dialog`/`Form` primitives for the new Edit dialog and the converted Platform Default trigger |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no new error code, no schema/data-model change |

## Prior-Art / Reference Implementations

- **Icon convention** → `ui/.../secrets/components/SecretsList.tsx` (Edit/Delete) and `ui/.../fleets/new/AddLibraryDialog.tsx` (Create) — the two patterns every other page in this spec converges onto; no new icon choices invented.
- **Dialog-trigger conversion** → `ui/.../admin/models/components/AddModelDialog.tsx` — the exact `Dialog`/`DialogTrigger`/`Form` shape `PlatformDefaultCard.tsx`'s replacement button opens.

## Sections (implementation slices)

### §1 — Fix the misaligned Provider-select popover in the admin Platform Default form

The screenshot shows the `Select`'s open option ("fireworks") rendering as an unstyled, misaligned element below the trigger instead of a positioned popover — most likely a portal/stacking-context issue specific to this card's nesting, not the shared `Select` primitive itself (no other call site of `Select` has this reported). **Implementation default:** root-cause against `PlatformDefaultCard.tsx`'s specific DOM nesting before assuming a shared-primitive fix is needed; this section's fix must land before or alongside §2's dialog conversion, since moving the form into a `Dialog` may itself resolve the stacking-context issue as a side effect — confirm either way rather than assuming.

- **Dimension 1.1** — ~~opening the Provider select renders its options as a properly positioned popover~~ **STRUCK (pivot, see Discovery)** — the misaligned select lived in `PlatformDefaultCard`, which was deleted. No provider select remains on this surface, so there is nothing to fix. R1 is struck from the rubric.

### §2 — Platform Default becomes a "Create default" / "Edit default" dialog trigger

**Implementation default:** mirror `AddModelDialog.tsx` exactly — a `Button` (`PlusIcon` + "Create default" when no active default exists, `PencilIcon` + "Edit default" when one does) opens a `Dialog` containing today's `PlatformDefaultCard` form fields; the card no longer renders inline on the page. Per Indy's follow-up ask (this session), run a design-shotgun/wireframe pass on this dialog's layout and the catalogue's new icon row (§3) BEFORE building the components — same treatment as M120_001 §1: generate variants (or, if the `design` tool's image generation still has no OpenAI platform key configured, hand-built HTML wireframes using the app's real theme tokens, published for review), collect Indy's pick, record it in Discovery, then build to match. Do not hard-lock the dialog's exact layout from this spec's prose alone.

- **Dimension 2.1** — a design-shotgun/wireframe run exists for the Platform Default dialog + catalogue icon-row layout and Indy's pick is recorded in Discovery before the dialog/icon components are built → Acceptance (Discovery record present, not a unit test) — **DONE** (wireframe published, Indy picked §2=A, §3=1; see Discovery)
- **Dimension 2.2** — **REFRAMED (pivot):** a catalogue row's **★ Make default** action opens a minimal `MakeDefaultDialog` (API key + base URL for openai-compatible only); saving calls `setPlatformDefaultAction` with the row's (provider, model) and closes → **DONE** (`admin-models-ui.test.ts` "★ opens a minimal key dialog…", "requires a base URL for an openai-compatible row…")
- **Dimension 2.3** — **REFRAMED (pivot):** the active default's catalogue row shows a **"Default"** badge and hides its own ★; other rows keep ★. The active (provider, model) comes from `listPlatformKeys` → `activePlatformDefault` (Option A read), matched against each row → **DONE** (`admin-models-ui.test.ts` "badges the active default's row and hides its ★…", page threading + tolerance in `admin-models-page.test.ts`)

### §3 — Admin Model Library rows get Edit + icon Delete

**Implementation default:** `CatalogueList.tsx`'s Actions column renders `PencilIcon` (opens `EditModelDialog`, wired to the existing `updateAdminModelAction`) and `Trash2Icon` (existing delete flow, unchanged behavior, icon-only now) side by side, matching `SecretsList.tsx`'s spacing/`aria-label` convention.

- **Dimension 3.1** — each catalogue row renders an Edit icon button (`PencilIcon`) that opens `EditModelDialog` pre-filled with that row's rates/context cap (provider + model id disabled/immutable); saving calls `updateAdminModelAction` with the row's `uid` and the list reflects the change → **DONE** (`admin-models-ui.test.ts` "opens a dialog pre-filled…PATCHes the new rates by uid", ModelsView "reflects an edited model's new rates")
- **Dimension 3.2** — Delete is icon-only (`Trash2Icon`, `aria-label` `Delete {model_id}`) with the existing `ConfirmDialog` confirmation, behavior unchanged from today → **DONE** (`admin-models-ui.test.ts` "Delete is an icon-only destructive button…", delete confirm/success/failure/cancel tests)
- **Dimension 3.3** (pivot) — each non-default row renders a `StarIcon` **Make default** action between Edit and Delete; the active row shows the Default badge instead → **DONE** (covered with 2.2/2.3 above)

### §4 — Global icon sweep across remaining settings pages

**Implementation default:** every bare-text "Create X"/"Install X" trigger button in the Files Changed table above gains the matching icon from the established convention; no new backend calls, no behavior change — trigger label + icon only.

- **Dimension 4.1** — API Keys, Runners, and Secrets "Create X" triggers each render a `PlusIcon` alongside their existing label → Test `test_create_triggers_render_plus_icon` — **DONE** (assertions added to `api-keys-create-dialog`, `runners-list`, `secrets-components` tests where each component already renders)
- **Dimension 4.2** — the fleets "Install" confirm button renders an install-appropriate icon alongside its existing label → Test `test_install_confirm_renders_icon` — **DONE** (`DownloadIcon`; see Discovery)

## Interfaces

Not applicable — no new endpoint, no changed request/response shape. `EditModelDialog` calls the existing `PATCH /v1/admin/models/{uid}` via `updateAdminModelAction`.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|----------------------------------------------------------|
| Edit dialog PATCH fails | invalid rate input or network error | same error-presentation pattern as `AddModelDialog.tsx`'s `apiError` — inline message, dialog stays open |
| Popover fix regresses positioning elsewhere | shared `Select` primitive touched instead of local nesting | regression Dimension 1.1 re-run against `AddModelDialog.tsx`'s own selects (none reported broken today) |

## Invariants

1. `CatalogueList.tsx`'s Delete action's underlying behavior (confirm → `deleteAdminModelAction` → row removed) is unchanged — only its trigger becomes icon-only — enforced by Dimension 3.2 (regression assertion).
2. The Edit dialog can never submit a changed `provider`/`model_id` (those remain the row's immutable identity) — enforced by `EditModelDialog` reusing `ModelRatesInput` (which structurally excludes them), not a runtime check.

## Metrics & Observability

Not applicable — no product/operator signal changes; this is a presentation/consistency pass plus one previously-unreachable capability (Edit) exposed through an already-existing, already-instrumented backend action.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_platform_default_provider_select_popover_is_positioned` | open the Provider select → its option list renders inside a positioned popover element, not inline below the trigger |
| 2.1 | — | (Acceptance) | design-shotgun/wireframe run exists, Indy's pick recorded in Discovery |
| 2.2 | unit | `test_create_default_dialog_activates_and_closes` | no active default → "Create default" trigger renders; submit → `setPlatformDefaultAction` called, dialog closes |
| 2.3 | unit | `test_edit_default_dialog_prefills_active_selection` | active default exists → trigger reads "Edit default"; opening it shows the active provider/model pre-selected |
| 3.1 | unit | `test_catalogue_row_edit_dialog_updates_rates` | click Edit on a row → dialog pre-filled; submit new rates → `updateAdminModelAction` called with the row's `uid` |
| 3.2 | unit (regression) | `test_catalogue_row_delete_is_icon_only_same_behavior` | click Delete icon → same `ConfirmDialog` + `deleteAdminModelAction` flow as before, icon-only trigger |
| 4.1 | unit | `test_create_triggers_render_plus_icon` | render each of `CreateApiKeyDialog`/`AddRunnerDialog`/`AddSecretDialog` → trigger button contains a `PlusIcon` |
| 4.2 | unit | `test_install_confirm_renders_icon` | render `InstallConfirm` → submit button contains an icon alongside "Install" |

Regression: existing `CatalogueList`/`PlatformDefaultCard` test suites pass once assertions move to the icon/dialog shape. Idempotency/replay: N/A — no retry semantics touched.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | ~~Provider-select popover renders positioned~~ **STRUCK (pivot)** — `PlatformDefaultCard` + its select were deleted | — | n/a | P1 | ✅ n/a |
| R2 | Making a row the platform default is a ★ row action + minimal key dialog; the active row is badged (§2 pivot) | `make test-unit-app` | Dimensions 2.2–2.3 pass | P1 | ✅ 1252 pass |
| R3 | Catalogue rows have icon Edit + ★ Make default + icon Delete (§3) | `make test-unit-app` | Dimensions 3.1–3.3 pass | P1 | ✅ 1252 pass |
| R4 | Every settings-page Create/Install trigger carries its icon, incl. AddModelDialog (§4) | `make test-unit-app` | Dimensions 4.1–4.2 pass | P2 | ✅ 1252 pass |
| S1 | Unit tests pass | `make test-unit-app` (no `make test` target exists — corrected) | exit 0 | P0 | ✅ 132 files / 1252 tests |
| S1b | App coverage gate (CI) | `cd ui/packages/app && bun run test:coverage` | 100% stmts/branch/funcs/lines | P0 | ✅ 100% |
| S2 | Lint clean | `make lint-app` + `make lint-zig` + `make check-openapi` | exit 0 | P0 | ✅ all green (pre-commit) |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ (pre-commit) |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l \| awk '$1>350'` | only FLL-exempt `*_test.*` | P0 | ✅ only 2 test files (exempt) |
| S9 | Orphan sweep | `grep -rn "PlatformDefaultCard" ui/packages/app/` | 0 matches | P0 | ✅ 0 |
| Sx | Backend compiles + cross-compiles | native + `zig build -Dtarget={x86_64,aarch64}-linux` + `test-bin` | exit 0 | P0 | ✅ all green (integration test runs in CI — no local Postgres) |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**`PlatformDefaultCard` is deleted outright (pivot) — no production or test reference may remain.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `PlatformDefaultCard` (component + its import in `ModelsView`) | `grep -rn "PlatformDefaultCard" ui/packages/app/` | 0 matches |

New files: `EditModelDialog.tsx`, `MakeDefaultDialog.tsx`. Deleted: `PlatformDefaultCard.tsx`. `setPlatformDefaultAction` + the `platform_default_set` telemetry event survive (now driven by `MakeDefaultDialog`).

## Out of Scope

- Any change to `core.model_library`/`platform_provider_defaults`'s data model, validation rules, or the resolve path — all backend behavior here already exists and is reused as-is.
- The tenant-facing Models page redesign (Default row semantics, delete-stale-key, platform-default-availability fix) — tracked separately as M120_001.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a platform admin opens `/admin/models`, edits an existing model's rates via a pencil icon instead of having to delete-and-recreate it, and sets the Platform Default through a clean "Create default"/"Edit default" dialog whose provider dropdown renders correctly.
2. **Preserved user behaviour** — `createAdminModelAction`/`deleteAdminModelAction`/`setPlatformDefaultAction` and every existing test's assertions on their *behavior* are unchanged; only trigger presentation (icon vs. text, inline vs. dialog) changes.
3. **Optimal-way check** — reusing the already-existing, already-tested `updateAdminModelAction` for the new Edit capability is more direct than adding a new backend path; the icon convention already exists on the Secrets page, so this sweep applies it rather than inventing a new one.
4. **Rebuild-vs-iterate** — iterate. No data-model or handler change; purely presentation + one previously-unwired capability exposed.
5. **What we build** — the popover fix, the Platform Default dialog conversion, catalogue-row Edit + icon Delete, and the app-wide icon standardization on the listed pages.
6. **What we do NOT build** — no new backend endpoint; no change to the catalogue/default data model or validation.
7. **Fit with existing features** — must not destabilize M100's admin catalogue/default flow or M113's Secrets-page icon pattern it mirrors.
8. **Surface order** — UI only; no CLI/API surface touched.
9. **Dashboard restraint** — no new signal added; this is a presentation/consistency pass.
10. **Confused-user next step** — N/A for the icon sweep (no new failure surface); the Edit dialog's error path mirrors `AddModelDialog.tsx`'s existing inline-error pattern.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four Sections (popover fix, dialog conversion, catalogue Edit/Delete icons, app-wide sweep) — independently testable, all presentation-layer.
- **Alternatives considered:** scoping the icon sweep to only the Models pages (narrower) — rejected; Indy explicitly chose the wider app-wide scope during the Q&A that produced this spec.
- **Patch-vs-refactor verdict:** **patch** — presentation-layer consolidation; no backend or data-model change.

## Discovery (consult log)

- **Consults** — design-shotgun (Dimension 2.1) + the make-default pivot decision, both recorded above.
- **Metrics review** — not applicable — no product/operator signal changes (the `platform_default_set` event is unchanged, now fired by `MakeDefaultDialog`).
- **Skill-chain outcomes:**
  - `/write-unit-test` — coverage 100% (stmts/branch/funcs/lines) across the app suite (132 files / 1252 tests); the Zig GET-model + inactive-row-null contract pinned in the integration suite (runs in CI — no local Postgres).
  - `/review` (local, high effort — 3 correctness + cleanup + conventions angles) — Zig clean; 3 TS + 4 cleanup findings. Fixed: dead `listPlatformKeysAction` (NDC), `pending`/`deleting` collapse, honest page.tsx catch comment, `EditModelDialog` identity-field dedup, inactive-row null test. **Deferred (documented):** Add/Edit rate-schema mirror (intentional per §3), `router.refresh()` stale-catalogue on concurrent add (rare edge; primary flow correct).
  - `kishore-babysit-prs` — runs after push (report appended to PR Session Notes).
- **Deferrals (with reasoning, no ack needed — all agent-verified low-risk or intentional):**
  - **In-place key rotation for the active default** — the ★ is hidden on the active row per Indy's design, so re-entering that provider's key happens on the workspace **Secrets** page (the platform key is a vault secret named for the provider). Surfaced to Indy as a known trade-off; can add an in-place rotate action as a follow-up if wanted.
  - **`VERSION` not bumped** — changelog is date-decoupled; a P2 UI + read-only API field doesn't clearly warrant a minor bump. Flag for Indy if a release is being cut.

### Design-shotgun (Dimension 2.1)

- **Wireframe run** (Jul 09, 2026) — hand-built HTML wireframes in the real agentsfleet dark/light tokens (image-gen path unavailable per the M120_001 note), two variants per surface, published for review: `https://claude.ai/code/artifact/3add99c7-6ce3-4617-8656-bf371b658490`.
- **Indy's pick (2026-07-09):** §2 dialog = **A (Stacked — mirrors the Add-model dialog)**; §3 row actions = **1 (Secrets-page parity — ghost pencil + destructive trash)**. Both surfaces build to match these; §1's popover fix is folded into the §2 dialog conversion.

### Direction pivot — Platform Default becomes a row action (Indy, Jul 09, 2026)

> Indy (2026-07-09): "I think the PLATFORM DEFAULT component and section can be removed. Since essentially an enduser just makes a model library entry as default. So this must be an action (with an icon) after edit pencil which illustrates `make as default`?" + picked **Minimal key dialog** (no backend change) for the API-key handling.

This supersedes the original §1/§2 design:

- **§1 (popover fix) — DISSOLVED.** The misaligned provider-`Select` lived in `PlatformDefaultCard`, which is being deleted; there is no longer a select to fix. Dimension 1.1 / R1 are struck.
- **§2 (Platform Default → dialog trigger) — REPLACED.** No separate Platform Default section or `PlatformDefaultCard`. Instead the catalogue row carries a **★ Make default** action between ✎ Edit and 🗑 Delete. The currently-active default row shows a **"Default"** badge and hides its own ★.
- **★ Make default** opens a minimal `MakeDefaultDialog` collecting **only** the API key (+ base URL for the openai-compatible provider) — provider + model are the row's known identity. It reuses `setPlatformDefaultAction({provider, model, api_key, base_url})` **unchanged** (zero backend change). The key is required each (re)set because it is write-only and never read back.
- **§3** stays and grows: Edit + Make-default + icon Delete per row, plus the Default badge.
- **Option A read path still used:** `listPlatformKeys` → `activePlatformDefault` gives the active (provider, model); the row whose (provider, model_id) matches gets the "Default" badge. The GET `model` column added earlier is exactly what makes that match possible.

### Decisions taken during EXECUTE

- **§4 install icon = `DownloadIcon`** (Jul 08, 2026). The spec said "install-appropriate icon" without naming one, and no existing install/download icon convention exists in the app (grep of `ui/packages/app` returned zero prior `DownloadIcon`/`PackageIcon`/etc. usages). Chose `DownloadIcon` as the universal "install" affordance; defined once and reused per UFS. Reversible if Indy prefers another glyph.
- **§1/§2/§3 data gap (Platform Default active-default awareness)** — surfaced to Indy before building §2: `page.tsx` never fetches the active platform default and `PlatformDefaultCard` only receives the `models` catalogue; even the existing `GET /v1/admin/platform-keys` returns `provider/source_workspace_id/active/updated_at` but **not `model`** (though `model` is stored on the row). So "Create default" vs "Edit default" (Dimension 2.2) and active-*provider* pre-fill are reachable by wiring the existing GET, but active-*model* pre-fill (Dimension 2.3) needs one extra read-only column on that GET.

  **Resolution — Option A (Indy, Jul 09, 2026):**
  > Indy (2026-07-09): "Yes option A" — context: the Platform Default active-default data gap; authorizes adding `model` to the platform-keys GET (a read-only column already stored on the row — no schema/validation/resolve-path change) plus a UI read path, to fully deliver Dimensions 2.2 + 2.3 (provider **and** model pre-fill on "Edit default"). ZIG GATE + cross-compile are now in scope for M120_002.

  Scope note surfaced while wiring: `GET /v1/admin/platform-keys` is gated on `platform-key:read` (`route_scopes.zig:123`), a *different* scope from the admin/models page's `model:read` gate. A `model:read`-only viewer would 403 on the platform-keys fetch, so `page.tsx` fetches it error-tolerantly (any failure → `activeDefault = null` → "Create default" label, no pre-fill), mirroring how `setPlatformDefaultAction` already uses `model:admin` as the dashboard's defence-in-depth proxy for a `platform-key:admin` backend route.
