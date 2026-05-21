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

# M76_002: Account deletion — tenant soft-delete, scheduled hard-delete, Clerk reconciliation

**Prototype:** v2.0.0
**Milestone:** M76
**Workstream:** 002
**Date:** May 21, 2026
**Status:** DEFERRED
**Priority:** P2 — operator self-service account deletion; also closes a latent orphan-data gap (Clerk `user.deleted` is ignored today). Backend-heavy and gated on a billing-policy decision, hence graduated out of M76_001.
**Categories:** API, AUTH, INFRA, UI
**Batch:** B1 — sequence after M76_001 (shares the settings surface) and after the billing-policy decision lands.
**Branch:** {feat/mNN-name — added when work begins}
**Depends on:** M74_002 (merged) — owns the auth/identity-events surface in `src/http/handlers/auth/`; this spec adds a `user.deleted` branch there, so mirror its dispatch shape. Billing-policy decision (Captain) gates §2 behaviour.
**Provenance:** human-written — graduated from M76_001 §8 (Captain ack May 21, 2026: "Ship API Keys + Theme Toggle + Avatar all into this PR" → account deletion splits out). The §8 design-of-record (current-state findings + 7-step process) is carried forward verbatim in Discovery.

**Canonical architecture:** `docs/ARCHITECTURE.md` (tenant lifecycle, principal scope, identity-event reconciliation).

---

## Implementing agent — read these first

1. `src/http/handlers/auth/identity_events_clerk.zig` — the Clerk webhook dispatcher; `user.deleted` is currently ignored (the branch this spec adds). Mirror the existing event-branch dispatch shape.
2. `src/http/handlers/api_keys/tenant.zig` — the revoke path reused when a soft-delete revokes all of a tenant's keys.
3. `src/http/handlers/tenants/` + the nearest existing scheduled/background worker — the lifecycle + `conn.query`/`.drain()` pattern the new endpoint and hard-delete worker must follow.
4. `ui/packages/app/app/(dashboard)/settings/api-keys/` (lands in M76_001) — mirror its server-side RBAC guard + confirm-dialog for the Danger Zone.
5. `docs/REST_API_DESIGN_GUIDELINES.md` — `DELETE /v1/tenants/me` is a NEW route; follow URL-design + route-registration + handler-signature conventions.

---

## PR Intent & comprehension handshake

> The bridge from spec to merged PR — the agent confirms intent before writing code.

- **PR title (eventual):** Account deletion: tenant soft-delete, scheduled hard-delete, Clerk reconciliation
- **Intent (one sentence):** An owner/admin deletes their account from the dashboard — soft-delete now (disable zombies/triggers, revoke keys, start a grace window), hard-delete after grace, with the currently-ignored Clerk `user.deleted` webhook reconciled to the same cascade so self-deletion never orphans tenant data.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent in your own words and list assumptions (`ASSUMPTIONS I'M MAKING: …`). The load-bearing one: **the billing-gate behaviour is policy-gated** — at v2.0 (prepaid credits) deletion is NOT blocked on balance (forfeiture warning only); the v2.1 (postpaid) block ships behind a flag that is default-off until Stripe lands. A mismatch with the Intent above → STOP and reconcile before any edit.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal; RULE NDC, RULE NLR.
- **`docs/AUTH.md`** — webhook handling + identity events live behind the auth surface; read before touching `identity_events_clerk.zig`.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — new `DELETE /v1/tenants/me`: URL design, route registration, handler signature.
- **`docs/ZIG_RULES.md`** — new Zig logic (endpoint handler, hard-delete worker, webhook branch): pg-drain lifecycle, tagged-union results, multi-step `errdefer`, cross-compile both linux targets.
- **`docs/SCHEMA_CONVENTIONS.md`** — the tenant `deleted_at` (+ any grace-tracking) column follows existing migration conventions; update `schema/embed.zig` + the migration array.

---

## Applicable Gates

