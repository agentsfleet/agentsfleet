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
**Status:** IN_PROGRESS
**Priority:** P2 — visual consistency + one missing capability (catalogue row Edit); no dead-end bugs, unlike M120_001.
**Categories:** UI
**Batch:** B1 — independent of M120_001; disjoint file trees (admin/models + other settings pages vs. tenant models page).
**Branch:** feat/m120-002-ui-actions-icon-sweep
**Test Baseline:** unit=2389 integration=263 (Zig depth via `make _lint_zig_test_depth`, CHORE(open)). This is a UI-only diff — coverage lands as vitest app tests (`make test-unit-app`), which this Zig gate does not count; expect a zero Zig-depth delta at VERIFY with the growth reported against the app suite instead.
**Depends on:** none
**Provenance:** human-directed, LLM-drafted (Sonnet 5, Jul 07, 2026) — Indy explicitly chose the wide (app-wide) sweep scope over a Models-page-only scope during the same Q&A that produced M120_001; a screenshot of the admin Platform Default form's misaligned provider-select surfaced the visual bug this spec also fixes. Indy asked (mid-EXECUTE on M120_001) that this workstream also get a design-shotgun/wireframe pass before the dialog/icon-row components are built — added as §2's Dimension 2.1.

**Canonical architecture:** none — presentation-only; no data-flow or resolve-path change. `docs/architecture/billing_and_provider_keys.md` is unaffected (the admin catalogue/default *data model* is untouched, only its form's presentation).

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
| `ui/.../admin/models/components/CatalogueList.tsx` | EDIT | Delete becomes an icon button (`Trash2Icon`); add an Edit icon button (`PencilIcon`) opening the new dialog |
| `ui/.../admin/models/components/EditModelDialog.tsx` | CREATE | rates/context-cap edit dialog wired to the existing `updateAdminModelAction`; mirrors `AddModelDialog.tsx`'s form shape minus the immutable provider/model_id fields |
| `ui/.../admin/models/components/PlatformDefaultCard.tsx` | EDIT | root-cause + fix the misaligned provider-`Select` popover; replace the always-inline form with a "Create default" / "Edit default" button that opens a `Dialog` (mirrors `AddModelDialog.tsx`) |
| `ui/.../admin/models/actions.ts` | EDIT | export `updateAdminModelAction` wiring if not already re-exported for the new dialog (already defined; confirm import path) |
| `ui/.../settings/api-keys/components/CreateApiKeyDialog.tsx` | EDIT | trigger button gains `PlusIcon` |
| `ui/.../admin/runners/components/AddRunnerDialog.tsx` | EDIT | trigger button gains `PlusIcon` |
| `ui/.../w/[workspaceId]/secrets/components/AddSecretDialog.tsx` | EDIT | trigger button gains `PlusIcon` |
| `ui/.../w/[workspaceId]/fleets/new/InstallConfirm.tsx` | EDIT | "Install" submit button gains an install-appropriate icon |
| `ui/.../admin/models/components/CatalogueList.test.tsx` (or equivalent) | EDIT/CREATE | icon-button + Edit-dialog coverage |
| `ui/.../admin/models/components/PlatformDefaultCard.test.tsx` (or equivalent) | EDIT | dialog-trigger + popover-fix coverage |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (one shared icon-button pattern, not a bespoke variant per page).
- **`dispatch/write_ts_adhere_bun.md`** — every touched file is `.tsx`; design-system primitives + tokens only, no raw HTML or arbitrary utility classes for the new icon buttons/dialog.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no `.zig` touched — `updateAdminModel`'s backend already exists |
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

- **Dimension 1.1** — opening the Provider select renders its options as a properly positioned popover, not an inline misaligned element → Test `test_platform_default_provider_select_popover_is_positioned`

### §2 — Platform Default becomes a "Create default" / "Edit default" dialog trigger

**Implementation default:** mirror `AddModelDialog.tsx` exactly — a `Button` (`PlusIcon` + "Create default" when no active default exists, `PencilIcon` + "Edit default" when one does) opens a `Dialog` containing today's `PlatformDefaultCard` form fields; the card no longer renders inline on the page. Per Indy's follow-up ask (this session), run a design-shotgun/wireframe pass on this dialog's layout and the catalogue's new icon row (§3) BEFORE building the components — same treatment as M120_001 §1: generate variants (or, if the `design` tool's image generation still has no OpenAI platform key configured, hand-built HTML wireframes using the app's real theme tokens, published for review), collect Indy's pick, record it in Discovery, then build to match. Do not hard-lock the dialog's exact layout from this spec's prose alone.

- **Dimension 2.1** — a design-shotgun/wireframe run exists for the Platform Default dialog + catalogue icon-row layout and Indy's pick is recorded in Discovery before the dialog/icon components are built → Acceptance (Discovery record present, not a unit test)
- **Dimension 2.2** — with no active platform default, the section renders a "Create default" trigger; saving inside the dialog activates a default and closes it → Test `test_create_default_dialog_activates_and_closes`
- **Dimension 2.3** — with an active default, the trigger reads "Edit default" and pre-fills the dialog with the active provider/model → Test `test_edit_default_dialog_prefills_active_selection`

### §3 — Admin Model Library rows get Edit + icon Delete

**Implementation default:** `CatalogueList.tsx`'s Actions column renders `PencilIcon` (opens `EditModelDialog`, wired to the existing `updateAdminModelAction`) and `Trash2Icon` (existing delete flow, unchanged behavior, icon-only now) side by side, matching `SecretsList.tsx`'s spacing/`aria-label` convention.

- **Dimension 3.1** — each catalogue row renders an Edit icon button that opens a dialog pre-filled with that row's rates/context cap; saving calls `updateAdminModelAction` and the list reflects the change → Test `test_catalogue_row_edit_dialog_updates_rates`
- **Dimension 3.2** — Delete is icon-only (`Trash2Icon`, `aria-label`) with the existing `ConfirmDialog` confirmation, behavior unchanged from today → Test `test_catalogue_row_delete_is_icon_only_same_behavior`

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
| R1 | Provider-select popover renders positioned, not misaligned (§1) | `make test-unit-app` | Dimension 1.1 passes | P1 | |
| R2 | Platform Default is a dialog trigger, not an inline form (§2) | `make test-unit-app` | Dimensions 2.2–2.3 pass | P1 | |
| R3 | Catalogue rows have icon Edit + icon Delete (§3) | `make test-unit-app` | Dimensions 3.1–3.2 pass | P1 | |
| R4 | Every listed settings page's Create/Install trigger carries its icon (§4) | `make test-unit-app` | Dimensions 4.1–4.2 pass | P2 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**Orphaned references — the inline `PlatformDefaultCard` form, if fully replaced rather than reused inside the dialog.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| any now-unused inline-form-only styling/state from the pre-dialog `PlatformDefaultCard` | `grep -rn "PlatformDefaultCard" ui/packages/app/ --include="*.tsx" \| grep -v "\.test\."` | only the dialog-trigger version remains |

No files deleted — `EditModelDialog.tsx` is new; everything else is edited in place.

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

- **Consults** — empty at creation.
- **Metrics review** — not applicable — no product/operator signal changes.
- **Skill-chain outcomes** — empty at creation.
- **Deferrals** — empty at creation.

### Decisions taken during EXECUTE

- **§4 install icon = `DownloadIcon`** (Jul 08, 2026). The spec said "install-appropriate icon" without naming one, and no existing install/download icon convention exists in the app (grep of `ui/packages/app` returned zero prior `DownloadIcon`/`PackageIcon`/etc. usages). Chose `DownloadIcon` as the universal "install" affordance; defined once and reused per UFS. Reversible if Indy prefers another glyph.
- **§1/§2/§3 data gap (Platform Default active-default awareness)** — surfaced to Indy before building §2: `page.tsx` never fetches the active platform default and `PlatformDefaultCard` only receives the `models` catalogue; even the existing `GET /v1/admin/platform-keys` returns `provider/source_workspace_id/active/updated_at` but **not `model`** (though `model` is stored on the row). So "Create default" vs "Edit default" (Dimension 2.2) and active-*provider* pre-fill are reachable by wiring the existing GET, but active-*model* pre-fill (Dimension 2.3) needs one extra read-only column on that GET. Awaiting Indy's scope call — recorded here pending the answer.
