<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
- See docs/TEMPLATE.md "Prohibited" section above for canonical list.
-->

# M76_001: Settings self-service — API keys, avatar fallback, theme toggle

**Prototype:** v2.0.0
**Milestone:** M76
**Workstream:** 001
**Date:** May 18, 2026
**Status:** DONE
**Priority:** P2 — backend already ships for API keys; closing the operator-experience gap across the settings surface (API-key mint/revoke today requires `curl`; no avatar fallback control; no light/dark toggle despite the tokens existing).
**Categories:** API, AUTH, DOCS, UI
**Batch:** B1 — no parallel siblings; the settings surface.
**Branch:** feat/m76-001-settings-self-service
**Depends on:** Nothing — API-keys §1–§7 (handlers + `core.api_keys` + `bearer_or_api_key`), §9 avatar (Clerk appearance), and §10 theme (`[data-theme="light"]` palette) all ship over surfaces already in place. Account deletion graduated to **M76_002** (new backend + billing decision).
**Provenance:** human-written (Captain ack, May 18, 2026). **Scope expanded May 20, 2026** then **narrowed May 21, 2026** (Captain ask): ship API keys + §9 avatar fallback + §10 theme toggle here; **account deletion graduated to M76_002** (backend-heavy, billing/auth-sensitive — same rationale that split API-keys out of M71_001 P2).

**Canonical architecture:** `docs/ARCHITECTURE.md` (settings dashboard surface, tenant principal scope).

---

## Implementing agent — read these first

1. `src/http/handlers/api_keys/tenant.zig` — canonical mint/revoke/delete handler. Note `KEY_PREFIX = "zmb_t_"`, `MAX_NAME_LEN = 64`, `isValidKeyName` (alphanumeric + `-_`), and that the raw key is returned **only** in the create response.
2. `src/http/handlers/api_keys/list.zig` — pagination contract (`page`, `page_size` ≤ 100), `sort` allowlist (`created_at|-created_at|key_name|-key_name`), row shape including `last_used_at` + `revoked_at`.
3. `ui/packages/app/app/(dashboard)/credentials/page.tsx` (+ `components/`, `actions.ts`) — mirror this layout for the new `/settings/api-keys` page: server-rendered list, server-action mutations, dialog-driven create, optimistic UI.
4. `ui/packages/app/app/(dashboard)/settings/page.tsx` — add the new settings card here using the existing `SettingsLink` shape; do not invent a parallel index.
5. `ui/packages/app/lib/api/tenant_provider.ts` — pattern for a typed server-only API client wrapping a `/v1/...` endpoint with token + workspace context.

---

## PR Intent & comprehension handshake

> The bridge from spec to merged PR — the agent confirms intent before writing code.

- **PR title (eventual):** Settings self-service: dashboard API-key management + theme/avatar
- **Intent (one sentence):** Operators manage `zmb_t_*` API keys (mint / list / revoke / delete) from the dashboard with zero `curl`, plus a working light/dark toggle and a themed avatar fallback — closing the operator-experience gap on the settings surface.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent in your own words and list the assumptions you proceed on (`ASSUMPTIONS I'M MAKING: …`). The load-bearing one: this PR is **UI over existing endpoints + one comment-only Zig edit + theme/avatar wiring** — zero new HTTP routes, zero new schema, zero new error codes. Account deletion (new backend + billing-policy decision + auth-surface coordination) is **out of scope here and tracked in M76_002**. A mismatch with the Intent above → STOP and reconcile before any edit.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal; pay attention to RULE NDC, RULE NLR, RULE TST-NAM.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — no new endpoints are introduced; if the spec needs a new route, that's a scope violation.
- **`docs/AUTH.md`** — the new UI consumes endpoints behind `operator()` gating; the page guard must mirror that role check rather than fail at the API boundary.
- **`docs/ZIG_RULES.md`** — only applies to the one comment edit in `src/http/handlers/api_keys/tenant.zig` (drop the "no first-party UI" sentence). No new Zig logic.
- **No new schema.** `core.api_keys` already carries every column we render (`id`, `key_name`, `description`, `active`, `created_at`, `last_used_at`, `revoked_at`).

---

## Applicable Gates