> Which Action-Triggered Guards this PR trips, and how each stays clean. Blast radius: new Zig handler + worker + a webhook branch, a new schema column, new error codes, and a new UI Danger Zone.

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | new handler/worker/webhook logic — read ZIG_RULES, cross-compile both linux targets, `.drain()` every `conn.query`. |
| PUB / Struct-Shape | yes | new `pub fn` on the handler + worker — own shape verdict per surface, no inherited justification. |
| File & Function Length (≤350/≤50/≤70) | yes | split endpoint / cascade / worker / webhook-branch so no file or fn approaches the cap. |
| LIFECYCLE | yes | worker + cascade own heap/handles — `init`/`deinit`/`errdefer` adjacent to every alloc. |
| ERROR REGISTRY | yes | new `UZ-*` codes for delete-blocked-unsettled (v2.1) + already-deleted — declare in `error_registry.zig`. |
| SCHEMA | yes | `deleted_at` (+ grace tracking) — SCHEMA GUARD output captured; single-concern migration. |
| LOGGING | yes | deletion is an audited action — structured log of the lifecycle transition, no PII in the message. |
| UFS | yes | grace-window constant, the route path, and the cascade order shared verbatim across Zig/TS where they cross. |
| UI Substitution / DESIGN TOKEN | yes | Danger Zone dialog from design-system primitives + `theme.css` tokens — no raw HTML, no arbitraries. |

---

## Overview

**Goal (testable):** `DELETE /v1/tenants/me` from an owner/admin soft-deletes the tenant (`deleted_at` set), immediately disables every zombie + trigger and revokes all tenant API keys, and returns the scheduled hard-delete date; a scheduled worker cascades the full delete after the grace window and removes the Clerk user; and a Clerk `user.deleted` webhook drives the same soft-delete cascade so no path orphans tenant data.

**Problem:** There is no account-deletion path today. `route_table.zig` has no tenant/account delete route, and Clerk's `user.deleted` event is explicitly ignored — so a user who deletes themselves in Clerk orphans their tenant → workspaces → zombies → credentials → events → billing rows. Operators also have no in-product way to leave.

**Solution summary:** A tenant-lifecycle delete endpoint performs a reversible soft-delete (disable + revoke + grace window) and returns the hard-delete date; a scheduled worker performs the irreversible cascade after grace and deletes the Clerk user; the ignored `user.deleted` webhook is reconciled to the same server-side cascade. A de-emphasised Danger Zone card (owner/admin-gated, confirm-by-typing) is the dashboard surface, with a billing pre-flight whose blocking behaviour is policy-gated (prepaid forfeiture-warning at v2.0; postpaid settle-first block flagged-off until Stripe).

---

## Prior-Art / Reference Implementations

> Mirror existing handler/worker/webhook shapes — don't invent a new lifecycle pattern.

