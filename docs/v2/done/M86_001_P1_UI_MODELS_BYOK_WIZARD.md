# M86_001: Models page sets up a self-managed provider end-to-end on one screen

**Prototype:** v2.0.0
**Milestone:** M86
**Workstream:** 001
**Date:** Jun 06, 2026
**Status:** DONE
**Priority:** P1 — operator-facing: the bring-your-own-key path is the first thing a paying tenant configures, and today it dead-ends.
**Categories:** UI
**Batch:** B1 — standalone UI slice; no concurrent workstream.
**Branch:** feat/dashboard-launch-polish
**Depends on:** M48_001 (self-managed provider — the action/data model this rides), M45_001 (structured vault credentials — the credential shape), M86_002 (the public `cap.json` catalogue this wizard server-fetches — same PR/branch, build first)
**Provenance:** agent-generated (pre-spec) from `/design-shotgun` variant A + `/plan-ceo-review`; LLM-drafted (claude-opus-4-8, Jun 06, 2026)

> **Provenance is load-bearing.** LLM-drafted — cross-check every claim against the named files before EXECUTE.

**Canonical architecture:** `docs/architecture/user_flow.md` (dashboard provider-setup flow) + the `/design-shotgun` reference mockup `~/.gstack/projects/usezombie-usezombie/designs/models-byok-20260606/board.html` (variant A) and CEO plan `~/.gstack/projects/usezombie-usezombie/ceo-plans/2026-06-06-models-byok-activation.md`.

---

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/settings/models/components/ProviderSelector.tsx` — the current mode-radio + form-action orchestration this slice reshapes into the wizard. The `disabled={isSelfManaged && noCredentials}` Save is the dead-end being removed.
2. `ui/packages/app/app/(dashboard)/settings/models/components/ProviderKeyFields.tsx` — holds the cross-page bounce: the `noCredentials` branch renders an Alert linking to `/credentials`. That link is what this spec eliminates.
3. `ui/packages/app/app/(dashboard)/credentials/components/AddCredentialForm.tsx` + `credentials/actions.ts` — the existing `createCredentialAction(workspaceId, {name, data})` the inline create reuses. A BYOK credential is the JSON object `{provider, api_key, model}` (see `ProviderKeyFields` lines 60-61).
4. `ui/packages/app/app/(dashboard)/settings/models/actions.ts` — `setProviderSelfManagedAction({credential_ref, model?})` and `resetProviderAction()`; reuse verbatim, no new actions.
5. `dispatch/write_ts_adhere_bun.md` — Bun/TS + UI-substitution + design-token discipline this diff is gated on.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Models page: inline-progressive BYOK setup wizard (no cross-page credential bounce)
- **Intent (one sentence):** A tenant configures a self-managed model provider end-to-end on the Models page — select or create a credential inline, name the model, activate — without ever navigating to `/credentials`; platform-managed stays one click.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`; a mismatch with the Intent above → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal. Specifically pinned: **TSC** + **TSJ** (const/import/Bun-native, error style), **UIS** (design-system primitive over raw HTML), **DTK** (named token utility, no arbitrary `*-[...]`), **UFS** (provider/prefix/model-default literals → named constants, shared verbatim across detect util + tests), **NDC** (no dead code — the old `/credentials` dead-end branch is removed, not left), **NLR** (touch-it-fix-it on `ProviderKeyFields`/`ProviderSelector`), **MSID** (no `M86`/§ refs in source or test names — RULE TST-NAM), **FLL** (file/function length), **GRP** (end-of-turn diff audit vs RULES codes).
- **`dispatch/write_ts_adhere_bun.md`** — TS FILE SHAPE DECISION at PLAN for each new component.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG / PUB / SCHEMA / ERROR REGISTRY / LOGGING / LIFECYCLE | no | UI-only; no `*.zig`, no schema, no backend handler, no new error codes. |
| UI Substitution (UIS) | yes | All new chrome uses `@usezombie/design-system` primitives (Select, Input, Button, Alert, Badge, RadioGroup, Form*); no raw `<select>`/`<input>`. |
| DESIGN TOKEN (DTK) | yes | Token utilities only (`text-muted-foreground`, `border-border`, `text-pulse`/focus ring); zero arbitrary `*-[#hex]` values. |
| TS FILE SHAPE | yes | PLAN-time verdict per new component (presentational vs container). |
| File & Function Length (≤350/≤50) | yes | Split wizard orchestration from presentational step components if `ProviderSelector` approaches the cap. |
| UFS | yes | Provider-key prefixes + provider names + default-model lookups are named constants in one detect module, imported by both the component and its test. |
| MILESTONE-ID (MSID) | yes | No `M86_001` / §x.y / `T{n}` in any `.tsx`/`.ts` body or `test "…"` name. |