> Which Action-Triggered Guards this PR trips, and how each stays clean. Blast radius: new `*.tsx`/`*.ts` under `ui/packages/app/`, one comment-only `*.zig` edit, an `e2e` `*.ts` spec. No new schema, no new HTTP route.

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes (comment-only) | the `tenant.zig` edit is a comment correction — no logic; still cross-compile both linux targets to confirm no break. |
| PUB / Struct-Shape | no | the `*.zig` edit adds no `pub`; UI is TypeScript. |
| File & Function Length (≤350/≤50/≤70) | yes | the surface is split into list / create-dialog / revoke-confirm / loading components (see Files Changed) so no single `.tsx` approaches the cap. |
| UFS (repeated/semantic literals) | yes | share the `zmb_t_` prefix, the `[A-Za-z0-9_\-]{1,64}` name regex, and the sort allowlist verbatim with the Zig handler's constants; the `ERR_*`→toast map is named once. |
| UI Substitution / DESIGN TOKEN | yes | use design-system primitives (`asChild` for HTML semantics) and `theme.css` tokens — no raw HTML, no arbitrary values; §10 reuses the existing `[data-theme="light"]` palette (no new color tokens). |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING: yes | the raw key is secret from the network boundary — no `console.log(result/key)` in `actions.ts` (Invariant 2). ERROR REGISTRY: no (consumes existing `ERR_*`, mints none — new codes graduate with M76_002). SCHEMA: no (existing `core.api_keys`). LIFECYCLE: no. |

---

## Overview

**Goal (testable):** From `/settings/api-keys`, an operator-role tenant user can mint a `zmb_t_*` API key (raw value revealed exactly once and copyable), list all keys with status + last-used timestamp, revoke an active key, and delete an already-revoked key — entirely through the dashboard, with zero `curl` use, gated by RBAC role at the page and action layer.

**Problem:** Today the only way to obtain a tenant API key is `POST /v1/api-keys` with a Clerk session bearer. The `src/http/handlers/api_keys/tenant.zig` module documents itself as "Operational/bootstrap-only surface today. No first-party UI/CLI consumes these routes." Operators cannot rotate or revoke keys without shell access; the `bearer_or_api_key` substitution path that this token type was built to enable is unreachable for normal product users.

**Solution summary:** Add a `/settings/api-keys` route under the existing dashboard shell. Render a list of the tenant's keys with name, status, created/last-used/revoked timestamps. A "New API key" dialog calls a server action that proxies to `POST /v1/api-keys`, then reveals the raw `zmb_t_…` exactly once in a copy-to-clipboard panel with a "Done — I've stored it" confirm. Revoke and delete buttons hit the existing PATCH/DELETE endpoints with confirm modals. The settings index gains a third card linking to the new page. The backend stays untouched except for a comment-block correction in `tenant.zig`.

---

## Prior-Art / Reference Implementations

> Mirror the existing dashboard pattern — don't invent a new page shape.

