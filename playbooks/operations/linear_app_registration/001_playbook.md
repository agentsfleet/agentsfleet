---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Register the Linear application

## What this does

Creates one Linear Open Authorization (OAuth) 2.0 application per environment and stores it as the `linear-app` bag.
Repeat for production only after development acceptance.
Indy owns Linear settings and 1Password; the agent syncs and verifies.

| Environment | App name | Callback URL | Access |
|---|---|---|---|
| Development | `agentsfleet-dev` | `https://app-dev.agentsfleet.net/api/connectors/linear/callback` | Private |
| Production | `agentsfleet` | `https://app.agentsfleet.net/api/connectors/linear/callback` | Public, before customer acceptance |

## Before you begin

The environment's [admin bootstrap](../admin_bootstrap/001_playbook.md) is complete, `/readyz` passes and Indy can create a Linear application.

## Steps

1. Indy opens Linear **Settings → API → OAuth applications** and creates an application with the table's callback URL.
2. Indy keeps client-credentials access off; tenants use the authorization-code flow.
3. Indy leaves the webhook unset; `agentsfleet` has no Linear webhook receiver.
4. Indy stores the `linear-app` fields: `client_id`, `client_secret`.
5. The agent syncs `linear-app` with the [shared bag procedure](../admin_bootstrap/001_playbook.md#sync-a-provider-secret-bag), then the control plane restarts.
   Token refresh uses the credentials `agentsfleetd` loaded at startup.

The connect request asks for `read,comments:create`.
Linear documents `comments:create` as the narrow alternative to write access, and its code exchange returns rotating refresh tokens.
See [Linear OAuth 2.0](https://linear.app/developers/oauth-2-0-authentication) and [application manifests](https://linear.app/developers/oauth-app-manifests).

## Verify it works

1. Indy connects a test Linear workspace from the dashboard.
2. The agent confirms the workspace holds a `linear` credential with a refresh token and the expected `connector.installs` row.
3. The agent proves one token refresh succeeds and safely replaces the prior credential.
4. The agent confirms a comment can be created without a broader write grant.