---

## Overview

**Goal (testable):** From the Models page with zero credentials, a tenant pastes an API key, the provider + default model auto-fill, they save the credential inline (no navigation), pick the model, click Set active, and the page shows the provider as active — all without a request to `/credentials`.

**Problem:** The bring-your-own-key path dead-ends. Choosing "Use my own provider key" with an empty vault renders an Alert that links to `/credentials` and disables Save (`ProviderSelector` `disabled={isSelfManaged && noCredentials}`). The tenant must leave the page, create a credential as raw JSON, and return — a two-step cross-page flow with a dead button in the middle.

**Solution summary:** Reshape the Models page into an inline-progressive, gated wizard. Platform-managed stays a one-click escape hatch. The BYOK path becomes two numbered steps on one screen: **(1)** select an existing credential or create one inline via a structured provider-key form; **(2)** choose the model (catalogue-backed picker) and Set active, locked until step 1 is satisfied. A paste-to-fill helper auto-detects the provider from the key prefix (client-side heuristic) and prefills that provider's default model from the catalogue. The catalogue is server-fetched (`page.tsx` → `lib/api/model_caps.ts` → public `cap.json`) and passed to the wizard as props; the `cap.json` endpoint itself is the sibling spec **M86_002** (same PR/branch, built first). A one-line switch-safety note states new runs use the new provider while in-flight agents finish on their current one. No new zombied endpoint in THIS spec — reuses `setProviderSelfManagedAction`, `resetProviderAction`, `createCredentialAction`, plus one new client-side catalogue fetch.

---

## Prior-Art / Reference Implementations

- **UI** → `@usezombie/design-system` primitives + `theme.css` tokens (the dark/Commit-Mono/mint-pulse system). The wizard mirrors the existing `ProviderSelector` form-action + `useActionState` pattern; it reshapes that file rather than inventing a new state container.
- **Provider/credential model** → M48_001 (self-managed provider action + `TenantProvider`) and M45_001 (structured vault credential `{provider, api_key, model}`). Divergence: the inline create is a **structured** provider-key form (provider/key/model fields), not the generic raw-JSON `AddCredentialForm`, which stays on `/credentials`. Both call the same `createCredentialAction`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `…/settings/models/page.tsx` | EDIT | Server-fetch the catalogue (via `lib/api/model_caps.ts`) and pass it to the wizard; dual-state render: active-config card + "Change provider" when configured, wizard when not. |
| `…/settings/models/components/ProviderSelector.tsx` | EDIT | Reshape into the gated wizard orchestrator (platform escape hatch + steps 1/2). |
| `…/settings/models/components/ProviderKeyFields.tsx` | EDIT | Remove the empty-vault dead-end Alert; keep a secondary "Manage all credentials →" link; host the credential combobox + inline-create trigger + paste-to-fill. |
| `…/settings/models/components/InlineProviderKeyCreate.tsx` | CREATE | Structured provider-key create form (auto-prefilled editable name + provider/api_key/model) → `createCredentialAction`; selects on success. Purpose-built structured form, NOT the raw-JSON `AddCredentialForm` — divergence intentional; no shared-hook extraction (overlap = the action call + error string, too small to abstract). |
| `…/settings/models/lib/detect-provider.ts` | DONE (committed) | Pure: API-key-prefix → provider slug from a named-constant table (client-side key-format heuristic, no catalogue dependency). Returns `string \| null`. |
| `ui/packages/app/lib/api/model_caps.ts` | CREATE | Server-side fetch of the public `cap.json` catalogue (unauth); returns the model list. Called from `page.tsx`, passed to the wizard as props. The endpoint it hits = M86_002. |
| `…/settings/models/components/{Step1Credential,Step2Model}.tsx` | CREATE | Presentational step sub-components split out of `ProviderSelector` up front (FLL: keep the orchestrator ≤50/line fn, ≤350/file). Step2's model field is a catalogue-backed `Select` (from the passed-in catalogue), not free-text. |
| `…/settings/models/components/ModeRadio.tsx` | EDIT (if needed) | Reuse for the platform/self-managed choice. |
| `…/app/tests/provider-selector.test.ts` | EDIT | Cover gating, dual-state, no-`/credentials`-link. |
| `…/app/tests/inline-provider-key-create.test.ts` | CREATE | Inline create reuses action, selects credential, no navigation. |
| `…/app/tests/detect-provider.test.ts` | DONE (committed) | Prefix detection (provider slug only) + ordering + unknown-prefix no-op. |
| `…/app/tests/e2e/acceptance/settings-models.spec.ts` | EDIT | e2e: inline BYOK setup with zero starting credentials. |

