# Register the Slack App

**Owners:** 🤠 Indy for Slack settings and 1Password; 🦉 Orly for secret sync
and verification
**Updated:** Jul 31, 2026
**Prerequisite:** the target environment's admin bootstrap is complete, its API
host passes `/readyz`, and its public Slack ingress is reachable

Create a private development app first. Create and distribute the production
app only after development acceptance passes.

| Environment | App name | API base | Distribution |
|---|---|---|---|
| Development | `agentsfleet-dev` | `https://api-dev.agentsfleet.net` | Private test workspace |
| Production | `agentsfleet` | `https://api.agentsfleet.net` | Customer workspaces |

## 1. Indy: create the app

At [Slack app management](https://api.slack.com/apps), choose **Create New App
→ From a manifest** and use this manifest with the target `API_BASE`:

```yaml
display_information:
  name: agentsfleet-dev
features:
  bot_user:
    display_name: agentsfleet-dev
    always_online: true
oauth_config:
  redirect_urls:
    - <API_BASE>/v1/connectors/slack/callback
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

For production, change both names to `agentsfleet`. Keep the bot scope list
exactly `app_mentions:read`, `chat:write`, and `channels:history`; that is the
list requested by `agentsfleetd`. Do not add channel-wide message or user
directory access.

Slack verifies the event URL against the live handler. Do not continue until
**Event Subscriptions** shows **Verified**. Slack's current authorization flow
is documented in
[Installing with Open Authorization (OAuth)](https://docs.slack.dev/authentication/installing-with-oauth/).
For production, complete
[Slack app distribution](https://docs.slack.dev/app-management/distribution/);
keep development private.

## 2. Indy: vault the three fields

In the matching 1Password vault, create or update `slack-app` with:

- `client_id`.
- `client_secret`.
- `signing_secret`.

Use the 1Password application. Never paste a value into chat, a ticket, or a
shell command.

## 3. Orly: sync the platform bag

After Indy approves the target, run:

```bash
ENV=dev \
ALLOW_VAULT_READS=1 \
ALLOW_PLATFORM_SECRET_WRITES=1 \
  ./playbooks/lib/platform_secret_sync.sh slack-app
```

Change `ENV` to `prod` only for the production run. The Slack bag is read on
demand, but rerunning the matching founding deployment after all provider bags
are synced keeps the environment restart sequence uniform.

## 4. Indy and Orly: prove the live path

1. Indy installs the app in a test workspace through the dashboard.
2. Indy invites the bot to a test channel and mentions it.
3. Orly confirms one signed `app_mention` reaches the expected tenant and
   receives one threaded reply.
4. Orly confirms a bad signature and a replay do not create a fleet event.

## Complete when

- The event URL is verified and the exact three scopes are granted.
- The three-field bag exists in the admin workspace.
- A real install, mention, reply, bad-signature check, and replay check pass.
