# M87_001: Unify Models & Credentials into one legible page with edit + crash fix

**Prototype:** v2.0.0
**Milestone:** M87
**Workstream:** 001
**Date:** Jun 07, 2026
**Status:** DONE
**Priority:** P1 — operator-facing dashboard confusion + a live console crash on the Models page.
**Categories:** UI
**Batch:** B1 — standalone UI workstream, no concurrent dependants.
**Branch:** feat/m87-models-credentials-redesign
**Depends on:** none (M86_001 BYOK wizard + cap.json already merged in #373).
**Provenance:** agent-generated (pre-spec, `/Users/kishore/.claude/plans/i-want-a-redesign-transient-pearl.md`, ratified with Indy via plan approval Jun 07, 2026).

> **Provenance is load-bearing.** Plan was reviewed and approved by Indy; layout (Option 3), edit semantics (rotate-default + guarded rename), and crash-fix bundling are ratified decisions, not agent guesses.

**Canonical architecture:** `docs/DESIGN_SYSTEM.md` (Operational Restraint — mono chrome, named tokens, no arbitrary Tailwind). No backend architecture doc applies; this is a UI re-layout over existing, unchanged endpoints.

---

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/settings/models/page.tsx` — the page being repurposed into the unified surface; already fetches provider + credentials + catalogue in one `Promise.all`.
2. `ui/packages/app/app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog.tsx` — the dialog+`Form`(react-hook-form+zod)+reveal pattern to mirror for `EditCredentialDialog`.
3. `ui/packages/app/app/(dashboard)/credentials/components/CredentialsList.tsx` — per-item `useTransition` + `ConfirmDialog` pattern to extend with `[edit]`.
4. `docs/DESIGN_SYSTEM.md` — visual law: mono UI chrome, named design tokens only, borders over shadows, no arbitrary `*-[...]` utilities.
5. `schema/019_model_caps.sql` — read-only context: `core.model_caps` is keyed `(provider, model_id)`; the same `model_id` legitimately recurs across providers. This is *why* the crash exists; do not "fix" the data.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Unify Models & Credentials into one page; add credential edit; fix model-picker crash
- **Intent (one sentence):** An end user can, on one page, choose platform-default vs bring-your-own model, add a provider key without hand-writing JSON, and edit/rotate any stored secret — with no console crash.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + list `ASSUMPTIONS I'M MAKING: …`; mismatch with Intent → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal discipline. Specific IDs this diff trips: **NDC** (no dead code at write time), **NLR** (touch-it-fix-it cleanup on every file edited), **NLG** (pre-v2: no "legacy" framing; removed `/credentials` route is a redirect, not a 410 shim), **ORP** (orphan sweep after the route collapse).
- **`dispatch/write_ts_adhere_bun.md`** — `*.tsx`/`*.ts` discipline: `const`/import hygiene, TS FILE SHAPE at PLAN, UI Component Substitution + DESIGN TOKEN gates.
- **`docs/DESIGN_SYSTEM.md`** — Operational Restraint tokens/typography/motion.
- UFS named-constants applied **by hand** (the UFS audit skips `ui/`): the `"rotate" | "rename"` edit-mode union and any repeated literal extracted `as const`.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | No `*.zig` touched — UI-only. |
| PUB / Struct-Shape | no | No Zig pub surface. |
| File & Function Length (≤350/≤50/≤70) | yes (soft) | New `EditCredentialDialog.tsx` + reworked `InlineProviderKeyCreate.tsx` stay ≤350; split sub-components if a file approaches the cap. |
| UFS (repeated/semantic literals) | yes (manual) | `ui/` is not auto-audited — extract the edit-mode union + repeated copy as `as const` by hand. |
| UI Substitution / DESIGN TOKEN | yes | Every control is a `@usezombie/design-system` primitive; named tokens only, zero arbitrary `*-[...]`. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | No backend/schema/log surface. |
| SECRET HANDLING | yes (judgment) | Vault stays write-only: never read/echo/log a secret value; edit re-enters the value rather than pre-filling it. |

---

## Overview

**Goal (testable):** The Models page renders its model picker with no duplicate React key across a catalogue containing repeated `model_id`s, exposes a structured provider→model→key create flow that POSTs a `{provider,api_key,model}` credential, and offers per-credential rotate (same-name upsert) + guarded rename (create-new + delete-old) — all on one route, with `/credentials` redirecting in.

**Problem:** Operators bounce between `/settings/models` and `/credentials` to do one task (a self-managed model needs a credential); adding any secret means typing a raw JSON blob; stored credentials can't be edited (typo/rotate = delete+recreate); and the Models page currently throws `Encountered two children with the same key, "claude-opus-4-8"` and renders a broken Select.

**Solution summary:** Collapse the two routes into one "Models & Credentials" page — a `§ MODEL` section (platform vs bring-your-own + provider-scoped picker) above a `§ SECRETS` section (list + add + edit). The bring-your-own create flow becomes a structured form (Provider Select + provider-scoped Model Select + API-key paste with auto-detect), creating the credential behind the scenes. Edit ships as rotate-default with rename behind an Advanced ⚠ warning. The crash is fixed client-side by deduping the override picker by `model_id` and provider-scoping the inline create. Zero backend/schema/Zig change — the upsert (`crypto_store.zig:44-47`) and provider-from-credential resolution already exist.

---

## Prior-Art / Reference Implementations

- **UI dialog/form** → `CreateApiKeyDialog.tsx` (Dialog + react-hook-form + zod + reveal) — `EditCredentialDialog` mirrors it; divergence: two modes (rotate/rename) instead of a one-shot reveal.
- **UI destructive per-item action** → `CredentialsList.tsx` (`useTransition` + `ConfirmDialog`) — `[edit]` wires in beside the existing `[delete]`.
- **Catalogue/provider model** → `lib/api/model_caps.ts` + `schema/019_model_caps.sql` — `(provider, model_id)` identity is the contract the picker must respect.
- **Tokens/typography** → design-system primitives + `docs/DESIGN_SYSTEM.md`.

No new architecture; this mirrors existing app patterns.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/app/(dashboard)/settings/models/page.tsx` | EDIT | Becomes the unified page: §MODEL (provider selector) above §SECRETS (list + add). |
| `ui/packages/app/app/(dashboard)/settings/models/components/Step2Model.tsx` | EDIT | Dedupe override picker by `model_id` (crash fix). |
| `ui/packages/app/app/(dashboard)/settings/models/components/InlineProviderKeyCreate.tsx` | EDIT | Explicit Provider Select; provider-scoped Model Select; structured create. |
| `ui/packages/app/app/(dashboard)/settings/models/components/Step1Credential.tsx` | EDIT | `/credentials` link → in-page `§ SECRETS` anchor. |
| `ui/packages/app/app/(dashboard)/credentials/components/CredentialsList.tsx` | EDIT | Add `[edit]` button + dialog wiring. |
| `ui/packages/app/app/(dashboard)/credentials/components/EditCredentialDialog.tsx` | CREATE | Rotate (locked name, upsert) + Advanced rename (create-new + delete-old, ⚠). |
| `ui/packages/app/app/(dashboard)/credentials/page.tsx` | EDIT | Body → `redirect("/settings/models")`. |
| `ui/packages/app/app/(dashboard)/credentials/loading.tsx` | DELETE | Orphaned once the page is a redirect (NDC/ORP). |
| `ui/packages/app/components/layout/Shell.tsx` | EDIT | Collapse two `CONFIGURATION_NAV` entries into one. |
| `ui/packages/app/app/(dashboard)/settings/models/components/__tests__/*` · `credentials/components/__tests__/*` | CREATE | Vitest coverage per Test Specification. |

> Final orphan set (e.g. whether `AddCredentialForm`/`CredentialsList` physically move vs are imported in place) confirmed during EXECUTE; any file left unreferenced is deleted, not stranded.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, five Sections — page/nav unification, crash fix, structured create, edit, dead-code sweep. They share one diff because they touch the same four components and one page; splitting would thrash the same files across PRs.
- **Alternatives considered:** (a) ship the crash fix as a standalone hotfix PR first — rejected by Indy ("bundle into redesign") since Step2Model is reworked here anyway; (b) Option 1 (Models fully owns LLM keys, Credentials = generic only) and Option 2 (two pages clarified) — rejected in favour of Option 3 (unified single page) per Indy; (c) add a `provider`/`kind` column to the credentials list response to provider-scope the override picker from data — rejected as backend scope creep; client-side dedupe suffices.
- **Patch-vs-refactor verdict:** **refactor** (of the UI layer only) — the confusion is structural (two routes for one task), so a re-layout is right; but it stays UI-only, no backend refactor. No follow-up backend spec needed unless a future "credential kind" surfaces.

---

## Sections (implementation slices)

### §1 — Unified page & navigation

One route owns the whole task: `§ MODEL` (active config + platform/bring-your-own selector) sits above `§ SECRETS` (credential list + add). The standalone `/credentials` route redirects in; the sidebar shows one Configuration entry. **Implementation default:** unified page lives at `/settings/models` (keeps the settings grouping); `/credentials` → `redirect()` (pre-launch, avoids bookmark 404s; not a compat shim).

- **Dimension 1.1** — `/credentials` redirects to the unified route → Test `test_credentials_route_redirects`.
- **Dimension 1.2** — sidebar renders exactly one combined Configuration entry (no separate Credentials link) → Test `test_nav_collapses_models_credentials`.
- **Dimension 1.3** — unified page renders both a Model section and a Secrets section → Test `test_unified_page_has_both_sections`.

### §2 — Crash fix: provider-aware model picker

The override picker (existing credential, provider unknown to UI) dedupes the catalogue by `model_id` so each renders once; `key`/`value` stay the bare `model_id`. The inline-create picker instead filters the catalogue to the chosen provider (unique by the `(provider, model_id)` PK, with that provider's true caps).

- **Dimension 2.1** — a catalogue with duplicate `model_id` (anthropic + pioneer `claude-opus-4-8`) renders each id once, no duplicate-key warning → Test `test_model_picker_dedupes_duplicate_ids`.
- **Dimension 2.2** — inline-create model options are scoped to the selected provider → Test `test_inline_model_options_scoped_to_provider`.

### §3 — Structured bring-your-own-key flow

Adding a provider key never requires raw JSON. An explicit Provider `Select` (paste auto-detect via `detect-provider.ts` pre-selects but is overridable), a provider-scoped Model `Select`, and an API-key field create the credential as `{provider,api_key,model}`.

- **Dimension 3.1** — submitting the structured form calls `createCredentialAction` with `{provider,api_key,model}` data → Test `test_structured_create_posts_provider_key`.
- **Dimension 3.2** — pasting a recognised key prefix pre-selects the provider; user can override it → Test `test_key_paste_autodetects_and_is_overridable`.

### §4 — Credential edit (rotate default + guarded rename)

`[edit]` opens a dialog. **Rotate** (default): name locked, re-enter value → same-name `createCredentialAction` (upsert). **Advanced → Rename**: a ⚠ Alert warns the rename breaks `${secrets.<old>...}` refs; on confirm, create under the new name then delete the old.

- **Dimension 4.1** — rotate calls `createCredential` with the unchanged name → Test `test_rotate_upserts_same_name`.
- **Dimension 4.2** — rename calls create(new) then delete(old), in that order, and shows the warning → Test `test_rename_creates_then_deletes_with_warning`.
- **Dimension 4.3** — a failed create/delete surfaces via `presentErrorString` and does not refresh → Test `test_edit_surfaces_api_error`.

### §5 — Dead-code & orphan sweep

Every file edited gets NLR cleanup (dead imports/decls stripped); every file the route collapse strands (e.g. `credentials/loading.tsx`, any retired component) is deleted from disk + git, with a zero-reference grep.

- **Dimension 5.1** — no unreferenced credentials-page artifact remains; grep for retired symbols is empty → Test (eval) `E8` orphan sweep clean.

---

## Interfaces

> Backend contract is **unchanged** — locked here so the agent does not alter it.

```
POST   /v1/workspaces/{workspaceId}/credentials   { name, data: {provider, api_key, model} } → { name }   (upsert)
DELETE /v1/workspaces/{workspaceId}/credentials/{name} → 204
PUT    /v1/tenants/me/provider   { mode: "self_managed", credential_ref, model? } → TenantProvider
DELETE /v1/tenants/me/provider → TenantProvider (platform default)
GET    /_um/<key>/cap.json → { models: ModelCap[ {id, provider, context_cap_tokens, ...rates} ], rates, billing }
```

Client edit-mode contract (new, internal): `type EditMode = "rotate" | "rename"` (`as const`). Rotate ⇒ one `createCredential(sameName)`; Rename ⇒ `createCredential(newName)` then `deleteCredential(oldName)`.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Duplicate model_id | catalogue has repeated id across providers | Picker dedupes/scopes; one option per id; no React key collision. |
| Catalogue fetch fails | `cap.json` non-2xx | Model field degrades to free-text input (existing behaviour preserved). |
| Rotate API error | POST upsert fails | `presentErrorString` Alert in dialog; no `router.refresh`; dialog stays open. |
| Rename partial failure | create(new) succeeds, delete(old) fails | Surface the error; old name persists (no silent orphan); user retries delete. Documented in dialog copy. |
| Rename breaks refs | user renames a referenced secret | Loud ⚠ Alert before confirm; rename is opt-in behind Advanced. |
| Malformed structured input | empty provider/key | Submit disabled until required fields valid (zod). |

---

## Invariants

1. No secret value is ever read back, echoed, or logged — edit re-enters values; vault stays write-only. Enforced by: there is no GET-plaintext call in the diff (grep-checkable) and the edit form's value field defaults empty.
2. Every `<SelectItem>` in a model picker has a unique `key`/`value` within its render. Enforced by: dedupe-by-`model_id` / provider-scope in code + a regression test asserting no duplicate-key warning.
3. Rename is never silent: a rename path always renders the ⚠ warning before mutating. Enforced by: the rename branch is gated behind the Advanced disclosure whose confirm renders the Alert; covered by `test_rename_creates_then_deletes_with_warning`.
4. No arbitrary Tailwind (`*-[...]`); design-system primitives only. Enforced by: DESIGN TOKEN + UI GATE on `*.tsx`.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_credentials_route_redirects` | rendering `/credentials` page module issues `redirect("/settings/models")`. |
| 1.2 | unit | `test_nav_collapses_models_credentials` | `Shell` nav has one combined Configuration entry; no standalone "Credentials" link. |
| 1.3 | unit | `test_unified_page_has_both_sections` | unified page output contains a Model section and a Secrets section. |
| 2.1 | unit | `test_model_picker_dedupes_duplicate_ids` | catalogue `[opus@anthropic, opus@pioneer, sonnet@anthropic]` → `opus` rendered once; no duplicate-key console error. |
| 2.2 | unit | `test_inline_model_options_scoped_to_provider` | provider=anthropic → only anthropic models listed. |
| 3.1 | unit | `test_structured_create_posts_provider_key` | filled form → `createCredentialAction` called with `data={provider,api_key,model}`. |
| 3.2 | unit | `test_key_paste_autodetects_and_is_overridable` | paste `sk-ant-…` → provider preset anthropic; changing the Select overrides it. |
| 4.1 | unit | `test_rotate_upserts_same_name` | rotate submit → `createCredential` called with original name, new data. |
| 4.2 | unit | `test_rename_creates_then_deletes_with_warning` | rename submit → create(new) then delete(old) in order; ⚠ Alert present. |
| 4.3 | unit | `test_edit_surfaces_api_error` | action returns `{ok:false}` → error string shown, no refresh. |
| 5.1 | eval | `E8 orphan sweep` | grep for retired credentials-page symbols → 0 matches. |

- **Regression:** the catalogue-absent free-text fallback (Step2Model) and existing delete-with-confirm must still pass — assert both.
- **Idempotency:** rotate (same-name upsert) is idempotent — re-running with identical data is a no-op observable as one stored row.
- Feed both branches of provider-detected vs manual-override and rotate-vs-rename (branch coverage > line coverage). Gate on aggregate coverage (bun erases type-only lines).

---

## Acceptance Criteria

- [x] `/settings/models` renders with **no console error** across a duplicate-`model_id` catalogue — Vitest `dedupes a catalogue with the same model_id across providers` passes (asserts no "same key" warning).
- [x] Structured create stores `{provider,api_key,model}` — `submits {provider, api_key, model}` + `lists only the selected provider's models` pass.
- [x] Rotate keeps the name; rename warns + create-then-delete — `rotate: upserts under the same name and never deletes`, `rename: warns, then creates the new name before deleting the old` pass.
- [x] `/credentials` redirects to the unified page — `credentials route redirects into the unified Models & Credentials page` passes.
- [x] Lint clean (UI + DESIGN TOKEN gates) · app `bun run build` succeeds · `tsc` clean.
- [x] Orphan sweep empty · no file over 350 lines added · gitleaks runs in pre-commit.

All Sections §1–§5 implemented; Dimensions 1.1–5.1 each have a passing test (see Verification Evidence).

---

## Eval Commands (post-implementation)

```bash
# E1: Unit tests (app workspace)
cd ui/packages/app && bun run test 2>&1 | tail -20 && echo "PASS" || echo "FAIL"
# E2: Build
cd ui/packages/app && bun run build 2>&1 | tail -5
# E3: Lint (UI GATE / DESIGN TOKEN GATE)
make lint 2>&1 | grep -E "✓|FAIL|app"
# E4: Gitleaks
gitleaks detect 2>&1 | tail -3
# E5: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E6: Orphan sweep (empty = pass)
grep -rn "credentials/loading" ui/packages/app | head
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `ui/packages/app/app/(dashboard)/credentials/loading.tsx` | `test ! -f "ui/packages/app/app/(dashboard)/credentials/loading.tsx"` |

> Additional orphans (any component left unreferenced after the route collapse) confirmed + deleted during EXECUTE; this table updated in the same commit.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `/credentials` link (Step1Credential) | `grep -rn '"/credentials"' ui/packages/app` | 0 (replaced by anchor) |
| retired loading skeleton | `grep -rn "credentials/loading" ui/packages/app` | 0 matches |

---

## Discovery (consult log)

> **Empty at creation.** Append as work surfaces consults, skill outcomes, and Indy-acked deferral quotes.

- **Ratified decisions (plan approval, Jun 07, 2026):** Option 3 unified layout; rotate-default + guarded rename; crash fix bundled into this PR; UI-only (no backend/schema). `/credentials` → redirect chosen over hard-remove to avoid bookmark 404s.
- **Scoping call — Provider stays a free-text Input (not a Select):** the inline create form keeps Provider as an auto-filled text field rather than a strict dropdown, because the existing suite proves a custom-proxy provider capability ("my-proxy") that a closed Select would drop. The end-user win lands on the **Model** field, which is now a provider-scoped picker (can't typo a model_id). Provider is still auto-filled from the pasted key for known prefixes.
- **Model-on-provider-change semantics:** switching providers now re-defaults the model to the new provider's first catalogue entry (a model is provider-specific). This replaced the prior "preserve user-typed model across provider change" behaviour, which is incoherent with a scoped picker; the affected test was updated to assert the new, more-correct behaviour.
- **Regression fan-out (route collapse):** the `/credentials` → redirect rippled to 6 pre-existing tests (loading-states, app-components nav, dashboard-placeholder). All updated to the new behaviour (one Configuration nav entry; redirect assertion; list/add coverage relocated to the models-page + credentials-component tests). The `credentials-lifecycle` Playwright spec now asserts the redirect lands on `/settings/models`.
- **Skill chain outcomes:** `/review` (code-review high, 7 finder angles) — dispositioned: **1 CONFIRMED bug fixed** (rename partial-failure now `router.refresh()`es so the created name shows + a recovery message, instead of returning with a stale list); **1 UX fix** (`applyProvider` keeps a still-valid model across provider re-edits instead of always clobbering); **1 Reuse fix** (extracted shared `credentials/lib/credential-data.ts` — `parseCredentialDataObject` + `CREDENTIAL_NAME_MAX` + `jsonParseErrorMessage`, consumed by both AddCredentialForm and EditCredentialDialog); **1 Altitude fix** (`uniqueModelIds`/`modelsForProvider` helpers in `model_caps.ts` centralise the `(provider, model_id)` keying). REFUTED: Radix value-desync (`applyProvider` keeps `model ∈ providerModels` by construction), rotate NAME_MAX asymmetry (backend caps names ≤64 at create), `getTenantProvider`→em-dash (pre-existing, unchanged by this diff). `/review-pr` + `kishore-babysit-prs` — {pending, post-push}.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification. | Clean; iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs spec, DESIGN_SYSTEM, Failure Modes, Invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` then `kishore-babysit-prs` | PR-comment the open diff; poll greptile to green. | Comments addressed; babysit final report in Discovery. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `cd ui/packages/app && bun run test` | `Test Files 89 passed (89) · Tests 831 passed (831)` | ✅ |
| Typecheck | `cd ui/packages/app && bun run typecheck` | `tsc --noEmit` — clean | ✅ |
| Build | `cd ui/packages/app && bun run build` | `next build` — succeeded, all routes compiled | ✅ |
| Lint | `cd ui/packages/app && bun run lint` | `oxlint --type-aware .` — exit 0 | ✅ |
| Orphan sweep | `git grep "credentials/loading" -- ui/packages/app` | 0 matches (file deleted) | ✅ |
| Stale route links | `git grep '"/credentials"' -- ui/packages/app/{app,components,lib}` | 0 (only test `not.toContain` + redirect target) | ✅ |

---

## Out of Scope

- Any `core.model_caps` / `vault.secrets` schema change, Zig handler change, or new endpoint.
- A `provider`/`kind` column on the credentials list response (would be its own backend spec if a future "credential type" UI needs server-side scoping).
- Generic-secret structured forms — the raw-JSON "Add secret" form stays for non-LLM service credentials (fly/stripe).
