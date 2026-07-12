<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M127_001: Platform fleet-library onboarding surface at /admin/fleet-libraries

**Prototype:** v2.0.0
**Milestone:** M127
**Workstream:** 001
**Date:** Jul 12, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing; without it the platform catalog can only be filled by raw curl, so every deployment ships an empty catalog
**Categories:** UI
**Batch:** B1 — single workstream, no parallel siblings
**Branch:** feat/m127-platform-library-ui
**Test Baseline:** unit=2585 integration=311 (Zig depth gate; this spec adds no Zig, so the delta that matters is the app vitest/playwright count — recorded in VERIFY)
**Depends on:** none — M110_001 (done) recorded this exact surface as its deferred follow-up
**Provenance:** LLM-drafted (claude-fable-5, Jul 12, 2026)
**Canonical architecture:** `docs/architecture/fleet_bundles.md` §Two-tier catalog

---

## Overview

**Goal (testable):** A session holding `platform-library:write` onboards a GitHub repository into the platform catalog from `/admin/fleet-libraries` and the entry appears in every workspace's fleet-library gallery; a session without the scope never sees the surface and is redirected when it tries the Uniform Resource Locator (URL) directly.
**Problem:** Operators cannot populate the platform catalog from the dashboard. `POST /v1/admin/fleet-libraries` exists and works, but no page, nav entry, API client, or Command-Line Interface (CLI) verb calls it — onboarding `agentsfleet/github-pr-reviewer` today requires a hand-built curl with a manually minted operator token, and migration-seeded slug rows stay uninstallable.
**Solution summary:** Build the operator surface M110_001 deferred: a scope constant mirroring the backend, a `POST /v1/admin/fleet-libraries` client, a scope-guarded `/admin/fleet-libraries` page with an onboard dialog mirroring the tenant `AddLibraryDialog`, a `PLATFORM_NAV` entry, and a registry analytics event. Zero backend changes — onboarded rows are stored `visibility='public'` and already surface in the workspace gallery.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(ui): platform fleet-library onboarding at /admin/fleet-libraries
- **Intent (one sentence):** Let a platform operator onboard a fleet-library entry from the dashboard so every tenant's gallery gains it, without touching curl or backend code.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/admin/models/page.tsx` — the scope-guarded admin page pattern to mirror (hasScope → redirect to a `/settings?notice=…` constant, token mint, ApiError 401/403 handling).
2. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/AddLibraryDialog.tsx` and `…/fleets/actions.ts` — the onboard form to mirror: zod owner/repo validation, requestId double-submit guard, `presentError` mapping, `withToken` server action.
3. `ui/packages/app/lib/api/fleet-library.ts` and `ui/packages/app/lib/auth/scopes.ts` — the client module and scope registry this spec extends; scopes.ts is a verbatim mirror of `src/agentsfleetd/http/route_scopes.zig`.
4. `src/agentsfleetd/http/handlers/library/onboard.zig` and `src/agentsfleetd/http/handlers/fleet_bundles/resolve.zig` — interface truth: request/response shapes, importer failure codes. Read-only for this spec.
5. `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts` — the fixture-user minting helper the e2e Section extends to seed an operator-scoped Clerk user.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/auth/scopes.ts` | EDIT | Add the `platform-library:write` mirror constant; no closure entry (independent scope, no read rung) |
| `ui/packages/app/lib/api/fleet-library.ts` | EDIT | Add the platform onboard client posting `/v1/admin/fleet-libraries` |
| `ui/packages/app/lib/analytics/events.ts` | EDIT | Add the `platform_library_onboarded` registry event + prop keys |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/page.tsx` | CREATE | Scope-guarded server page |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/actions.ts` | CREATE | `withToken` server action wrapping the platform onboard client |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/loading.tsx` | CREATE | Loading state, mirroring admin/models |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView.tsx` | CREATE | Page body: onboard affordance + last-onboarded result card |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/OnboardPlatformLibraryDialog.tsx` | CREATE | The onboard form dialog |
| `ui/packages/app/components/layout/Shell.tsx` | EDIT | `PLATFORM_NAV` entry "Fleet libraries" gated on the new scope |
| `ui/packages/app/tests/admin-fleet-libraries-page.test.ts` | CREATE | Page guard + action unit tests, mirroring `tests/admin-models-page.test.ts` |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/OnboardPlatformLibraryDialog.test.tsx` | CREATE | Dialog behaviour tests (colocated, like `RunnerList.test.tsx`) |
| `ui/packages/app/tests/e2e/acceptance/platform-library-onboarding.spec.ts` | CREATE | User-centric e2e: scope gating, error path, success path |
| `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts` | EDIT | Operator fixture user whose `public_metadata.scopes` carries `platform-library:write` |
| `ui/packages/app/tests/e2e/acceptance/fixtures/constants.ts` | EDIT | Operator fixture user spec constants, if the fixture registry lives here |

