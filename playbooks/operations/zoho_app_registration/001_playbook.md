---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Register the Zoho Desk application

## What this does

Creates one Zoho server-based client per environment and stores it as the `zoho-app` bag.
Repeat for production only after development acceptance.
Indy owns Zoho settings and 1Password; the agent syncs and verifies.

| Environment | Client name | Callback URL |
|---|---|---|
| Development | `agentsfleet-dev` | `https://app-dev.agentsfleet.net/api/connectors/zoho/callback` |
| Production | `agentsfleet` | `https://app.agentsfleet.net/api/connectors/zoho/callback` |

## Before you begin

The environment's [admin bootstrap](../admin_bootstrap/001_playbook.md) is complete, `/readyz` passes and Indy can create a client in the [Zoho API Console](https://api-console.zoho.com).

## Steps

1. Indy creates a **Server-based Application** with the table's callback URL.
   Add no per-region callback; the callback finds the data center and stores its regional accounts base.
2. Indy stores the `zoho-app` fields: `client_id`, `client_secret`.
3. The agent syncs `zoho-app` with the [shared bag procedure](../admin_bootstrap/001_playbook.md#sync-a-provider-secret-bag), then the control plane restarts.
   Token refresh uses the credentials `agentsfleetd` loaded at startup.

The connect request asks for exactly `Desk.organization.READ,Desk.basic.READ`; the client registration stores no scopes.
See [Zoho web server applications](https://www.zoho.com/developer/oauth/web-server-apps/overview.html) and the [Zoho Desk API](https://desk.zoho.com/DeskAPIDocument).

## Verify it works

1. Indy connects a test Zoho Desk organization from the dashboard.
2. The agent confirms the callback completes for the expected tenant.
3. The agent confirms the workspace's `zoho` credential holds the organization's regional accounts base.
4. The agent proves one token refresh succeeds after the first access token expires.
