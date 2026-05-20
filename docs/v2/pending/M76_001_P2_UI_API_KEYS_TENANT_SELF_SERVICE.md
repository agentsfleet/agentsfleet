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

# M76_001: Settings self-service — API keys, account deletion, avatar fallback, theme toggle

**Prototype:** v2.0.0
**Milestone:** M76
**Workstream:** 001
**Date:** May 18, 2026
**Status:** PENDING
**Priority:** P2 — backend already ships for API keys; closing the operator-experience gap across the settings surface (API-key mint/revoke today requires `curl`; no account-deletion flow; no avatar fallback control; no light/dark toggle despite the tokens existing).
**Categories:** UI, API, DOCS, AUTH
**Batch:** B1 — no parallel siblings; the settings surface.
**Branch:** {feat/m76-001-name — added at CHORE(open)}
**Depends on:** API-keys §1–§7 depend on nothing (handlers + `core.api_keys` + `bearer_or_api_key` already in place). **§8 account deletion depends on new backend** (tenant-lifecycle delete endpoint + scheduled hard-delete + Clerk `user.deleted` webhook wiring — none exist today) and touches `src/http/handlers/auth/`, which **M74_002 owns** — sequence after/with M74_002 or graduate §8 to its own spec.
**Provenance:** human-written (Captain ack, May 18, 2026). **Scope expanded May 20, 2026** (Captain ask) from API-keys-only to the broader settings self-service surface: §8 account deletion, §9 avatar fallback, §10 light/dark theme toggle. See the §8 callout — account deletion is a backend-heavy, billing/auth-sensitive feature and is a strong candidate to graduate to its own spec (same rationale that split API-keys out of M71_001 P2).

**Canonical architecture:** `docs/ARCHITECTURE.md` (settings dashboard surface, tenant principal scope).

---

## Implementing agent — read these first

1. `src/http/handlers/api_keys/tenant.zig` — canonical mint/revoke/delete handler. Note `KEY_PREFIX = "zmb_t_"`, `MAX_NAME_LEN = 64`, `isValidKeyName` (alphanumeric + `-_`), and that the raw key is returned **only** in the create response.
2. `src/http/handlers/api_keys/list.zig` — pagination contract (`page`, `page_size` ≤ 100), `sort` allowlist (`created_at|-created_at|key_name|-key_name`), row shape including `last_used_at` + `revoked_at`.
3. `ui/packages/app/app/(dashboard)/credentials/page.tsx` (+ `components/`, `actions.ts`) — mirror this layout for the new `/settings/api-keys` page: server-rendered list, server-action mutations, dialog-driven create, optimistic UI.
4. `ui/packages/app/app/(dashboard)/settings/page.tsx` — add the new settings card here using the existing `SettingsLink` shape; do not invent a parallel index.
5. `ui/packages/app/lib/api/tenant_provider.ts` — pattern for a typed server-only API client wrapping a `/v1/...` endpoint with token + workspace context.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal; pay attention to RULE NDC, RULE NLR, RULE TST-NAM.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — no new endpoints are introduced; if the spec needs a new route, that's a scope violation.
- **`docs/AUTH.md`** — the new UI consumes endpoints behind `operator()` gating; the page guard must mirror that role check rather than fail at the API boundary.
- **`docs/ZIG_RULES.md`** — only applies to the one comment edit in `src/http/handlers/api_keys/tenant.zig` (drop the "no first-party UI" sentence). No new Zig logic.
- **No new schema.** `core.api_keys` already carries every column we render (`id`, `key_name`, `description`, `active`, `created_at`, `last_used_at`, `revoked_at`).

---

## Overview

**Goal (testable):** From `/settings/api-keys`, an operator-role tenant user can mint a `zmb_t_*` API key (raw value revealed exactly once and copyable), list all keys with status + last-used timestamp, revoke an active key, and delete an already-revoked key — entirely through the dashboard, with zero `curl` use, gated by RBAC role at the page and action layer.