Existing unit suites that assert nav shape or the scopes registry may need updated expectations; those test files join the diff under this table's intent (test-only, same surfaces).

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (paths, notice value, event name, nav label → named constants), NDC (no speculative list/delete plumbing), NRC, NLR (touched files leave cleaner), TST-NAM (test identifiers milestone-free), ORP (nav/scope additions swept for stale references).
- `dispatch/write_ts_adhere_bun.md` — every file in this diff is TypeScript (TS): FILE SHAPE DECISION at PLAN, const/import discipline, design-system primitives only, no arbitrary Tailwind values.
- Analytics single-source discipline (`ui/packages/app/lib/analytics/events.ts` header) — the new event lands in `EVENTS`, `EventProps`, and `EVENT_PROP_KEYS` together; call sites import, never re-spell.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no `*.zig` touched | — |
| PUB / Struct-Shape | no — TS only | — |
| File & Function Length (≤350/≤50/≤70) | yes | Dialog mirrors AddLibraryDialog (~190 lines); view/page stay small; split components if any file nears 350 |
| UFS (repeated/semantic literals) | yes | `NOT_ADMIN` notice, admin path, sample repo, event name as named constants; scope string lives only in scopes.ts |
| UI Substitution / DESIGN TOKEN | yes | `@agentsfleet/design-system` primitives only (Dialog/Form/Input/Alert/Spinner); token utilities, no `*-[…]` arbitrary values |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | UI consumes `UZ-…` codes via `presentError`; mints none |
| MILESTONE-ID | yes | Spec ID in commit messages; source comments cite the spec where non-obvious |

## Prior-Art / Reference Implementations

- **Reference:** `admin/models` surface (page/loading/actions/components split, scope guard, notice redirect) × the M110_001 tenant `AddLibraryDialog` (form, validation, error presentation, analytics emit). This spec composes the two; the one justified divergence is the result card — the tenant flow refreshes a gallery, the platform page has no list endpoint, so success renders the onboarded entry inline instead.

## Sections (implementation slices)

### §1 — Scope constant + API client

The frontend can neither gate nor call the platform endpoint today. Extend the two registries so no other file ever spells the scope or the path. **Implementation default:** no `SCOPE_INCLUDES` entry — the backend treats `platform-library:write` as an independent scope with no read rung; adding a closure edge would diverge from `route_scopes.zig`.

- **Dimension 1.1** — `SCOPE.PLATFORM_LIBRARY_WRITE = "platform-library:write"` exists; `expandScopes` grants it only when held verbatim → Test `test_platform_library_scope_independent`
- **Dimension 1.2** — the platform onboard client POSTs `/v1/admin/fleet-libraries` with the given body and bearer token → Test `test_onboard_client_posts_admin_endpoint`

### §2 — Scope-guarded admin page + server action

The operator surface itself, defence-in-depth in front of the backend's `requireScope`. **Implementation default:** redirect constant `/settings?notice=fleet-libraries-platform-admin-only`, mirroring the models page's notice shape.

