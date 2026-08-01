# Register the Zoho Desk Application

**Owners:** 🤠 Indy for Zoho settings and 1Password; 🦉 Orly for secret sync
and verification
**Updated:** Jul 31, 2026
**Prerequisite:** the target environment's admin bootstrap is complete, its API
host passes `/readyz`, and Indy can create a client in the Zoho API Console

Register development first and repeat for production only after live
development acceptance.

| Environment | Client name | Callback URL |
|---|---|---|
| Development | `agentsfleet-dev` | `https://api-dev.agentsfleet.net/v1/connectors/zoho/callback` |
| Production | `agentsfleet` | `https://api.agentsfleet.net/v1/connectors/zoho/callback` |

## 1. Indy: create and vault the client

At the [Zoho API Console](https://api-console.zoho.com), create a
**Server-based Application** with the matching callback URL.

The connect request supplies exactly
`Desk.organization.READ,Desk.basic.READ`. Scopes are granted during
authorization, not stored on the client registration. The callback resolves
the Zoho data center and stores the correct regional accounts base; do not add
per-region callback URLs.

The provider steps are described in
[Zoho web server applications](https://www.zoho.com/developer/oauth/web-server-apps/overview.html)
and the [Zoho Desk API documentation](https://desk.zoho.com/DeskAPIDocument).

In the matching 1Password vault, create or update `zoho-app` with:

- `client_id`.
- `client_secret`.

Use the 1Password application. Never paste a value into chat, a ticket, or a
shell command.

## 2. Orly: sync the platform bag

After Indy approves the target, run:

```bash
ENV=dev \
ALLOW_VAULT_READS=1 \
ALLOW_PLATFORM_SECRET_WRITES=1 \
  ./playbooks/lib/platform_secret_sync.sh zoho-app
```

Change `ENV` to `prod` only for the production run. Refresh-token minting uses
credentials captured when `agentsfleetd` starts, so rerun the matching
founding deployment after all provider bags are synced.

## 3. Indy and Orly: prove the live path

1. Indy connects a test Zoho Desk organization from the dashboard.
2. Orly confirms the callback completes for the expected tenant.
3. Orly confirms the vaulted `fleet:zoho` handle has the organization-correct
   regional accounts base.
4. Orly proves one token refresh succeeds after the initial access token is no
   longer current.

## Complete when

- The callback is exact and the two-field bag exists.
- `agentsfleetd` has restarted after the sync.
- A real connect and refresh pass for the correct Zoho data center.