**Problem:** Today the only way to obtain a tenant API key is `POST /v1/api-keys` with a Clerk session bearer. The `src/http/handlers/api_keys/tenant.zig` module documents itself as "Operational/bootstrap-only surface today. No first-party UI/CLI consumes these routes." Operators cannot rotate or revoke keys without shell access; the `bearer_or_api_key` substitution path that this token type was built to enable is unreachable for normal product users.

**Solution summary:** Add a `/settings/api-keys` route under the existing dashboard shell. Render a list of the tenant's keys with name, status, created/last-used/revoked timestamps. A "New API key" dialog calls a server action that proxies to `POST /v1/api-keys`, then reveals the raw `zmb_t_…` exactly once in a copy-to-clipboard panel with a "Done — I've stored it" confirm. Revoke and delete buttons hit the existing PATCH/DELETE endpoints with confirm modals. The settings index gains a third card linking to the new page. The backend stays untouched except for a comment-block correction in `tenant.zig`.

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
| **§8 account deletion (new backend — graduate candidate)** | | |
| `ui/packages/app/app/(dashboard)/settings/account/{page,actions}.tsx` + `components/DeleteAccountDialog.tsx` | CREATE | Danger Zone + confirm-by-typing dialog + billing pre-flight. |
| `src/http/handlers/tenants/lifecycle.zig` (+ route_table/router wiring) | CREATE | `DELETE /v1/tenants/me` — soft-delete (`deleted_at`), disable zombies/triggers, revoke API keys; returns scheduled hard-delete date. |
| scheduled hard-delete worker | CREATE | Cascade tenant→workspaces→zombies→credentials→events→billing after grace; delete Clerk user. |
| `src/http/handlers/auth/identity_events_clerk.zig` | EDIT (M74_002 territory) | Add `user.deleted` branch → soft-delete cascade. Coordinate with M74_002. |
| **§9 avatar fallback** | | |
| `ui/packages/app/lib/clerkAppearance.ts` (+ test) | EDIT | Theme the Clerk default initials avatar with design-system tokens (or an identicon component if chosen). |
| **§10 theme toggle** | | |
| `ui/packages/app/app/layout.tsx` + a `ThemeToggle.tsx` + cookie helper | EDIT/CREATE | SSR-stamp `data-theme` from cookie; client toggle writes cookie + flips attribute. No new color tokens (`[data-theme="light"]` already exists). |

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

### §8 — Account deletion ("Danger Zone")

