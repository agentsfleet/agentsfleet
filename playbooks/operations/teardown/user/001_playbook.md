---
type: troubleshooting
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Development user teardown

## Symptom

A deleted Clerk user still has a database account.

## Why it happened

The signed identity endpoint acts on `user.created` and answers `user.deleted` with `status: ignored`.
Resending the deletion event changes nothing.
See `rustd/crates/daemon/afd_api_ingress/src/handler/webhook/identity_route.rs`, section `Why user.deleted is answered and not acted on`.

## How to fix it

No per-user purge API exists.
The Human owns identity deletion and any decision to erase application data.

1. Copy the Clerk user identifier from the development application's user page.
2. Inspect the account in the development database console.

   ```sql
   SELECT id, tenant_id FROM identity.users WHERE oidc_subject = '<CLERK_USER_ID>';
   ```

   ```text
   <varies: the account row, or zero rows if no signup was saved>
   ```

3. Stop before claiming erasure.
   Never improvise deletion SQL across identity, private content and retained billing records.
4. For an approved full development rebuild, run [database teardown](../database/001_playbook.md), then the [clean-database priming route](../../../README.md#prime-a-clean-database).
   That removes every account in the database and needs separate approval.
