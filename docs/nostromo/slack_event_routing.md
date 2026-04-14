# Slack Event Routing — Agent-Executable Guide

**Published at:** docs.usezombie.com/integrations/slack-event-routing
**Use case:** Route incoming Slack events (messages, reactions) to the correct zombie
**Actors:** Slack → UseZombie event handler → workspace_integrations lookup → zombie queue
**Human involvement:** One-time setup. Runtime is fully automated.

---

## What this does

When someone posts in a Slack channel where UseZombie's bot is present, Slack sends an
event to UseZombie's events endpoint. UseZombie:

1. Verifies the Slack signing secret (constant-time, replay-protected)
2. Looks up which workspace owns this Slack team ID via `workspace_integrations`
3. Finds the zombie configured to handle Slack events in that workspace
4. Enqueues the event for the zombie to process

The zombie wakes up with the Slack event as its input and reasons about what to do next.

---

## Actors

```
[User posts in #hiring]
    → Slack Events API
        → POST /v1/slack/events
            → UseZombie:
                1. Verify x-slack-signature (HMAC-SHA256, <5min timestamp window)
                2. Filter: if event.bot_id present → ignore (prevent loops)
                3. Filter: if type=url_verification → echo challenge
                4. Lookup: workspace_integrations WHERE provider='slack' AND external_id=team_id
                5. Find zombie in workspace with Slack event trigger configured
                6. Enqueue event to Redis worker queue
                7. HTTP 200 to Slack (always — Slack retries on non-200)
            → Zombie wakes up with event payload as input
```

---

## Prerequisites

- Workspace has Slack connected (OAuth or CLI path — see `slack_oauth_install.md` or `slack_cli_credential.md`)
- A zombie exists in the workspace with a Slack event trigger configured
- Zombie has an approved Slack integration grant (M9) for any outbound Slack calls

---

## Setup — step by step

### Step 1: Get your webhook URL

```bash
zombiectl zombie triggers list --zombie {zombie_id}
# Returns: https://api.usezombie.com/v1/webhooks/{zombie_id}
```

### Step 2: Configure Slack Events API

In your Slack App dashboard (https://api.slack.com/apps):

1. **Event Subscriptions** → Enable Events → Request URL:
   ```
   https://api.usezombie.com/v1/slack/events
   ```
   UseZombie responds to the `url_verification` challenge automatically.

2. **Subscribe to bot events:**
   - `message.channels` — messages in public channels the bot is in
   - `message.groups` — messages in private channels (if needed)
   - `reaction_added` — emoji reactions (optional, for approval gate feedback)

3. **Reinstall your Slack App** to apply the new event subscriptions.

### Step 3: Configure the zombie trigger

```bash
zombiectl zombie update --zombie {zombie_id} --trigger '{
  "type": "slack_event",
  "event_types": ["message.channels"],
  "filter_channel": "#hiring"
}'
```

The filter is optional — without it, the zombie receives all events from all channels
the bot is in. Scoping to a channel prevents noisy cross-channel triggers.

### Step 4: Add the bot to the channel

```
/invite @usezombie
```

---

## How routing works at runtime

UseZombie uses `workspace_integrations` — not the webhook URL — to route events:

```sql
SELECT workspace_id
FROM core.workspace_integrations
WHERE provider = 'slack'
  AND external_id = $1   -- Slack team_id from event payload
  AND status = 'active'
```

This means ALL events from a connected Slack workspace route to the right UseZombie
workspace regardless of which channel or user triggered them. The zombie's event filter
(channel, event type) is applied after workspace routing.

**Why not route via webhook URL?**
Slack's Events API sends all events to a single registered URL — not per-zombie URLs.
The workspace routing step is necessary to fan out from one endpoint to multiple zombies.

---

## Bot loop prevention

UseZombie automatically ignores events where `event.bot_id` is set. This prevents
a zombie's own messages from re-triggering itself.

If your zombie posts to Slack via M9 execute and UseZombie's own bot ID appears in the
event, it is silently dropped. No configuration needed.

---

## Event payload passed to zombie

The zombie receives the raw Slack event payload as its input:

```json
{
  "type": "event_callback",
  "team_id": "T01ABC",
  "event": {
    "type": "message",
    "channel": "C0HIRING123",
    "user": "U0HR_USER",
    "text": "Jane Smith applied for Senior Engineer — thoughts?",
    "ts": "1712930400.000100",
    "thread_ts": null
  },
  "event_id": "Ev01XYZ",
  "event_time": 1712930400
}
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Slack shows "Your URL didn't respond with 200" | UseZombie returned error | Check signing secret config: `SLACK_SIGNING_SECRET` env var |
| Zombie not receiving events | Bot not in the channel | `/invite @usezombie` in the channel |
| Events from wrong workspace routed incorrectly | Two workspaces share a bot | Each Slack workspace needs its own UseZombie installation |
| `UZ-WH-010` in logs | Signing secret mismatch | Verify `SLACK_SIGNING_SECRET` matches your Slack App's signing secret |
| `UZ-WH-011` in logs | Slack event timestamp too old | Clock skew > 5 min between Slack and UseZombie — check server time |
| Events arrive but zombie not waking | Zombie trigger filter too narrow | Check filter_channel matches actual channel ID, not name |

---

## Testing event routing locally

```bash
# Simulate a Slack event (replace SLACK_SIGNING_SECRET with your value)
TS=$(date +%s)
BODY='{"type":"event_callback","team_id":"T01ABC","event":{"type":"message","text":"test","channel":"C01ABC","user":"U01ABC","ts":"'$TS'.000100"}}'
SIG=$(echo -n "v0:${TS}:${BODY}" | openssl dgst -sha256 -hmac "your_signing_secret" | awk '{print "v0="$2}')

curl -X POST http://localhost:4001/v1/slack/events \
  -H "Content-Type: application/json" \
  -H "x-slack-signature: ${SIG}" \
  -H "x-slack-request-timestamp: ${TS}" \
  -d "${BODY}"
```

Expected: `HTTP 200`. Check Redis queue for the enqueued event.