> No zombied/schema change in THIS spec (the `cap.json` endpoint is M86_002, same PR). New client code: `lib/api/model_caps.ts` (catalogue fetch). `createCredential`, `setTenantProviderSelfManaged`, `resetTenantProvider` are reused as-is.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** reshape the three existing model components in place (`ProviderSelector`/`ProviderKeyFields` + one new inline-create + one pure detect util). ~80% recomposition.
- **Alternatives considered:** (a) the minimal patch — only un-dead-end the empty-vault case by adding an inline create, leaving the flat form; rejected because the gating + dual-state are what make it read as one coherent flow. (b) The full activation platform (multi-config list, dedicated onboarding hub, per-agent override); rejected for this pass — needs backend, captured as the deferred spec C.
- **Patch-vs-refactor verdict:** **patch** (in-place recomposition of existing components). The larger refactor (multi-config data model) is a separate backend spec, not mud-patched here.

---

## Sections (implementation slices)

### §1 — Gated wizard shell + dual-state

Reshape the Models page so an unconfigured tenant sees the wizard and a configured tenant sees their active config with an edit affordance. Platform-managed is a one-click escape hatch at the top in both states. **Implementation default:** keep the `useActionState` form-action pattern already in `ProviderSelector`.

- **Dimension 1.1** — Configured tenant renders the active-config card + "Change provider" trigger; unconfigured renders the wizard. → Test `test_models_dual_state_render`
- **Dimension 1.2** — Step 2 (model + Set active) is disabled until a credential is selected or created in step 1. → Test `test_model_step_gated_until_credential`
- **Dimension 1.3** — Platform-managed one-click path resets via `resetProviderAction` from either state. → Test `test_platform_one_click_reset`

### §2 — Inline credential create (no cross-page bounce)

Replace the empty-vault dead-end with an inline structured provider-key create. The empty-vault warning-Alert dead-end is removed entirely (NDC); a secondary "Manage all credentials →" link to `/credentials` remains as an optional escape hatch (not a dead-end).

- **Dimension 2.1** — "＋ New key" expands an inline structured form (provider, api_key, model); submit calls `createCredentialAction` with `{provider, api_key, model}` and selects the new credential. → Test `test_inline_create_selects_credential`
- **Dimension 2.2** — Zero-credential vault renders the inline create as the primary path (no warning-Alert dead-end, Save not disabled-trapped). A secondary "Manage all credentials →" link to `/credentials` is allowed — it is an optional escape, not the empty-vault dead-end. → Test `test_empty_vault_inline_create_primary`
- **Dimension 2.3** — Inline create never navigates (no `router.push`/`Link` to `/credentials`); stays on the Models page through success. → Test `test_inline_create_no_navigation`

### §3 — Paste-to-fill + switch-safety copy

Two delights accepted at CEO review. Paste-to-fill's provider detection is client-side (`detect-provider.ts`); the default-model prefill reads the server-fetched catalogue (`cap.json`, via M86_002). Switch-safety is pure copy.

- **Dimension 3.1** — Pasting an API key whose prefix maps to a known provider sets the provider (client heuristic) and prefills that provider's default model from the server-fetched catalogue; an unknown prefix leaves both untouched (no error). → Test `test_paste_to_fill_provider_and_default_model`
- **Dimension 3.2** — A one-line note near Set active states new runs use the new provider and in-flight agents finish on their current one; present whenever a self-managed change is pending. → Test `test_switch_safety_copy_present`

### §4 — Wizard interaction states

Every state designed, not just the happy path.

- **Dimension 4.1** — Success: after a save the active provider shows a live (mint) active badge. → Test `test_active_badge_on_success`
- **Dimension 4.2** — Error: `createCredentialAction` or `setProviderSelfManagedAction` failure (4xx/5xx) surfaces an Alert and keeps the wizard mounted; no unhandled throw. → Test `test_save_failure_surfaces_alert`
- **Dimension 4.3** — Partial: credential created but model unset → step 2 prompts for a model and Set active stays gated. → Test `test_partial_credential_without_model_gated`

---

## Interfaces

