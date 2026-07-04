<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M113_001: Models page collapses to one uniform provider-row list; real dropdown for Other provider

**Prototype:** v2.0.0
**Milestone:** M113
**Workstream:** 001
**Date:** Jul 04, 2026
**Status:** DONE
**Priority:** P1 — the confusing "Bring your own key" affordance and redundant hero copy were flagged directly against the shipping v2.0.0 dashboard.
**Categories:** UI
**Batch:** B1 — independent of M113_002 (error copy) and M113_003 (Secrets split); both touch this page's file tree but different concerns.
**Branch:** feat/m108-connector-platform — folded into the SAME branch/PR (#477) as M108/M112, by Indy's explicit instruction this session.
**Test Baseline:** unit=2309 integration=249
**Depends on:** none
**Provenance:** LLM-drafted (Claude Sonnet 5, Jul 04, 2026) from a live design pass + targeted codebase investigation this session (traced "Bring your own key" to a scroll-only anchor with no form-open behavior, confirmed the model-catalogue endpoint already backs a dropdown for Model but not Provider).

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §8 (self-managed credential model, provider resolution) — this spec changes layout/presentation only, not the credential/provider data model.

---

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/settings/models/components/ActiveModelHero.tsx` — the special-cased hero card being folded away; read fully, including its "Bring your own key" anchor-link and the `canRotate`/reset logic that must survive the move.
2. `ui/packages/app/app/(dashboard)/settings/models/components/ProviderSwitchList.tsx` — the target shape every row (including the former hero) converges into; read its row rendering, `uniqueProviders(models)` helper, and the generic "Other provider" expand-in-place form.
3. `ui/packages/app/app/(dashboard)/integrations/components/connector-rows.tsx` — the Prior-Art reference: a single catalog-driven row component already serving this exact "one list, one row shape, per-row action" pattern for a sibling settings page (M108). Mirror its shape rather than inventing a new one.
4. `ui/packages/app/app/(dashboard)/settings/models/components/ProviderModelSelect.tsx` and `ui/packages/app/lib/api/model_caps.ts` — the existing model-catalogue fetch + dropdown-degrades-to-free-text logic; Dimension 2.1 extends this same data to the Provider field rather than adding a new fetch.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Models page: one row list, not a hero + a list; real Provider dropdown for Other provider
- **Intent (one sentence):** the platform-default model reads and behaves like every other provider row, and picking an unlisted provider no longer requires guessing a string by hand.
- **Handshake:** implementing agent restates intent + assumptions at PLAN, before EXECUTE.

---

## Product Clarity

1. **Successful user moment** — a user opens Settings → Models and sees one list of rows (Platform default, Custom — OpenAI-compatible, Other provider, any already-configured named provider); each row shows its status and one action; there is no separate hero card and no button whose only effect is an anchor scroll.
2. **Preserved user behaviour** — the "DEFAULT · ACTIVE MODEL" pill semantics, the underlying activate/reset/rotate server actions (`setProviderSelfManagedAction`, reset-to-platform, `HeroReplaceKeyPanel`'s rotate), and every existing test's assertions on those actions' *behavior* are unchanged — only the layout/copy around them moves.
3. **Optimal-way check** — this converges Models onto the exact row-list pattern the Integrations Connectors page (M108) already established and tested; reusing it is more direct than inventing a second "unified settings row" shape.
4. **Rebuild-vs-iterate** — iterate. `ActiveModelHero`'s internal logic (balance/status resolution, rotate/reset calls) is correct; only its card-vs-row presentation and the anchor-scroll button change. A full rewrite of the provider/credential data flow is out of scope and unjustified.
5. **What we build** — fold the hero's content into a row rendered by the same list `ProviderSwitchList` owns; remove the "Bring your own key" button; trim the redundant "Platform default model" / "Managed by agentsfleet · no key needed" copy given the pill already states the same fact; wire the "Other provider" form's Provider field to a real dropdown sourced from the catalogue's known providers, with free-text fallback preserved for an unrecognized entry.
6. **What we do NOT build** — no new backend endpoint (the public `/_um/.../cap.json` catalogue already supplies the provider list); no change to `CustomEndpointForm` (openai-compatible deliberately bypasses the catalogue — Product Clarity #6 stays true); no change to the actual activate/store/rotate server actions' signatures.
7. **Fit with existing features** — must not destabilize the Secrets & ENVs section this page currently also renders; M113_003 moves that section out to its own page entirely, so this spec's row-list section sits alone on the page once both land — implementing agent should avoid hard-coding assumptions about what renders below the row list.
8. **Surface order** — UI only; no CLI/API surface touched.
9. **Dashboard restraint** — no new signal added (no provider-health indicator, no new badge); this is a layout/copy consolidation, not a new capability.
10. **Confused-user next step** — if a user's provider isn't in the dropdown, the existing free-text fallback (already implemented in `ProviderModelSelect`) remains available — no dead end.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal repo discipline.
- `dispatch/write_ts_adhere_bun.md` — every touched file is `.ts`/`.tsx`; TS FILE SHAPE DECISION at PLAN for any component the agent chooses to split out of `ActiveModelHero`.
- `dispatch/write_any.md` — UFS (no new duplicated literal strings; reuse/relocate existing constants rather than copy-pasting).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no `.zig` touched |
| PUB / Struct-Shape | yes — new/changed component props | shape verdict per the merged row component at PLAN |
| File & Function Length | possible | if the merged list component approaches 350 lines, split per existing per-row-component convention (mirror `connector-rows.tsx`'s split) |
| UFS | yes | reuse existing action/copy constants where the row moves, rather than duplicating |
| UI Substitution / DESIGN TOKEN | yes | reuse `DashboardRow`/`DashboardRowGroup` (or whatever primitive `ProviderSwitchList` already uses) for the folded-in row — no new bespoke card markup |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | not touched |

---

## Overview

**Goal (testable):** the Models page renders the platform-default model as one row inside the same list as every other provider, with no separate hero card and no "Bring your own key" button; the generic "Other provider" form's Provider field is a dropdown sourced from the model catalogue, not free text.

**Problem:** the current page mixes a special hero card (redundant heading + subtext, a button that only scrolls) with a separate list below it, so users experience "one click, nothing happens" and can't tell what providers/models are actually supported without guessing.

**Solution summary:** merge the hero's content into `ProviderSwitchList`'s row shape; delete the anchor-scroll button; source the Provider field's options from the catalogue data already fetched for the Model field.

---

## Prior-Art / Reference Implementations

- **UI** → `ui/packages/app/app/(dashboard)/integrations/components/connector-rows.tsx` (M108) — the exact "one catalog-driven row component, one action per row" shape to mirror for the Models page's row list.
- Design-system primitives already in use on this page (`DashboardRow`/`DashboardRowGroup`, `StatusPill`) — no new primitive needed.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/app/(dashboard)/settings/models/components/ActiveModelHero.tsx` | EDIT (likely reduced to a thin adapter, or deleted with its logic folded into `ProviderSwitchList.tsx`) | remove the hero-card treatment + "Bring your own key" button |
| `ui/packages/app/app/(dashboard)/settings/models/components/ProviderSwitchList.tsx` | EDIT | render the platform-default row first in the same list; own the merged row shape |
| `ui/packages/app/app/(dashboard)/settings/models/components/ProviderKeyForm.tsx` | EDIT | Provider field becomes a dropdown (unlocked/generic case only) |
| `ui/packages/app/app/(dashboard)/settings/models/page.tsx` | EDIT | composition update to the merged list |
| `ui/packages/app/tests/active-model-hero.test.tsx` | EDIT | assertions move to match the merged row |
| `ui/packages/app/tests/provider-switch-list.test.tsx` | EDIT | new assertions for the merged default row |
| `ui/packages/app/tests/hero-change-model-panel.test.tsx` | EDIT | update render target if the panel's mount point moves |
| `ui/packages/app/tests/provider-key-form.test.tsx` | EDIT | dropdown assertions for the Provider field |
| `ui/packages/app/tests/e2e/acceptance/settings-models.spec.ts` | EDIT | update selectors if the DOM structure changes |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** two Sections — row-list unification, then the Provider dropdown — each independently testable.
- **Alternatives considered:** leave the hero card but just fix the button (make it open a form instead of scrolling) — rejected because it doesn't address the actual complaint (redundant heading, inconsistent visual weight against the rest of the list); the row-unification is the same size of change and fixes both.
- **Patch-vs-refactor verdict:** **patch** — presentation-layer consolidation of existing, correct logic; no data-model or server-action change.

---

## Sections (implementation slices)

### §1 — Fold the hero card into the row list — DONE

**Implementation default:** the platform-default row renders through the same component `ProviderSwitchList` uses for every other row, positioned first; its distinguishing content (balance-independent "DEFAULT · ACTIVE MODEL" pill, provider/context/billing facts) becomes that row's expanded/detail content, not a separate card.

- **Dimension 1.1** — the platform-default row and every other provider row share one list container and one row shape → Test `test_models_page_renders_one_row_list`
- **Dimension 1.2** — no element with accessible name "Bring your own key" exists; the default row's own action opens the same "Other provider"-style form inline (one click, not a scroll) → Test `test_bring_your_own_key_button_removed`
- **Dimension 1.3** — the redundant "Platform default model" heading / "Managed by agentsfleet · no key needed" subtext is trimmed to what the pill doesn't already say → Test `test_default_row_copy_not_redundant_with_pill`

### §2 — Real Provider dropdown for Other provider — DONE

- **Dimension 2.1** — the generic "Other provider" form's Provider field renders as a dropdown populated from the catalogue's known providers (the same list `ProviderSwitchList`'s `uniqueProviders(models)` already computes), with the Model field's existing dropdown-or-free-text behavior unchanged → Test `test_other_provider_field_is_dropdown`
- **Dimension 2.2** — regression: `CustomEndpointForm` (openai-compatible) is unaffected — its Provider is fixed, not part of this dropdown → Test `test_custom_endpoint_form_unchanged`

---

## Metrics & Observability

Not applicable — no product/operator signal changes; this is a layout/copy consolidation of an existing surface.

---

## Interfaces

Not applicable — no new endpoint, no changed request/response shape. The catalogue fetch (`GET /_um/.../cap.json`) already exists and is already consumed client-side.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Catalogue fetch fails | `cap.json` network error | already degrades silently to free-text Model field (existing behavior, `ModelCatalogueProvider`); the new Provider dropdown degrades the same way — free-text Provider field, unchanged from today |
| Provider not in catalogue | user's provider is real but uncatalogued | free-text fallback remains available (Product Clarity #10) |

---

## Invariants

1. Every server action this page calls (`setProviderSelfManagedAction`, reset-to-platform, rotate) keeps its exact signature — enforced by the unchanged existing action test files.
2. `CustomEndpointForm`'s Provider is never sourced from the new dropdown list — enforced by Dimension 2.2's regression test.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_models_page_renders_one_row_list` | render the Models page → platform-default and every provider render inside one shared list container |
| 1.2 | unit | `test_bring_your_own_key_button_removed` | render → `queryByRole("button", { name: /bring your own key/i })` is null; clicking the default row's action reveals a form in the same render pass |
| 1.3 | unit | `test_default_row_copy_not_redundant_with_pill` | render → heading/subtext text does not duplicate what the "DEFAULT"/"ACTIVE MODEL" pill already states |
| 2.1 | unit | `test_other_provider_field_is_dropdown` | render the generic form → Provider is a listbox/select with the catalogue's known provider names as options |
| 2.2 | unit (regression) | `test_custom_endpoint_form_unchanged` | render `CustomEndpointForm` → Provider field behavior identical to before this spec |

Regression: existing `active-model-hero.test.tsx`/`provider-switch-list.test.tsx`/`hero-change-model-panel.test.tsx` suites still pass once their render targets are updated to the merged shape.

Idempotency/replay: N/A — no retry semantics touched.

---

## Acceptance Criteria

- [x] One shared row list renders both the platform default and every provider — verify: `make test-unit-app` (Dimension 1.1)
- [x] No "Bring your own key" button remains — verify: grep `ui/packages/app/app/(dashboard)/settings/models/` for the literal string
- [x] Other-provider Provider field is a dropdown — verify: `make test-unit-app` (Dimension 2.1)
- [x] `make lint-app` clean · no file over 350 lines added
- [x] `gitleaks detect` clean

---

## Eval Commands (post-implementation)

```bash
# E1: no leftover "Bring your own key" string
grep -rn "Bring your own key" "ui/packages/app/app/(dashboard)/settings/models/" && echo "FAIL" || echo "PASS"
# E2: Build — cd ui/packages/app && bun run build
# E3: Tests — make test-unit-app
# E4: Lint — make lint-app 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — N/A, no Zig touched
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

Mandatory if `ActiveModelHero.tsx` is deleted rather than reduced to an adapter.

| File to delete | Verify |
|----------------|--------|
| `ui/packages/app/app/(dashboard)/settings/models/components/ActiveModelHero.tsx` (iff fully folded, not kept as a thin wrapper) | `test ! -f ui/packages/app/app/(dashboard)/settings/models/components/ActiveModelHero.tsx` |

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `ActiveModelHero` (iff deleted) | `grep -rn "ActiveModelHero" ui/packages/app/ --include="*.tsx" --include="*.ts" \| grep -v "\.test\."` | 0 matches outside the merged component itself |

---

## Discovery (consult log)

- **Design decision:** `ActiveModelHero` folds into `ProviderSwitchList` via `DashboardRow`'s existing (previously unused on this page) `meta` slot rather than a new expand-toggle — every existing hero button/panel assertion still resolves via `getByRole`/`getByText` regardless of DOM nesting. Required extending the shared test stub `tests/helpers/models-component-mocks.tsx` (not in the original Files Changed list) since its `DashboardRow` mock didn't render `meta` or spread arbitrary props — without that, hero tests would've gone dark under the mock.
- **Bug found + fixed during `/review` (RULE DID):** merging the hero gave it an independent expand-state (`panel`) separate from `ProviderSwitchList`'s own `open` state. `ProviderKeyForm` hardcodes its field ids (`provider-key-provider` etc.), which was safe pre-merge (a single shared `open` toggle meant only one instance ever mounted) but became reachable as a real bug post-merge — the hero's own inline add-key form and any other row's add-key form can now be open simultaneously, colliding ids and breaking `htmlFor` association. Fixed with `React.useId()`; proved with a red-green test (`tests/provider-key-form.test.tsx` — reverted the fix, confirmed the new test fails, restored, confirmed it passes).
- **UFS violation found + fixed:** "Add key & model" was a literal repeated across `ProviderSwitchList.tsx` (2 pre-existing sites) and newly in `ActiveModelHero.tsx` (1 site) — `audit-ufs.sh` doesn't catch this (ui/ carve-out is manual duty per `dispatch/write_ts_adhere_bun.md` §2). Extracted `ADD_KEY_AND_MODEL_LABEL` from `ActiveModelHero.tsx`, imported into `ProviderSwitchList.tsx`.
- **Metrics review:** not applicable — no product/operator signal changes.
- **Skill chain outcomes:** `/write-unit-test` audit found and closed one gap (the Provider-dropdown's `onValueChange` reset-model interaction lacked a firing test). `/review` (adversarial diff review vs `docs/greptile-learnings/RULES.md` + `dispatch/write_ts_adhere_bun.md`) found the RULE DID and UFS issues above; both fixed in this diff.
- **Deferrals:** none.

---

## Skill-Driven Review Chain (mandatory)

Standard chain — `/write-unit-test` → `/review` → `/review-pr`, per `AGENTS.md`.

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-app` | 128 files, 1175/1175 (baseline at CHORE(open): 1169) | ✅ |
| Lint | `make lint-app` | Oxlint + tsc clean | ✅ |
| Gitleaks | `gitleaks protect --staged` (pre-commit hook, every commit this workstream) | 0 leaks | ✅ |
| Dead code sweep | N/A — `ActiveModelHero.tsx` kept (reduced to a row component), not deleted | — | N/A |
| 350-line gate | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l \| awk '$1>350'` | no M113_001-touched file over 350 lines (pre-existing over-length files from M108/M112 unrelated to this diff) | ✅ |

---

## Out of Scope

- Error-message friendliness pass on this same page — tracked as M113_002.
- Secrets & ENVs page split — tracked as M113_003.
- Any new backend endpoint or provider-catalogue data change — none needed; existing `/_um/.../cap.json` suffices.
