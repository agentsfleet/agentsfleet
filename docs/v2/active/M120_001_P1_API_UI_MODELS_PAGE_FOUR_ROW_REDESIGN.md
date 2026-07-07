<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M120_001: Tenant Models page collapses to 4 fixed rows; Default is read-only, stale keys are deletable, Switch never dead-ends

**Prototype:** v2.0.0
**Milestone:** M120
**Workstream:** 001
**Date:** Jul 07, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — a tenant-facing dead-end error and a no-delete stale-key gap were reported directly against the shipping page.
**Categories:** API, UI
**Batch:** B1 — independent of M120_002 (icon sweep); different file trees.
**Branch:** feat/m120-models-page-four-row-redesign
**Test Baseline:** unit=2377 integration=255
**Depends on:** none (M100_001, M113_001 are DONE prior art, not blocking)
**Provenance:** human-directed, LLM-drafted (Sonnet 5, Jul 07, 2026) — reconciled from a live bug-report + code-archaeology session with Indy (6 reported "funky" behaviors, root-caused against the running code) plus a follow-up Q&A locking the target row design.

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §8 (self-managed credential model, provider resolution) — this spec changes presentation + adds one derived read field; the credential/resolve data model is unchanged.

## Overview

**Goal (testable):** the tenant Models page renders exactly 4 fixed rows (Default, Anthropic, Other provider, Custom — OpenAI-compatible), the LIVE pill renders on whichever row is active with no separate hero card, the Default row never exposes an edit action, "Switch to Default" never dead-ends on an unconfigured platform default, a stale stored key can be deleted, and the Model field offers a static known-model list before degrading to free text.
**Problem:** today's row list grows unpredictably (one row per typed-in provider), a separate "hero" card duplicates the LIVE/DEFAULT pill's own row, the Default row shows edit actions that don't apply to it, clicking "Switch" to the platform default can round-trip into a raw, unexplained error, no stored key can ever be removed from this page, and an uncatalogued provider (e.g. Fireworks) forces fully hand-typed Provider and Model fields.
**Solution summary:** collapse the row list to 4 fixed rows with the LIVE pill living on the active row itself; strip all edit affordances from the Default row; give the "Other provider" row a picker across the tenant's stored non-Anthropic keys; surface a new `platform_default_available` boolean so Switch can disable itself with an explanation instead of dead-ending; add a delete/forget icon action reusing the existing secret-delete flow; and add a small static per-provider model-name list as a client-side autocomplete fallback ahead of free text.

## PR Intent & comprehension handshake

