---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Register the GitHub App

## What this does

Creates one GitHub App per environment and stores it as the `github-app` bag.
Finish development, including live acceptance, before production.
Indy owns GitHub settings and 1Password; the agent syncs and verifies.

| Environment | App name | `APP_HOST` | `API_BASE` | Install on |
|---|---|---|---|---|
| Development | `agentsfleet-dev` or another unique name | `app-dev.agentsfleet.net` | `https://api-dev.agentsfleet.net` | Owner test account |
| Production | `agentsfleet` or another unique name | `app.agentsfleet.net` | `https://api.agentsfleet.net` | Any account |

## Before you begin

The environment's [admin bootstrap](../admin_bootstrap/001_playbook.md) is complete, `/readyz` passes and the dashboard is reachable.

## Steps

1. Indy opens GitHub **Settings → Developer settings → GitHub Apps** and creates the app:
   - Homepage URL: `https://<APP_HOST>`.
   - Callback URL: `https://<APP_HOST>/api/connectors/github/callback`.
   - **Request user authorization during installation**: on. The callback uses the one-time `code` and the signed-in person to verify or discover an installation.
   - Webhook: active, URL `<API_BASE>/v1/ingress/github`, a new high-entropy secret, Secure Sockets Layer (SSL) verification on.
   - Events: **Pull request**, **Workflow run**, **Deployment status**.
   - Repository permissions: Metadata read-only; Contents read and write; Pull requests read and write; Actions, Checks and Deployments read-only.

   Add a permission only when a shipped fleet needs it.
   See [Registering a GitHub App](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/registering-a-github-app) and [Choosing permissions](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/choosing-permissions-for-a-github-app).
2. Indy accepts changed permissions on every existing installation before the incident drill, development first.
   A pending request stops the fleet reading checks or opening a draft fix.
3. Indy names the expected deployment integration for each mapped repository.
4. Indy stores the `github-app` fields: `app_id` is the App ID, `app_slug` comes from `github.com/apps/<app_slug>`, and `private_key_pem` is the full Privacy-Enhanced Mail (PEM) key.
5. Indy moves the downloaded key file to Trash and empties it.
6. The agent syncs `github-app` with the [shared bag procedure](../admin_bootstrap/001_playbook.md#sync-a-provider-secret-bag), then the control plane restarts.

Any identity that can push can create a deployment status.
The handler accepts any signed status from the mapped installation and checks neither its creator nor App identity.
That is GitHub-origin proof, not Vercel attestation.

## Verify it works

1. Indy selects **Connect** in the dashboard.
   With an existing App, GitHub authorizes the user and `agentsfleet` restores the workspace binding; otherwise Indy installs it on one test repository.
2. The agent confirms the connector reads connected and the installation belongs to the expected tenant.
3. Indy opens a test pull request.
   The agent confirms exactly one intended fleet gets the delivery and posts a review with a short-lived installation token.
4. Indy uses the expected integration to create a completed production deployment status on the merged test commit.
   The agent confirms the signed delivery reaches `/v1/ingress/github` with the repository, commit, deployment, deployment-status, delivery and creator identifiers GitHub shows.
5. The agent replays each delivery identifier and confirms no second fleet event, review or production result.
6. Indy selects **Disconnect**; the dashboard reads **Not connected** while the App stays installed.
7. Indy selects **Connect** again; the dashboard reads **Connected**.

Record in the Pull Request (PR) Session Notes: environment, repository, PR URL, fleet identifier, expected integration, received creator, deployment, deployment-status and delivery identifiers.
Record no credential values.
The record is an audit trail, not an enforced single-writer rule.