- **Dimension 2.1** — a session without the scope is redirected to the notice URL; 401 from the backend redirects to sign-in → Test `test_admin_fleet_libraries_redirects_without_scope`
- **Dimension 2.2** — a scope-holding session renders the onboard surface → Test `test_admin_fleet_libraries_renders_with_scope`
- **Dimension 2.3** — the server action wraps the client in `withToken` and maps `ApiError` to `{ok:false, errorCode}` → Test `test_platform_onboard_action_maps_apierror`

### §3 — Onboard dialog: validation, outcomes, analytics

The form an operator actually touches. Mirrors the tenant dialog's zod pattern, requestId guard, and `presentError` flow; on success it renders the returned entry (id, content hash, requirements) as the page's result card instead of refreshing a gallery.

- **Dimension 3.1** — malformed `source_ref` (`"notarepo"`, `""`) shows an inline error; the action is never invoked → Test `test_platform_onboard_blocks_bad_source_ref`
- **Dimension 3.2** — a successful onboard closes the dialog and renders the entry's id + content hash → Test `test_platform_onboard_success_renders_entry`
- **Dimension 3.3** — a failed onboard keeps the dialog open with the mapped `UZ-…` presentation → Test `test_platform_onboard_failure_surfaces_mapped_error`
- **Dimension 3.4** — success emits `platform_library_onboarded` with allowed props only → Test `test_platform_onboard_emits_analytics_event`

### §4 — Nav discoverability

Operators find the surface the same way they find Runners and Model library. **Implementation default:** label "Fleet libraries", gated on `SCOPE.PLATFORM_LIBRARY_WRITE` (no read rung exists); icon is the agent's pick from lucide, consistent with neighbors.

- **Dimension 4.1** — `PLATFORM_NAV` carries the entry; the nav renders it only for a scope-holding session → Test `test_nav_fleet_libraries_scope_gated`

### §5 — e2e acceptance (user-centric)

Proves the real path: real Next.js app, real agentsfleetd, real Clerk session. **Implementation default:** a dedicated operator fixture user — the existing clerk-admin helper PATCHes `public_metadata.scopes` to include `platform-library:write`; the standard fixture user stays scope-free so existing nav assertions hold.

- **Dimension 5.1** — standard fixture user: no "Fleet libraries" nav entry; direct visit to `/admin/fleet-libraries` lands on the settings notice → Test `test_e2e_platform_onboarding_scope_gated`
- **Dimension 5.2** — operator fixture user: page renders; onboarding a nonexistent repo shows the mapped error inline, dialog stays open → Test `test_e2e_platform_onboarding_error_path`
- **Dimension 5.3** — operator fixture user: onboarding the sample repo succeeds and the entry appears in a workspace's fleet-library gallery; re-running upserts (no duplicate card) → Test `test_e2e_platform_onboarding_success_visible_in_gallery`

## Interfaces

