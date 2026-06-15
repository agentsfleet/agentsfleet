# Teardown — DEV user

**Tier:** operations (destructive) · **Env:** DEV only · **Owner:** Human (Clerk delete)

Remove a dev signup. Deleting the user in the **dev** Clerk instance fires the
`user.deleted` webhook, which hard-purges the tenant and every dependent row in
PlanetScale-dev (`account_teardown`). No manual SQL needed.

## Steps

1. **Clerk Dashboard → dev instance → Users →** find the email **→ Delete user.**
   Confirm the dev webhook has the **`user.deleted`** event enabled
   (→ `https://api-dev.agentsfleet.net/v1/auth/identity-events/clerk`).

2. **Verify** in PlanetScale-dev (empty = purged):

   ```sql
   SELECT tenant_id FROM core.users WHERE oidc_subject = '<clerk_user_id>';
   ```

3. **If rows remain** (webhook didn't fire): Clerk → Webhooks → the `user.deleted`
   message → **Resend**.

The purge is keyed by `oidc_subject` and removes the tenant plus its users,
workspaces, memberships, agents, sessions, secrets, billing, and keys in one
transaction.