- **UI** → `ui/packages/app/app/(dashboard)/credentials/page.tsx` (+ `components/`, `actions.ts`): mirror this layout for `/settings/api-keys` — server-rendered list, server-action mutations, dialog-driven create, optimistic UI. Typed server-only client follows `lib/api/tenant_provider.ts`. Build from design-system primitives + `theme.css` tokens; §10's toggle reuses the existing `[data-theme="light"]` palette in `tokens.css`.
- **Backend contract** → `src/http/handlers/api_keys/tenant.zig` + `list.zig` — the endpoints the UI consumes (mint/revoke/delete + pagination/sort allowlist), unchanged.
- **Alignment:** mirror `/credentials` verbatim. **Divergence:** the one-time secret-reveal panel is new UX with no in-repo analog — design it deliberately (locked dismissal, no DOM persistence after close), per Invariants 1.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/app/(dashboard)/settings/api-keys/page.tsx` | CREATE | Server-rendered list + mint/revoke/delete affordances. |
| `ui/packages/app/app/(dashboard)/settings/api-keys/actions.ts` | CREATE | Server actions wrapping POST/PATCH/DELETE `/v1/api-keys`. |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/ApiKeyList.tsx` | CREATE | List rendering, sort dropdown, pagination controls. |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog.tsx` | CREATE | Mint dialog with one-time secret reveal + clipboard handling. |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/RevokeConfirm.tsx` | CREATE | Two-step revoke/delete confirms. |
| `ui/packages/app/app/(dashboard)/settings/api-keys/loading.tsx` | CREATE | Skeleton consistent with `/credentials` and `/settings/provider`. |
| `ui/packages/app/app/(dashboard)/settings/page.tsx` | EDIT | Add a third `SettingsLink` row for "API keys". |
| `ui/packages/app/lib/api/api_keys.ts` | CREATE | Typed server-only client: `listApiKeys`, `createApiKey`, `revokeApiKey`, `deleteApiKey`. |
| `ui/packages/app/tests/e2e/acceptance/settings-api-keys.spec.ts` | CREATE | Mint → one-time-reveal → revoke → delete round-trip. |
| `src/http/handlers/api_keys/tenant.zig` | EDIT | Comment-only: replace the "Operational/bootstrap-only … No first-party UI/CLI consumes these routes" block with the actual consumer pointer. |
| **§9 avatar fallback** | | |
| `ui/packages/app/lib/clerkAppearance.ts` (+ test) | EDIT | Theme the Clerk default initials avatar with design-system tokens (or an identicon component if chosen). |
| **§10 theme toggle** | | |
| `ui/packages/app/app/layout.tsx`, `components/layout/ThemeToggle.tsx`, `lib/theme.ts` (+ `theme-toggle.test.ts`), `components/layout/Shell.tsx` | EDIT/CREATE | SSR-stamp `data-theme` from cookie; client toggle writes cookie + flips attribute. No new color tokens (`[data-theme="light"]` already exists). |
| **Folded API parity** (Captain override 2026-05-21 — see Discovery + memory `project_m76_folded_parity_scope`; **Zig is source of truth**) | | |
| `ui/packages/app/lib/api/client.ts` (+ `client.test.ts`) | EDIT | Parse the RFC 7807 envelope the backend actually emits (`detail`/`title`/`error_code`/`request_id`), not phantom `error`/`code` — this silently broke every dashboard error map, including M76's own `UZ-APIKEY-*` toasts. |
| `ui/packages/app/lib/api/credentials.ts` (+ test) | EDIT | `created_at` is epoch-ms `number`, not `string`. |
| `ui/packages/app/lib/api/tenant_billing.ts`, `lib/types.ts`, `lib/errors.ts` (+ tests) | EDIT | Add `free_trial` (emitted by Zig, missing from TS); shared type/error-map alignment. |
| `ui/packages/app/app/(dashboard)/settings/provider/page.tsx` (+ tenant_provider type, tests) | EDIT | Remove phantom `synthesised_default`/`error` fields (never emitted) + their dead UI branches. |
| `ui/packages/app/lib/api/zombies.ts`, `app/(dashboard)/zombies/{actions.ts,components/ZombiesList.tsx}` (+ tests) | EDIT | `setZombieStatus` return type; remove phantom `errored` status enum. |
| `public/openapi/{root,components/schemas,paths/authentication,paths/billing}.yaml`, `paths/auth-session-exchange.yaml` (new), regenerated `public/openapi.json` | EDIT/CREATE | `ZombieSummary` +`triggers`/`stopped`; auth `/verify` +`UZ-AUTH-018`, `GET sessions` +401-on-expired; billing `free_trial`. |
| ~8 dashboard test-mock files (`tests/{app-components,app-pages,billing-card,clerk-appearance,provider-selector}.test.ts`, `lib/api/{retry,tenant_billing,tenant_provider,workspaces}.test.ts`) | EDIT | Migrate mocks to the corrected RFC 7807 envelope + epoch-ms shapes. |
| **Test-file splits + coverage determinism** (Captain calls: split not override; build a shared harness) | | |
| `tests/helpers/dashboard-mocks.tsx`, `tests/helpers/dashboard-app-mocks.tsx` | CREATE | Shared mock harness (common + dashboard app-specific) so the monolith test files split into ≤350-line shards via hoist-safe `vi.mock` delegation. |
| `tests/zombies.test.ts` + `zombies-{api-client,routes,install-form}.test.ts` | EDIT/CREATE | Split the 952-line monolith into ≤350 shards. |
| `tests/api-keys-components.test.ts` + `api-keys-create-dialog.test.ts` | EDIT/CREATE | Split the 364-line file. |
| `tests/dashboard-coverage.test.ts` → `dashboard-{placeholder,overview,killswitch,zombies-list,workspace}.test.ts` | DELETE/CREATE | Split the 1277-line monolith into 5 shards; original removed. |
| `ui/packages/app/vitest.config.ts`, `package.json`, root `bun.lock` | EDIT | Coverage provider `v8`→`istanbul` (deterministic; v8 mis-attributes async-React branch coverage at the exact-97% margin — vitest #7660/#9725) + `@vitest/coverage-istanbul@4.1.6` dep; exclude `tests/**` mock harnesses from coverage. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1–§7 (API-keys UI over *existing* endpoints, zero new backend) are the shippable core; §9 (avatar fallback) and §10 (theme toggle) are small, token-complete riders kept here because they're cheap and ship over surfaces already in place. Account deletion was graduated to **M76_002**.
- **Alternatives considered:** (a) keep account deletion inline — rejected; it violates M76's "no new endpoints" rule, needs a billing-policy decision, and couples to M74_002's auth surface, so it became its own spec (M76_002). (b) hold §9/§10 for a separate polish PR — rejected per Captain ask (May 21); they're token-complete and low-risk, so they ride with the API-keys work.
- **Patch-vs-refactor verdict:** §1–§7/§9/§10 are an **additive feature over existing endpoints** (UI + one comment edit + theme/avatar wiring) — a patch in blast-radius terms. The new-backend change lives in M76_002 rather than mud-patching a delete endpoint into this PR.

---

## Sections (implementation slices)

### §1 — Settings index entry + route shell

Land the route directory, the `loading.tsx` skeleton, and the new `SettingsLink` card in `/settings`. Empty list page that just shows the page header and an `EmptyState`. Wired into the dashboard shell's existing breadcrumb/navigation. **Implementation default:** key icon (`lucide-react KeyRoundIcon`) for the settings card, mirroring the sidebar's `/credentials` row, because operators already associate that icon with secret material.

### §2 — Server-only API client

`lib/api/api_keys.ts` mirrors `lib/api/tenant_provider.ts`: takes a Clerk bearer token, returns typed shapes. Never logs or echoes the raw `key` field back through console — it is treated as secret material from the moment it crosses the network boundary. **Implementation default:** parse responses with the same defensive style used in `tenant_provider.ts` (forward-compat on unknown fields).

### §3 — Listing, pagination, sort

Render `key_name`, status badge (`active` / `revoked`), `created_at`, `last_used_at` (or "never used"), `revoked_at` if applicable. Pagination + sort controls respect the backend's allowlist; passing anything outside `created_at|-created_at|key_name|-key_name` shows a "sort not supported" toast rather than blanking the page. **Implementation default:** initial sort `-created_at`, page size 25 — matches `list.zig` defaults; users don't get an extra parameter to misuse on first load.

### §4 — Mint dialog with one-time reveal

The create form takes `key_name` (validated client-side against the same `[A-Za-z0-9_\-]{1,64}` regex enforced server-side in `isValidKeyName`) and an optional `description` (≤ 256 chars). On success, the dialog swaps to a "reveal" state showing the raw `zmb_t_…` value, a copy-to-clipboard button, a warning that it will not be shown again, and a single "I've stored it — close" button that dismisses and refreshes the list. The raw value is **not** rendered into any other DOM node and is **not** logged. **Implementation default:** the reveal panel locks closing-by-overlay-click — only the explicit confirm dismisses, so a stray click cannot lose the value.

### §5 — Revoke + delete confirms

Revoke: PATCH `{active: false}`. Active keys show a "Revoke" button; the confirm modal names the key. Delete: DELETE on already-revoked rows only — the UI mirrors the backend's two-step model (cannot delete an active key; reactivation is not possible). The list refreshes from the server after each mutation; no client-only optimistic updates that could lie if the action failed. **Implementation default:** revoked-but-not-deleted rows stay visible (with `revoked_at` timestamp) so operators can confirm an action landed before they delete; matches the audit-trail expectation Captain has expressed for credential UX.

### §6 — RBAC page guard

The page server-side checks the user's `AuthRole`. `user` role → redirect to `/settings` with a "Contact a tenant operator to manage API keys" toast key in the query string. `operator` or `admin` → render. Mirrors the policy in `src/http/route_table.zig` so a non-operator never sees a button that the backend will reject. **Implementation default:** read the role from the same Clerk JWT path the rest of the dashboard reads — do not introduce a new role-resolution helper.

### §7 — Source comment correction

Replace the "Operational/bootstrap-only surface today … No first-party UI/CLI consumes these routes; if you add one (e.g. self-service key rotation in the dashboard), drop the playbook reference" block in `src/http/handlers/api_keys/tenant.zig` with one referencing `/settings/api-keys` as the first-party consumer. This is the one Zig edit in the spec and must satisfy RULE NLR — the old framing now contradicts shipped reality.

### §9 — Avatar fallback

**Current state:** the top-right avatar is Clerk's `<UserButton>` (`AuthUserButton` in `components/layout/Shell.tsx:119`), themed by `AUTH_APPEARANCE`. With no uploaded image, Clerk renders its **default initials avatar** (the user's initials on a solid fill) — **not** a GitHub-style identicon. There is no first-party fallback in our code.

