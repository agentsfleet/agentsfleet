---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Bootstrap the platform administrator

## What this does

Creates the administrator account, records its workspace, and enables platform operations in one environment.
Tenant, workspace and Clerk user identifiers are three different values.

## Before you begin

1. Deploy the matching API and dashboard, and finish migrations.
2. Configure the Clerk signed-event endpoint.
3. Get separate approval for vault reads, runtime changes and production.
4. Set `ENV` to `dev` or `prod`, and set `VAULT_DEV` and `VAULT_PROD` from the deployment environment.
5. Select the environment and check health.

   ```bash
   case "${ENV:?Choose dev or prod}" in
     dev)
       export APP_URL=https://app-dev.agentsfleet.net
       export API_BASE=https://api-dev.agentsfleet.net
       export VAULT="${VAULT_DEV:?Set the development vault name}"
       export FLY_APP=agentsfleetd-dev
       ;;
     prod)
       export APP_URL=https://app.agentsfleet.net
       export API_BASE=https://api.agentsfleet.net
       export VAULT="${VAULT_PROD:?Set the production vault name}"
       export FLY_APP=agentsfleetd-prod
       ;;
     *) echo "ENV must be dev or prod" >&2; exit 2 ;;
   esac
   curl --fail --silent --show-error "$API_BASE/healthz" >/dev/null
   curl --fail --silent --show-error "$API_BASE/readyz" >/dev/null
   ```

   ```text
   <no output; exit status 0>
   ```

   An unset vault name or a failed readiness check stops here.

## Steps

### 1. Create or recover the account

1. Sign up at `$APP_URL` with the identity in the vault's `agentsfleet-admin` item.
2. Verify the primary email.
   Clerk's signed `user.created` delivery to `/v1/auth/identity-events/clerk` creates the account.

The delivery writes `identity.tenants`, `identity.users`, `identity.workspaces`, memberships and `billing.wallets`, then writes the tenant ID and owner scopes to Clerk public metadata.

After a database wipe, signing in to an existing Clerk user creates no rows.
Indy resends that user's signed `user.created` event from the configured Clerk webhook endpoint.
Never delete the Clerk user or send an unsigned imitation.

A replayed delivery finds the same account and returns `created: false`.
The replay rewrites scopes, so do step 2 after it.

### 2. Add operator scopes

1. In the Clerk user's public metadata, keep `tenant_id` and unrelated keys.
2. Keep the owner scopes in the space-delimited `scopes` string.
   Signup writes exactly these:

   ```text
   fleet:admin schedule:write secret:write apikey:admin grant:write connector:write billing:read workspace:admin library:write approval:resolve
   ```

3. Append the platform scopes:

   ```text
   runner:enroll runner:write stream:read model:admin platform-key:admin platform-library:write workspace:any
   ```

4. Confirm the session-token template projects the audience and metadata.
   Replace `<API_BASE>` with the selected API URL.

   ```json
   {
     "aud": "<API_BASE>",
     "scopes": "{{user.public_metadata.scopes}}",
     "metadata": {"tenant_id": "{{user.public_metadata.tenant_id}}"}
   }
   ```

5. Sign out and in to get a new JSON Web Token (JWT).
6. Confirm `/admin/runners`, `/admin/models` and `/admin/fleet-libraries` open.

### 3. Record the workspace identifier

1. Select the administrator's workspace in the dashboard.
2. Copy `<WORKSPACE_ID>` from the `/w/<WORKSPACE_ID>/fleets` URL.
3. Save it in the 1Password app at `op://<VAULT>/agentsfleet-admin/platform_admin_workspace_id`.
   Never use the Clerk `tenant_id` or `user.id`; a UUID shape alone does not prove ownership.
4. After approval, set the runtime pointer and wait for readiness.

   ```bash
   workspace_id="$(op read "op://$VAULT/agentsfleet-admin/platform_admin_workspace_id")"
   uuidv7_pattern='^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
   printf '%s' "$workspace_id" | rg --quiet "$uuidv7_pattern"
   flyctl secrets set --app "$FLY_APP" "PLATFORM_ADMIN_WORKSPACE_ID=$workspace_id"
   unset workspace_id
   curl --fail --retry 12 --retry-all-errors --retry-delay 5 \
     --silent --show-error "$API_BASE/readyz" >/dev/null
   ```

   ```text
   <varies: Fly's secret update and restart lines>
   ```

   The change restarts `agentsfleetd`, and the readiness command exits zero without output.

