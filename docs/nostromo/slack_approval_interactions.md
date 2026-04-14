# Slack Approval Interactions — Agent-Executable Guide

**Published at:** docs.usezombie.com/integrations/slack-approval-interactions
**Use case:** Approve or deny zombie actions directly from Slack button clicks
**Actors:** Zombie (posts approval request) → Slack (shows buttons) → Human (clicks) → UseZombie (relays decision)
**Human involvement:** Per-action click for high-risk actions. One-time for integration grants.

---

## What this does

When a zombie hits an action that requires human approval (firewall gate or integration
grant request), UseZombie posts a Slack message with [Approve] and [Deny] buttons.
The human clicks a button. Slack sends the interaction to UseZombie's interactions
endpoint. UseZombie relays the decision — approval gate resumes or integration grant
is recorded.

Two types of interactions are handled:

1. **Approval gate** (M4) — per-action, for firewall-gated endpoints
   Example: "Hiring Agent wants to send an offer letter to Jane Smith — Approve?"

2. **Integration grant** (M9) — one-time, for new service access requests
   Example: "Ops Zombie is requesting Grafana access — Approve?"

---

## Actors

```
[Zombie reaches a gated action]
    → UseZombie posts Block Kit message to Slack (Approve/Deny buttons)
        → Human clicks [Approve] or [Deny]
            → Slack sends POST /v1/slack/interactions
                → UseZombie:
                    1. Verify x-slack-signature (HMAC-SHA256, <5min window)
                    2. Parse interaction payload
                    3. Route by action_id:
                       - gate_* prefix → approval gate handler (M4)
                       - grant_* prefix → integration grant handler (M9)
                    4. Update state (gate approved, grant approved/revoked)
                    5. HTTP 200 to Slack
            → Zombie execution resumes (approve) or receives denied error (deny)
```

---

## Prerequisites

- Workspace has Slack connected (see `slack_oauth_install.md`)
- **Interactivity enabled** in Slack App dashboard (see setup below)
- `SLACK_SIGNING_SECRET` environment variable configured in UseZombie

---

## Setup — step by step

### Step 1: Enable Interactivity in Slack

In your Slack App dashboard (https://api.slack.com/apps):

1. **Interactivity & Shortcuts** → Toggle **Interactivity** ON
2. **Request URL:**
   ```
   https://api.usezombie.com/v1/slack/interactions
   ```
3. Save changes and reinstall the app if prompted.

That's it. UseZombie handles the rest.

### Step 2: Configure your zombie's approval gates (if using M4)

```bash
zombiectl zombie firewall set --zombie {zombie_id} --config '{
  "endpoint_rules": [
    {
      "method": "POST",
      "domain": "lever.co",
      "path": "/v1/offers",
      "action": "requires_approval"
    }
  ]
}'
```

When the zombie calls this endpoint via `/v1/execute`, the firewall intercepts it,
posts the Slack approval message, and waits for the human's response.

---

## What the approval message looks like

**Approval gate (M4):**
```
🔐 Hiring Agent wants to execute an action

Tool: lever_api
Action: create_offer
Details: Candidate: Jane Smith | Role: Senior Engineer | Salary: $180k

[Approve] [Deny]
```

**Integration grant request (M9):**
```
🔐 Ops Zombie is requesting Grafana access

Reason: "Need to query Loki log streams and fetch alert context to classify incidents"
Scopes: Full access (*)

[Approve] [Deny]
```

---

## What happens after clicking

### Approve

- **Approval gate:** Zombie execution resumes. The gated API call is made, credential
  injected, response returned to zombie.
- **Integration grant:** Grant status set to `approved`. All future execute calls from
  this zombie to Grafana are now authorized without re-asking.

### Deny

- **Approval gate:** Zombie receives a `gate_denied` error. Zombie can reason about
  what to do next (e.g. notify the team, skip the action, try an alternative).
- **Integration grant:** Grant status set to `revoked`. Zombie receives `UZ-GRANT-003`
  on any execute call for that service.

---

## How UseZombie matches buttons to gates

Each Slack button has an `action_id` with an encoded payload:

```
gate_{zombie_id}_{action_id}_{decision}
grant_{zombie_id}_{grant_id}_{decision}
```

UseZombie extracts these fields from the interaction payload and routes accordingly.
The button payload is signed — tampering is rejected at the signature verification step.

---

## Multi-channel approval

If your workspace has both Slack and Discord configured, approval requests are sent to
all configured channels simultaneously. A click in either channel approves globally.
UseZombie handles the deduplication — a second click after the decision is already
recorded is a no-op.

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Clicking Approve does nothing | Interactivity not configured | Enable in Slack App → Interactivity, set Request URL |
| "Expired action" from Slack | Gate timed out (default 1 hour) | Zombie already timed out; re-trigger the action |
| `UZ-WH-010` on click | Signing secret mismatch | Verify `SLACK_SIGNING_SECRET` matches Slack App's signing secret |
| Approve sent but zombie still waiting | Redis gate key expired | Gate TTL (2h) passed; zombie received timeout error |
| Grant approval not reflecting | Grant ID mismatch | Check `zombiectl grant list --zombie {id}` for current grant status |
| Buttons not appearing in message | Bot not added to DM channel | Start a DM with the bot first: search `@usezombie` in Slack |

---

## Security properties

- **Signing verification:** Every interaction payload is verified via HMAC-SHA256 before
  processing. Invalid signatures return HTTP 401 `UZ-WH-010`.
- **Timestamp freshness:** Payloads with `x-slack-request-timestamp` older than 5 minutes
  are rejected as potential replays (`UZ-WH-011`).
- **Constant-time comparison:** Signature bytes are compared without short-circuiting
  (RULE CTM) — no timing side-channel on the secret.
- **No credential exposure:** The interaction payload never contains credential values.
  Only action IDs, grant IDs, and decisions pass through this endpoint.