**What lands:** a deliberate decision + a pinned test, not a guess. Default recommendation: **keep Clerk's initials avatar** (consistent with the auth widget, zero new deps) and only theme it via `AUTH_APPEARANCE` so the fallback fill uses `--surface-2` / `--text` tokens rather than Clerk's stock palette. If a GitHub-style generated identicon is desired instead, that's an explicit upgrade (deterministic hash → pattern/gradient keyed on user id); spec it as an alternative, don't ship both. Acceptance: a test asserts the rendered fallback uses design-system tokens (or the identicon component if chosen), so the "what do I see with no avatar" answer is pinned, not incidental.

### §10 — Light/dark theme toggle

**Current state:** the design system **already ships both palettes** — dark is the default (`:root` in `tokens.css`), light is fully defined under `[data-theme="light"]` (`tokens.css:201`). But **nothing sets `data-theme`** — no `next-themes`, no provider, no toggle — so light mode is currently **dormant/unreachable**. The token work is done; only the wiring is missing.

**What lands:** a theme toggle (a header control) that sets `data-theme` on `<html>`, persisted via cookie so SSR renders the right palette with no flash. **Implementation default:** a cookie read in the root layout stamps `data-theme` server-side (cookie absent → the dark brand default), plus a small client toggle that writes the cookie and flips the attribute. **Dark is the first-visit default — not `prefers-color-scheme`** (amended during EXECUTE): the root `<html>` is shared with the dark-only auth pages (`app/(auth)/`), so auto-switching to an OS light preference flipped sign-in/sign-up to light and broke the established dark auth-theme contract (`auth-theme.spec.ts`). A cookie-less visit therefore renders dark; light is reachable only via the explicit toggle. No client init script is needed — the SSR stamp is authoritative (so no `dangerouslySetInnerHTML`). No new color tokens — `[data-theme="light"]` already covers the surface. Acceptance: toggling flips `<html data-theme>`, the choice survives reload (cookie), SSR markup matches the persisted theme (no hydration mismatch), and a cookie-less visit renders dark.

