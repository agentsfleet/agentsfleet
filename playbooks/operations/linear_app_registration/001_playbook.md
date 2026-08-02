# Register the Linear Application

**Owners:** 🤠 Indy for Linear settings and 1Password; 🦉 Orly for secret sync
and verification
**Updated:** Jul 31, 2026
**Prerequisite:** the target environment's admin bootstrap is complete, its API
host passes `/readyz`, and Indy can create an application in Linear

Register development first and repeat for production only after live
development acceptance.

| Environment | App name | Callback URL | Access |
|---|---|---|---|
| Development | `agentsfleet-dev` | `https://api-dev.agentsfleet.net/v1/connectors/linear/callback` | Private test workspace |
| Production | `agentsfleet` | `https://api.agentsfleet.net/v1/connectors/linear/callback` | Public customer workspaces |

## 1. Indy: create and configure the application

In Linear **Settings → API → OAuth applications**, create an Open
Authorization (OAuth) 2.0 application with the matching callback URL.

- Keep client-credentials access disabled; tenants use the authorization-code
  flow.
- Leave the webhook unset; `agentsfleet` has no Linear app webhook receiver.
- Keep development private. Make production public before customer acceptance.

The authorization request supplies `read,comments:create`. Linear documents
`comments:create` as the targeted alternative to broad write access, and its
authorization-code response includes rotating refresh tokens. See
[Linear OAuth 2.0 authentication](https://linear.app/developers/oauth-2-0-authentication)
and [application manifests](https://linear.app/developers/oauth-app-manifests).

## 2. Indy: vault the two fields

In the matching 1Password vault, create or update `linear-app` with:

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
  ./playbooks/lib/platform_secret_sync.sh linear-app
```

Change `ENV` to `prod` only for the production run. Refresh-token minting uses
credentials captured when `agentsfleetd` starts, so rerun the matching
founding deployment after all provider bags are synced.

## 4. Indy and Orly: prove the live path

1. Indy connects a test Linear workspace from the dashboard.
2. Orly confirms the expected tenant receives a `fleet:linear` handle with a
   refresh token.
3. Orly proves one token refresh succeeds and the prior handle is replaced
   safely.
4. Orly confirms a comment can be created without a broader write grant.

## Complete when

- The callback and scope list are exact and the two-field bag exists.
- `agentsfleetd` has restarted after the sync.
- A real connect, refresh, and targeted comment write pass.
