# M110_001: Add a dashboard flow to onboard a Fleet template into a workspace

**Prototype:** v2.0.0
**Milestone:** M110
**Workstream:** 001
**Date:** Jul 02, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — user-facing: completes M103's API-first onboarding with the missing dashboard surface, so an empty workspace is no longer a dead end.
**Categories:** DOCS, UI
**Batch:** B1 — standalone; no sibling workstream dependency.
**Branch:** feat/m110-template-onboarding
**Test Baseline:** unit=2270 integration=243
**Depends on:** M103_001 (two-tier catalog + the `POST …/fleet-templates` onboard endpoints this consumes; already `done/`). M104_001 (the `template:write` scope the endpoint gates on; already live).
**Provenance:** LLM-drafted (claude-opus-4-8, Jul 02, 2026) — drafted from a live debugging session that surfaced the gap.

> **Provenance is load-bearing.** LLM-drafted — cross-check every file pointer and the M103 onboard-vs-install distinction against the code before EXECUTE.

**Canonical architecture:** `docs/architecture/fleet_bundles.md` — the onboard-vs-install storage roles M103 established; this workstream adds no new architecture, only the UI entry point to an existing route.

---

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/templates/onboard.zig` — the tenant onboard route (`innerTenantOnboard`): request shape `ImportRequest` (in `handlers/fleet_bundles/resolve.zig`), the `respond()` success body, the `template:write` scope gate (upstream) + `authorizeWorkspace` ownership check. The UI targets this endpoint; do not change it.
2. `ui/packages/app/lib/actions/with-token.ts` + `ui/packages/app/app/(dashboard)/settings/api-keys/actions.ts` — the `withToken((t) => …) → ActionResult<T>` Server Action pattern every mutation mirrors (token stays server-side; `ApiError → {errorCode}` normalization).
3. `ui/packages/app/lib/api/fleet-templates.ts` — the GET-only gallery client to extend with the POST onboard call.
4. `docs/v2/done/M103_001_P1_API_DOCS_INFRA_TWO_TIER_TEMPLATE_CATALOG_ONBOARDING.md` §Product Clarity (items 2–8) — **onboarding via a GitHub source-ref is the supported path (item 3); GitHub/paste as an *install* source was removed (item 2). Do not conflate — this spec adds an onboard affordance, not an install source.**
5. `ui/packages/app/app/(dashboard)/fleets/new/InstallEntry.tsx` + `…/fleets/page.tsx` — where the affordance surfaces (empty-state gallery + install page).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Add dashboard flow to onboard a Fleet template into a workspace
- **Intent (one sentence):** A workspace admin can add a GitHub-sourced Fleet template to their gallery from the dashboard, instead of the template gallery being a read-only dead end that only an API call can populate.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + list `ASSUMPTIONS I'M MAKING: …`; mismatch → STOP and reconcile.

---

## Product Clarity

