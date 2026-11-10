---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Register the Slack App

## What this does

Creates one Slack app per environment and stores it as the `slack-app` bag.
Distribute production only after development acceptance passes.
Indy owns Slack settings and 1Password; the agent syncs and verifies.

| Environment | App name | `APP_HOST` | `API_BASE` | Distribution |
|---|---|---|---|---|
| Development | `agentsfleet-dev` | `app-dev.agentsfleet.net` | `https://api-dev.agentsfleet.net` | Private test workspace |
| Production | `agentsfleet` | `app.agentsfleet.net` | `https://api.agentsfleet.net` | Customer workspaces |

## Before you begin

The environment's [admin bootstrap](../admin_bootstrap/001_playbook.md) is complete, `/readyz` passes and the public Slack ingress is reachable.

## Steps

1. Indy opens [Slack app management](https://api.slack.com/apps) and selects **Create New App → From a manifest**.
2. Indy pastes this manifest, filling `<APP_HOST>` and `<API_BASE>` from the table.
   For production, both names become `agentsfleet`.

   ```yaml
   display_information:
     name: agentsfleet-dev
   features:
     bot_user:
       display_name: agentsfleet-dev
       always_online: true
   oauth_config:
     redirect_urls:
       - https://<APP_HOST>/api/connectors/slack/callback
     scopes:
       bot:
         - app_mentions:read
         - chat:write
         - channels:history
   settings:
     event_subscriptions:
       request_url: <API_BASE>/v1/connectors/slack/events
       bot_events:
         - app_mention
     org_deploy_enabled: false
     socket_mode_enabled: false
   ```

   Keep exactly those three bot scopes; `agentsfleetd` requests no others.
3. Indy waits until **Event Subscriptions** shows **Verified**; Slack checks the URL against the live handler.
4. For production only, Indy completes [Slack app distribution](https://docs.slack.dev/app-management/distribution/).
5. Indy stores the `slack-app` fields: `client_id`, `client_secret`, `signing_secret`.
6. The agent syncs `slack-app` with the [shared bag procedure](../admin_bootstrap/001_playbook.md#sync-a-provider-secret-bag).
   `agentsfleetd` reads this bag per request, so no restart is needed.

Slack's flow is described in [Installing with Open Authorization (OAuth)](https://docs.slack.dev/authentication/installing-with-oauth/).

## Verify it works

1. Indy installs the app in a test workspace from the dashboard.
2. Indy invites the bot to a test channel and mentions it.
3. The agent confirms one signed `app_mention` reaches the expected tenant and gets one threaded reply.
4. The agent confirms a bad signature and a replay create no fleet event.
