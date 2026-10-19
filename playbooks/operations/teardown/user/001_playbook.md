# Development user teardown

Deleting a Clerk user currently leaves that user's database account intact.
The signed identity endpoint handles `user.created`; it answers `user.deleted` with `status: ignored`.
Resending the deletion event does not change that result.

## Inspect before deleting an identity

Record the non-secret Clerk user identifier from the selected development application's user page.
Use that value for `<CLERK_USER_ID>` in the development database console:

```sql
SELECT id, tenant_id FROM identity.users WHERE oidc_subject = '<CLERK_USER_ID>';
```

Expected output is the matching account row, or zero rows if no signup was saved.
A successful Clerk deletion is not evidence of a database purge.

## Current removal options

For an approved complete development rebuild, use [database teardown](../database/001_playbook.md), then the [clean-database priming route](../../../README.md#prime-a-clean-database).
That action removes every account in the selected database and needs separate approval.

There is no documented per-user purge API on this branch.
Stop before claiming erasure; do not improvise deletion SQL across identity, private content, and retained billing records.
The Human owns identity deletion and any decision to erase application data.

Source: `rustd/crates/afd_api_ingress/src/handler/webhook/identity_route.rs`, section `Why user.deleted is answered and not acted on`.