```
POST /v1/admin/fleet-libraries          (backend-owned; UI conforms, never changes it)
  Authorization: Bearer <Clerk api-template JWT carrying platform-library:write>
  { "source_kind": "github", "source_ref": "agentsfleet/github-pr-reviewer" }
  → 201 { "id", "name", "visibility": "platform", "content_hash",
          "requirements", "support_files" }
  → 403 UZ-AUTH-022 (scope missing) · importer UZ-… codes on invalid bundles

SCOPE.PLATFORM_LIBRARY_WRITE = "platform-library:write"   (lib/auth/scopes.ts)
EVENTS.platform_library_onboarded                          (lib/analytics/events.ts)
  props: { source_kind: string; outcome: string; entry_id?: string }
Route: /admin/fleet-libraries · Nav label: "Fleet libraries" (PLATFORM_NAV)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Scope missing (page) | non-operator hits the URL | server redirect to the settings notice; nothing rendered → `test_admin_fleet_libraries_redirects_without_scope` |
| Scope missing (action) | scope revoked mid-session | backend 403 `UZ-AUTH-022`; dialog shows the mapped presentation → `test_platform_onboard_failure_surfaces_mapped_error` |
| Session expired | 401 from backend | page path redirects to sign-in; action path returns `{ok:false}` with code → `test_platform_onboard_action_maps_apierror` |
| Invalid bundle | repo lacks root SKILL.md, oversize, secret-shaped content | importer `UZ-…` code; dialog stays open with the message → `test_e2e_platform_onboarding_error_path` |
| Malformed input | `source_ref` not owner/repo | zod inline error; no network call → `test_platform_onboard_blocks_bad_source_ref` |
| Network blip | fetch failure in the server action | `presentError` retryable presentation; dialog stays open → `test_platform_onboard_failure_surfaces_mapped_error` |
| Double submit | rapid re-click while pending | requestId guard drops the stale response; single result rendered → `test_platform_onboard_success_renders_entry` |
| Replay / re-onboard | same repo onboarded twice | backend upserts (`ON CONFLICT (id) DO UPDATE`); success both times, one gallery card → `test_e2e_platform_onboarding_success_visible_in_gallery` |

## Invariants

1. The scope string appears exactly once in the UI codebase — in `scopes.ts`; every consumer imports `SCOPE.PLATFORM_LIBRARY_WRITE` — enforced by the UFS gate + a grep assertion in the page unit test (`platform-library:write` matches only scopes.ts).
2. The browser never holds the api-audience token — the onboard call happens only in a `"use server"` action via `withToken` — enforced by module directive; the client bundle has no import of `lib/api/client`.
3. UI gating is defence-in-depth only; the backend `requireScope` stays the authoritative gate — enforced by the backend (unchanged in this diff), asserted end-to-end by `test_e2e_platform_onboarding_scope_gated`.
4. The new analytics event carries only registry-listed props — enforced by `EVENT_PROP_KEYS … satisfies` (compile-time) + the existing Personally Identifiable Information (PII) emit-path test.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `platform_library_onboarded` | product | operator submits the onboard dialog (success or failure) | `source_kind`, `outcome`, `entry_id` (success only) | no repo free-text beyond the ref-derived entry id; no tokens, no markdown content | `test_platform_onboard_emits_analytics_event` |

Nav-click telemetry for the new entry flows through the existing `navSource` derivation — no event renamed or removed; existing `fleet_library_onboarded` (tenant) is untouched. No analytics/funnel playbook update required: the event is a single new action counter, not a funnel change.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_platform_library_scope_independent` | constant equals the wire string; `expandScopes(["model:admin","runner:write"])` does not contain it; holding it verbatim does |
| 1.2 | unit | `test_onboard_client_posts_admin_endpoint` | `({github, owner/repo}, token)` → POST `/v1/admin/fleet-libraries`, that JSON body, bearer header |
| 2.1 | unit | `test_admin_fleet_libraries_redirects_without_scope` | scopeless session → redirect to the notice URL; backend 401 → `/sign-in` |
| 2.2 | unit | `test_admin_fleet_libraries_renders_with_scope` | scope-holding session → view rendered with the onboard trigger |
| 2.3 | unit | `test_platform_onboard_action_maps_apierror` | endpoint 403 `UZ-AUTH-022` → `{ok:false, errorCode:"UZ-AUTH-022"}` |
| 3.1 | unit | `test_platform_onboard_blocks_bad_source_ref` | `"notarepo"` / `""` → inline zod error, action not invoked |
| 3.2 | unit | `test_platform_onboard_success_renders_entry` | mocked 201 → dialog closes, result card shows id + content hash; stale requestId response dropped |
| 3.3 | unit | `test_platform_onboard_failure_surfaces_mapped_error` | `{ok:false, errorCode}` → dialog open, `presentError` title/body/code visible |
| 3.4 | unit | `test_platform_onboard_emits_analytics_event` | success → one capture with exactly the registry props |
| 4.1 | unit | `test_nav_fleet_libraries_scope_gated` | operatorScopes with/without the scope → entry present/absent in `PLATFORM_NAV` filtering |
| 5.1 | e2e | `test_e2e_platform_onboarding_scope_gated` | standard fixture user: nav lacks the entry; direct URL lands on settings notice |
| 5.2 | e2e | `test_e2e_platform_onboarding_error_path` | operator fixture user: nonexistent repo → mapped error stays inline |
| 5.3 | e2e | `test_e2e_platform_onboarding_success_visible_in_gallery` | operator onboards the sample repo → success card; entry renders in a workspace gallery; second run upserts, one card |