---

## Interfaces

No new HTTP endpoints. The UI consumes the existing surface verbatim:

```
POST   /v1/api-keys             — body: {key_name, description?}        → 201 {id, key_name, key (raw, ONCE), created_at}
GET    /v1/api-keys             — query: page, page_size, sort           → 200 {items[], total, page, page_size}
PATCH  /v1/api-keys/{id}        — body: {active: false}                  → 200 {id, active:false, revoked_at}
DELETE /v1/api-keys/{id}        — only when active=false                 → 204
```

Error shape is the standard `ec.ERR_*` envelope already produced by the handler (`ERR_APIKEY_NAME_TAKEN`, `ERR_APIKEY_NOT_FOUND`, `ERR_APIKEY_READONLY_FIELD`, `ERR_APIKEY_ALREADY_REVOKED`, `ERR_APIKEY_MUST_REVOKE_FIRST`, `ERR_INVALID_REQUEST`, `ERR_FORBIDDEN`). The UI maps each error code to a user-readable toast — no raw `UZ-…` strings shown to end users, but the `error_code` is included in the toast's data attribute for support triage.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Name collision | User creates a key with an existing name in the tenant | Backend returns `ERR_APIKEY_NAME_TAKEN`; dialog highlights the name field and asks the user to pick a unique one — dialog stays open, no secret was minted. |
| Bootstrap principal | Caller has no `tenant_id` (env-var bootstrap) | Backend returns `ERR_FORBIDDEN`; page guard already redirected; treat as defense-in-depth (page should be unreachable). |
| Reactivation attempt | User somehow PATCHes `{active:true}` | Backend returns `ERR_APIKEY_READONLY_FIELD`; UI never sends this — only `{active:false}`. |
| Delete-while-active | Race: user clicks Delete on a row that's still active | Backend returns `ERR_APIKEY_MUST_REVOKE_FIRST`; toast tells the user, list refresh shows current state. |
| Already-revoked revoke | Race: two operators revoke the same key | Backend returns `ERR_APIKEY_ALREADY_REVOKED`; list refresh resolves visible state. |
| Network failure during reveal | The mint succeeded server-side but the response never reached the client | Dialog shows a "the key may have been created — refresh the list and revoke if you see an unknown name" recovery message. No retry — retrying would mint a second key. |
| Clipboard API blocked | Browser refuses `navigator.clipboard.writeText` | Fall back to a selectable read-only `<input>` plus a "Copy failed — select manually" hint; the reveal remains intact. |
| Non-operator role | `user` role lands on the page via direct URL | Server component redirects to `/settings` with a role-mismatch toast key; no API call is made. |
| Sort param tampering | User crafts a URL with `sort=foo` | API returns `ERR_INVALID_REQUEST`; UI resets to the default sort and shows a "sort reset" toast. |