- **API** → the closest existing handler under `src/http/handlers/` + `docs/REST_API_DESIGN_GUIDELINES.md`; the `tenant.zig` revoke path is reused to revoke keys on soft-delete.
- **AUTH webhook** → the existing event branches in `identity_events_clerk.zig` — mirror the dispatch shape for the new `user.deleted` branch.
- **Worker** → the nearest existing scheduled/background worker — mirror its lifecycle + pg-drain.
- **UI** → the M76_001 `/settings/api-keys` RBAC guard + confirm dialog; the Danger Zone mirrors it.
- **Alignment:** mirror existing handler/worker/webhook shapes verbatim. **Divergence:** the confirm-by-typing destructive dialog is new UX with no in-repo analog — design it deliberately.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/http/handlers/tenants/lifecycle.zig` (+ `route_table`/`router` wiring) | CREATE | `DELETE /v1/tenants/me` — soft-delete (`deleted_at`), disable zombies/triggers, revoke API keys; returns the scheduled hard-delete date. |
| scheduled hard-delete worker (under the existing worker surface) | CREATE | Cascade tenant → workspaces → zombies → credentials → events → billing after grace; delete the Clerk user. |
| `src/http/handlers/auth/identity_events_clerk.zig` | EDIT | Add the `user.deleted` branch → the same soft-delete cascade (closes the orphan gap). |
| `schema/*.sql` (tenant `deleted_at` + grace tracking) + `schema/embed.zig` + migration array | CREATE/EDIT | Reversible soft-delete state + grace deadline. |
| `src/errors/error_registry.zig` | EDIT | New `UZ-*` codes: delete-blocked-unsettled (v2.1), already-deleted. |
| `ui/packages/app/app/(dashboard)/settings/account/{page,actions}.tsx` + `components/DeleteAccountDialog.tsx` | CREATE | Danger Zone + confirm-by-typing dialog + billing pre-flight. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** soft-delete + grace + scheduled hard-delete + webhook reconciliation. This separates the reversible "disable now" from the irreversible "purge later," and makes Clerk-initiated and in-app deletion converge on a single server-side cascade.
- **Alternatives considered:** (a) immediate hard-delete on click — rejected; no recovery window and it races the GDPR export. (b) UI-only delete with no webhook reconciliation — rejected; leaves the orphan gap open for Clerk-initiated deletions, which is the actual bug.
- **Patch-vs-refactor verdict:** this is a **new-backend feature** on the tenant-lifecycle surface — refactor-scale, not an additive UI patch. It was graduated out of M76_001 precisely because bundling a delete endpoint + worker + schema change into a UI PR is the mud-patch the rule warns against.

---

## Sections (implementation slices)

### §1 — Tenant soft-delete endpoint

`DELETE /v1/tenants/me`, owner/admin only: set `deleted_at`, disable every zombie + trigger, revoke all tenant API keys, return the scheduled hard-delete date. Reversible within the grace window. **Implementation default:** reuse the `tenant.zig` revoke path for key revocation rather than a parallel implementation.

- **Dimension 1.1** — soft-delete sets state + disables + revokes + returns date → Test `test_delete_soft_deletes_and_disables`

### §2 — Billing pre-flight + policy gate

At v2.0 (prepaid credits) deletion is NOT blocked on balance; if `balance_nanos > 0` show a forfeiture warning (free-trial credits have no cash value, no auto-refund). At v2.1 (postpaid) block while an invoice is unsettled — flagged-off until Stripe lands. **Implementation default:** the postpaid block flag defaults off.

- **Dimension 2.1** — prepaid credits → forfeiture warning, deletion allowed → Test `test_delete_billing_preflight_warns_on_credits`
- **Dimension 2.2** — postpaid unsettled invoice → blocked (flagged) → Test `test_delete_blocks_on_unsettled_balance`

### §3 — Re-auth + confirm-by-typing Danger Zone

A de-emphasised Danger Zone card at the bottom of the account section, owner/admin-gated (mirrors M76_001's RBAC guard). The destructive button stays disabled until the tenant name (or email) is typed exactly; a fresh auth check precedes the call.

- **Dimension 3.1** — button disabled until exact name typed → Test `test_delete_requires_confirm_typing`
- **Dimension 3.2** — non-owner/admin → Danger Zone hidden + endpoint 403 → Test `test_non_owner_cannot_delete`

### §4 — Data export (GDPR portability)

Offer a portability export (at minimum events + billing-charge history) before the point of no return. **Implementation default:** reuse the existing export surface if one exists; otherwise the minimum two datasets.

### §5 — Scheduled hard-delete worker

After the grace window, cascade tenant → workspaces → zombies → credentials → events → billing, then delete the Clerk user via the Clerk backend API. Resumable + idempotent.

- **Dimension 5.1** — cascade purges all rows after grace + deletes Clerk user → Test `test_hard_delete_cascades_after_grace`

### §6 — Clerk `user.deleted` webhook reconciliation

Wire the currently-ignored event to the same server-side soft-delete cascade, so a Clerk-initiated deletion is consistent with the in-app flow and never orphans data.

- **Dimension 6.1** — `user.deleted` drives the soft-delete cascade → Test `test_clerk_user_deleted_webhook_cascades`
- **Dimension 6.2** — replayed event is a no-op on an already-deleted tenant → Test `test_cascade_idempotent_on_replay`

---

## Interfaces

```
DELETE /v1/tenants/me   — owner/admin only
  → 202 {deleted_at, hard_delete_at}
  → 403 when caller is not owner/admin
  → 409 when a postpaid invoice is unsettled (v2.1, behind the postpaid flag)
```

New error codes (assigned at implementation per ERROR REGISTRY GATE): a delete-blocked-unsettled code (v2.1) and an already-deleted code. The Clerk `user.deleted` webhook reuses the existing identity-events route; no new public route for it.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Non-owner delete | `user` role calls the endpoint | 403; Danger Zone hidden in the UI so the button is never shown. |
| Unsettled invoice (v2.1) | Postpaid invoice outstanding | 409 with settle-first; flagged-off until Stripe, so v2.0 never hits this. |
| Webhook replay | Clerk re-delivers `user.deleted` | Idempotent: re-applying soft-delete on an already-deleted tenant is a no-op. |
| Cancel within grace | User clicks the cancel-within-grace link | Tenant reactivated; zombies/triggers stay disabled until explicit re-enable. |
| Worker crash mid-cascade | Process dies between cascade steps | Resumable/idempotent cascade; partial deletes don't strand rows. |
| Clerk user-delete API fails | Clerk backend API errors during hard-delete | Retry; rows already purged are not re-created; transition logged. |

---

## Invariants

1. **No path orphans tenant data** — in-app delete and Clerk `user.deleted` both route to one server-side cascade function (enforced by single-function routing + an integration test asserting no orphaned rows after either path).
2. **Soft-delete reversible within grace; hard-delete irreversible and worker-only** — the endpoint never hard-deletes; the worker is the sole caller of the cascade (enforced by separating the two and a test that the endpoint leaves rows intact).
3. **Cascade is idempotent** — delete-if-exists semantics (enforced by a replay test).
4. **Postpaid block ships disabled until Stripe** — the flag defaults off (enforced by the default + a test asserting v2.0 deletion is not balance-blocked).

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_delete_soft_deletes_and_disables` | `DELETE /v1/tenants/me` sets `deleted_at`, disables zombies/triggers, revokes API keys, returns the hard-delete date. |
| `test_delete_requires_confirm_typing` | Destructive button stays disabled until the tenant name is typed exactly. |
| `test_non_owner_cannot_delete` | Non-owner/admin → Danger Zone hidden + endpoint returns 403. |
| `test_delete_billing_preflight_warns_on_credits` | `balance_nanos > 0` → forfeiture warning shown; deletion still allowed (prepaid, v2.0). |
| `test_delete_blocks_on_unsettled_balance` | Unsettled invoice → deletion blocked with settle-first (behind the postpaid flag). |
| `test_hard_delete_cascades_after_grace` | Worker purges tenant → workspaces → zombies → credentials → events → billing after grace and deletes the Clerk user. |
| `test_clerk_user_deleted_webhook_cascades` | A `user.deleted` event triggers the same soft-delete cascade (no orphaned tenant). |
| `test_cascade_idempotent_on_replay` | A replayed `user.deleted` is a no-op on an already-deleted tenant. |

Regression: the existing `identity_events_clerk.zig` event branches still dispatch unchanged; adding `user.deleted` does not alter other event handling.

---

## Acceptance Criteria

- [ ] `DELETE /v1/tenants/me` soft-deletes + returns the hard-delete date — verify: `make test-integration`
- [ ] Clerk `user.deleted` drives the same cascade (no orphans) — verify: integration test above
- [ ] Danger Zone owner/admin-gated + confirm-by-typing — verify: e2e
- [ ] `make lint` clean · `make test` passes
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no file over 350 lines added
- [ ] New route registered per the REST guide; SCHEMA GUARD output captured for `deleted_at`

---

## Eval Commands (post-implementation)

```bash
# E1: Integration (endpoint + webhook cascade)
make test-integration 2>&1 | tail -5
# E2: Build
zig build && echo "PASS" || echo "FAIL"
# E3: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E4: Lint
make lint 2>&1 | grep -E "✓|FAIL"
# E5: New route registered
git diff origin/main -- src/http/router.zig src/http/route_table.zig | head
# E6: Gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

N/A — no files deleted. This spec adds an endpoint, a worker, a webhook branch, a schema column, and a UI surface; nothing is removed.

---

## Discovery (consult log)

**Deferred May 22, 2026 (Indy).** Cleared out of the pending queue — not shipping now; moved to `done/` as a record. The design-of-record below is preserved verbatim. **Reactivation conditions:** (a) operator self-service account deletion gets prioritized, OR (b) the latent orphan-data bug needs closing on its own — Clerk `user.deleted` is still ignored (`src/http/handlers/auth/identity_events_clerk.zig`), so a Clerk-initiated self-deletion orphans tenant → workspaces → zombies → credentials → events → billing rows. The webhook-reconciliation slice (§6) can graduate to its own spec independently of the full delete UX if only the orphan gap needs closing.

**May 21, 2026 — graduated from M76_001 §8.** Captain ack: "Ship API Keys + Theme Toggle + Avatar all into this PR" — account deletion splits out (backend-heavy + billing-policy decision + auth-surface coordination). The §8 design-of-record is carried forward below verbatim so nothing is lost.

**Current-state findings (May 20, 2026):**
- **No deletion path exists.** `route_table.zig` has no tenant/account delete route. Clerk's `user.deleted` event is explicitly **ignored** (`src/http/handlers/auth/identity_events_clerk.zig:113-114`), so a user deleting themselves via Clerk today **orphans** their tenant, workspaces, zombies, credentials, events, and billing rows.
- **Billing is prepaid credits — there is no "dues"/arrears model.** `TenantBilling = { balance_nanos, is_exhausted }` (no `owed`/`outstanding` field). An exhausted tenant is already gate-blocked from incurring new charges; Stripe purchase + any postpaid invoice is **v2.1**. So deletion is **not** blocked by debt today.

**Recommended process (the design of record):**

1. **Scope.** "Delete account" = soft-delete the **tenant** (owner/admin only) + delete the **Clerk user**. Tenants are single-user at alpha, so the two coincide; defer multi-member org semantics (member-leave vs tenant-delete) until orgs exist.
2. **Billing gate.** Today (prepaid): do **not** block on balance; if `balance_nanos > 0`, show a forfeiture warning (free-trial credits have no cash value), no auto-refund. v2.1 (Stripe/postpaid): **block** while an invoice is unsettled — the grace period is the settlement window; refund of remaining *purchased* credits follows the published refund policy.
3. **Re-auth + confirm.** Require a fresh auth check and **confirm-by-typing** the tenant name (or email) before the destructive call — no single-click delete.
4. **Data export first.** Offer a GDPR-portability export (at minimum events + billing-charge history) before the point of no return.
5. **Soft-delete + grace window (default 30 days).** On confirm, mark the tenant `deleted_at`, **immediately** disable every zombie + trigger and revoke all tenant API keys, but **retain data**. Email a confirmation with a cancel-within-grace link.
6. **Hard-delete after grace.** A scheduled job cascades tenant → workspaces → zombies → credentials → events → billing, then deletes the Clerk user via the Clerk backend API.
7. **Webhook reconciliation.** Wire the ignored Clerk `user.deleted` webhook → the same server-side soft-delete cascade, so a Clerk-initiated deletion is consistent with the in-app flow. (This closes the orphan gap regardless of the UI.)

**Open decision (gates §2):** billing-gate policy — prepaid forfeiture-warning vs postpaid settle-first block. The v2.1 block ships behind a flag, default-off, until Stripe lands.

---

## Out of Scope

- **Multi-member org semantics** (member-leave vs tenant-delete) — deferred until orgs exist; alpha tenants are single-user.
- **Stripe purchase + postpaid invoicing** — v2.1; this spec only gates §2 on the flag, it does not implement the postpaid surface.
- **Refund execution** — follows the published refund policy for purchased credits; free-trial credits have no cash value and are forfeited.