> ⚠️ **Graduate-to-own-spec candidate.** Unlike §1–§7 (UI over existing endpoints), §8 needs **new backend** (tenant-lifecycle delete endpoint + scheduled hard-delete job + the currently-ignored Clerk `user.deleted` webhook) and a **billing policy decision**. It also touches `src/http/handlers/auth/` (M74_002's reserved surface). Recommendation: split §8 into its own milestone once the process below is Captain-approved; keep it here only as the design of record until then.

**Current-state findings (May 20, 2026):**
- **No deletion path exists.** `route_table.zig` has no tenant/account delete route. Clerk's `user.deleted` event is explicitly **ignored** (`src/http/handlers/auth/identity_events_clerk.zig:113-114`), so a user deleting themselves via Clerk today **orphans** their tenant, workspaces, zombies, credentials, events, and billing rows.
- **Billing is prepaid credits — there is no "dues"/arrears model.** `TenantBilling = { balance_nanos, is_exhausted }` (no `owed`/`outstanding` field). An exhausted tenant is already gate-blocked from incurring new charges; Stripe purchase + any postpaid invoice is **v2.1**. So deletion is **not** blocked by debt today.

**Recommended process (the design of record):**

1. **Scope.** "Delete account" = soft-delete the **tenant** (owner/admin only) + delete the **Clerk user**. Tenants are single-user at alpha, so the two coincide; defer multi-member org semantics (member-leave vs tenant-delete) until orgs exist.
2. **Billing gate.**
   - **Today (prepaid):** do **not** block on balance. If `balance_nanos > 0`, show a forfeiture warning ("X credits will be forfeited — free-trial credits have no cash value"). No auto-refund.
   - **v2.1 (Stripe/postpaid):** **block** deletion while an invoice is unsettled — "Settle your outstanding balance before deleting." The grace period (below) is the settlement window. Refund of remaining *purchased* credits follows the published refund policy.
3. **Re-auth + confirm.** Require a fresh auth check and **confirm-by-typing** the tenant name (or email) before the destructive call — no single-click delete.
4. **Data export first.** Offer a GDPR-portability export (at minimum events + billing-charge history) before the point of no return.
5. **Soft-delete + grace period (default 30 days).** On confirm, the server marks the tenant `deleted_at`, **immediately** disables every zombie + trigger (stop serving/incurring) and revokes all tenant API keys, but **retains data**. Email the user a confirmation with a cancel-within-grace link.
6. **Hard-delete after grace.** A scheduled job cascades tenant → workspaces → zombies → credentials → events → billing rows, then deletes the Clerk user via the Clerk backend API.
7. **Webhook reconciliation.** Wire the currently-ignored Clerk `user.deleted` webhook → the same server-side soft-delete cascade, so a Clerk-initiated deletion is consistent with the in-app flow. (This closes the orphan gap above regardless of §8's UI.)

**UI surface:** a de-emphasised "Danger Zone" card at the bottom of an account/settings section, owner/admin-gated (mirrors §6 RBAC). The destructive button opens a confirm-by-typing dialog showing the forfeiture/settlement state from the billing pre-flight.

**New backend (out of M76's "no new endpoints" rule — hence the graduate flag):** `DELETE /v1/tenants/me` (soft-delete + return scheduled hard-delete date), a scheduled hard-delete worker, and a `user.deleted` branch in `identity_events_clerk.zig`. All auth-adjacent → coordinate with M74_002.

### §9 — Avatar fallback

**Current state:** the top-right avatar is Clerk's `<UserButton>` (`AuthUserButton` in `components/layout/Shell.tsx:119`), themed by `AUTH_APPEARANCE`. With no uploaded image, Clerk renders its **default initials avatar** (the user's initials on a solid fill) — **not** a GitHub-style identicon. There is no first-party fallback in our code.

**What lands:** a deliberate decision + a pinned test, not a guess. Default recommendation: **keep Clerk's initials avatar** (consistent with the auth widget, zero new deps) and only theme it via `AUTH_APPEARANCE` so the fallback fill uses `--surface-2` / `--text` tokens rather than Clerk's stock palette. If a GitHub-style generated identicon is desired instead, that's an explicit upgrade (deterministic hash → pattern/gradient keyed on user id); spec it as an alternative, don't ship both. Acceptance: a test asserts the rendered fallback uses design-system tokens (or the identicon component if chosen), so the "what do I see with no avatar" answer is pinned, not incidental.

### §10 — Light/dark theme toggle

**Current state:** the design system **already ships both palettes** — dark is the default (`:root` in `tokens.css`), light is fully defined under `[data-theme="light"]` (`tokens.css:201`). But **nothing sets `data-theme`** — no `next-themes`, no provider, no toggle — so light mode is currently **dormant/unreachable**. The token work is done; only the wiring is missing.

**What lands:** a theme toggle (a control in the header or settings) that sets `data-theme` on `<html>`, persisted (cookie so SSR renders the right palette with no flash; `localStorage` mirror optional), defaulting to `prefers-color-scheme` on first visit. **Implementation default:** a cookie read in the root layout that stamps `data-theme` server-side (avoids the light-flash-on-dark FOUC), plus a small client toggle that writes the cookie and flips the attribute. No new color tokens — `[data-theme="light"]` already covers the surface. Acceptance: toggling flips `<html data-theme>`, the choice survives reload (cookie), and SSR markup matches the persisted theme (no hydration mismatch).

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
| **§8 account deletion** | |
| `test_delete_requires_confirm_typing` | Destructive button stays disabled until the tenant name is typed exactly. |
| `test_delete_billing_preflight_warns_on_credits` | `balance_nanos > 0` → forfeiture warning shown; deletion still allowed (prepaid, v2.0). |
| `test_delete_blocks_on_unsettled_balance` (v2.1) | Unsettled invoice → deletion blocked with "settle first" (gated behind the postpaid flag). |
| `test_delete_soft_deletes_and_disables` | `DELETE /v1/tenants/me` sets `deleted_at`, disables zombies/triggers, revokes API keys, returns hard-delete date. |
| `test_clerk_user_deleted_webhook_cascades` | A `user.deleted` event triggers the same soft-delete cascade (no orphaned tenant). |
| `test_non_owner_cannot_delete` | Non-owner/admin role → Danger Zone hidden + endpoint 403. |
| **§9 avatar fallback** | |
| `test_avatar_fallback_uses_tokens` | With no image, the rendered Clerk fallback (or identicon, if chosen) uses design-system tokens — pins "what you see with no avatar". |
| **§10 theme toggle** | |
| `test_theme_toggle_flips_data_theme` | Toggle sets `<html data-theme="light">`/removes it. |
| `test_theme_persists_across_reload` | Cookie persists the choice; SSR markup matches (no hydration mismatch / no FOUC). |

Regression: existing `settings-provider.spec.ts`, `settings-billing.spec.ts`, `signout-and-signin.spec.ts` still pass — settings shell layout did not change beyond the new card.

---

## Acceptance Criteria

- [ ] `/settings/api-keys` route renders for operator+ roles — verify: `bun run test --filter settings-api-keys`
- [ ] One-time-reveal invariant holds in Playwright — verify: `bun playwright test tests/e2e/acceptance/settings-api-keys.spec.ts`
- [ ] No raw-key string survives the reveal close — verify: included in the e2e assertion above
- [ ] Comment block in `src/http/handlers/api_keys/tenant.zig` no longer claims "no first-party UI" — verify: `grep -n "No first-party UI" src/http/handlers/api_keys/tenant.zig` returns 0 matches
- [ ] `make lint` clean
- [ ] `make test` passes (ui + zig)
- [ ] `make test-integration` passes (the existing `tenant_integration_test.zig` still asserts the HTTP surface unchanged)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` (the one Zig comment edit must not break either target)
- [ ] `gitleaks detect` clean
- [ ] No file over 350 lines added
- [ ] No new HTTP route appears in `src/http/router.zig` or `src/http/route_table.zig` — verify: `git diff origin/main -- src/http/router.zig src/http/route_table.zig` is empty

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
| UI unit tests | `bun --filter @usezombie/app run test` | {paste} | |
| Playwright e2e | `bun --filter @usezombie/app run e2e settings-api-keys.spec.ts` | {paste} | |
| Zig build | `zig build` | {paste} | |
| Cross-compile linux x86_64 | `zig build -Dtarget=x86_64-linux` | {paste} | |
| Cross-compile linux aarch64 | `zig build -Dtarget=aarch64-linux` | {paste} | |
| Comment correction | `grep -n "No first-party UI" src/http/handlers/api_keys/tenant.zig` | {paste, expect 0} | |
| No new routes | `git diff origin/main -- src/http/router.zig src/http/route_table.zig` | {paste, expect empty} | |
| Gitleaks | `gitleaks detect` | {paste} | |
| 350L gate | `wc -l` audit | {paste} | |

---

## Out of Scope

- **Workspace-scoped API keys** — the `api` zombie trigger variant (`src/zombie/config_helpers.zig:31`) requires a workspace-scoped surface; that is a separate milestone and intentionally not addressed here. This spec covers tenant-scoped keys only (the existing `zmb_t_*` surface).
- **Agent-scoped API keys** — `src/http/handlers/api_keys/agent.zig` exists as a separate surface; its UI is a follow-up.
- **Per-user filtering** — every `operator+` user in the tenant sees every tenant key. A "my keys only" filter and lowering the route to `user` role with `created_by`-scoped reads is a follow-up worth scoping after operator UX lands.
- **Rotation as a single action** — the backend has no atomic rotate; the UI exposes mint + revoke separately. A future "rotate" affordance (mint new, prompt to revoke old) is deferred.
- **Audit log surface** — `last_used_at` is rendered, but per-request audit (which IP, which route) is not surfaced. Defer to the broader observability surface.
- **CLI consumer** — `zombiectl` does not yet authenticate with a tenant API key; CLI auth is owned by the M74_002 worktree.