---

## Invariants

1. **Raw key never persists in the DOM after the reveal dialog closes** — enforced by the `CreateApiKeyDialog` discarding the value in its unmount cleanup and by an `e2e` assertion that the page text no longer contains the prefix after the close button is pressed.
2. **Raw key never appears in logs** — enforced by a server-action lint pattern (no `console.log(result)` / `console.log(key)` in `actions.ts`); covered by a unit-test grep.
3. **Page is unreachable for `user` role** — enforced by a server component guard with a regression test asserting the redirect.
4. **All four mutations re-fetch the list before resolving** — enforced by the server action returning the fresh list payload; a test asserts the list state mirrors backend reality after each action.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_settings_card_links_to_api_keys` | Settings index renders the new card linking to `/settings/api-keys`. |
| `test_user_role_redirected` | A `user`-role principal visiting the page is redirected to `/settings`. |
| `test_operator_role_lists_keys` | An `operator` principal sees the list with `active` + `revoked` rows ordered `-created_at` by default. |
| `test_mint_happy_path_reveals_once` | Dialog → submit → raw `zmb_t_*` value visible exactly once; after close, that string is no longer in the DOM. |
| `test_mint_name_validation_client_side` | Invalid characters in `key_name` block submit and show inline validation. |
| `test_mint_name_collision_keeps_dialog_open` | Server returns `ERR_APIKEY_NAME_TAKEN`; dialog stays open; no key revealed. |
| `test_revoke_active_key` | Active row → revoke → list reflects `revoked_at` populated, row marked inactive. |
| `test_revoke_already_revoked_toast` | Already-revoked row revoke attempt shows `ERR_APIKEY_ALREADY_REVOKED` toast and refreshes list. |
| `test_delete_revoked_key` | Revoked row → delete → row gone from list (`204`). |
| `test_delete_active_key_blocked` | Active row delete attempt shows `ERR_APIKEY_MUST_REVOKE_FIRST` toast and refreshes list. |
| `test_sort_param_invalid_resets` | Direct URL with `sort=foo` resets to default and toasts. |
| `test_pagination_bounds` | `page_size=200` is rejected client-side before request fires. |
| `test_e2e_round_trip` (Playwright) | mint → reveal → close → list shows new row → revoke → delete → list is back to original state. Reveal-secret invariant asserted post-close. |
| **§9 avatar fallback** | |
| `test_avatar_fallback_uses_tokens` | With no image, the rendered Clerk fallback (or identicon, if chosen) uses design-system tokens — pins "what you see with no avatar". |
| **§10 theme toggle** | |
| `test_theme_toggle_flips_data_theme` | Toggle sets `<html data-theme="light">`/removes it. |
| `test_theme_persists_across_reload` | Cookie persists the choice; SSR markup matches (no hydration mismatch / no FOUC). |

Regression: existing `settings-provider.spec.ts`, `settings-billing.spec.ts`, `signout-and-signin.spec.ts` still pass — settings shell layout did not change beyond the new card.

---

## Acceptance Criteria

- [x] `/settings/api-keys` route renders for operator+ roles — `api-keys-page.test.ts` (RBAC redirect on `user` role + operator render)
- [ ] One-time-reveal invariant holds in Playwright — **acceptance lane (post-deploy local→vercel→prod), not a local pre-merge gate**; spec present at `tests/e2e/acceptance/settings-api-keys.spec.ts`
- [x] No raw-key string survives the reveal close — `api-keys-create-dialog.test.ts` (discard-on-close) + new `tests/no-api-key-logging.test.ts` (Invariant 2 grep-gate) + `ph-no-capture` on the reveal panel
- [x] Comment block in `src/http/handlers/api_keys/tenant.zig` no longer claims "no first-party UI" — `grep -c` returns 0
- [x] `make lint-all` clean (zig + website + app + design-system + zombiectl + shell + openapi + schema-gate + gh-actions)
- [x] `make test` passes (ui: 662 tests, istanbul coverage 97.15% branches deterministic over 8 runs)
- [ ] `make test-integration` — **deferred to CI (canonical DB+Redis gate)**; no Zig logic changed this PR (comment-only `tenant.zig`), HTTP surface unchanged
- [x] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` — both OK
- [x] `gitleaks detect` clean (no leaks, 2137 commits)
- [x] No file over 350 lines added
- [x] No new HTTP route appears in `src/http/router.zig` or `src/http/route_table.zig` — diff empty

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Settings UI tests
bun --filter @usezombie/app run test --filter settings-api-keys && echo "PASS" || echo "FAIL"

