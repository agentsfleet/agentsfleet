---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Register the Jira application

## What this does

Creates one Atlassian Open Authorization (OAuth) 2.0 integration per environment and stores it as the `jira-app` bag.
Repeat for production only after development acceptance.
Indy owns Atlassian settings and 1Password; the agent syncs and verifies.

| Environment | App name | Callback URL | Access |
|---|---|---|---|
| Development | `agentsfleet-dev` | `https://app-dev.agentsfleet.net/api/connectors/jira/callback` | Private test sites |
| Production | `agentsfleet` | `https://app.agentsfleet.net/api/connectors/jira/callback` | Sharing on, before customer-site acceptance |

## Before you begin

The environment's [admin bootstrap](../admin_bootstrap/001_playbook.md) is complete, `/readyz` passes and Indy can create an app in the [Atlassian Developer Console](https://developer.atlassian.com/console/myapps/).

## Steps

1. Indy creates an OAuth 2.0 integration in the Atlassian Developer Console.
2. Under **Authorization**, Indy configures three-legged OAuth with the table's callback URL.
3. Under **Permissions**, Indy adds the Jira platform and Jira Service Management APIs.
4. Indy grants exactly these scopes, and no administration or user-management access:

   ```text
   read:jira-work read:jira-user write:jira-work read:servicedesk-request write:servicedesk-request
   ```

   The connect request also asks for `offline_access`, which the selector may not show.
5. Indy stores the `jira-app` fields: `client_id`, `client_secret`.
6. The agent syncs `jira-app` with the [shared bag procedure](../admin_bootstrap/001_playbook.md#sync-a-provider-secret-bag), then the control plane restarts.
   Token refresh uses the credentials `agentsfleetd` loaded at startup.

The callback resolves the chosen site's `cloud_id`, so registration holds no cloud identifier.
See Atlassian's [three-legged OAuth setup](https://developer.atlassian.com/cloud/oauth/getting-started/enabling-oauth-3lo/) and [scope reference](https://developer.atlassian.com/cloud/jira/platform/scopes-for-oauth-2-3LO-and-forge-apps/).

## Verify it works

1. Indy connects a test Jira Cloud site from the dashboard.
2. The agent confirms the callback stored the site's `cloud_id` and `site_url` for the expected tenant.
3. The agent confirms a second tenant cannot claim the first tenant's connector handle.
4. The agent proves one token refresh succeeds.