1. **Successful user moment** — A user with an empty Fleets gallery clicks **Add template**, types `owner/repo`, submits; seconds later the card appears in the same gallery and they click **Use template** to install — never leaving the dashboard, never touching an API client.
2. **Preserved user behaviour** — The existing template-only install flow (gallery pick → `/fleets/new` → live states), the GET gallery union (platform ∪ own-tenant), and the API-first onboard endpoints all keep working unchanged. This adds an entry point; it removes nothing.
3. **Optimal-way check** — The most direct shape is a single form (one `owner/repo` field) posting the existing tenant endpoint with `source_kind:"github"`. The gap vs unconstrained-optimal: no `upload` (paste markdown) or `template`-clone source in the UI yet, and no platform-tier onboarding UI. Acceptable — GitHub source-ref is M103's blessed path and covers the moment; the others are additive follow-ups.
4. **Rebuild-vs-iterate** — Iterate. The backend, R2 store, and gallery already exist; this is a thin UI + one Server Action. A refactor would trade nothing for risk.
5. **What we build** — A POST client in `fleet-templates.ts`, an `onboardTemplateAction` Server Action, an `AddTemplate` dialog/form client component, its surfacing in the empty-state gallery + install page, and a post-success gallery refresh.
6. **What we do NOT build** — Platform-tier (`/admin/fleet-templates`) onboarding UI; `upload`/paste-markdown and `template`-clone sources; a CLI onboard command; template edit/delete UI; per-source commit pinning.
7. **Fit with existing features** — Compounds with the M103 gallery + install flow. Must not destabilize the read path (`listWorkspaceFleetTemplatesCached`) or the token-never-in-browser invariant.
8. **Surface order** — UI-first here by design: the API already shipped in M103; this workstream is the deferred UI surface. CLI onboard stays out of scope.
9. **Dashboard restraint** — Show only identity-level inputs (`owner/repo`); never expose R2 keys, content hashes as controls, or object-store paths. The dialog is hidden from callers without `template:write` (see §3) rather than showing a button that always 403s.
10. **Confused-user next step** — On failure the dialog shows the mapped error (bad repo, insufficient scope, fetch failed) with the `UZ-…` code and a one-line fix, plus a link to the onboarding doc — never a dead end.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal discipline; specifically **NDC** (no dead code — no speculative `upload`/`template` branches shipped unused), **NLG** (no new legacy framing — the onboard affordance is not the removed github-*install* source; name the distinction), **UFS** (`source_kind` value, endpoint path, event name → named constants).
- **`dispatch/write_ts_adhere_bun.md`** — `const`/import discipline, TS FILE SHAPE at PLAN, **UI Component Substitution** (design-system primitives, no raw HTML) + **DESIGN TOKEN** (no arbitrary `*-[…]` utilities).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — read-only here: the client must match the existing route/verb; **no endpoint changes**.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | No `*.zig` touched — backend already ships. |
| PUB / Struct-Shape | no | No Zig pub surface. |
| File & Function Length | yes | New client TS files kept small; dialog split from action from api-lib. |
| UFS | yes | `SOURCE_KIND_GITHUB`, the onboard path builder, and the analytics event name are named constants (shared verbatim with the Zig `importer.SOURCE_KIND_*` wire values). |
| UI Substitution / DESIGN TOKEN | yes | Dialog/form built from `@agentsfleet/design-system` primitives + theme tokens; zero raw HTML controls, zero arbitrary values. |
| LOGGING / ERROR REGISTRY / SCHEMA | no | Reuses existing `UZ-…` codes surfaced via `presentError`; no new codes, no schema. |

---

## Overview

**Goal (testable):** A workspace admin submits `owner/repo` in the dashboard **Add template** dialog; a Server Action POSTs `{source_kind:"github", source_ref:"owner/repo"}` to `/v1/workspaces/{ws}/fleet-templates` with a server-minted token; on `201` the gallery refreshes and the onboarded template card is present without a page navigation.

**Problem:** The dashboard gallery only *lists* templates (`fleet-templates.ts` is GET-only) and the empty state says "Onboard a template into your workspace" with no control that does it — onboarding is reachable only by hand-crafting an API call. New workspaces hit a dead end.

**Solution summary:** Add the missing write path at the UI layer only — a POST client, a `withToken` Server Action, and an `AddTemplate` dialog surfaced from the empty-state gallery and install page — that drives the already-shipped M103 tenant onboard endpoint and refreshes the gallery on success. No backend, schema, or endpoint change.

---

## Prior-Art / Reference Implementations

