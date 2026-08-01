# Register the Jira Application

**Owners:** 🤠 Indy for Atlassian settings and 1Password; 🦉 Orly for secret
sync and verification
**Updated:** Jul 31, 2026
**Prerequisite:** the target environment's admin bootstrap is complete, its API
host passes `/readyz`, and Indy can create an app in the Atlassian Developer
Console

Register development first and repeat for production only after live
development acceptance.

| Environment | App name | Callback URL | Access |
|---|---|---|---|
| Development | `agentsfleet-dev` | `https://api-dev.agentsfleet.net/v1/connectors/jira/callback` | Private test sites |
| Production | `agentsfleet` | `https://api.agentsfleet.net/v1/connectors/jira/callback` | Shared for customer sites |

## 1. Indy: create and configure the app

In the [Atlassian Developer Console](https://developer.atlassian.com/console/myapps/),
create an Open Authorization (OAuth) 2.0 integration. Under
**Authorization**, configure three-legged OAuth and enter the exact callback
URL. Under **Permissions**, add the Jira platform and Jira Service Management
APIs.

Grant exactly `read:jira-work read:jira-user write:jira-work
read:servicedesk-request write:servicedesk-request`. The authorization request
also supplies `offline_access`; it may not appear in the permission selector.
Do not grant administration or user-management access.

The callback resolves the selected site's `cloud_id`; there is no static cloud
identifier in this registration. Follow Atlassian's current
[three-legged OAuth setup](https://developer.atlassian.com/cloud/oauth/getting-started/enabling-oauth-3lo/)
and
[scope reference](https://developer.atlassian.com/cloud/jira/platform/scopes-for-oauth-2-3LO-and-forge-apps/).
Keep development private. Enable sharing for production before customer-site
acceptance.

## 2. Indy: vault the two fields

In the matching 1Password vault, create or update `jira-app` with:

- `client_id`.
- `client_secret`.

Use the 1Password application. Never paste a value into chat, a ticket, or a
shell command.

## 3. Orly: sync the platform bag

After Indy approves the target, run:

```bash
ENV=dev \
ALLOW_VAULT_READS=1 \
ALLOW_PLATFORM_SECRET_WRITES=1 \
  ./playbooks/lib/platform_secret_sync.sh jira-app
```

Change `ENV` to `prod` only for the production run. Refresh-token minting uses
credentials captured when `agentsfleetd` starts, so rerun the matching
founding deployment after all provider bags are synced.

## 4. Indy and Orly: prove the live path

1. Indy connects a test Jira Cloud site from the dashboard.
2. Orly confirms the callback stores the selected `cloud_id` and `site_url`
   for the expected tenant.
3. Orly confirms a second tenant cannot claim the first tenant's connector
   handle.
4. Orly proves one token refresh succeeds.

## Complete when

- The callback and permission set are exact and the two-field bag exists.
- `agentsfleetd` has restarted after the sync.
- A real connect, tenant-isolation check, and refresh pass.