```
detectProviderFromKey(apiKey: string): string | null            // committed (client-side key-format heuristic)
  - pure; prefix table is a named constant; returns the provider slug, null when no prefix matches

getModelCatalogue(): Promise<{ models: ModelCap[] }>            // NEW lib/api/model_caps.ts (server-side)
  - server-fetch of the public cap.json (unauth); page.tsx awaits it and passes models to the wizard
  - the wizard resolves a provider's default model from this catalogue (paste-to-fill prefill)

Reused as-is (NO new zombied endpoint here — cap.json rename+extend is M86_002):
  createCredentialAction(workspaceId, { name, data: { provider, api_key, model } }) -> ActionResult<{name}>
  setProviderSelfManagedAction({ credential_ref, model? })                          -> ActionResult<TenantProvider>
  resetProviderAction()                                                              -> ActionResult<TenantProvider>
```

The credential `data` shape `{provider, api_key, model}` is the contract from M45/M48; do not change it.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Credential create rejected | duplicate name / 4xx / 5xx | Alert via `presentErrorString`; wizard stays mounted; credential not selected; no navigation. |
| Provider save rejected | model not in caps catalogue / 5xx | Alert; provider unchanged; active badge not shown. |
| Unknown key prefix | user pastes a non-matching key | `detectProviderFromKey` returns null; provider/model fields untouched; no error toast. |
| Empty vault | tenant has zero credentials | inline create is the primary path (Save not trapped); the secondary "Manage all credentials →" link is optional, not a forced bounce. |
| Activate without credential | step 2 reached with no credential | Set active disabled (gated); cannot submit. |

---

## Invariants

