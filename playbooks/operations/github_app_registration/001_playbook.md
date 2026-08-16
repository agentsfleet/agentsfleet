# Register the GitHub App

**Owners:** 🤠 Indy for GitHub settings and 1Password; 🦉 Orly for secret sync
and verification
**Updated:** Aug 16, 2026
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
- Callback URL: `https://<APP_HOST>/api/connectors/github/callback`. Use
  `app-dev.agentsfleet.net` for development and `app.agentsfleet.net` for
  production.
- Enable **Request user authorization during installation**. The dashboard
  callback uses the one-time authorization `code` and the current signed-in
  person to verify a claimed installation or discover one accessible install.
- Keep webhooks active with
  `<API_BASE>/v1/ingress/github` and a new high-entropy webhook secret.
- Keep Secure Sockets Layer (SSL) verification enabled.
- Subscribe to **Pull request**, **Workflow run**, and **Deployment status**.
- Set the minimum repository permissions:
  - Metadata: read-only.
  - Contents: read-only.
  - Pull requests: read and write.
  - Actions: read-only.
  - Deployments: read-only.

Add another permission only when a shipped fleet requires it. GitHub's current
registration and least-privilege guidance is in
[Registering a GitHub App](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/registering-a-github-app)
and
[Choosing permissions](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/choosing-permissions-for-a-github-app).

For every mapped repository, name the expected deployment integration. GitHub
permits every push-capable identity to create a deployment status, so all of
those identities are inside this first spine's trusted producer boundary. The
`agentsfleet` handler accepts any signed status from the mapped GitHub
installation. It does not verify the status creator or App identity, and this
is GitHub-origin proof rather than Vercel attestation.

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

1. Indy selects **Connect** in the dashboard. If the App already exists, GitHub
   authorizes the user and `agentsfleet` restores the internal workspace
   binding. If the App is absent, Indy installs it on one test repository.
2. Orly confirms the connector reports connected and the installation belongs
   to the expected tenant.
3. Indy opens a test pull request; Orly confirms exactly one intended fleet
   receives the delivery and can post a review with a short-lived installation
   token.
4. Indy uses the expected deployment integration to create a completed
   production deployment status for the merged test commit. Orly confirms the
   signed delivery reaches `/v1/ingress/github` and records the same repository,
   commit, deployment identifier, deployment-status identifier, delivery identifier,
   and creator identity shown by GitHub.
5. Orly replays each delivery identifier and confirms no second fleet event,
   review, or production result is created.
6. Indy selects **Disconnect**. Orly confirms the dashboard reports **Not
   connected** while the GitHub App remains installed. Indy selects **Connect**
   again and confirms the dashboard returns to **Connected**.

Record the development environment, repository, Pull Request (PR) URL, fleet
identifier, expected deployment integration, received creator identity,
deployment identifier, deployment-status identifier, and delivery identifier in the
Pull Request (PR) Session Notes.
Record no credential values. This is an audit record, not an enforceable
single-writer rule.

## Complete when

- The exact environment URLs are configured and reachable.
- The six-field bag exists in the admin workspace.
- `agentsfleetd` has restarted after the sync.
- The install, callback, Pull Request, deployment-status, outbound review, and
  replay checks pass.
- Pull Request (PR) Session Notes contain the expected deployment integration,
  received creator identity, deployment identifier, deployment-status identifier,
  and delivery identifier.
