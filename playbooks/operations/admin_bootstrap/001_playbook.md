# Platform Operator Bootstrap

**Updated:** Jul 31, 2026
**Owners:** 🤠 Indy grants identity and copies one-time values; 🦉 Orly verifies
the result and updates the Fly.io runtime pointer.
**Prerequisite:** The target API and dashboard are deployed, Clerk session
claims are configured, and the environment's `agentsfleet-admin` 1Password item
contains `username` and `credential`.

Run development first, then production. This runbook deliberately uses the
dashboard for privileged writes so a short-lived Clerk session—not a long-lived
tenant API key—authorizes platform operations.

## Select the environment

| Environment | Dashboard | API | Vault | Fly.io app |
|---|---|---|---|---|
| Development | `https://app-dev.agentsfleet.net` | `https://api-dev.agentsfleet.net` | `ZMB_CD_DEV` | `agentsfleetd-dev` |
| Production | `https://app.agentsfleet.net` | `https://api.agentsfleet.net` | `ZMB_CD_PROD` | `agentsfleetd-prod` |

```bash
export ENV=dev

case "$ENV" in
  dev)
    export APP_URL=https://app-dev.agentsfleet.net
    export API_BASE=https://api-dev.agentsfleet.net
    export VAULT=ZMB_CD_DEV
    export FLY_APP=agentsfleetd-dev
    ;;
  prod)
    export APP_URL=https://app.agentsfleet.net
    export API_BASE=https://api.agentsfleet.net
    export VAULT=ZMB_CD_PROD
    export FLY_APP=agentsfleetd-prod
    ;;
  *)
    echo "ENV must be dev or prod" >&2
    exit 2
    ;;
esac

curl --fail --silent --show-error "$API_BASE/healthz" >/dev/null
curl --fail --silent --show-error "$API_BASE/readyz" >/dev/null
```

## Handoff

| Order | Owner | Action |
|---|---|---|
| 1 | 🤠 Indy | Sign up or sign in with the `agentsfleet-admin` 1Password identity. |
| 2 | 🤠 Indy | In Clerk, preserve `tenant_id` and grant the exact platform-operator scopes. |
| 3 | 🤠 Indy | Sign out and in, then verify the operator pages are visible. |
| 4 | 🤠 Indy | Copy the Clerk `tenant_id` to the `platform_admin_workspace_id` 1Password field. |
| 5 | 🦉 Orly | Verify the identifier and set the Fly.io runtime pointer. |
| 6 | 🤠 Indy | Create and vault one tenant API key for connector provisioning. |
| 7 | 🤠 Indy | Configure the model catalogue and platform default in the dashboard. |
| 8 | 🦉 Orly | Verify health, readiness, and the operator surfaces. |

## 1. Create the identity

🤠 Indy opens `$APP_URL` and signs up with the values stored at:

```text
op://<vault>/agentsfleet-admin/username
op://<vault>/agentsfleet-admin/credential
```

On a repeat run, sign in instead. A successful first sign-up creates the tenant,
workspace, and Clerk `public_metadata.tenant_id`.

## 2. Grant platform scopes in Clerk

In the matching Clerk application, open the user and edit Public metadata.
Preserve the existing `tenant_id`; set `scopes` to this exact space-delimited
value:

```text
runner:enroll runner:write stream:read model:admin platform-key:admin platform-library:write workspace:any
```

The Clerk session-token customization must project:

```json
{
  "aud": "<environment API URL>",
  "scopes": "{{user.public_metadata.scopes}}",
  "metadata": {
    "tenant_id": "{{user.public_metadata.tenant_id}}"
  }
}
```

Do not change another user's metadata. Sign out and back in after saving; an
existing JSON Web Token (JWT) does not gain newly assigned scopes.

## 3. Verify the operator session

The refreshed dashboard session must expose:

- `$APP_URL/admin/runners`
- `$APP_URL/admin/models`
- `$APP_URL/admin/fleet-libraries`

If a page is hidden or returns forbidden, stop. Recheck the Clerk application,
the exact scope string, the session-token customization, and the API audience.

## 4. Record and activate the admin workspace

🤠 Indy copies the unchanged Clerk `public_metadata.tenant_id` directly into:

```text
op://<vault>/agentsfleet-admin/platform_admin_workspace_id
```

🦉 Orly validates and applies the non-secret pointer:

```bash
workspace_id="$(op read "op://$VAULT/agentsfleet-admin/platform_admin_workspace_id")"
uuidv7_pattern='^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
printf '%s' "$workspace_id" | rg --quiet "$uuidv7_pattern"

flyctl secrets set \
  --app "$FLY_APP" \
  "PLATFORM_ADMIN_WORKSPACE_ID=$workspace_id"
unset workspace_id

curl --fail --retry 12 --retry-all-errors --retry-delay 5 \
  --silent --show-error \
  "$API_BASE/readyz" >/dev/null
```

Setting the Fly.io secret restarts `agentsfleetd`. On a fresh install it moves
the connector credential broker from static-only mode to the admin workspace.

## 5. Create the tenant provisioning key

🤠 Indy opens `$APP_URL/settings/api-keys`, creates `platform-provisioning`, and
copies the once-revealed `agt_t...` value directly into the concealed
`api-key` field on the environment's `agentsfleet-admin` 1Password item.

This key has tenant scopes only. It may write provider credentials into the
admin workspace, but it cannot enroll runners, manage the platform model
catalogue, or call platform-default endpoints. Use the dashboard session for
those operations.

## 6. Configure the platform model

🤠 Indy opens `$APP_URL/admin/models`:

1. Add the intended provider model and current pricing if it is not already in
   the catalogue.
2. Select **Make default** on that catalogue row.
3. Enter the provider API key directly in the dialog.
4. Confirm the provider, model, and active-default indicator.

The server action stores the provider key in the admin workspace and activates
the selected catalogued model. The key is never returned. Do not hard-code a
provider, model name, rate, or context limit in this runbook; those values
change independently of the deployment.

## Required result

- `/healthz` and `/readyz` return success.
- The refreshed operator session exposes runners, models, and Fleet library.
- `platform_admin_workspace_id` is a valid Universally Unique Identifier
  version 7 (UUIDv7) and is set on the matching Fly.io app.
- The `api-key` field contains the once-revealed tenant provisioning key.
- The dashboard shows one active platform model default.
- No raw credential appears in terminal output, process arguments, or chat.

Run the relevant provider-registration playbooks next, then restart
`agentsfleetd` once so its startup credential broker reloads those values.
