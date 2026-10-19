---
type: how-to
audience: operator
verified: 2026-10-19
product_version: 0.61.0
executable: false
---

# Bootstrap the platform administrator

## What this does

Create an administrator's account, select its workspace, and enable platform operations in one environment.
The tenant, workspace, and Clerk user identifiers are different values.

## Before you begin

The matching API and dashboard must be deployed, migrations complete, and the Clerk signed-event endpoint configured.
Use the environment's existing `agentsfleet-admin` 1Password identity.
Obtain separate approval for vault reads, runtime changes, and production.

Set `ENV` to `dev` or `prod`.
Set `VAULT_DEV` and `VAULT_PROD` to the actual 1Password vault names from the deployment environment.

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

Expected output is empty with exit status zero.
An unset vault name or failed readiness check stops this procedure.

## Steps

### 1. Create or recover the account

Sign up in `$APP_URL` using the identity stored in the selected vault's `agentsfleet-admin` item.
A verified primary email and a signed Clerk `user.created` delivery to `/v1/auth/identity-events/clerk` create the account.

That delivery writes `identity.tenants`, `identity.users`, `identity.workspaces`, memberships, and the account's `billing.wallets` row.
It also writes the new tenant identifier and owner scopes to Clerk public metadata.

After a database wipe, signing into an existing Clerk identity does not recreate these rows.
Have the Human resend that user's signed `user.created` event through the configured Clerk webhook endpoint.
Do not delete the Clerk user or send an unsigned imitation.

A repeated delivery finds the same account and returns `created: false`.
The metadata writeback can replace the user's scopes, so perform step 2 after replaying the event.

### 2. Preserve owner access and add operator access

In the selected Clerk application's user metadata, preserve `tenant_id` and unrelated metadata.
Preserve existing valid scopes and add the required operator scopes to the space-delimited `scopes` string.
A fresh administrator needs both sets below.

Owner access, written at signup:

```text
fleet:admin schedule:write secret:write apikey:admin grant:write connector:write billing:read workspace:admin library:write approval:resolve
```

Platform access, added by the Human:

```text
runner:enroll runner:write stream:read model:admin platform-key:admin platform-library:write workspace:any
```

The session-token template must project the matching API audience and metadata:

```json
{
  "aud": "<API_BASE>",
  "scopes": "{{user.public_metadata.scopes}}",
  "metadata": {"tenant_id": "{{user.public_metadata.tenant_id}}"}
}
```

Replace `<API_BASE>` with the environment URL selected above.
Sign out and in to receive a new JSON Web Token (JWT).
Confirm `/admin/runners`, `/admin/models`, and `/admin/fleet-libraries` open successfully.

### 3. Record the workspace identifier

Select the administrator's workspace in the dashboard.
Copy the workspace identifier from `/w/<WORKSPACE_ID>/fleets`, and save it in the 1Password application:

```text
op://<VAULT>/agentsfleet-admin/platform_admin_workspace_id
```

`<VAULT>` is the selected vault name; `<WORKSPACE_ID>` comes from that selected workspace's URL.
Never substitute Clerk `tenant_id` or `user.id`.
A UUID-shaped value alone does not prove workspace ownership.

After approval, apply this non-secret runtime pointer:

```bash
workspace_id="$(op read "op://$VAULT/agentsfleet-admin/platform_admin_workspace_id")"
uuidv7_pattern='^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
printf '%s' "$workspace_id" | rg --quiet "$uuidv7_pattern"
flyctl secrets set --app "$FLY_APP" "PLATFORM_ADMIN_WORKSPACE_ID=$workspace_id"
unset workspace_id
curl --fail --retry 12 --retry-all-errors --retry-delay 5 \
  --silent --show-error "$API_BASE/readyz" >/dev/null
```

Expected output includes Fly's successful secret update; the final readiness command exits zero without output.
The change restarts `agentsfleetd`; wait for readiness before continuing.

### 4. Create the credential used for platform-bag sync

Open `$APP_URL/settings/api-keys` and create `platform-priming`.
Copy its once-revealed `agt_t` value directly into the concealed `api-key` field on the selected `agentsfleet-admin` item.

A tenant API key resolves its creator's current Clerk capabilities.
The key can therefore inherit platform capabilities granted to this administrator; it is not automatically limited to owner scopes.
Keep it in 1Password and use it only through the guarded sync scripts.

The first successful platform-secret sync verifies this key can write to the recorded workspace.
A forbidden or missing-workspace result blocks later priming steps.

### 5. Configure a usable platform model

Follow [model catalogue priming](../model_catalogue/001_playbook.md) to seed identities, exact prices, context limits, and inference settings.
Then open `$APP_URL/admin/models`, choose the intended model, and select **Make default**.
Enter the provider credential directly in the dialog and confirm the active-default indicator.

A priced catalogue row alone cannot run setup chat.
The selected row needs inference settings and a credential before it becomes a usable platform default.

## Verify it works

- Health and readiness requests return success.
- The administrator can open all three platform pages.
- The stored workspace identifier belongs to the selected workspace and matches `PLATFORM_ADMIN_WORKSPACE_ID`.
- The first guarded platform-bag sync succeeds in that workspace.
- The catalogue shows the intended active platform default with saved inference settings.
- No raw credential appears in terminal output, process arguments, or chat.

Continue with the [clean-database priming index](../../README.md#prime-a-clean-database).
Restart once after all startup-loaded provider bags are synced.

## Common problems

| Symptom | Cause | Action |
|---|---|---|
| Sign-in succeeds but the workspace is absent | Clerk identity survived the database wipe | Resend the signed `user.created` delivery, then refresh scopes and session |
| Platform pages are forbidden | Missing operator grant or stale token | Preserve owner scopes, add operator scopes, then sign in again |
| Secret sync cannot find the workspace | A tenant ID was saved as the workspace ID | Copy the actual selected workspace ID and update the runtime pointer |
| New fleet has no usable model | Pricing exists without inference or credentials | Finish catalogue configuration and select a credentialed default |

The bootstrap handler does not retry a failed metadata writeback itself.
It logs the failure; inspect the delivery and Clerk metadata before adding operator scopes.

## Remove or undo

The Human owns this administrator, its scopes, API key, and provider defaults.
Revoke an unused API key in settings; remove operator scopes in Clerk and refresh the session to revoke access.
Deleting the Clerk user does not purge database rows; see [user teardown](../teardown/user/001_playbook.md).

## Related pages

- [Authentication and signup](../../../docs/AUTH.md)
- [Model catalogue priming](../model_catalogue/001_playbook.md)
- [Provider credential rotation](../credential_rotation/001_playbook.md)