- **PR title (eventual):** Models page: 4 fixed rows, read-only Default, no dead-end Switch, deletable stale keys
- **Intent (one sentence):** the tenant Models page always shows a small, predictable set of rows where the Default is admin-managed and read-only, switching to it never dead-ends, and stale keys can be removed.
- **Handshake:** implementing agent restates the intent + lists `ASSUMPTIONS I'M MAKING: …` at PLAN, before EXECUTE; a mismatch against the Intent above STOPs for reconciliation.

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderSwitchList.tsx` — the row list to collapse from a dynamic per-provider loop to exactly 4 fixed rows.
2. `.../models/components/ActiveModelRow.tsx` — the hero's rotate/reset/change-model logic that must survive folding into the active row; also today's source of the separate LIVE/DEFAULT pill this spec relocates.
3. `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.tsx` — the icon Edit/Delete pattern (`PencilIcon`/`Trash2Icon`, `aria-label`, `ConfirmDialog`) and the `protectedFromDelete` guard (blocks deleting a secret "in model setup") — mirror both exactly for this spec's delete action and its active-secret guard.
4. `src/agentsfleetd/http/handlers/tenant_provider.zig` (`readProviderView`) — where the new `platform_default_available` field slots into the existing GET response; `tenant_provider.platformDefaultView()` already returns `null` when no active `platform_llm_keys` row exists.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/.../settings/models/components/ProviderSwitchList.tsx` | EDIT | collapse to exactly 4 fixed rows (Default, Anthropic, Other provider, Custom — OpenAI-compatible); LIVE pill renders on whichever row is active; remove the dynamic namedProviders loop |
| `ui/.../settings/models/components/ActiveModelRow.tsx` | EDIT | fold into per-row rendering, not a separate hero; the Default row keeps no edit action (Change model/Replace key never apply to it) |
| `ui/.../settings/models/components/ProviderKeyForm.tsx` | EDIT | add the delete/forget action for a stored key; the Other-provider row gains a picker across the tenant's stored non-Anthropic provider-key secrets plus "add another" |
| `ui/.../settings/models/components/ProviderModelSelect.tsx` | EDIT | fall back to the new static known-model list before degrading to free text |
| `ui/.../settings/models/lib/known-models.ts` | CREATE | static, client-only per-provider model-name list for autocomplete — decoupled from the priced, admin-managed `core.model_library` catalogue |
| `ui/.../settings/models/page.tsx` | EDIT | composition update for the collapsed row list |
| `src/agentsfleetd/http/handlers/tenant_provider.zig` | EDIT | `readProviderView` computes `platform_default_available` unconditionally (not only in platform mode) |
| `ui/packages/app/lib/types.ts` | EDIT | `TenantProvider` gains `platform_default_available: boolean` |
| `ui/packages/app/lib/api/tenant_provider.ts` | EDIT | thread the new field through the client read |
| `ui/.../tests/{provider-switch-list,active-model-hero,provider-key-form}.test.tsx` | EDIT | assertions move to the collapsed 4-row shape + new Dimensions |
| `src/agentsfleetd/state/tenant_provider_test.zig` + a handler test | EDIT | cover `platform_default_available` in both tenant modes |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (reuse `ADD_KEY_AND_MODEL_LABEL`-style named constants rather than new duplicated literals for the picker/delete copy); NDC/NLR (no dead code left from folding `ActiveModelRow`).
- **`dispatch/write_ts_adhere_bun.md`** — every UI file touched is `.tsx`.
- **`dispatch/write_zig.md`** — `tenant_provider.zig` handler edit (cross-compile both linux targets; no pg-drain change, the read path already exists).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile both linux targets after the `readProviderView` addition |
| PUB / Struct-Shape | yes | `ProviderView` (Zig) and `TenantProvider` (TS) both gain a field — shape verdict at PLAN |
| File & Function Length (≤350/≤50/≤70) | possible | split `ProviderSwitchList.tsx` per-row if it nears the cap (mirror `connector-rows.tsx`'s existing split) |
| UFS | yes | named constants for the picker/delete copy, not new duplicated literals |
| UI Substitution / DESIGN TOKEN | yes | mirror `SecretsList.tsx`'s icon-button + `ConfirmDialog` primitives; no bespoke markup |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no new error code; no schema change — `platform_default_available` is derived, not persisted |

## Prior-Art / Reference Implementations

- **UI** → `ui/.../secrets/components/SecretsList.tsx` — icon Edit/Delete + `protectedFromDelete` guard + `ConfirmDialog`; the exact shape this spec's delete action and active-secret guard mirror.
- **UI** → `ui/.../integrations/components/connector-rows.tsx` (M108) — the single catalog-driven-row-per-slot pattern M113 already converged the Models page onto; this spec tightens it to a fixed 4 slots rather than a dynamic N.
- **API** → `src/agentsfleetd/state/tenant_provider.zig`'s `platformDefaultView()` — already returns `null`/a populated view; this spec only threads that boolean into the GET response unconditionally.

## Sections (implementation slices)

### §1 — Design-shotgun pass informs the 4-row visual design

Before the row-list component is rebuilt, run `/design-shotgun` scoped to this page: generate multiple visual variants for the Default/Anthropic/Other-provider/Custom-endpoint row layout, open the comparison board, and record Indy's picks + reconciled decisions in Discovery. The row *semantics* below (§2–§6) are fixed; the *visual* arrangement is not — this step settles it before component work starts, not from chat description alone.

- **Dimension 1.1** — a design-shotgun run exists for this layout and Indy's picks are recorded in Discovery before §2 begins → Acceptance (Discovery record present, not a unit test)

### §2 — Row list collapses to 4 fixed rows; LIVE lives on the active row

**Implementation default:** the 4 rows are Default, Anthropic, Other provider, Custom — OpenAI-compatible, in that order, always rendered regardless of catalogue size or how many provider-key secrets exist. Whichever row is currently active renders the LIVE pill directly on itself — no separate hero card above the list.

- **Dimension 2.1** — the page always renders exactly 4 rows → Test `test_models_page_renders_exactly_four_rows`
- **Dimension 2.2** — the LIVE pill renders on whichever row is active (Default, Anthropic, Other-provider, or Custom), never on a separate hero → Test `test_live_pill_renders_on_active_row_not_a_separate_hero`
- **Dimension 2.3** — the Default row never renders "Change model" or "Replace key" in any state → Test `test_default_row_has_no_edit_actions`

### §3 — Other-provider row: first-class labeling + multi-secret picker

**Implementation default:** whatever non-Anthropic provider is currently active in that slot is named prominently in the row's own label/description (e.g. "Other provider — OpenAI"), not left as generic text. Expanding the row when more than one non-Anthropic provider-key secret is stored reveals a picker across all of them plus an "add another" affordance — no stored secret is silently hidden, replaced, or deleted by adding a new one.

- **Dimension 3.1** — the active non-Anthropic provider's name appears in the Other-provider row's own label/description → Test `test_other_provider_row_names_active_provider`
- **Dimension 3.2** — with more than one stored non-Anthropic provider-key secret, expanding the row shows a picker across all of them; switching activates the chosen one without deleting the others → Test `test_other_provider_row_offers_picker_across_stored_secrets`

### §4 — Platform-default Switch never dead-ends

**Implementation default:** `readProviderView` computes `platform_default_available` from `tenant_provider.platformDefaultView(alloc, conn) != null`, unconditionally — today it only surfaces provider/model when the tenant's own mode is already `platform`; this spec makes the boolean visible regardless of the tenant's current mode, so the Default row can gate its own Switch action before the click, not after a failed round-trip.

- **Dimension 4.1** — `platform_default_available` reflects whether an active `platform_llm_keys` row exists, independent of the tenant's own current mode → Test `test_platform_default_available_reflects_active_row_unconditionally`
- **Dimension 4.2** — when `platform_default_available` is false, the Default row's Switch action is disabled with inline copy explaining why, instead of a clickable action that errors after the round-trip → Test `test_switch_disabled_when_platform_default_unavailable`

### §5 — Delete/forget a stored key

**Implementation default:** the Anthropic and Other-provider rows expose a delete/forget icon action (reusing the existing `deleteSecretAction`) whenever a key is stored, mirroring `SecretsList.tsx`'s icon + `ConfirmDialog` pattern exactly.

- **Dimension 5.1** — a stored, non-active provider-key secret can be deleted from its Models-page row via the icon action → Test `test_stored_key_row_offers_delete_action`
- **Dimension 5.2** — deleting the secret that is currently the tenant's active `secret_ref` is blocked (mirrors `SecretsList.tsx`'s existing `protectedFromDelete` guard) → Test `test_cannot_delete_the_active_secret`

