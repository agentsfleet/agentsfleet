# Slack CLI Credential — Agent-Executable Guide

**Published at:** docs.usezombie.com/integrations/slack-cli
**Use case:** Add Slack credentials manually via CLI (bring-your-own bot token)
**Actors:** Workspace owner (human) → zombiectl CLI → zombie vault
**Human involvement:** Token generation in Slack API dashboard + one CLI command

---

## When to use this path

Use this instead of the "Add to Slack" OAuth button when:

- You already have a Slack bot token from an existing Slack App
- You're running UseZombie in a restricted environment that can't receive OAuth redirects
- You're setting up UseZombie from a script or CI pipeline
- Your Slack workspace admin requires you to use a pre-approved internal Slack App

Both paths write to the same zombie vault slot. The runtime (M9 execute pipeline) sees
no difference between OAuth-acquired and CLI-added credentials.

---

## Actors

```
[Workspace owner generates bot token in Slack API dashboard]
    → zombiectl credential add slack <token>
        → UseZombie:
            1. Encrypts token → vault.secrets(workspace_id, "slack")
            2. Upserts workspace_integrations row (provider="slack", source="cli")
        → Confirmation: "Slack credential stored. workspace_integrations updated."
```

---

## Prerequisites

You need a Slack bot token. To get one:

### Create a Slack App (if you don't have one)

1. Go to https://api.slack.com/apps → **Create New App** → **From scratch**
2. Name it (e.g. "UseZombie") and select your workspace
3. Under **OAuth & Permissions** → **Bot Token Scopes**, add:
   - `chat:write`
   - `channels:read`
   - `channels:history`
   - `reactions:write`
   - `users:read`
4. Click **Install to Workspace** → **Allow**
5. Copy the **Bot User OAuth Token** (starts with `xoxb-`)

---

## Setup — step by step

### Step 1: Store the token

```bash
zombiectl credential add slack xoxb-your-token-here --workspace {workspace_id}
```

The token is encrypted and stored in `vault.secrets(workspace_id, "slack")`.
The raw value is never shown again — store it in your password manager before running this.

### Step 2: Verify

```bash
zombiectl integration list --workspace {workspace_id}

# Expected:
# PROVIDER  EXTERNAL_ID  STATUS  SOURCE  INSTALLED_AT
# slack     (pending)    active  cli     2026-04-12T15:30:00Z
```

Note: `external_id` may be `(pending)` until a Slack event arrives and UseZombie
resolves the team ID from the token. This does not block zombie execution.

### Step 3: Add the bot to a channel

In Slack, add the bot to the channels it will read from or post to:

```
/invite @usezombie
```

---

## Difference from OAuth path

| | OAuth ("Add to Slack") | CLI (`zombiectl credential add slack`) |
|--|------------------------|----------------------------------------|
| Token source | Slack issues it via OAuth flow | You generate it in Slack API dashboard |
| Team ID resolved | Immediately (from OAuth response) | On first event or manual update |
| Scopes recorded | From OAuth scope param | Not recorded (you control your Slack App) |
| `source` column | `oauth` | `cli` |
| Bot token slot | `vault.secrets(workspace_id, "slack")` | `vault.secrets(workspace_id, "slack")` |
| Runtime behavior | Identical | Identical |

---

## Rotating the token

If you rotate your Slack bot token:

```bash
zombiectl credential add slack xoxb-new-token-here --workspace {workspace_id}
```

This overwrites the existing slot in the vault. The `workspace_integrations` row is
updated with `updated_at`. Running zombies pick up the new token on their next execute call.

---

## Revoking Slack access

```bash
zombiectl credential remove slack --workspace {workspace_id}
```

This removes the token from the vault and sets the `workspace_integrations` row
`status='revoked'`. Zombies using `credential_ref: "slack"` will receive `UZ-TOOL-001`
on their next execute call.

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `UZ-TOOL-001` in execute | Credential missing from vault | Run `zombiectl credential add slack <token>` |
| `UZ-GRANT-001` in execute | No integration grant for zombie | Request grant: `POST /v1/zombies/{id}/integration-requests` |
| Token silently rejected by Slack | Token was revoked or expired | Generate new token in Slack API dashboard, re-run `credential add` |
| `external_id` shows `(pending)` | Team ID not yet resolved | Post a test event to the webhook URL or run `zombiectl integration refresh slack` |

---

## Next steps

After the credential is stored, request zombie-level Slack access via M9:

```bash
curl -X POST https://api.usezombie.com/v1/zombies/{zombie_id}/integration-requests \
  -H "Authorization: Bearer {your_workspace_token}" \
  -d '{"service": "slack", "reason": "Need to post to #hiring channel"}'
```

Approve the grant in Slack or the dashboard. The zombie is now live.