### 4. Create the sync credential

1. Open `$APP_URL/settings/api-keys` and create `platform-priming`.
2. Copy the once-revealed `agt_t` value into the concealed `api-key` field of `agentsfleet-admin`.

The key carries its creator's current Clerk scopes, platform scopes included, so use it only through the guarded sync script.

### 5. Set a usable platform model

1. Seed the catalogue with [model catalogue priming](../model_catalogue/001_playbook.md).
2. Open `$APP_URL/admin/models`, choose the model, and select **Make default**.
3. Enter the provider credential in the dialog and confirm the active-default indicator.

A priced row without inference settings and a credential cannot run setup chat.

## Sync a provider secret bag

Provider registration pages use this after Indy creates the provider app.

1. Get Indy's approval for the target environment; production needs its own approval.
2. Indy enters the bag's fields from the table below in the 1Password app, in the environment's vault.
   Never paste a value into chat, a ticket or a shell command.
3. Sync the bag. `<BAG>` is the table's first column.

   ```bash
   ENV=dev ALLOW_VAULT_READS=1 ALLOW_PLATFORM_SECRET_WRITES=1 ./playbooks/lib/platform_secret_sync.sh <BAG>
   ```

   ```text
   ✓ <BAG> created in dev admin workspace
   ```

   The line says `replaced` when the bag exists. Use `ENV=prod` for production.
   Leave `VAULT` unset to use the environment's vault, or set `VAULT=<VAULT_NAME>` to override it.

4. Restart once after every startup-read bag is synced, as step 8 of the [clean-database priming index](../../README.md#prime-a-clean-database) says.

| Bag | 1Password item and fields | `agentsfleetd` reads it |
|---|---|---|
| `github-app` | `github-app`: `app_id`, `app_slug`, `client_id`, `client_secret`, `private_key_pem`, `webhook_secret` | At startup; restart |
| `slack-app` | `slack-app`: `client_id`, `client_secret`, `signing_secret` | Per request |
| `linear-app`, `jira-app`, `zoho-app` | Same-named item: `client_id`, `client_secret` | Refresh uses the startup copy; restart |
| `qstash` | `qstash`: `token`, `current-signing-key`, `next-signing-key`, `url` | At startup; restart |
| `smtp-relay` | `smtp-relay`: `host`, `port`, `username`, `password`, `from_address` | Per send |

The script reads `api-key` and `platform_admin_workspace_id` from `agentsfleet-admin`, then replaces the whole bag.
No value reaches process arguments or output.

Each refusal names the missing approval or the bad 1Password field.
An HTTP 403 or 404 means the key's scopes or the workspace ID are wrong, and later priming waits.

## Verify it works

- The administrator opens all three platform pages.
- `PLATFORM_ADMIN_WORKSPACE_ID` matches the selected workspace, and the first bag sync succeeds there.
- The catalogue shows a credentialed active default with saved inference settings.
- No credential appears in terminal output, process arguments or chat.

## Common problems

| Symptom | Cause | Action |
|---|---|---|
| Platform pages are forbidden | Missing operator scopes or a stale token | Keep owner scopes, add operator scopes, sign in again |
| New fleet has no usable model | Price exists without inference settings or a credential | Finish catalogue setup and select a credentialed default |

The signup handler logs a failed metadata writeback and does not retry it.
Inspect the delivery and Clerk metadata before adding operator scopes.

## Remove or undo

Indy owns this administrator, its scopes, its API key and the provider defaults.
Revoke an unused key in settings; remove operator scopes in Clerk and sign in again.
Deleting the Clerk user leaves database rows; see [user teardown](../teardown/user/001_playbook.md).

## Related pages

- [Authentication and signup](../../../docs/AUTH.md)
- [Model catalogue priming](../model_catalogue/001_playbook.md)
- [Credential rotation](../credential_rotation/001_playbook.md)