### §6 — Static known-model list for autocomplete

**Implementation default:** `known-models.ts` is a small, static, client-only `Record<provider, string[]>` of common current model names per well-known provider — explicitly not a new backend table, not priced, not yet tenant-editable. `ProviderModelSelect` checks the admin catalogue first, then this static list, then free text.

- **Dimension 6.1** — a provider with zero admin-catalogued models but a static-list entry offers those names in the Model select, instead of degrading straight to free text → Test `test_model_select_falls_back_to_static_list_before_free_text`
- **Dimension 6.2** — a provider absent from both the catalogue and the static list still degrades to free text (regression, unchanged) → Test `test_model_select_free_text_when_uncatalogued_and_unlisted`

## Interfaces

```
GET /v1/tenants/me/provider  (existing — gains one field)
  → { mode, provider, model, context_cap_tokens, secret_ref, platform_default_available }
  platform_default_available: true iff an active core.platform_llm_keys row exists —
  computed via tenant_provider.platformDefaultView() != null, regardless of `mode`.
  No new endpoint; no request-shape change.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|----------------------------------------------------------|
| Platform default absent | no active `core.platform_llm_keys` row | Default row's Switch is disabled with inline copy; no round-trip error |
| Delete targets the active secret | user targets the secret currently powering the LIVE row | action disabled, same guard as the Secrets page's `protectedFromDelete` |
| Static list has no entry | provider/model neither catalogued nor in the static list | free-text fallback (unchanged existing behavior) |
| Multiple stored other-provider secrets | tenant configured more than one non-Anthropic key over time | picker surfaces all; none silently hidden or deleted |

## Invariants

1. The Default row never renders an edit action (Change model/Replace key) — enforced by component logic + Dimension 2.3.
2. `platform_default_available` always reflects the platform key's actual state, independent of the tenant's own mode — enforced by the handler computing it unconditionally + Dimension 4.1.
3. A provider-key secret currently referenced as the tenant's active `secret_ref` cannot be deleted from the Models page — enforced by the same disabled-state guard as the Secrets page + Dimension 5.2.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| n/a | — | — | — | — | — |

Not applicable — no new event. The delete action reuses the existing `deleteSecretAction`, which itself emits no analytics event today (parity with the Secrets page); the existing `provider_reset`/`model_added`/`model_changed`/`key_rotated` events are unchanged by this spec.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 2.1 | unit | `test_models_page_renders_exactly_four_rows` | render with N catalogue providers + M stored secrets → exactly 4 rows in the DOM |
| 2.2 | unit | `test_live_pill_renders_on_active_row_not_a_separate_hero` | tenant active on each of the 4 slots in turn → LIVE pill on that row only, no separate hero element |
| 2.3 | unit | `test_default_row_has_no_edit_actions` | Default active or not → `queryByRole("button", {name: /change model|replace key/i})` within the Default row is null |
| 3.1 | unit | `test_other_provider_row_names_active_provider` | active secret provider="openai" → row text includes "OpenAI" |
| 3.2 | unit | `test_other_provider_row_offers_picker_across_stored_secrets` | 2 stored non-Anthropic secrets → expand shows both + "add another"; switching keeps both stored |
| 4.1 | integration | `test_platform_default_available_reflects_active_row_unconditionally` | tenant in self_managed mode, no active platform key → GET returns `platform_default_available: false`; an active row → `true` |
| 4.2 | unit | `test_switch_disabled_when_platform_default_unavailable` | `platform_default_available: false` → Default row's Switch is disabled with explanatory copy, no click-through error |
| 5.1 | unit | `test_stored_key_row_offers_delete_action` | stored, non-active secret → delete icon present and calls `deleteSecretAction` on confirm |
| 5.2 | unit | `test_cannot_delete_the_active_secret` | secret matches current `secret_ref` → delete action disabled |
| 6.1 | unit | `test_model_select_falls_back_to_static_list_before_free_text` | provider uncatalogued but in `known-models.ts` → Select renders those options |
| 6.2 | unit (regression) | `test_model_select_free_text_when_uncatalogued_and_unlisted` | provider in neither source → free-text input (unchanged) |

Regression: existing `provider-switch-list.test.tsx`/`active-model-hero.test.tsx` suites pass once render targets move to the collapsed 4-row shape. Idempotency/replay: N/A — no retry semantics touched.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Page always renders exactly 4 rows (§2) | `make test-unit-app` | Dimension 2.1 passes | P0 | |
| R2 | Default row is read-only (§2) | `make test-unit-app` | Dimension 2.3 passes | P0 | |
| R3 | Other-provider picker surfaces all stored secrets (§3) | `make test-unit-app` | Dimension 3.2 passes | P1 | |
| R4 | Switch never dead-ends on an unconfigured platform default (§4) | `make test-integration` | Dimensions 4.1–4.2 pass | P0 | |
| R5 | Stale/non-active keys are deletable; the active one is not (§5) | `make test-unit-app` | Dimensions 5.1–5.2 pass | P0 | |
| R6 | Static list backs autocomplete before free text (§6) | `make test-unit-app` | Dimensions 6.1–6.2 pass | P1 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes (handler touched) | `make test-integration` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**Orphaned references — the folded hero, if fully removed rather than reduced to a row adapter.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `ActiveModelRow` (iff deleted, not kept as a per-row adapter) | `grep -rn "ActiveModelRow" ui/packages/app/ --include="*.tsx" \| grep -v "\.test\."` | 0 matches outside the merged component, or the component still exists as the Default/active-row renderer |

No files deleted otherwise — `known-models.ts` is new, everything else is edited in place.

## Out of Scope

- Any change to the priced, admin-managed `core.model_library` catalogue's data model or `/admin/models` (tracked separately — M120_002 touches its presentation only).
- Per-tenant override of the platform default; CLI surface for provider switching.
- Tenant-editable/savable model library (the static list in §6 is autocomplete-only, not persisted).

---

## Product Clarity (authoring record)

1. **Successful user moment** — a tenant opens Settings → Models, sees exactly 4 rows; whichever is active shows LIVE directly on that row; clicking Switch on Default either works or explains inline why it can't; a stale test key (e.g. an old Anthropic key) can be deleted with one click.
2. **Preserved user behaviour** — `setProviderSelfManagedAction`, `resetProviderAction`, rotate, and every existing test's assertions on those actions' *behavior* are unchanged; only row composition, labeling, and the two fixed gaps change.
3. **Optimal-way check** — collapsing an unbounded, dynamically-typed provider list to 4 predictable rows is the most direct fix for "the list grows every time someone types a new provider name"; §1's design-shotgun pass validates the exact visual arrangement rather than locking it from chat description.
4. **Rebuild-vs-iterate** — iterate. The credential/activate/rotate data flow (M87/M100/M113) is correct; only row composition, two real gaps (Items 5/6), and the autocomplete source change.
5. **What we build** — the 4-row list; a read-only Default row; the Other-provider picker; `platform_default_available`; the delete-secret action; the static known-model list.
6. **What we do NOT build** — no change to the priced admin catalogue's data model; no per-tenant default override; no CLI surface.
7. **Fit with existing features** — must not destabilize M100's platform-default resolve chain or M113's row-list foundation; compounds directly with both.
8. **Surface order** — UI-first, with one small additive API (Zig) field backing §4.
9. **Dashboard restraint** — no new provider-health indicator beyond the existing LIVE/DEFAULT pill; the delete action only appears once a key is actually stored.
10. **Confused-user next step** — Switch disabled on Default → inline copy names the reason instead of a dead-end error; free-text model entry remains the ultimate fallback when even the static list doesn't cover a provider.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, six independently-testable Sections (design pass, row collapse, other-provider picker, platform-default fix, delete action, static list) — they share the same small file set, so reviewing together keeps the "4 rows" mental model coherent in one PR.
- **Alternatives considered:** splitting Items 5/6 (the two bug fixes) into their own workstream — rejected; both are small, share files with the row-collapse work, and splitting would only add PR overhead for no review-clarity gain.
- **Patch-vs-refactor verdict:** **patch** — no data-model change; presentation consolidation plus one additive derived boolean field.

## Discovery (consult log)

- **Consults** — empty at creation.
- **Metrics review** — no analytics/funnel playbook update required (no new event; see Metrics & Observability).
- **Skill-chain outcomes** — empty at creation.
- **Deferrals** — empty at creation.