Regression: existing app suites (`tests/admin-models-page.test.ts`, nav tests, tenant `AddLibraryDialog` tests) pass unchanged — the tenant onboard path and gallery are untouched. Idempotency: Dimension 5.3 covers replay/upsert.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Operator onboards from the dashboard; entry visible in a workspace gallery (§3, §5) | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts platform-library-onboarding.spec.ts` | exit 0 | P0 | |
| R2 | Non-operators never see the surface (§2, §4) | `cd ui/packages/app && bunx vitest run tests/admin-fleet-libraries-page.test.ts` | exit 0, contains `redirects_without_scope` | P0 | |
| R3 | Scope string single-sourced (§1) | `grep -rn "platform-library:write" ui/packages/app --include='*.ts' --include='*.tsx' -l \| grep -v node_modules \| grep -v scopes.ts \| grep -cv test` | `0` | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit + coverage lanes pass | `make test-unit-app && make test-coverage-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Platform catalog list/browse on the admin page — needs a `GET /v1/admin/fleet-libraries` backend route; backend surface gets its own spec (write_spec authoring discipline). Until then the page shows the onboard affordance + last result only.
- Platform entry delete/deprecate — no backend route exists.
- `agentsfleet` CLI verb for platform onboarding — no demonstrated operator demand; the dashboard is the surface.
- Clerk scope provisioning automation — stays manual per `docs/AUTH.md` §Manually-provisioned.
- Any change to the tenant onboarding flow, gallery, or install flow.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy opens "Fleet libraries" in the Configuration nav, pastes `agentsfleet/github-pr-reviewer`, clicks Onboard, and sees the entry card with its content hash; opening any workspace's Fleets → New shows github-pr-reviewer installable.
2. **Preserved user behaviour** — the tenant "Create fleet library" dialog, workspace gallery, install flow, and the two existing admin pages keep working unchanged.
3. **Optimal-way check** — most direct shape: reuse the existing endpoint and compose two proven UI patterns. Gap to unconstrained-optimal: no catalog table on the page (list endpoint missing); acceptable — the gallery is the verification surface.
4. **Rebuild-vs-iterate** — iterate; this is additive UI over stable backend + established page patterns.
5. **What we build** — one scope constant, one client function, one page (+action/loading/view/dialog), one nav entry, one analytics event, unit + e2e tests.
6. **What we do NOT build** — admin list route (backend follow-up), delete/deprecate (no route), CLI verb (no demand), scope-provisioning UI (Clerk-manual by design).
7. **Fit with existing features** — compounds M110 tenant onboarding and the M112 library rename; must not destabilize the `/w/[workspaceId]/fleets/new` install flow.
8. **Surface order** — UI-first; the API already exists and this spec is precisely its deferred UI. CLI rejected in item 6.
9. **Dashboard restraint** — no catalog table, counters, or usage claims until the list endpoint is real; the page shows the onboard form and the last onboard's actual API result, nothing invented.
10. **Confused-user next step** — a non-operator landing on the page gets the settings notice naming the operator requirement; dialog failures carry the `UZ-…` code with `presentError`'s suggestion; the form's "Learn more" links the library-authoring doc.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, one PR — a thin UI over an existing endpoint; Sections split registry plumbing → page guard → dialog → nav → e2e so each Dimension lands with its test.
- **Alternatives considered:** (a) full catalog-management page with a new backend list/delete surface — rejected: backend routes carry a different review profile and get their own spec; (b) a tier toggle inside the tenant AddLibraryDialog — rejected: mixes operator and workspace authorization on one surface and hides an admin action inside a tenant flow.
- **Patch-vs-refactor verdict:** this is a **patch** because it adds a parallel instance of established patterns without reshaping any of them; the follow-up list endpoint + catalog view is the named larger step if operators need browse/audit.

## Discovery (consult log)

- **Consults** —
- **Metrics review** —
- **Skill-chain outcomes** —
- **Deferrals** —