1. The empty-vault **dead-end** is gone — the inline create is the primary path and Save is never disabled-trapped on an empty vault. A secondary "Manage all credentials →" link to `/credentials` is permitted (an optional escape hatch, not a dead-end). Enforced by `test_empty_vault_inline_create_primary` + `test_inline_create_no_navigation` (the create-submit flow itself never navigates).
2. Set active cannot fire without a selected/created credential — enforced by the disabled-state guard + `test_model_step_gated_until_credential`.
3. `detectProviderFromKey` is pure and table-driven (named constants, UFS) — enforced by `detect-provider.test.ts` and the lint/UFS gate. The model catalogue (incl. the default model) comes from the server-fetched `cap.json`, never hardcoded client-side.
4. Credential `data` is exactly `{provider, api_key, model}` — enforced by the create call signature + test asserting the object passed to `createCredentialAction`.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_models_dual_state_render` | provider set → config card + "Change provider"; unset → wizard. |
| 1.2 | unit | `test_model_step_gated_until_credential` | no credential → Set active disabled; after select → enabled. |
| 1.3 | unit | `test_platform_one_click_reset` | platform option → `resetProviderAction` called once. |
| 2.1 | unit | `test_inline_create_selects_credential` | inline submit → `createCredentialAction({name,data:{provider,api_key,model}})`; new name becomes selected `credential_ref`. |
| 2.2 | unit | `test_empty_vault_inline_create_primary` | zero credentials → inline create form present + Save not disabled; the warning-Alert dead-end (`provider-key-no-credentials`) absent. |
| 2.3 | unit | `test_inline_create_no_navigation` | success path → router push mock never called. |
| 3.1 | unit | `test_paste_to_fill_provider_and_default_model` | known prefix → provider set + default model prefilled from the (mocked) catalogue; unknown prefix → both untouched, no throw. |
| 3.2 | unit | `test_switch_safety_copy_present` | pending self-managed change → safety line in DOM. |
| 4.1 | unit | `test_active_badge_on_success` | save resolves ok → active (mint) badge rendered. |
| 4.2 | unit | `test_save_failure_surfaces_alert` | action returns `{ok:false}` → Alert text shown; no throw. |
| 4.3 | unit | `test_partial_credential_without_model_gated` | credential set, model empty → Set active gated, model prompt shown. |
| all | e2e | `test_byok_setup_from_empty_vault` | acceptance: sign in → /settings/models → create key inline → pick model → active, URL never leaves /settings/models. |

**Regression:** existing `settings-tabs` + `app-components` Model→Models assertions must stay green. **Idempotency:** N/A — no retry semantics added.

---

## Acceptance Criteria

- [ ] BYOK setup completes without leaving `/settings/models` — verify: `test_byok_setup_from_empty_vault` (e2e acceptance)
- [ ] Empty-vault dead-end removed (inline create primary, Save not trapped, `provider-key-no-credentials` Alert gone) — verify: `test_empty_vault_inline_create_primary`. A secondary "Manage all credentials →" link is allowed, so the old `href="/credentials" → 0` grep gate no longer applies.
- [ ] `cd ui/packages/app && bun run typecheck && bun run lint` clean
- [ ] `cd ui/packages/app && bun run test:coverage` passes, aggregate ≥ 99%
- [ ] design-system suite green (run sequentially per launch-polish handoff)
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: empty-vault dead-end (warning Alert) removed — the secondary manage-link is allowed
grep -rn 'provider-key-no-credentials' "ui/packages/app/app/(dashboard)/settings/models/" | head && echo "CHECK(empty=pass)"
# E2: typecheck + lint
cd ui/packages/app && bun run typecheck && bun run lint 2>&1 | tail -3
# E3: unit + coverage
cd ui/packages/app && bun run test:coverage 2>&1 | tail -8
# E4: e2e acceptance (models)
cd ui/packages/app && bunx playwright test tests/e2e/acceptance/settings-models.spec.ts 2>&1 | tail -8
# E6: gitleaks
gitleaks detect 2>&1 | tail -3
# E7: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

**1. Orphaned files** — none deleted (in-place recomposition).

**2. Orphaned references** — after removing the `/credentials` dead-end branch from `ProviderKeyFields`:

| Removed | Grep | Expected |
|---------|------|----------|
| `provider-key-no-credentials` empty-vault warning Alert (the dead-end); the `/credentials` link survives only as the optional secondary "Manage all credentials →" affordance | `grep -rn 'provider-key-no-credentials' ui/packages/app \| head` | 0 |

---

## Discovery (consult log)

> Empty at creation. Append consults, skill-chain outcomes, and Indy-acked deferral quotes as work proceeds.

- Scope locked at `/plan-ceo-review` (SELECTIVE EXPANSION, scope B → reduced to variant A only). Dashboard stepper + multi-config deferred (see Out of Scope).
- Indy directive (Jun 06): land on existing `feat/dashboard-launch-polish` branch (PR #373), reuse existing worktree, no new worktree at CHORE(open).
- Indy directive (Jun 07): keep a secondary "Manage all credentials →" link to `/credentials` in the BYOK path as an optional escape hatch; only the empty-vault dead-end (the `provider-key-no-credentials` warning Alert + disabled-Save trap) is removed. Invariant 1, Dimension 2.2, the acceptance check, and Eval E1 amended accordingly.
- `/plan-eng-review` + Indy (Jun 07): **D1** — the paste heuristic stays client-side and provider-only (`detect-provider.ts` returns a slug). **D2** — the model catalogue + default-model prefill come from the public `cap.json`, server-fetched by `page.tsx` (`lib/api/model_caps.ts`); the model field is a catalogue-backed picker, not free-text. `cap.json` (rename of `model-caps.json` + rates/globals) is sibling spec **M86_002**; **both land on PR #373 / `feat/dashboard-launch-polish`, cap.json built first**. Inline create is a purpose-built structured form (no `AddCredentialForm` extract); credential name auto-prefilled to the provider slug (editable); `ProviderSelector` split up front into orchestrator + Step1/Step2.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Clean; iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Clean OR every finding dispositioned. |
| After `gh pr create` (PR #373 update) | `/review-pr` + `kishore-babysit-prs` | Comments addressed; Greptile re-poll to two empty polls. |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit + coverage | `bun run test:coverage` | {paste} | |
| Lint + typecheck | `bun run lint && bun run typecheck` | {paste} | |
| e2e (models) | `playwright test settings-models.spec.ts` | {paste} | |
| Empty-vault dead-end gone | `grep -rn 'provider-key-no-credentials' …/models/` → 0 | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |

---

## Out of Scope

- Dashboard Getting-Started stepper — `FirstInstallCard` empty state stays as-is (Indy, Jun 06).
- Credentials standalone-page list redesign (generic `AddCredentialForm` raw-JSON form stays).
- Key-shape sanity hint and live step checkmarks — deferred UI follow-ups.
- Live provider-key validation (needs a backend endpoint).
- Multiple saved provider configs + default + per-agent model override — separate backend spec (CEO plan approach C).

---

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | CLEAR | variant A locked; multi-config deferred (approach C) |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR | 4 issues, 0 critical gaps — all dispositioned |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | — |

- **UNRESOLVED:** 0 — D1 (catalogue source) and D2 (one PR / branch) both decided by Indy (Jun 07).
- **VERDICT:** ENG CLEARED. Architecture settled: provider detection is a client-side key-format heuristic; the model catalogue + default-model prefill are server-fetched from the public `cap.json` (sibling spec **M86_002**). Tests complete (duplicate-name added). 0 critical failure gaps. Build order on PR #373: `cap.json` (M86_002) first, then the wizard (M86_001).