- **UI mutation** → `settings/api-keys/actions.ts` (`createApiKeyAction`) + `lib/actions/with-token.ts` — mirror the `withToken → ActionResult<T>` shape and `errorCode` propagation verbatim.
- **Gallery client** → `lib/api/fleet-templates.ts` `listWorkspaceFleetTemplates` — the same `request<T>(path, {method}, token)` call, POST instead of GET.
- **Dialog/form UI** → an existing design-system dialog form in `(dashboard)/settings/**` (e.g. api-keys create dialog) — reuse its primitives, validation, and error rendering.
- **Onboard-vs-install semantics** → M103_001 §Product Clarity — the divergence to honor.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/api/fleet-templates.ts` | EDIT | Add `onboardWorkspaceFleetTemplate` POST client + return type. |
| `ui/packages/app/lib/types.ts` | EDIT | `OnboardTemplateRequest` + `OnboardedTemplate` (mirror `respond()`). |
| `ui/packages/app/app/(dashboard)/fleets/actions.ts` | EDIT | `onboardTemplateAction(workspaceId, body)` via `withToken`. |
| `ui/packages/app/app/(dashboard)/fleets/new/AddTemplateDialog.tsx` | CREATE | Client dialog/form: `owner/repo` input, submit, error render, on-success refresh. |
| `ui/packages/app/app/(dashboard)/fleets/new/InstallEntry.tsx` | EDIT | Surface the **Add template** affordance beside/above the gallery. |
| `ui/packages/app/app/(dashboard)/fleets/page.tsx` | EDIT | Thread `workspaceId` + caller scope hint into the empty state / affordance. |
| `ui/packages/app/tests/**` | CREATE/EDIT | Unit (client, action, dialog) + e2e (onboard → gallery refresh). |
| `~/Projects/docs/**` (cross-repo, own-branch) | EDIT | Onboarding how-to page + changelog `<Update>`; done at DOCUMENT/CHORE(close). |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Three thin slices — data path (§1), UI affordance (§2), refresh + error/scope handling (§3) — because each is independently testable and the whole is additive over M103.
- **Alternatives considered:** (a) A full `/fleets/templates/new` route page instead of a dialog — heavier, and the moment is a one-field form; rejected. (b) Also build platform-tier + `upload` sources now — scope creep past moment #1; deferred to Out of Scope.
- **Patch-vs-refactor verdict:** **patch** — it completes an intentionally API-first feature with its UI entry point. No refactor is warranted; the follow-ups (platform tier, upload source) are named in Out of Scope, not silently mud-patched in.

---

## Sections (implementation slices)

### §1 — Tenant onboard data path (client + Server Action)

Add the write half of `fleet-templates.ts` and the Server Action that fronts it, so the UI can onboard with a server-minted token and a typed body. **Implementation default:** `source_kind` fixed to the `github` wire value for MVP (matching `importer.SOURCE_KIND_GITHUB`); the request type is a discriminated union so `upload`/`template` extend it later without a breaking change.

- **Dimension 1.1** — `onboardWorkspaceFleetTemplate(workspaceId, body, token)` issues `POST /v1/workspaces/{workspaceId}/fleet-templates` with the JSON body and returns the parsed `OnboardedTemplate`. → Test `test_onboard_client_posts_tenant_endpoint`
- **Dimension 1.2** — `onboardTemplateAction(workspaceId, body)` wraps it in `withToken`, returns `ActionResult`, and surfaces `ApiError.code` as `errorCode` on failure. → Test `test_onboard_action_maps_apierror_to_errorcode`

### §2 — Add-template affordance (dialog + surfacing)

A client dialog with a single `owner/repo` field submitting to `onboardTemplateAction`, surfaced from the empty-state gallery and the install page. Built only from design-system primitives.

- **Dimension 2.1** — The dialog renders a labelled `owner/repo` input + submit built from design-system primitives (no raw HTML control, no arbitrary utility). → Test `test_add_template_dialog_renders_primitives`
- **Dimension 2.2** — The **Add template** trigger appears in `InstallEntry` (empty gallery + install page); clicking opens the dialog. → Test `test_add_template_affordance_present`
- **Dimension 2.3** — Empty or malformed `source_ref` (not `owner/repo` shape) is blocked client-side with an inline message before any submit. → Test `test_add_template_blocks_bad_source_ref`

### §3 — Success refresh, error surfacing, scope gating

On `201` refresh the gallery so the new card appears; on failure keep the dialog open and show the mapped error; hide the trigger from callers without `template:write`.

- **Dimension 3.1** — On success the dialog closes and the gallery re-reads (`router.refresh()` / `revalidatePath`), so the onboarded template is visible without navigation. → Test `test_onboard_success_refreshes_gallery`
- **Dimension 3.2** — A `403` (insufficient scope / workspace-ownership) or a GitHub fetch failure keeps the dialog open and renders the `UZ-…`-mapped message via `presentError`. → Test `test_onboard_failure_surfaces_mapped_error`
- **Dimension 3.3** — The **Add template** trigger is not rendered when the caller's session scopes lack `template:write`. → Test `test_add_template_hidden_without_scope`

---

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `fleet_template_onboarded` | product | Server Action returns `ok` for a tenant onboard | `workspace_id`, `visibility` (`tenant`), `source_kind`, `outcome` | no repo credentials, no token, no R2 key/path | `test_onboard_emits_analytics_event` |

Adds one action event; no existing gallery/install event is renamed or removed. Funnel note: this is a new first-run activation step — update the activation funnel in the analytics playbook in the same PR; if the playbook is unaffected, Discovery records `Metrics review: no analytics/funnel playbook update required` with the reason.

---

## Interfaces

```
POST /v1/workspaces/{workspace_id}/fleet-templates      (unchanged — M103)
  scope: template:write   +   workspace ownership
  request  (OnboardTemplateRequest, MVP variant):
    { "source_kind": "github", "source_ref": "owner/repo" }
  response (201, OnboardedTemplate — mirrors onboard.zig respond()):
    { "id","name","visibility":"tenant","content_hash","requirements",
      "support_files":[{ name,size }] }

// ui/packages/app/lib/api/fleet-templates.ts
onboardWorkspaceFleetTemplate(
  workspaceId: string, body: OnboardTemplateRequest, token: string,
): Promise<OnboardedTemplate>

// ui/packages/app/app/(dashboard)/fleets/actions.ts   ("use server")
onboardTemplateAction(
  workspaceId: string, body: OnboardTemplateRequest,
): Promise<ActionResult<OnboardedTemplate>>
```

`OnboardTemplateRequest` is a discriminated union on `source_kind` (MVP: only the `github` variant materialized); the response type mirrors the Zig `respond()` body — do not invent fields.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Insufficient scope | Caller lacks `template:write` | Trigger hidden (§3.3); if forced, endpoint `403 UZ-AUTH-022` → dialog shows mapped message, stays open. |
| Workspace not owned | `authorizeWorkspace` fails | `403` → mapped "workspace access denied"; dialog stays open. |
| Malformed source-ref | Not `owner/repo` | Blocked client-side (§2.3); never reaches the network. |
| GitHub fetch/import failure | Repo missing/private/invalid bundle | Endpoint import error → `presentError` by code; dialog stays open with a retry-able message. |
| Duplicate onboard | Same repo re-submitted | `insertOrUpdate` upserts by content hash; gallery shows one entry, not a duplicate (§3.1 refresh is idempotent). |
| Not authenticated | Session/token absent | `withToken` returns `{ok:false, errorCode: AUTH_401}` → redirect/sign-in per existing action behaviour. |
| Body too large | Oversized payload | Endpoint `413` → mapped message; dialog stays open. |

---

## Invariants

1. The onboard token is minted and used server-side only — enforced by `"use server"` + `withToken` (the browser never receives an api-audience token), same as every existing action.
2. `source_kind` is a constrained union value, never free-text — enforced by the TypeScript type + a named constant equal to the Zig wire value (`importer.SOURCE_KIND_GITHUB`).
3. After a successful onboard the gallery reflects the new template — enforced by `router.refresh()` / `revalidatePath` in the success path, asserted by e2e (§3.1).
4. The Add-template trigger is gated on `template:write` — enforced by a scope check on the session claim, asserted by §3.3.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_onboard_client_posts_tenant_endpoint` | `("ws_1",{github,owner/repo})` → POST `/v1/workspaces/ws_1/fleet-templates` with that JSON body. |
| 1.2 | unit | `test_onboard_action_maps_apierror_to_errorcode` | endpoint `403 UZ-AUTH-022` → `{ok:false, errorCode:"UZ-AUTH-022"}`. |
| 2.1 | unit | `test_add_template_dialog_renders_primitives` | dialog mounts a design-system input + button; no raw `<input>`/arbitrary class. |
| 2.2 | unit | `test_add_template_affordance_present` | `InstallEntry` (empty gallery) renders an Add-template trigger. |
| 2.3 | unit | `test_add_template_blocks_bad_source_ref` | `"notarepo"` / `""` → inline error, action not invoked. |
| 3.1 | e2e | `test_onboard_success_refreshes_gallery` | submit valid repo → 201 → gallery shows the new card without navigation. |
| 3.2 | unit | `test_onboard_failure_surfaces_mapped_error` | `403`/import-fail → dialog open, `presentError` text shown. |
| 3.3 | unit | `test_add_template_hidden_without_scope` | session scopes without `template:write` → no trigger rendered. |
| Metrics | unit | `test_onboard_emits_analytics_event` | successful action → `fleet_template_onboarded` with allowed props only. |

Regression: `test_gallery_get_unchanged` — the GET gallery read path and install flow behave exactly as before. Idempotency: covered by §3.3/duplicate Failure Mode (re-onboard upserts, no duplicate card).

---

## Acceptance Criteria

- [ ] Empty Fleets gallery shows a working **Add template** control — verify: e2e `test_onboard_success_refreshes_gallery`
- [ ] Onboard posts the tenant endpoint with a server-minted token — verify: `test_onboard_client_posts_tenant_endpoint`
- [ ] Failures surface the mapped `UZ-…` message, dialog stays open — verify: `test_onboard_failure_surfaces_mapped_error`
- [ ] Trigger hidden without `template:write` — verify: `test_add_template_hidden_without_scope`
- [ ] `make lint` clean · `make test` passes (UI unit) · UI e2e passes
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: UI unit — the onboard client/action/dialog tests
make test 2>&1 | grep -E "onboard|add_template" || echo "check runner"
# E2: Build — cd ui && bun run build (app package)
# E3: e2e — the onboard→gallery scenario (repo e2e runner)
# E4: Lint — make lint 2>&1 | grep -E "✓|FAIL"
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md) —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

N/A — no files deleted. This workstream is purely additive; `InstallEntry`'s "onboard a template into your workspace" prose becomes a live control rather than dead instructional text.

---

## Discovery (consult log)

> **Empty at creation.** Append as work surfaces consults and decisions.

- **Consults** — NLG (onboard-vs-install) distinction confirmed against M103_001 §Product Clarity before EXECUTE.
- **Metrics review** — {events added; `/review` findings; analytics/funnel playbook update or explicit no-change reason}.
- **Skill chain outcomes** — {`/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs`}.
- **Deferrals** — {Indy-acked verbatim quotes only}.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs the Test Specification. | Clean; iteration count in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `dispatch/write_ts_adhere_bun.md`, Failure Modes, Invariants, Metrics. | Clean or every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | {paste} | |
| e2e (user-centric) | `{e2e onboard scenario}` | {paste} | |
| Lint | `make lint` | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |

---

## Out of Scope

- Platform-tier onboarding UI (`POST /v1/admin/fleet-templates`, `platform-template:write`) — operator surface, follow-up spec.
- `upload` (paste `SKILL.md`/`TRIGGER.md`) and `template`-clone onboard sources — additive `source_kind` variants later.
- A CLI onboard command (`agentsfleet templates add …`) — CLI stays browse-only for now.
- Template edit/delete/resync UI and per-source commit pinning.