# E2: Playwright round-trip
bun --filter @usezombie/app run e2e tests/e2e/acceptance/settings-api-keys.spec.ts

# E3: Zig still builds (one comment edit)
zig build && echo "PASS" || echo "FAIL"

# E4: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3

# E5: Comment-block correction landed
grep -n "No first-party UI" src/http/handlers/api_keys/tenant.zig
echo "E5: expected 0 matches"

# E6: No new routes introduced
git diff origin/main -- src/http/router.zig src/http/route_table.zig | head

# E7: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines (limit 350)" }'

# E8: gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

N/A — no files deleted. The one Zig edit is a comment correction; no symbols are removed.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits coverage of the Test Specification — especially the reveal-once invariant and the role-guard redirect. | Skill returns clean; iteration count + coverage summary recorded in Ripley's Log. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review against this spec, `docs/AUTH.md` (role guard), Failure Modes, Invariants (secret never logged, never persists in DOM). | Skill returns clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Comments the PR diff post-rebase. | Comments addressed inline before requesting human review. |

After every push: `kishore-babysit-prs` per CLAUDE.md.

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| UI unit tests + coverage | `bun --filter @usezombie/app run test:coverage` | 659 passed; istanbul deterministic over 8 runs — branches 97.15% (1025/1055), stmts 98.8%, funcs 98.63%, lines 99.57% (all ≥97% gate) | ✅ |
| Playwright e2e | `bun --filter @usezombie/app run e2e settings-api-keys.spec.ts` | Not run this session — needs a live app + Clerk auth (acceptance lane); spec file present. Deferred to CI/live acceptance. | ⏸ |
| Lint (all packages) | `make lint-all` | ✓ All lint checks passed (lint-zig + website + app + design-system + zombiectl + shell + openapi + schema-gate + gh-actions) | ✅ |
| Cross-compile linux x86_64 | `zig build -Dtarget=x86_64-linux` | X86_64_OK | ✅ |
| Cross-compile linux aarch64 | `zig build -Dtarget=aarch64-linux` | AARCH64_OK | ✅ |
| Comment correction | `grep -c "No first-party UI" src/http/handlers/api_keys/tenant.zig` | 0 | ✅ |
| No new routes | `git diff origin/main -- src/http/router.zig src/http/route_table.zig` | empty | ✅ |
| OpenAPI | `make check-openapi` | bundle + lint + error-schema (problem+json) + URL-shape all green | ✅ |
| Gitleaks | `gitleaks detect` | no leaks found (2137 commits) | ✅ |
| 350L gate | `wc -l` audit over changed+untracked source | no file >350 | ✅ |

---

## Discovery (consult log)

**May 18, 2026 — API-keys self-service acked by Indy** (split out of M71_001 P2, same rationale that split API-keys out before). **Scope expanded May 20, 2026** (Indy ask) to the broader settings surface: account deletion, §9 avatar fallback, §10 theme toggle.

**May 21, 2026 — scope narrowed (Indy ask):** "Ship API Keys + Theme Toggle + Avatar all into this PR." Account deletion **graduated to M76_002** (new endpoint + scheduled hard-delete + Clerk `user.deleted` reconciliation + billing-policy decision + M74_002 coordination); its design-of-record (current-state findings + 7-step process) was carried into M76_002 verbatim. This PR keeps §1–§7 (API keys) + §9 (avatar) + §10 (theme) as the shippable core.

**Current-state findings (May 20, 2026), still relevant here:** the design system already ships both palettes (`tokens.css` `:root` dark + `[data-theme="light"]`) but nothing sets `data-theme` — light mode is dormant, only the wiring is missing (§10). The top-right avatar is Clerk's `<UserButton>`; with no image it renders Clerk's stock initials fallback, not a themed one (§9).

