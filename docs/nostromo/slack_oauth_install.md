# Slack OAuth Install — Agent-Executable Guide

**Published at:** docs.usezombie.com/integrations/slack-install
**Use case:** Connect a workspace to Slack via the "Add to Slack" OAuth button
**Actors:** Workspace owner (human) → UseZombie OAuth handler → Slack API → zombie vault
**Human involvement:** One click. Everything after is automated.

---

## What this does

Clicking "Add to Slack" runs Slack's OAuth 2.0 V2 flow. When it completes:

- The Slack bot token is stored in the zombie vault (`vault.secrets`) scoped to your workspace
- A routing record is created so Slack events reach the right workspace
- A confirmation message appears in your Slack workspace
- Any zombie in your workspace can now use `credential_ref: "slack"` — the vault supplies
  the token at runtime, the zombie never sees it

This is the zero-config path. No manual token handling. No `zombiectl credential add` needed.

---

## Actors

```
[Workspace owner clicks "Add to Slack" on usezombie.com]
    → GET /v1/slack/install
        → 302 to slack.com/oauth/v2/authorize
            → Slack shows permission screen
                → Owner clicks Allow
                    → Slack redirects to GET /v1/slack/callback?code=xxx&state=yyy
                        → UseZombie:
                            1. Validates CSRF state (HMAC-SHA256)
                            2. Exchanges code for bot token (Slack oauth.v2.access API)
                            3. Stores bot token in vault.secrets(workspace_id, "slack")
                            4. Creates workspace_integrations row (provider="slack", external_id=team_id)
                            5. Posts confirmation to Slack
                            6. Redirects to dashboard
```

---

## What scopes are requested

| Scope | Why |
|-------|-----|
| `chat:write` | Post messages and approval requests |
| `channels:read` | Read channel list to find default post target |
| `channels:history` | Read messages in channels the bot is in |
| `reactions:write` | Add emoji reactions (approval gate feedback) |
| `users:read` | Resolve user identity for approval notifications |
| `incoming-webhook` | Post to a channel without specifying channel ID each time |

---

## Setup — step by step

### Step 1: Go to the dashboard

```
https://app.usezombie.com/dashboard
```

If you don't have a workspace yet, create one first via the dashboard or:

```bash
zombiectl workspace create --name "My Team"
```

### Step 2: Click "Add to Slack"

The button is on the dashboard integrations page. It redirects you to Slack.

### Step 3: Approve permissions in Slack

Slack shows the permission screen. Select the workspace (top-right dropdown if you're in
multiple workspaces). Click **Allow**.

### Step 4: You're connected

UseZombie redirects you back to the dashboard with `?slack=connected`. A message appears
in your Slack workspace's default channel:

```
UseZombie is connected!
Your zombie vault now has Slack credentials.
Configure your Zombie at https://app.usezombie.com and request Slack access —
your team will see approval requests here.
```

---

## What's stored after install

**Zombie vault (`vault.secrets`):**
```
workspace_id = <your workspace UUID>
key_name     = "slack"
value        = <bot token — encrypted, never shown>
```

**Routing record (`core.workspace_integrations`):**
```
provider    = "slack"
external_id = "T01ABC"   ← your Slack team ID
workspace_id = <your workspace UUID>
scopes_granted = "chat:write channels:read channels:history reactions:write users:read"
source      = "oauth"
status      = "active"
```

No raw token is in any database column. The vault is the only place.

---

## Re-installing

If you click "Add to Slack" again (e.g. after revoking the bot or rotating tokens):

- The bot token in the vault is refreshed
- The routing record is updated (no duplicate row created)
- A new confirmation message is posted

---

## Verifying the install

```bash
# Check the workspace integration record
zombiectl integration list --workspace {workspace_id}

# Expected:
# PROVIDER  EXTERNAL_ID  STATUS  SOURCE  INSTALLED_AT
# slack     T01ABC       active  oauth   2026-04-12T15:30:00Z
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| "OAuth state invalid" (403) | CSRF state expired or tampered | Start install from the dashboard again — do not use a saved link |
| "Could not connect to Slack" (502) | Token exchange failed | Check SLACK_CLIENT_ID and SLACK_CLIENT_SECRET are configured |
| Redirect to Slack but no install | Wrong workspace selected in Slack | Use the workspace dropdown top-right in Slack's OAuth screen |
| No confirmation message in Slack | Bot not added to any channel | Add the bot to a channel: `/invite @usezombie` |
| Dashboard shows but slack=connected missing | Redirect blocked | Check browser doesn't block the redirect |

---

## Next steps after install

Once connected, your zombies can use Slack:

```bash
# Request Slack access for a zombie (M9 grant flow)
curl -X POST https://api.usezombie.com/v1/workspaces/{workspace_id}/zombies/{zombie_id}/integration-requests \
  -H "Authorization: Bearer {your_workspace_token}" \
  -d '{"service": "slack", "reason": "Need to post alerts to #ops channel"}'
```

You'll receive the approval request in Slack. After you approve, the zombie can post
messages via `credential_ref: "slack"` in execute calls — the vault supplies the token.
