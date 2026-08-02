# Register the GitHub App

**Owners:** 🤠 Indy for GitHub settings and 1Password; 🦉 Orly for secret sync
and verification
**Updated:** Jul 31, 2026
**Prerequisite:** the target environment's admin bootstrap is complete, its API
host passes `/readyz`, and its dashboard is reachable

Create a separate GitHub App for development and production. Complete
development first, including live acceptance, before repeating the same steps
for production.

| Environment | App name | API base | Installation scope |
|---|---|---|---|
| Development | `agentsfleet-dev` or another unique name | `https://api-dev.agentsfleet.net` | Owner test account |
| Production | `agentsfleet` or another unique name | `https://api.agentsfleet.net` | Any account |

## 1. Indy: register the app

In GitHub **Settings → Developer settings → GitHub Apps**, create the app:

- Homepage URL: the matching dashboard URL.
- Callback URL: `<API_BASE>/v1/connectors/github/callback`.
- Enable **Request user authorization during installation**. The callback needs
  both `installation_id` and the one-time authorization `code` to verify that
  the returning user may access the installation.
- Keep webhooks active with
  `<API_BASE>/v1/ingress/github` and a new high-entropy webhook secret.
- Keep Secure Sockets Layer (SSL) verification enabled.
- Subscribe only to **Pull request** and **Workflow run**.
- Set the minimum repository permissions:
  - Metadata: read-only.
  - Contents: read-only.
  - Pull requests: read and write.
  - Actions: read-only.

Add another permission only when a shipped fleet requires it. GitHub's current
registration and least-privilege guidance is in
[Registering a GitHub App](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/registering-a-github-app)
and
[Choosing permissions](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/choosing-permissions-for-a-github-app).

## 2. Indy: vault the six fields

In the matching 1Password vault, create or update `github-app` with:

- `app_id` — the App Identifier (ID).
- `app_slug` — the value from `github.com/apps/<app_slug>`.
- `client_id`.
- `client_secret`.
- `private_key_pem` — the full Privacy-Enhanced Mail (PEM) private key.
- `webhook_secret`.

Use the 1Password application for secret entry. After the PEM field is saved,
move the downloaded key file to Trash and empty it. Never paste a value into
chat, a ticket, or a shell command.

## 3. Orly: sync the platform bag

After Indy approves the target, run:

```bash
ENV=dev \
ALLOW_VAULT_READS=1 \
ALLOW_PLATFORM_SECRET_WRITES=1 \
  ./playbooks/lib/platform_secret_sync.sh github-app
```

Change `ENV` to `prod` only for the production run. The script creates or
replaces the entire bag without putting the admin key or provider values in
process arguments or output.

The GitHub App identity loads when `agentsfleetd` starts. After all provider
bags for this environment are synced, rerun the matching founding deployment
step once before live acceptance.

## 4. Indy and Orly: prove the live path

1. Indy installs the app on one test repository and completes **Connect
   GitHub** in the dashboard.
2. Orly confirms the connector reports connected and the installation belongs
   to the expected tenant.
3. Indy opens a test pull request; Orly confirms exactly one intended fleet
   receives the delivery and can post a review with a short-lived installation
   token.
4. Orly replays the delivery identifier and confirms no second fleet event or
   review is created.

Record the environment, repository, pull-request URL, fleet identifier, and
delivery identifier. Record no credential values.

## Complete when

- The exact environment URLs are configured and reachable.
- The six-field bag exists in the admin workspace.
- `agentsfleetd` has restarted after the sync.
- The install, callback, event, outbound review, and replay checks pass.