**May 21, 2026 — folded API parity sweep (Captain override).** After the split-preference conflict (`feedback_split_security_features`) was surfaced twice, Indy confirmed "fold everything into M76." Folded in (**Zig = source of truth**): (1) `client.ts` RFC 7807 envelope — backend emits `{detail, title, error_code, request_id}`, the client read phantom `error`/`code`, breaking all dashboard error mapping incl. M76's own `UZ-APIKEY-*` toasts; (2) domain type/OpenAPI drift — credentials `created_at` epoch-ms `number`, tenant_billing `free_trial`, tenant_provider phantom `synthesised_default`/`error` removed, zombies `setZombieStatus` return type + phantom `errored` enum removed, `ZombieSummary` +`triggers`/`stopped`; (3) auth OpenAPI gaps — `/verify` +`UZ-AUTH-018`, `GET sessions` +401-on-expired. The general split-preference still stands for future work; this was a one-time, explicitly-acked override.

**May 21, 2026 — test-monolith splits + shared harness (Captain calls).** Four decisions: (1) fold all parity into M76 (above); (2) **remove** provider phantom fields rather than keep+document them; (3) **split** the over-cap test monoliths (not a LENGTH-gate override); (4) build a shared mock harness now. Splits: `zombies.test.ts` (952L), `dashboard-coverage.test.ts` (1277L), `api-keys-components.test.ts` (364L) → ≤350-line shards over `tests/helpers/dashboard{,-app}-mocks.tsx`. Hoist-safe pattern: `vi.mock("mod", async () => (await import("./helpers/…")).fooMock())` resolves to the same instance as the shard's static import. Shards use `userEvent.setup({ delay: null })` — instant typing, immune to the per-test timeout starvation the higher file-parallelism otherwise introduced.

**May 21, 2026 — coverage gate flake → istanbul provider.** Under v8 the app's 97% **branch** gate flaked (1021⇆1024/1055) at zero margin: v8's byte-range→AST remap mis-attributes async-React branch coverage in `ZombieThread.tsx` (covered in isolation, dropped under parallel load — the "more statements / fewer branches" signature of a remap artifact, not a real test race). Confirmed pre-existing (ZombieThread + its tests untouched; flakes even sequentially). Per Indy, switched the **app** coverage provider `v8`→`istanbul` (source-instrumented counters → deterministic): 8/8 runs now 97.15% (1025/1055). Grounded in vitest's own guidance (istanbul is the sanctioned fallback for v8 accuracy edge cases) + issues #7660 (v8 mis-reports React coverage) / #9725. An `act()` test fix was tried first but reverted as redundant once istanbul fixed the root cause — keeping M76 from touching the pre-existing 600-line `zombie-thread.test.ts`. website (85% branch margin) + design-system stay on v8 unless they flake. The 6 pre-existing over-cap test files (`zombie-thread` 600, `approvals-list` 655, `cli-auth-page` 382, `use-zombie-event-stream` 370, …) are **out of M76 scope** — a dedicated test-infra pass.

Further consults logged here during EXECUTE.

---

## Out of Scope

- **Account deletion** — graduated to **M76_002** (tenant soft-delete + scheduled hard-delete + Clerk `user.deleted` reconciliation + billing-policy decision). Not in this PR.
- **Workspace-scoped API keys** — the `api` zombie trigger variant (`src/zombie/config_helpers.zig:31`) requires a workspace-scoped surface; that is a separate milestone and intentionally not addressed here. This spec covers tenant-scoped keys only (the existing `zmb_t_*` surface).
- **Agent-scoped API keys** — `src/http/handlers/api_keys/agent.zig` exists as a separate surface; its UI is a follow-up.
- **Per-user filtering** — every `operator+` user in the tenant sees every tenant key. A "my keys only" filter and lowering the route to `user` role with `created_by`-scoped reads is a follow-up worth scoping after operator UX lands.
- **Rotation as a single action** — the backend has no atomic rotate; the UI exposes mint + revoke separately. A future "rotate" affordance (mint new, prompt to revoke old) is deferred.
- **Audit log surface** — `last_used_at` is rendered, but per-request audit (which IP, which route) is not surfaced. Defer to the broader observability surface.
- **CLI consumer** — `zombiectl` does not yet authenticate with a tenant API key; CLI auth is owned by the M74_002 worktree.
